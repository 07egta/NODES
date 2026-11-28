//! A single-producer/single-consumer mailbox for snapshots, not event streams.
//! Only the most recent undelivered value is retained. A one-slot wake channel
//! avoids both an unbounded backlog and blocking the GUI on a running evaluator.
//! Revisions must still be checked by callers before publishing/using a result.
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, RecvError, SendError, SyncSender, TryRecvError, TrySendError},
};

struct Mailbox<T> {
    pending: Option<T>,
    receiver_alive: bool,
}

pub struct LatestSender<T> {
    state: Arc<Mutex<Mailbox<T>>>,
    wake: SyncSender<()>,
}

pub struct LatestReceiver<T> {
    state: Arc<Mutex<Mailbox<T>>>,
    wake: Receiver<()>,
}

pub fn channel<T>() -> (LatestSender<T>, LatestReceiver<T>) {
    let state = Arc::new(Mutex::new(Mailbox {
        pending: None,
        receiver_alive: true,
    }));
    let (sender, receiver) = mpsc::sync_channel(1);
    (
        LatestSender {
            state: Arc::clone(&state),
            wake: sender,
        },
        LatestReceiver {
            state,
            wake: receiver,
        },
    )
}

impl<T> LatestSender<T> {
    /// Replace the undelivered snapshot and signal a receiver without waiting.
    /// Dropping a superseded snapshot happens outside the mutex.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.receiver_alive {
            return Err(SendError(value));
        }
        // Signal while holding the state lock. A receiver may consume the wake,
        // but cannot take the snapshot until it has been published below.
        match self.wake.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => {}
            Err(TrySendError::Disconnected(())) => return Err(SendError(value)),
        }
        let old = state.pending.replace(value);
        drop(state);
        drop(old);
        Ok(())
    }
}

impl<T> LatestReceiver<T> {
    fn take(&self) -> Option<T> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .take()
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        loop {
            self.wake.recv()?;
            // A redundant wake can remain after a racing replacement was taken.
            if let Some(value) = self.take() {
                return Ok(value);
            }
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        loop {
            self.wake.try_recv()?;
            if let Some(value) = self.take() {
                return Ok(value);
            }
        }
    }
}

impl<T> Drop for LatestReceiver<T> {
    fn drop(&mut self) {
        // Mark closed before the wake receiver is dropped, so a concurrent
        // sender cannot publish a new snapshot into an abandoned mailbox.
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.receiver_alive = false;
        let pending = state.pending.take();
        drop(state);
        // User-defined destructors must never run under the mailbox lock.
        drop(pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn thousands_of_updates_keep_only_the_latest() {
        let (tx, rx) = channel();
        for i in 0..10_000 {
            tx.send(i).unwrap();
        }
        assert_eq!(rx.try_recv().unwrap(), 9_999);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        tx.send(10_000).unwrap();
        assert_eq!(rx.recv().unwrap(), 10_000);
    }

    #[test]
    fn superseded_snapshots_release_their_memory_immediately() {
        #[derive(Debug)]
        struct Snapshot(Arc<AtomicUsize>);
        impl Drop for Snapshot {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let count = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        for _ in 0..1_000 {
            tx.send(Snapshot(Arc::clone(&count))).unwrap();
        }
        assert_eq!(count.load(Ordering::Relaxed), 999);
        drop(rx.recv().unwrap());
        assert_eq!(count.load(Ordering::Relaxed), 1_000);
    }

    #[test]
    fn worker_can_consume_the_final_snapshot_before_disconnect() {
        let (tx, rx) = channel();
        tx.send(1).unwrap();
        let worker = std::thread::spawn(move || {
            assert_eq!(rx.recv().unwrap(), 1);
            // A second wake is not required when the producer closes.
            assert!(rx.recv().is_err());
        });
        drop(tx);
        worker.join().unwrap();
    }

    #[test]
    fn concurrent_burst_never_delivers_out_of_order() {
        let (tx, rx) = channel();
        let producer = std::thread::spawn(move || {
            for i in 1..=20_000 {
                tx.send(i).unwrap();
            }
        });
        let mut previous = 0;
        while let Ok(next) = rx.recv() {
            assert!(next > previous);
            previous = next;
        }
        producer.join().unwrap();
        assert_eq!(previous, 20_000);
    }

    #[test]
    fn dropping_receiver_releases_pending_snapshot_while_sender_lives() {
        let (tx, rx) = channel();
        let payload = Arc::new(vec![0_u8; 1024 * 1024]);
        let weak = Arc::downgrade(&payload);
        tx.send(payload).unwrap();
        assert!(weak.upgrade().is_some());
        drop(rx);
        assert!(weak.upgrade().is_none());
        // The sender still exists and returns ownership on failure.
        let rejected = Arc::new(vec![1_u8]);
        let weak = Arc::downgrade(&rejected);
        let error = tx.send(rejected).unwrap_err();
        assert!(weak.upgrade().is_some());
        assert_eq!(error.0.as_slice(), &[1]);
        drop(error);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn receiver_drop_racing_with_send_never_retains_a_snapshot() {
        for _ in 0..64 {
            let (tx, rx) = channel();
            let first = Arc::new(1);
            let first_weak = Arc::downgrade(&first);
            tx.send(first).unwrap();
            let second = Arc::new(2);
            let second_weak = Arc::downgrade(&second);
            let barrier = std::sync::Barrier::new(2);
            let result = std::thread::scope(|scope| {
                let producer = scope.spawn(|| {
                    barrier.wait();
                    tx.send(second)
                });
                barrier.wait();
                drop(rx);
                producer.join().unwrap()
            });
            if let Err(error) = result {
                assert_eq!(*error.0, 2);
            }
            assert!(first_weak.upgrade().is_none());
            assert!(second_weak.upgrade().is_none());
            assert!(tx.send(Arc::new(3)).is_err());
        }
    }

    #[test]
    fn disconnection_returns_the_submitted_snapshot() {
        let (tx, rx) = channel();
        drop(rx);
        assert_eq!(tx.send("pending").unwrap_err().0, "pending");
    }
}
