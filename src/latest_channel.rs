//! A single-producer/single-consumer mailbox for snapshots, not event streams.
//! Only the most recent undelivered value is retained. A one-slot wake channel
//! avoids both an unbounded backlog and blocking the GUI on a running evaluator.
//! Revisions must still be checked by callers before publishing/using a result.
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, RecvError, SendError, SyncSender, TryRecvError, TrySendError},
};

pub struct LatestSender<T> {
    pending: Arc<Mutex<Option<T>>>,
    wake: SyncSender<()>,
}

pub struct LatestReceiver<T> {
    pending: Arc<Mutex<Option<T>>>,
    wake: Receiver<()>,
}

pub fn channel<T>() -> (LatestSender<T>, LatestReceiver<T>) {
    let pending = Arc::new(Mutex::new(None));
    let (sender, receiver) = mpsc::sync_channel(1);
    (
        LatestSender {
            pending: Arc::clone(&pending),
            wake: sender,
        },
        LatestReceiver {
            pending,
            wake: receiver,
        },
    )
}

impl<T> LatestSender<T> {
    /// Replace the undelivered snapshot and signal a receiver without waiting.
    /// Dropping a superseded snapshot happens outside the mutex.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut slot = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let old = slot.replace(value);
        let result = match self.wake.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Disconnected(())) => {
                // The slot is still locked; the value we just placed is present.
                Err(SendError(
                    slot.take().expect("mailbox holds the submitted value"),
                ))
            }
        };
        drop(slot);
        drop(old);
        result
    }
}

impl<T> LatestReceiver<T> {
    fn take(&self) -> Option<T> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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
    fn disconnection_returns_the_submitted_snapshot() {
        let (tx, rx) = channel();
        drop(rx);
        assert_eq!(tx.send("pending").unwrap_err().0, "pending");
    }
}
