//! Cooperative diagnostic cancellation never publishes a partially built report.
//! Poll counts are observed per run, rather than assuming a callback schedule or
//! imposing a machine-dependent wall-time deadline.
use morphlet::diagnostics::{
    DiagnosticReport, MAX_REPORTED_AFFECTED, MAX_REPORTED_ROOTS, diagnose, diagnose_cancellable,
};
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, NodeId, NodeKind};
use std::cell::Cell;

fn add(graph: &mut Graph, kind: NodeKind, inputs: &[(&str, NodeId)]) -> NodeId {
    let id = graph.add(kind, [0.; 2]).unwrap();
    let node = graph.nodes.last_mut().unwrap();
    if kind == NodeKind::Divide {
        node.params.insert("b".into(), 0.);
    }
    node.inputs = inputs
        .iter()
        .map(|(port, source)| ((*port).into(), *source))
        .collect();
    id
}

fn counted_report(graph: &Graph, evaluation: &Evaluation) -> (DiagnosticReport, usize) {
    let calls = Cell::new(0);
    let report = diagnose_cancellable(graph, evaluation, || {
        calls.set(calls.get() + 1);
        false
    })
    .expect("an analysis that is never cancelled must return a complete report");
    let expected = serde_json::to_vec(&diagnose(graph, evaluation)).unwrap();
    assert!(
        serde_json::to_vec(&report).unwrap() == expected,
        "the existing API and never-cancelled API must serialize identically"
    );
    assert!(calls.get() > 0);
    (report, calls.get())
}

fn cancel_once_at(graph: &Graph, evaluation: &Evaluation, stop_at: usize) {
    let calls = Cell::new(0);
    let result = diagnose_cancellable(graph, evaluation, || {
        calls.set(calls.get() + 1);
        // A one-shot signal must end construction immediately. A second poll
        // would return false; no persistent external flag is required.
        calls.get() == stop_at
    });
    assert!(result.is_none(), "one-shot cancellation at poll {stop_at}");
    assert_eq!(calls.get(), stop_at, "no callback after a stop request");
}

fn assert_evaluation_unchanged(actual: &Evaluation, before: &Evaluation) {
    assert_eq!(actual.outputs, before.outputs);
    assert_eq!(actual.node_timings, before.node_timings);
    assert_eq!(actual.computed, before.computed);
    assert_eq!(actual.cached, before.cached);
    assert_eq!(actual.elapsed_ms.to_bits(), before.elapsed_ms.to_bits());
    assert_eq!(actual.cancelled, before.cancelled);
}

#[test]
fn cancellation_during_many_root_propagation_and_transpose_discards_the_report() {
    const ROOT_COUNT: usize = 512;
    const TAIL_COUNT: usize = 300;
    let mut graph = Graph::default();
    let roots: Vec<_> = (0..ROOT_COUNT)
        .map(|_| add(&mut graph, NodeKind::Divide, &[]))
        .collect();
    let mut tip = roots[0];
    for &root in &roots[1..] {
        tip = add(&mut graph, NodeKind::Add, &[("a", tip), ("b", root)]);
    }
    let final_join = tip;
    for _ in 0..TAIL_COUNT {
        tip = add(&mut graph, NodeKind::Add, &[("a", tip)]);
    }
    graph.validate().unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let graph_before = graph.clone();
    let evaluation_before = evaluation.clone();
    let (report, total_polls) = counted_report(&graph, &evaluation);

    // This oracle follows the graph's definition, not the implementation's
    // root-set grouping or bitset representation. Every prefix join fails.
    assert!(report.graph_error.is_none());
    assert!(report.summary.complete && !report.summary.cancelled);
    assert_eq!(report.summary.failed_count, graph.nodes.len());
    assert_eq!(report.summary.root_failure_count, ROOT_COUNT);
    assert_eq!(report.summary.blocked_count, ROOT_COUNT - 1 + TAIL_COUNT);
    assert_eq!(
        report.root_failures[0].affected_count,
        ROOT_COUNT - 1 + TAIL_COUNT
    );
    assert_eq!(
        report.root_failures[0].affected_nodes,
        (ROOT_COUNT as NodeId..(ROOT_COUNT + MAX_REPORTED_AFFECTED) as NodeId).collect::<Vec<_>>()
    );
    assert!(report.root_failures[0].affected_nodes_truncated);
    let final_root = report.root_failures.last().unwrap();
    assert_eq!(final_root.node.id, roots[ROOT_COUNT - 1]);
    assert_eq!(final_root.affected_count, TAIL_COUNT + 1);
    assert_eq!(final_root.affected_nodes.first(), Some(&final_join));
    let final_node = report.blocked_nodes.last().unwrap();
    assert_eq!(final_node.node.id, tip);
    assert_eq!(final_node.upstream_root_count, ROOT_COUNT);
    assert_eq!(final_node.upstream_roots, roots[..MAX_REPORTED_ROOTS]);
    assert!(final_node.upstream_roots_truncated);

    // Probe several interior boundaries and the final publication boundary. The
    // precise number of polls and the phase at a given poll are not API promises.
    let mut stop_points = vec![
        1,
        2,
        total_polls / 4,
        total_polls / 2,
        total_polls.saturating_sub(1),
        total_polls,
    ];
    stop_points.retain(|&point| point > 0 && point <= total_polls);
    stop_points.sort_unstable();
    stop_points.dedup();
    for point in stop_points {
        cancel_once_at(&graph, &evaluation, point);
    }
    assert_eq!(graph, graph_before);
    assert_evaluation_unchanged(&evaluation, &evaluation_before);
    assert!(
        serde_json::to_vec(&diagnose(&graph, &evaluation)).unwrap()
            == serde_json::to_vec(&report).unwrap(),
        "abandoned analyses must not affect a subsequent fresh analysis"
    );
}

