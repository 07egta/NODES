//! Speed contracts for the evaluator: only downstream nodes are recomputed, cancelled or cached
//! runs give the same result as a fresh run, and generous time limits catch accidental
//! slow-downs. The limits are far above normal values so slow CI machines do not fail;
//! the measured times are printed (`cargo test --test engine_perf_tests -- --nocapture`).
use nodes::engine::{Evaluation, Evaluator};
use nodes::model::{Graph, NodeId, NodeKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const CHAIN: usize = 2_000;
/// Far above any normal value; only a real regression (e.g. quadratic time) reaches it.
const LIMIT_MS: f64 = 20_000.;

fn chain(count: usize) -> Graph {
    let mut graph = Graph::default();
    graph.add(NodeKind::Number, [0., 0.]).unwrap();
    graph.add(NodeKind::Add, [200., 0.]).unwrap();
    let number = graph.nodes[0].clone();
    let mut add = graph.nodes[1].clone();
    add.params.insert("b".into(), 1.);
    graph.nodes = (0..count)
        .map(|index| {
            let mut node = if index == 0 {
                number.clone()
            } else {
                add.clone()
            };
            node.id = index as NodeId;
            if index > 0 {
                node.inputs.insert("a".into(), (index - 1) as NodeId);
            }
            node
        })
        .collect();
    graph.next_id = count as NodeId;
    graph.validate().unwrap();
    graph
}

/// Many independent meshes: one graph level wide enough to use the worker threads.
fn wide(count: usize) -> Graph {
    let mut graph = Graph::default();
    for i in 0..count {
        let sphere = graph.add(NodeKind::Sphere, [0., i as f32 * 80.]).unwrap();
        let node = graph.node_mut(sphere).unwrap();
        node.params.insert("segments".into(), (24 + i % 8) as f64);
        node.params.insert("rings".into(), (12 + i % 4) as f64);
    }
    graph
}

fn assert_same(a: &Evaluation, b: &Evaluation) {
    assert_eq!(a.outputs, b.outputs);
}

#[test]
fn edit_recomputes_only_downstream_and_results_match_a_fresh_run() {
    let mut graph = chain(CHAIN);
    let mut evaluator = Evaluator::default();
    let start = Instant::now();
    let cold = evaluator.evaluate(&graph, &|| false);
    let cold_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!((cold.computed, cold.cached), (CHAIN, 0));

    let start = Instant::now();
    let warm = evaluator.evaluate(&graph, &|| false);
    let warm_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!((warm.computed, warm.cached), (0, CHAIN));
    assert_same(&cold, &warm);

    // A change at the very end touches one node; a change at the start touches all of them.
    let last = (CHAIN - 1) as NodeId;
    graph.node_mut(last).unwrap().params.insert("b".into(), 5.);
    let start = Instant::now();
    let tail = evaluator.evaluate(&graph, &|| false);
    let tail_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!((tail.computed, tail.cached), (1, CHAIN - 1));
    assert_same(&tail, &Evaluator::default().evaluate(&graph, &|| false));

    graph.node_mut(0).unwrap().params.insert("value".into(), 3.);
    let start = Instant::now();
    let head = evaluator.evaluate(&graph, &|| false);
    let head_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(head.computed, CHAIN);
    assert_same(&head, &Evaluator::default().evaluate(&graph, &|| false));

    println!(
        "chain={CHAIN} cold_ms={cold_ms:.3} warm_ms={warm_ms:.3} edit_last_ms={tail_ms:.3} edit_first_ms={head_ms:.3}"
    );
    for ms in [cold_ms, warm_ms, tail_ms, head_ms] {
        assert!(ms < LIMIT_MS, "{ms} ms");
    }
}

#[test]
fn cancelled_run_stops_early_keeps_no_partial_result_and_next_run_is_correct() {
    let graph = chain(CHAIN);
    let mut evaluator = Evaluator::default();
    let polls = AtomicUsize::new(0);
    let start = Instant::now();
    let stopped = evaluator.evaluate(&graph, &|| polls.fetch_add(1, Ordering::Relaxed) >= 50);
    let stop_ms = start.elapsed().as_secs_f64() * 1000.;
    assert!(stopped.cancelled);
    assert!(stopped.computed < CHAIN);
    // Nothing from the interrupted run may leak into the next one.
    let again = evaluator.evaluate(&graph, &|| false);
    assert!(!again.cancelled);
    assert_eq!(again.computed + again.cached, CHAIN);
    assert_same(&again, &Evaluator::default().evaluate(&graph, &|| false));
    println!(
        "cancel after 50 polls: computed={} stop_ms={stop_ms:.3}",
        stopped.computed
    );
    assert!(stop_ms < LIMIT_MS);
}

#[test]
fn parallel_and_sequential_levels_give_identical_results() {
    let graph = wide(48);
    let mut sequential = Evaluator::default();
    sequential.set_max_threads(1);
    let start = Instant::now();
    let one = sequential.evaluate(&graph, &|| false);
    let one_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut parallel = Evaluator::default();
    parallel.set_max_threads(8);
    let start = Instant::now();
    let many = parallel.evaluate(&graph, &|| false);
    let many_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!((one.computed, many.computed), (48, 48));
    assert!(one.outputs.values().all(Result::is_ok));
    assert_same(&one, &many);
    println!("wide=48 one_thread_ms={one_ms:.3} eight_threads_ms={many_ms:.3}");
    assert!(one_ms < LIMIT_MS && many_ms < LIMIT_MS);
}
