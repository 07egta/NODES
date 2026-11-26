//! Recreated after runtime loss; observed times have no hardware speed threshold.
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, NodeId, NodeKind};
use std::cell::Cell;
fn node(g: &mut Graph, k: NodeKind) -> NodeId { g.add(k, [0.; 2]).unwrap() }
fn valid_times(e: &Evaluation) {
    let total: f64 = e.node_timings.values().map(|t| {
        assert!(t.elapsed_ms.is_finite() && t.elapsed_ms >= 0.);
        t.elapsed_ms
    }).sum();
    assert!(total <= e.elapsed_ms + 1e-6);
}
#[test]
fn timings_distinguish_cache_hits_and_downstream_edits() {
    let mut g = Graph::default();
    let source = node(&mut g, NodeKind::Number);
    let middle = node(&mut g, NodeKind::Add);
    let end = node(&mut g, NodeKind::Add);
    let unrelated = node(&mut g, NodeKind::Number);
    g.connect(source, middle, "a").unwrap();
    g.connect(middle, end, "a").unwrap();
    let mut engine = Evaluator::default();
    let cold = engine.evaluate(&g, &|| false);
    assert_eq!((cold.computed, cold.cached), (4, 0));
    assert_eq!(cold.node_timings.len(), 4);
    assert!(cold.node_timings.values().all(|t| !t.cached));
    valid_times(&cold);
    let warm = engine.evaluate(&g, &|| false);
    assert_eq!((warm.computed, warm.cached), (0, 4));
    assert!(warm.node_timings.values().all(|t| t.cached));
    valid_times(&warm);
    g.node_mut(middle).unwrap().params.insert("b".into(), 10.);
    let edit = engine.evaluate(&g, &|| false);
    assert_eq!((edit.computed, edit.cached), (2, 2));
    for (id, hit) in [(source, true), (unrelated, true), (middle, false), (end, false)] {
        assert_eq!(edit.node_timings[&id].cached, hit);
    }
    valid_times(&edit);
}
#[test]
fn disabled_nodes_have_uncached_bookkeeping_time() {
    let mut g = Graph::default();
    let id = node(&mut g, NodeKind::Number);
    let mut engine = Evaluator::default();
    engine.evaluate(&g, &|| false);
    g.node_mut(id).unwrap().off = true;
    let e = engine.evaluate(&g, &|| false);
    assert_eq!((e.computed, e.cached), (0, 0));
    assert_eq!(e.outputs[&id].as_ref().unwrap().item_count(), 0);
    assert!(!e.node_timings[&id].cached);
    valid_times(&e);
    g.node_mut(id).unwrap().off = false;
    let enabled = engine.evaluate(&g, &|| false);
    assert_eq!((enabled.computed, enabled.cached), (1, 0));
    assert!(!enabled.node_timings[&id].cached);
}
#[test]
fn cached_errors_keep_their_timing_status() {
    let mut g = Graph::default();
    let id = node(&mut g, NodeKind::Divide);
    g.node_mut(id).unwrap().params.insert("b".into(), 0.);
    let mut engine = Evaluator::default();
    let first = engine.evaluate(&g, &|| false);
    assert!(first.outputs[&id].is_err());
    assert!(!first.node_timings[&id].cached);
    let second = engine.evaluate(&g, &|| false);
    assert_eq!(second.outputs, first.outputs);
    assert!(second.node_timings[&id].cached);
    valid_times(&second);
}
#[test]
fn validation_and_prestart_cancellation_do_not_invent_node_times() {
    let mut g = Graph::default();
    let a = node(&mut g, NodeKind::Add);
    let b = node(&mut g, NodeKind::Add);
    g.node_mut(a).unwrap().inputs.insert("a".into(), b);
    g.node_mut(b).unwrap().inputs.insert("a".into(), a);
    let invalid = Evaluator::default().evaluate(&g, &|| false);
    assert!(invalid.outputs.values().all(Result::is_err));
    assert!(invalid.node_timings.is_empty());
    let stopped = Evaluator::default().evaluate(&g, &|| true);
    assert!(stopped.cancelled && stopped.outputs.is_empty() && stopped.node_timings.is_empty());
}
#[test]
fn interrupted_node_has_partial_time_but_no_partial_output_or_cache() {
    let mut g = Graph::default();
    let id = node(&mut g, NodeKind::Series);
    g.node_mut(id).unwrap().params.insert("count".into(), 512.);
    let calls = Cell::new(0);
    let mut engine = Evaluator::default();
    let stopped = engine.evaluate(&g, &|| { calls.set(calls.get() + 1); calls.get() == 20 });
    assert!(stopped.cancelled);
    assert!(stopped.outputs.is_empty());
    assert_eq!((stopped.computed, stopped.cached), (0, 0));
    assert!(!stopped.node_timings[&id].cached);
    valid_times(&stopped);
    let retried = engine.evaluate(&g, &|| false);
    assert_eq!((retried.computed, retried.cached), (1, 0));
    assert_eq!(retried.outputs[&id].as_ref().unwrap().item_count(), 512);
    let warm = engine.evaluate(&g, &|| false);
    assert_eq!((warm.computed, warm.cached), (0, 1));
}
