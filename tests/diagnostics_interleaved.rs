//! Analytic graph whose small affected IDs arrive in distinct root-set groups.
//! No test depends on the private grouping or preview accumulator representation.
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

#[test]
fn interleaved_ids_keep_exact_counts_and_the_smallest_affected_nodes() {
    const CORE_ROOTS: usize = 32;
    let limit = MAX_REPORTED_AFFECTED;
    let shared_count = limit + 17;
    let additional_count = limit - 1;
    let mut graph = Graph::default();
    let core_roots: Vec<_> = (0..CORE_ROOTS)
        .map(|_| add(&mut graph, NodeKind::Divide, &[]))
        .collect();
    let mut level = core_roots.clone();
    while level.len() > 1 {
        level = level
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| add(&mut graph, NodeKind::Add, &[("a", pair[0]), ("b", pair[1])]))
            .collect();
    }
    let core_tip = level[0];

    // The final balanced join and its long tail affect every core root. Its
    // smallest ID will be zero; all other tail IDs will be above the low joins.
    let mut tail = core_tip;
    let mut shared_nodes = vec![core_tip];
    for _ in 1..shared_count {
        tail = add(&mut graph, NodeKind::Add, &[("a", tail)]);
        shared_nodes.push(tail);
    }

    // Each extra root has precisely one descendant. Every low join additionally
    // depends on every core root, but does not feed the other low joins or tail.
    let mut extra_roots = Vec::new();
    let mut low_joins = Vec::new();
    for _ in 0..additional_count {
        let root = add(&mut graph, NodeKind::Divide, &[]);
        extra_roots.push(root);
        low_joins.push(add(
            &mut graph,
            NodeKind::Add,
            &[("a", core_tip), ("b", root)],
        ));
    }

    // Assign the final join ID 0 and the additional joins IDs 1..K. All other
    // IDs are larger, independently of topology and document storage order.
    let mut ids = vec![NodeId::MAX; graph.nodes.len()];
    ids[core_tip as usize] = 0;
    for (index, &id) in low_joins.iter().enumerate() {
        ids[id as usize] = (index + 1) as NodeId;
    }
    let mut next = limit as NodeId;
    for id in &mut ids {
        if *id == NodeId::MAX {
            *id = next;
            next += 1;
        }
    }
    assert_eq!(next, graph.next_id);
    for node in &mut graph.nodes {
        node.id = ids[node.id as usize];
        for source in node.inputs.values_mut() {
            *source = ids[*source as usize];
        }
    }
    graph.nodes.rotate_left(17);
    graph.nodes.reverse();
    graph.validate().unwrap();
    let before = graph.clone();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let report = diagnose(&graph, &evaluation);
    assert_eq!(graph, before);
    assert!(report.graph_error.is_none() && report.summary.complete);
    assert_eq!(report.summary.failed_count, graph.nodes.len());
    assert_eq!(
        report.summary.root_failure_count,
        CORE_ROOTS + additional_count
    );

    let core_ids: Vec<_> = core_roots.iter().map(|id| ids[*id as usize]).collect();
    let expected_preview: Vec<_> = (0..limit as NodeId).collect();
    // Each core root reaches log2(R) balanced joins, the shared tail, and every
    // low join. shared_count already includes the final balanced join.
    let expected_count = shared_count + additional_count + CORE_ROOTS.ilog2() as usize - 1;
    for root in &core_ids {
        let failure = report
            .root_failures
            .iter()
            .find(|failure| failure.node.id == *root)
            .unwrap();
        assert_eq!(failure.affected_count, expected_count);
        assert_eq!(failure.affected_nodes, expected_preview);
        assert!(failure.affected_nodes_truncated);
    }
    for (index, root) in extra_roots.iter().enumerate() {
        let failure = report
            .root_failures
            .iter()
            .find(|failure| failure.node.id == ids[*root as usize])
            .unwrap();
        assert_eq!(failure.affected_count, 1);
        assert_eq!(failure.affected_nodes, vec![(index + 1) as NodeId]);
        assert!(!failure.affected_nodes_truncated);

        let joined = report
            .blocked_nodes
            .iter()
            .find(|failure| failure.node.id == (index + 1) as NodeId)
            .unwrap();
        let mut expected_roots = core_ids.clone();
        expected_roots.push(ids[*root as usize]);
        expected_roots.sort_unstable();
        assert_eq!(joined.upstream_roots, expected_roots);
        assert_eq!(joined.upstream_root_count, CORE_ROOTS + 1);
        assert!(!joined.upstream_roots_truncated);
    }
    for id in shared_nodes {
        let shared = report
            .blocked_nodes
            .iter()
            .find(|failure| failure.node.id == ids[id as usize])
            .unwrap();
        assert_eq!(shared.upstream_root_count, CORE_ROOTS);
        assert_eq!(shared.upstream_roots, core_ids);
    }

    // Changing storage order cannot change which affected IDs survive the cap.
    let stable = serde_json::to_value(report).unwrap();
    graph.nodes.rotate_left(29);
    assert_eq!(
        stable,
        serde_json::to_value(diagnose(&graph, &evaluation)).unwrap()
    );
}
