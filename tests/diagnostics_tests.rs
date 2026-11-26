use morphlet::cluster::ClusterDef;
use morphlet::diagnostics::{MAX_REPORTED_AFFECTED, MAX_REPORTED_ROOTS, NodeFailure, diagnose};
use morphlet::engine::{Evaluation, Evaluator, NodeTiming};
use morphlet::model::{Graph, NodeId, NodeKind, Tree};
use std::cell::Cell;

fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add(kind, [0.; 2]).unwrap()
}

fn failing_divide(graph: &mut Graph) -> NodeId {
    let id = add(graph, NodeKind::Divide);
    graph.node_mut(id).unwrap().params.insert("b".into(), 0.);
    id
}

fn ids(failures: &[NodeFailure]) -> Vec<NodeId> {
    failures.iter().map(|failure| failure.node.id).collect()
}

#[test]
fn two_root_failures_merge_without_duplicate_causes_or_affected_nodes() {
    let mut graph = Graph::default();
    let a = failing_divide(&mut graph);
    let b = failing_divide(&mut graph);
    let first = add(&mut graph, NodeKind::Add);
    let joined = add(&mut graph, NodeKind::Add);
    let last = add(&mut graph, NodeKind::Multiply);
    let successful = add(&mut graph, NodeKind::Number);
    graph.connect(a, first, "a").unwrap();
    graph.connect(a, first, "b").unwrap();
    graph.connect(first, joined, "a").unwrap();
    graph.connect(b, joined, "b").unwrap();
    graph.connect(joined, last, "a").unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    assert!(evaluation.outputs[&successful].is_ok());
    let report = diagnose(&graph, &evaluation);
    assert!(report.graph_error.is_none());
    assert!(report.summary.complete);
    assert_eq!(report.causality, "dependency_inference");
    assert_eq!(report.summary.node_count, 6);
    assert_eq!(report.summary.connection_count, 5);
    assert_eq!(report.summary.completed_count, 6);
    assert_eq!(report.summary.successful_count, 1);
    assert_eq!(report.summary.failed_count, 5);
    assert_eq!(report.summary.root_failure_count, 2);
    assert_eq!(report.summary.blocked_count, 3);
    assert_eq!(ids(&report.root_failures), vec![a, b]);
    assert_eq!(ids(&report.blocked_nodes), vec![first, joined, last]);
    assert_eq!(
        report.root_failures[0].affected_nodes,
        vec![first, joined, last]
    );
    assert_eq!(report.root_failures[0].affected_count, 3);
    assert_eq!(report.root_failures[1].affected_nodes, vec![joined, last]);
    assert_eq!(report.blocked_nodes[0].upstream_roots, vec![a]);
    assert_eq!(report.blocked_nodes[1].upstream_roots, vec![a, b]);
    assert_eq!(report.blocked_nodes[2].upstream_root_count, 2);
    for failure in report.root_failures.iter().chain(&report.blocked_nodes) {
        assert_eq!(
            &failure.message,
            evaluation.outputs[&failure.node.id].as_ref().unwrap_err()
        );
    }

    graph.nodes.reverse();
    assert_eq!(
        serde_json::to_value(diagnose(&graph, &evaluation)).unwrap(),
        serde_json::to_value(report).unwrap(),
        "storage order must not change diagnostics or cause previews"
    );
}

#[test]
fn disabled_nodes_and_successful_empty_trees_break_failure_chains() {
    let mut graph = Graph::default();
    let root = failing_divide(&mut graph);
    let disabled = add(&mut graph, NodeKind::Add);
    let empty = add(&mut graph, NodeKind::Add);
    graph.connect(root, disabled, "a").unwrap();
    graph.connect(disabled, empty, "a").unwrap();
    graph.node_mut(disabled).unwrap().off = true;
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let empty_result = evaluation.outputs[&empty].as_ref().unwrap();
    assert_eq!(empty_result.branches.len(), 1);
    assert!(empty_result.branches[&vec![0]].is_empty());
    let report = diagnose(&graph, &evaluation);
    assert!(report.summary.complete);
    assert_eq!(report.summary.successful_count, 1);
    assert_eq!(report.summary.disabled_count, 1);
    assert_eq!(report.summary.failed_count, 1);
    assert_eq!(report.root_failures[0].affected_count, 0);
    assert!(report.blocked_nodes.is_empty());
    assert_eq!(report.disabled_nodes[0].id, disabled);
    assert!(
        report
            .hotspots
            .iter()
            .all(|hotspot| hotspot.node.id != disabled)
    );
}