#[test]
fn entry_cancellation_returns_none_for_valid_empty_and_invalid_graphs() {
    let empty = Graph::default();
    let mut valid = Graph::default();
    add(&mut valid, NodeKind::Number, &[]);
    let mut invalid = Graph::default();
    let a = add(&mut invalid, NodeKind::Add, &[]);
    let b = add(&mut invalid, NodeKind::Add, &[("a", a)]);
    invalid.node_mut(a).unwrap().inputs.insert("a".into(), b);
    for graph in [empty, valid, invalid] {
        let evaluation = Evaluator::default().evaluate(&graph, &|| false);
        let before = evaluation.clone();
        cancel_once_at(&graph, &evaluation, 1);
        assert_evaluation_unchanged(&evaluation, &before);
    }
}

#[test]
fn empty_success_disabled_and_invalid_reports_respect_late_cancellation() {
    let empty = Graph::default();
    let mut successful = Graph::default();
    let number = add(&mut successful, NodeKind::Number, &[]);
    add(&mut successful, NodeKind::Add, &[("a", number)]);
    let mut disabled = successful.clone();
    disabled.node_mut(number).unwrap().off = true;
    let mut invalid = Graph::default();
    let a = add(&mut invalid, NodeKind::Add, &[]);
    let b = add(&mut invalid, NodeKind::Add, &[("a", a)]);
    invalid.node_mut(a).unwrap().inputs.insert("a".into(), b);

    for (index, graph) in [empty, successful, disabled, invalid].iter().enumerate() {
        let evaluation = Evaluator::default().evaluate(graph, &|| false);
        let (report, last_poll) = counted_report(graph, &evaluation);
        assert!(!report.summary.cancelled);
        assert!(report.root_failures.is_empty() && report.blocked_nodes.is_empty());
        match index {
            0 => {
                assert!(report.summary.complete && report.graph_error.is_none());
                assert_eq!(report.summary.node_count, 0);
            }
            1 => {
                assert!(report.summary.complete && report.graph_error.is_none());
                assert_eq!(report.summary.successful_count, 2);
            }
            2 => {
                assert!(report.summary.complete && report.graph_error.is_none());
                assert_eq!(report.summary.successful_count, 1);
                assert_eq!(report.summary.disabled_count, 1);
                assert_eq!(report.disabled_nodes[0].id, number);
            }
            3 => {
                assert!(!report.summary.complete);
                assert_eq!(report.summary.failed_count, 2);
                assert_eq!(report.graph_error, graph.topological_order().err());
                assert!(report.graph_error.is_some());
                assert!(report.hotspots.is_empty());
            }
            _ => unreachable!(),
        }
        // Stop on the last observed callback, including fast return paths. No
        // graph-wide error or otherwise complete report may escape after it.
        cancel_once_at(graph, &evaluation, last_poll);
    }
}

#[test]
fn evaluation_cancellation_is_preserved_unless_diagnostic_construction_is_cancelled() {
    let mut graph = Graph::default();
    let completed = add(&mut graph, NodeKind::Number, &[]);
    let interrupted = add(&mut graph, NodeKind::Series, &[]);
    graph
        .node_mut(interrupted)
        .unwrap()
        .params
        .insert("count".into(), 512.);
    let unvisited = add(&mut graph, NodeKind::Add, &[("a", interrupted)]);
    let evaluation_calls = std::sync::atomic::AtomicUsize::new(0);
    let evaluation = Evaluator::default().evaluate(&graph, &|| {
        evaluation_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 == 30
    });
    assert!(evaluation.cancelled);
    assert!(evaluation.outputs[&completed].is_ok());
    assert!(!evaluation.outputs.contains_key(&interrupted));
    assert!(evaluation.node_timings.contains_key(&interrupted));
    assert!(!evaluation.node_timings.contains_key(&unvisited));
    let before = evaluation.clone();
    let (report, last_poll) = counted_report(&graph, &evaluation);
    assert!(report.summary.cancelled && !report.summary.complete);
    assert_eq!(report.summary.completed_count, 1);
    assert_eq!(report.summary.successful_count, 1);
    assert_eq!(report.summary.unevaluated_count, 2);
    assert_eq!(
        report
            .unevaluated_nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        vec![interrupted, unvisited]
    );
    assert!(report.root_failures.is_empty() && report.blocked_nodes.is_empty());
    let partial = report
        .hotspots
        .iter()
        .find(|hotspot| hotspot.node.id == interrupted)
        .unwrap();
    assert!(!partial.completed && !partial.cached);
    cancel_once_at(&graph, &evaluation, last_poll);
    assert_evaluation_unchanged(&evaluation, &before);

    // No evaluation has been attempted: unevaluated is distinct from a success
    // or a cancelled report, even with a non-cancelling analysis callback.
    let (unstarted, _) = counted_report(&graph, &Evaluation::default());
    assert!(!unstarted.summary.complete && !unstarted.summary.cancelled);
    assert_eq!(unstarted.summary.unevaluated_count, 3);
    assert_eq!(unstarted.summary.successful_count, 0);
    assert!(unstarted.hotspots.is_empty());
}
