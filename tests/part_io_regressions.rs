//! Reusable parts follow the same bounded regular-file contract as projects.
use morphlet::cluster;
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, NodeKind, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("nodes-part-io-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn existing_graph() -> Graph {
    let mut graph = Graph::default();
    graph.add(NodeKind::Number, [12., 34.]).unwrap();
    graph
}

#[test]
fn part_directories_are_rejected_before_reading_and_leave_the_graph_unchanged() {
    let temp = TempDir::new();
    let mut graph = existing_graph();
    let before = graph.clone();
    let error = cluster::load_part(&temp.0, &mut graph).unwrap_err();
    assert!(error.contains("通常のファイル"), "{error}");
    assert_eq!(graph, before);
}

#[test]
fn regular_parts_still_load_evaluate_and_keep_bounded_failure_atomic() {
    let temp = TempDir::new();
    let path = temp.0.join("reusable.morphpart");
    let mut source = Graph::default();
    let node = source.add(NodeKind::Number, [0., 0.]).unwrap();
    source
        .node_mut(node)
        .unwrap()
        .params
        .insert("value".into(), 42.);
    let instance = cluster::group_nodes(&mut source, &[node], "Answer").unwrap();
    let definition = source.node(instance).unwrap().cluster.unwrap();
    cluster::save_part(&path, &source, definition).unwrap();

    let mut target = existing_graph();
    let imported = cluster::load_part(&path, &mut target).unwrap();
    let node = cluster::instantiate(&mut target, imported, [200., 0.]).unwrap();
    let result = Evaluator::default().evaluate(&target, &|| false);
    let values: Vec<_> = result.outputs[&node]
        .as_ref()
        .unwrap()
        .branches
        .values()
        .flatten()
        .cloned()
        .collect();
    assert_eq!(values, vec![Value::Number(42.)]);

    let before = target.clone();
    fs::File::create(&path)
        .unwrap()
        .set_len((cluster::MAX_PART_BYTES + 1) as u64)
        .unwrap();
    assert!(cluster::load_part(&path, &mut target).is_err());
    assert_eq!(target, before);
    fs::write(&path, b"{invalid").unwrap();
    assert!(cluster::load_part(&path, &mut target).is_err());
    assert_eq!(target, before);
}

#[cfg(unix)]
#[test]
fn fifo_parts_are_rejected_without_waiting_for_a_writer() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    const CHILD_PATH: &str = "NODES_PART_IO_TEST_FIFO";

    // Re-execute only this test in a child so a regression cannot leave a
    // permanently blocked reader thread in the test process.
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        let mut graph = existing_graph();
        let before = graph.clone();
        let error = cluster::load_part(std::path::Path::new(&path), &mut graph).unwrap_err();
        assert!(error.contains("通常のファイル"), "{error}");
        assert_eq!(graph, before);
        return;
    }

    let temp = TempDir::new();
    let fifo = temp.0.join("named-pipe.morphpart");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fifo_parts_are_rejected_without_waiting_for_a_writer",
            "--nocapture",
        ])
        .env(CHILD_PATH, &fifo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("could not poll part loader: {error}");
            }
        }
        if Instant::now() >= deadline {
            let killed = child.kill();
            let waited = child.wait();
            panic!("part loader waited for a FIFO writer: kill={killed:?}, wait={waited:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
