//! Public diagnostics semantics for high-ID roots and successful failure cuts.
//! Expected relationships come from the fixtures, not internal bitset indices.
use morphlet::diagnostics::{MAX_REPORTED_AFFECTED, diagnose};
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, NodeId, NodeKind};

fn add(graph: &mut Graph, kind: NodeKind, inputs: &[(&str, NodeId)]) -> NodeId {
    let id = graph.add(kind, [0.; 2]).unwrap();
    let node = graph.nodes.last_mut().unwrap();
    if kind == NodeKind::Divide {
        node.params.insert("b".into(), 0.);
    }
    node.inputs = inputs
        .iter()
        .map(|(key, source)| ((*key).into(), *source))
        .collect();
    id
}

fn remap(graph: &mut Graph, ids: &[NodeId]) {
    assert_eq!(ids.len(), graph.nodes.len());
    for node in &mut graph.nodes {
        node.id = ids[node.id as usize];
        for source in node.inputs.values_mut() {
            *source = ids[*source as usize];
        }
    }
    graph.next_id = ids.iter().max().copied().unwrap() + 1;
    graph.nodes.reverse();
    graph.validate().unwrap();
}

#[test]
fn high_id_roots_keep_all_shared_descendants_and_sorted_public_ids() {
    const JOINS: usize = 600;
    let mut graph = Graph::default();
    let a = add(&mut graph, NodeKind::Divide, &[]);
    let b = add(&mut graph, NodeKind::Divide, &[]);
    for _ in 0..JOINS {
        add(&mut graph, NodeKind::Add, &[("a", a), ("b", b)]);
    }
    // Every root sorts after every descendant, with a large unused numeric gap.
    // Independent joins share the same two logical causes.
    let mut ids = vec![20_000, 10_000];
    ids.extend(0..JOINS as NodeId);
    remap(&mut graph, &ids);
    let before = graph.clone();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let report = diagnose(&graph, &evaluation);
    assert_eq!(graph, before);
    assert!(report.graph_error.is_none() && report.summary.complete);
    assert_eq!(report.summary.failed_count, JOINS + 2);
    assert_eq!(report.summary.root_failure_count, 2);
    assert_eq!(report.summary.blocked_count, JOINS);
    assert_eq!(
        report
            .root_failures
            .iter()
            .map(|failure| failure.node.id)
            .collect::<Vec<_>>(),
        vec![10_000, 20_000]
    );
    let expected_preview: Vec<_> = (0..MAX_REPORTED_AFFECTED as NodeId).collect();
    for root in &report.root_failures {
        assert_eq!(root.affected_count, JOINS);
        assert_eq!(root.affected_nodes, expected_preview);
        assert!(root.affected_nodes_truncated);
        assert!(root.upstream_roots.is_empty());
    }
    for (id, blocked) in report.blocked_nodes.iter().enumerate() {
        assert_eq!(blocked.node.id, id as NodeId);
        assert_eq!(blocked.upstream_roots, vec![10_000, 20_000]);
        assert_eq!(blocked.upstream_root_count, 2);
        assert!(!blocked.upstream_roots_truncated);
    }
    let stable = serde_json::to_value(report).unwrap();
    graph.nodes.rotate_left(43);
    assert_eq!(
        stable,
        serde_json::to_value(diagnose(&graph, &evaluation)).unwrap()
    );
}

#[test]
fn local_failure_after_disabled_and_successful_nodes_is_a_new_root() {
    let mut graph = Graph::default();
    let source = add(&mut graph, NodeKind::Divide, &[]);
    let disabled = add(&mut graph, NodeKind::Add, &[("a", source)]);
    graph.nodes.last_mut().unwrap().off = true;
    let empty = add(&mut graph, NodeKind::Add, &[("a", disabled)]);
    let counted = add(&mut graph, NodeKind::ListLength, &[("list", empty)]);
    let local_failure = add(&mut graph, NodeKind::Divide, &[("a", counted)]);
    let independent = add(&mut graph, NodeKind::Divide, &[]);
    add(
        &mut graph,
        NodeKind::Add,
        &[("a", source), ("b", independent)],
    );
    add(
        &mut graph,
        NodeKind::Add,
        &[("a", local_failure), ("b", independent)],
    );
    // Root 1000 is discovered after root 9000, yet must sort before it in every
    // public cause list. The enabled empty node and ListLength are successful.
    remap(&mut graph, &[9000, 2, 3, 4, 1000, 5000, 6000, 7000]);
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    for id in [2, 3] {
        assert_eq!(evaluation.outputs[&id].as_ref().unwrap().item_count(), 0);
    }
    assert_eq!(evaluation.outputs[&4].as_ref().unwrap().item_count(), 1);
    assert!(evaluation.outputs[&1000].is_err());
    let report = diagnose(&graph, &evaluation);
    assert!(report.graph_error.is_none() && report.summary.complete);
    assert_eq!(report.summary.disabled_count, 1);
    assert_eq!(report.summary.successful_count, 2);
    assert_eq!(report.summary.failed_count, 5);
    assert_eq!(report.summary.root_failure_count, 3);
    assert_eq!(report.summary.blocked_count, 2);
    for (root, (id, affected)) in report.root_failures.iter().zip([
        (1000, vec![7000]),
        (5000, vec![6000, 7000]),
        (9000, vec![6000]),
    ]) {
        assert_eq!(root.node.id, id);
        assert!(root.upstream_roots.is_empty());
        assert_eq!(root.affected_count, affected.len());
        assert_eq!(root.affected_nodes, affected);
        assert!(!root.affected_nodes_truncated);
    }
    for (blocked, (id, roots)) in report
        .blocked_nodes
        .iter()
        .zip([(6000, vec![5000, 9000]), (7000, vec![1000, 5000])])
    {
        assert_eq!(blocked.node.id, id);
        assert_eq!(blocked.upstream_root_count, roots.len());
        assert_eq!(blocked.upstream_roots, roots);
        assert!(!blocked.upstream_roots_truncated);
    }
}