#[test]
fn cancellation_keeps_finished_results_and_marks_the_interrupted_visit() {
    let mut graph = Graph::default();
    let completed = add(&mut graph, NodeKind::Number);
    let interrupted = add(&mut graph, NodeKind::Series);
    graph
        .node_mut(interrupted)
        .unwrap()
        .params
        .insert("count".into(), 512.);
    let later = add(&mut graph, NodeKind::Add);
    graph.connect(interrupted, later, "a").unwrap();
    let calls = Cell::new(0);
    let evaluation = Evaluator::default().evaluate(&graph, &|| {
        calls.set(calls.get() + 1);
        calls.get() == 30
    });
    assert!(evaluation.cancelled);
    assert!(evaluation.outputs[&completed].is_ok());
    assert!(!evaluation.outputs.contains_key(&interrupted));
    assert!(evaluation.node_timings.contains_key(&interrupted));
    let report = diagnose(&graph, &evaluation);
    assert!(report.summary.cancelled);
    assert!(!report.summary.complete);
    assert_eq!(report.summary.successful_count, 1);
    assert_eq!(report.summary.unevaluated_count, 2);
    assert_eq!(
        report
            .unevaluated_nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        vec![interrupted, later]
    );
    assert!(report.root_failures.is_empty() && report.blocked_nodes.is_empty());
    let hotspot = report
        .hotspots
        .iter()
        .find(|hotspot| hotspot.node.id == interrupted)
        .unwrap();
    assert!(!hotspot.completed && !hotspot.cached);
    assert!(
        report
            .hotspots
            .iter()
            .all(|hotspot| hotspot.node.id != later)
    );

    let never_started = diagnose(&graph, &Evaluation::default());
    assert!(!never_started.summary.complete);
    assert_eq!(never_started.summary.unevaluated_count, 3);
    assert!(never_started.root_failures.is_empty());
}

#[test]
fn graph_validation_errors_do_not_invent_node_causality() {
    let mut graph = Graph::default();
    let a = add(&mut graph, NodeKind::Add);
    let b = add(&mut graph, NodeKind::Add);
    graph.node_mut(a).unwrap().inputs.insert("a".into(), b);
    graph.node_mut(b).unwrap().inputs.insert("a".into(), a);
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    assert!(evaluation.outputs.values().all(Result::is_err));
    let report = diagnose(&graph, &evaluation);
    assert_eq!(report.graph_error, graph.topological_order().err());
    assert!(!report.summary.complete);
    assert_eq!(report.summary.failed_count, 2);
    assert_eq!(report.summary.root_failure_count, 0);
    assert_eq!(report.summary.blocked_count, 0);
    assert!(report.root_failures.is_empty() && report.blocked_nodes.is_empty());
    assert!(report.hotspots.is_empty());

    // A duplicate ID must remain a graph-wide error rather than picking one of
    // the colliding nodes and inventing a path through it.
    graph.node_mut(a).unwrap().inputs.clear();
    graph.node_mut(b).unwrap().id = a;
    let duplicate = diagnose(&graph, &Evaluation::default());
    assert!(duplicate.graph_error.is_some());
    assert!(duplicate.root_failures.is_empty());
}

#[test]
fn unused_cluster_inputs_do_not_hide_an_internal_or_missing_definition_failure() {
    let mut inner = Graph::default();
    let internal = failing_divide(&mut inner);
    let mut graph = Graph::default();
    let outer_root = failing_divide(&mut graph);
    let cluster = add(&mut graph, NodeKind::Cluster);
    graph.clusters.push(ClusterDef {
        id: 10,
        name: "内部の割り算".into(),
        graph: inner,
        inputs: Vec::new(),
        output: internal,
    });
    graph.node_mut(cluster).unwrap().cluster = Some(10);
    graph.connect(outer_root, cluster, "in7").unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let report = diagnose(&graph, &evaluation);
    assert_eq!(ids(&report.root_failures), vec![outer_root, cluster]);
    assert!(report.blocked_nodes.is_empty());
    assert!(report.root_failures[1].message.contains("内部の割り算"));

    graph.node_mut(cluster).unwrap().cluster = None;
    let missing = diagnose(&graph, &Evaluator::default().evaluate(&graph, &|| false));
    assert_eq!(ids(&missing.root_failures), vec![outer_root, cluster]);
    assert!(missing.blocked_nodes.is_empty());
}

#[test]
fn inferred_dependency_roots_preserve_an_independent_downstream_error_message() {
    let mut graph = Graph::default();
    let division = failing_divide(&mut graph);
    let extrusion = add(&mut graph, NodeKind::Extrude);
    graph.connect(division, extrusion, "height").unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    // The evaluator finds the missing curve before reading the failing height.
    let error = evaluation.outputs[&extrusion].as_ref().unwrap_err();
    assert!(error.contains("接続してください"));
    let report = diagnose(&graph, &evaluation);
    assert_eq!(report.causality, "dependency_inference");
    assert_eq!(report.blocked_nodes[0].upstream_roots, vec![division]);
    assert_eq!(&report.blocked_nodes[0].message, error);
}

