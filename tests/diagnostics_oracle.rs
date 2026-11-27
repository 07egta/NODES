//! Independent reachability oracle for diagnostics; no bitsets or shared root sets.
//! Real evaluator outcomes decide which edges can carry a failure. A simple BFS
//! from each earliest failed node supplies expected descendants and causes.
use nodes::diagnostics::{MAX_REPORTED_AFFECTED, MAX_REPORTED_ROOTS, diagnose};
use nodes::engine::Evaluator;
use nodes::model::{Graph, NodeId, NodeKind};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

struct FixedRandom(u64);

impl FixedRandom {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }

    fn index(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

/// Return a DAG whose node IDs and document order both differ from its topology.
/// The prefix guarantees a failed -> disabled -> empty-success chain, plus a
/// ListLength that turns empty data into a successful numeric value. A later
/// divide-by-zero setting must not turn empty data into error.
fn fixture(seed: u64, count: usize) -> (Graph, [NodeId; 5]) {
    let mut random = FixedRandom(seed);
    let mut graph = Graph::default();
    for index in 0..count {
        let kind = match index {
            0 | 3 => NodeKind::Divide,
            1 | 2 => NodeKind::Add,
            4 => NodeKind::ListLength,
            i if i.is_multiple_of(13) => NodeKind::Divide,
            _ => match random.index(4) {
                0 => NodeKind::Number,
                1 => NodeKind::Divide,
                2 => NodeKind::Multiply,
                _ => NodeKind::Add,
            },
        };
        let id = graph.add(kind, [0., 0.]).unwrap();
        let node = graph.node_mut(id).unwrap();
        if kind == NodeKind::Divide {
            node.params.insert("b".into(), 0.);
        }
        if index > 4 && !index.is_multiple_of(13) {
            for input in kind.spec().inputs {
                if random.index(5) < 3 {
                    node.inputs
                        .insert(input.key.into(), random.index(index) as NodeId);
                }
            }
            node.off = index.is_multiple_of(17);
        }
    }
    graph.nodes[1].inputs.insert("a".into(), 0);
    graph.nodes[1].off = true;
    graph.nodes[2].inputs.insert("a".into(), 1);
    graph.nodes[3].inputs.insert("a".into(), 2);
    graph.nodes[4].inputs.insert("list".into(), 2);

    let mut ids: Vec<_> = (0..count as NodeId).collect();
    for index in (1..count).rev() {
        let other = random.index(index + 1);
        ids.swap(index, other);
    }
    let landmarks = [ids[0], ids[1], ids[2], ids[3], ids[4]];
    for node in &mut graph.nodes {
        node.id = ids[node.id as usize];
        for source in node.inputs.values_mut() {
            *source = ids[*source as usize];
        }
    }
    graph.nodes.rotate_left(count / 3);
    graph.nodes.reverse();
    graph.validate().unwrap();
    (graph, landmarks)
}

#[test]
fn diagnostic_cause_sets_match_independent_breadth_first_reachability() {
    for (seed, count) in [(7, 96), (41, 127), (2026, 173), (99_991, 211)] {
        let (mut graph, [failure, disabled, empty, empty_division, counted]) = fixture(seed, count);
        let evaluation = Evaluator::default().evaluate(&graph, &|| false);
        assert!(!evaluation.cancelled, "seed {seed}");
        assert!(evaluation.outputs[&failure].is_err());
        for id in [disabled, empty, empty_division] {
            assert_eq!(evaluation.outputs[&id].as_ref().unwrap().item_count(), 0);
        }
        assert_eq!(
            evaluation.outputs[&counted].as_ref().unwrap().item_count(),
            1
        );

        let failed: BTreeSet<_> = graph
            .nodes
            .iter()
            .filter(|node| !node.off && evaluation.outputs[&node.id].is_err())
            .map(|node| node.id)
            .collect();
        let roots: BTreeSet<_> = graph
            .nodes
            .iter()
            .filter(|node| {
                failed.contains(&node.id)
                    && !node.inputs.values().any(|source| failed.contains(source))
            })
            .map(|node| node.id)
            .collect();
        let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        for node in &graph.nodes {
            for source in node.inputs.values() {
                children.entry(*source).or_default().push(node.id);
            }
        }
        let mut descendants: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        let mut causes: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for root in &roots {
            let mut seen = BTreeSet::from([*root]);
            let mut queue = VecDeque::from([*root]);
            while let Some(parent) = queue.pop_front() {
                for child in children.get(&parent).into_iter().flatten() {
                    if failed.contains(child) && seen.insert(*child) {
                        queue.push_back(*child);
                        causes.entry(*child).or_default().insert(*root);
                    }
                }
            }
            seen.remove(root);
            descendants.insert(*root, seen);
        }

        let before = graph.clone();
        let report = diagnose(&graph, &evaluation);
        assert_eq!(graph, before);
        assert!(report.graph_error.is_none() && report.summary.complete);
        assert_eq!(report.summary.failed_count, failed.len(), "seed {seed}");
        assert_eq!(report.summary.root_failure_count, roots.len());
        assert_eq!(report.summary.blocked_count, failed.len() - roots.len());
        assert_eq!(
            report
                .root_failures
                .iter()
                .map(|f| f.node.id)
                .collect::<Vec<_>>(),
            roots.iter().copied().collect::<Vec<_>>()
        );
        assert_eq!(
            report
                .blocked_nodes
                .iter()
                .map(|f| f.node.id)
                .collect::<Vec<_>>(),
            failed.difference(&roots).copied().collect::<Vec<_>>()
        );
        for root in &report.root_failures {
            let expected = &descendants[&root.node.id];
            assert_eq!(root.affected_count, expected.len(), "seed {seed}");
            assert_eq!(
                root.affected_nodes,
                expected
                    .iter()
                    .copied()
                    .take(MAX_REPORTED_AFFECTED)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                root.affected_nodes_truncated,
                expected.len() > MAX_REPORTED_AFFECTED
            );
            assert!(root.upstream_roots.is_empty());
        }
        for blocked in &report.blocked_nodes {
            let expected = &causes[&blocked.node.id];
            assert_eq!(blocked.upstream_root_count, expected.len(), "seed {seed}");
            assert_eq!(
                blocked.upstream_roots,
                expected
                    .iter()
                    .copied()
                    .take(MAX_REPORTED_ROOTS)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                blocked.upstream_roots_truncated,
                expected.len() > MAX_REPORTED_ROOTS
            );
            assert_eq!(
                &blocked.message,
                evaluation.outputs[&blocked.node.id].as_ref().unwrap_err()
            );
        }
        let stable = serde_json::to_value(&report).unwrap();
        graph.nodes.rotate_left(17);
        assert_eq!(
            stable,
            serde_json::to_value(diagnose(&graph, &evaluation)).unwrap()
        );
    }
}