#[test]
fn timing_ranking_includes_cache_hits_and_partial_work_without_claiming_cpu_percent() {
    let mut graph = Graph::default();
    let a = add(&mut graph, NodeKind::Number);
    let b = add(&mut graph, NodeKind::Number);
    let c = add(&mut graph, NodeKind::Number);
    let mut evaluation = Evaluation {
        elapsed_ms: 100.,
        cancelled: true,
        ..Default::default()
    };
    evaluation.outputs.insert(a, Ok(Tree::default()));
    evaluation.outputs.insert(b, Ok(Tree::default()));
    evaluation.node_timings.extend([
        (
            a,
            NodeTiming {
                elapsed_ms: 10.,
                cached: false,
            },
        ),
        (
            b,
            NodeTiming {
                elapsed_ms: 20.,
                cached: true,
            },
        ),
        (
            c,
            NodeTiming {
                elapsed_ms: 20.,
                cached: false,
            },
        ),
    ]);
    let report = diagnose(&graph, &evaluation);
    assert_eq!(report.summary.measured_node_ms, 50.);
    assert_eq!(
        report
            .hotspots
            .iter()
            .map(|item| item.node.id)
            .collect::<Vec<_>>(),
        vec![b, c, a]
    );
    assert_eq!(report.hotspots[0].share_percent, 40.);
    assert!(report.hotspots[0].cached && report.hotspots[0].completed);
    assert!(!report.hotspots[1].cached && !report.hotspots[1].completed);

    evaluation.elapsed_ms = f64::NAN;
    evaluation.node_timings.get_mut(&a).unwrap().elapsed_ms = f64::INFINITY;
    evaluation.node_timings.get_mut(&b).unwrap().elapsed_ms = -1.;
    let invalid_times = diagnose(&graph, &evaluation);
    assert_eq!(invalid_times.summary.elapsed_ms, 0.);
    assert_eq!(invalid_times.hotspots.len(), 1);
    let json = serde_json::to_value(&invalid_times).unwrap();
    assert!(json["summary"]["elapsed_ms"].is_number());
    assert!(json["summary"]["measured_node_ms"].is_number());
    assert!(json["hotspots"][0]["elapsed_ms"].is_number());
    assert!(json["hotspots"][0]["share_percent"].is_number());
}

#[test]
fn cached_errors_are_still_actionable_and_diagnosis_does_not_mutate_the_graph() {
    let mut graph = Graph::default();
    let root = failing_divide(&mut graph);
    let child = add(&mut graph, NodeKind::Add);
    graph.connect(root, child, "a").unwrap();
    let before = graph.clone();
    let mut evaluator = Evaluator::default();
    evaluator.evaluate(&graph, &|| false);
    let evaluation = evaluator.evaluate(&graph, &|| false);
    let report = diagnose(&graph, &evaluation);
    assert_eq!(report.summary.cached_count, 2);
    assert_eq!(report.summary.computed_count, 0);
    assert!(report.hotspots.iter().all(|item| item.cached));
    assert_eq!(report.root_failures[0].node.id, root);
    assert_eq!(report.root_failures[0].affected_nodes, vec![child]);
    assert_eq!(graph, before);
}

#[test]
fn wide_root_merge_and_long_chain_bound_previews_but_keep_exact_counts() {
    let mut graph = Graph::default();
    let roots: Vec<_> = (0..MAX_REPORTED_ROOTS + 8)
        .map(|_| failing_divide(&mut graph))
        .collect();
    let mut previous = roots[0];
    let mut blocked = Vec::new();
    for root in &roots[1..] {
        let id = add(&mut graph, NodeKind::Add);
        // Fixture writes avoid repeatedly validating an ever-growing graph; the
        // normal evaluator still validates and evaluates the entire final graph.
        graph
            .node_mut(id)
            .unwrap()
            .inputs
            .extend([("a".into(), previous), ("b".into(), *root)]);
        previous = id;
        blocked.push(id);
    }
    for _ in 0..MAX_REPORTED_AFFECTED + 12 {
        let id = add(&mut graph, NodeKind::Add);
        graph
            .node_mut(id)
            .unwrap()
            .inputs
            .insert("a".into(), previous);
        previous = id;
        blocked.push(id);
    }
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let report = diagnose(&graph, &evaluation);
    assert_eq!(report.summary.root_failure_count, roots.len());
    assert_eq!(report.summary.blocked_count, blocked.len());
    let first = &report.root_failures[0];
    assert_eq!(first.affected_count, blocked.len());
    assert_eq!(first.affected_nodes, blocked[..MAX_REPORTED_AFFECTED]);
    assert!(first.affected_nodes_truncated);
    let last = report.blocked_nodes.last().unwrap();
    assert_eq!(last.upstream_root_count, roots.len());
    assert_eq!(last.upstream_roots, roots[..MAX_REPORTED_ROOTS]);
    assert!(last.upstream_roots_truncated);
    assert!(
        report
            .blocked_nodes
            .iter()
            .all(|node| node.affected_nodes.is_empty())
    );
    assert!(
        report
            .root_failures
            .iter()
            .all(|node| node.affected_nodes.len() <= MAX_REPORTED_AFFECTED)
    );
    assert!(
        report
            .blocked_nodes
            .iter()
            .all(|node| node.upstream_roots.len() <= MAX_REPORTED_ROOTS)
    );
    let stable = serde_json::to_value(&report).unwrap();
    graph.nodes.reverse();
    assert_eq!(
        stable,
        serde_json::to_value(diagnose(&graph, &evaluation)).unwrap()
    );
}
