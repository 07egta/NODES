//! Read-only diagnostics for a graph and its evaluation snapshot.
//! Does not evaluate nodes, mutate a project, or assume that an upstream error
//! is the sole cause of a downstream error.
use crate::engine::Evaluation;
use crate::model::{Graph, NodeId, NodeKind};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Ready,
    Empty,
    Disabled,
    Error,
    NotEvaluated,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NodeDiagnostic {
    pub node_id: NodeId,
    pub kind: NodeKind,
    pub status: NodeStatus,
    pub direct_failed_inputs: Vec<NodeId>,
    pub direct_disabled_inputs: Vec<NodeId>,
    pub missing_required_inputs: Vec<&'static str>,
    pub error: Option<String>,
    pub elapsed_ms: Option<f64>,
    pub cached: Option<bool>,
    pub branches: Option<usize>,
    pub items: Option<usize>,
    pub downstream_nodes: usize,
    /// Longest upstream chain ending at this node, including this node.
    pub dependency_depth: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphDiagnostic {
    pub nodes: Vec<NodeDiagnostic>,
    pub wire_count: usize,
    pub dependency_depth: usize,
    pub error_nodes: usize,
    /// Failed nodes without a directly failed input. Start here, but a
    /// downstream node can also have its own independent error.
    pub first_error_candidates: Vec<NodeId>,
    /// Heaviest dependency path among nodes with recorded timings; not the
    /// duration of the whole evaluation or a prediction of parallel speedup.
    pub measured_dependency_path: Vec<NodeId>,
    pub measured_dependency_path_ms: f64,
    pub cancelled: bool,
}

/// Require a valid graph. Evaluation must correspond to the same snapshot.
/// Uses one topological pass; never enumerates all paths through a DAG.
pub fn diagnose(graph: &Graph, evaluation: &Evaluation) -> Result<GraphDiagnostic, String> {
    let order = graph.topological_nodes()?;
    let disabled: BTreeSet<_> = order
        .iter()
        .filter(|node| node.off)
        .map(|node| node.id)
        .collect();
    let mut direct_children = BTreeMap::<NodeId, usize>::new();
    let mut wire_count = 0usize;
    for node in &order {
        wire_count += node.inputs.len();
        for parent in node.inputs.values().copied().collect::<BTreeSet<_>>() {
            *direct_children.entry(parent).or_default() += 1;
        }
    }

    let mut depth = BTreeMap::<NodeId, usize>::new();
    let mut weighted = BTreeMap::<NodeId, (f64, Option<NodeId>)>::new();
    let mut candidates = Vec::new();
    let mut nodes = Vec::with_capacity(order.len());
    let mut error_nodes = 0;
    let mut max_depth = 0;
    let mut endpoint: Option<(f64, NodeId)> = None;

    for node in order {
        let parents: BTreeSet<_> = node.inputs.values().copied().collect();
        let current_depth = 1 + parents.iter().map(|id| depth[id]).max().unwrap_or(0);
        depth.insert(node.id, current_depth);
        max_depth = max_depth.max(current_depth);

        let result = evaluation.outputs.get(&node.id);
        let status = match result {
            Some(Err(_)) => NodeStatus::Error,
            Some(Ok(_)) if node.off => NodeStatus::Disabled,
            Some(Ok(tree)) if tree.item_count() == 0 => NodeStatus::Empty,
            Some(Ok(_)) => NodeStatus::Ready,
            None => NodeStatus::NotEvaluated,
        };
        let failed_inputs: Vec<_> = parents
            .iter()
            .copied()
            .filter(|id| matches!(evaluation.outputs.get(id), Some(Err(_))))
            .collect();
        let disabled_inputs = parents
            .iter()
            .copied()
            .filter(|id| {
                matches!(
                    evaluation.outputs.get(id),
                    Some(Ok(tree)) if tree.item_count() == 0
                ) && disabled.contains(id)
            })
            .collect();
        if status == NodeStatus::Error {
            error_nodes += 1;
            if failed_inputs.is_empty() {
                candidates.push(node.id);
            }
        }

        let timing = evaluation.node_timings.get(&node.id);
        let elapsed = timing
            .map(|t| t.elapsed_ms)
            .filter(|t| t.is_finite() && *t >= 0.0);
        if let Some(ms) = elapsed {
            let mut best = (0.0, None);
            for parent in &parents {
                if let Some(&(cost, _)) = weighted.get(parent)
                    && (cost > best.0 || best.1.is_none())
                {
                    best = (cost, Some(*parent));
                }
            }
            let cost = best.0 + ms;
            if cost.is_finite() {
                weighted.insert(node.id, (cost, best.1));
                if endpoint.is_none_or(|(old, _)| cost > old) {
                    endpoint = Some((cost, node.id));
                }
            }
        }
        let tree = result.and_then(|r| r.as_ref().ok());
        // These are declared required ports, not a promise that every special
        // evaluator uses the ordinary item-access path.
        let missing_required_inputs = if node.off
            || matches!(node.kind, NodeKind::Script | NodeKind::Cluster)
        {
            Vec::new()
        } else {
            node.kind
                .spec()
                .inputs
                .iter()
                .filter(|spec| spec.default.is_none() && !node.inputs.contains_key(spec.key))
                .map(|spec| spec.key)
                .collect()
        };
        nodes.push(NodeDiagnostic {
            node_id: node.id,
            kind: node.kind,
            status,
            direct_failed_inputs: failed_inputs,
            direct_disabled_inputs: disabled_inputs,
            missing_required_inputs,
            error: result
                .and_then(|r| r.as_ref().err())
                .map(|e| e.chars().take(2048).collect()),
            elapsed_ms: elapsed,
            cached: timing.map(|t| t.cached),
            branches: tree.map(|t| t.branches.len()),
            items: tree.map(|t| t.item_count()),
            downstream_nodes: direct_children.get(&node.id).copied().unwrap_or(0),
            dependency_depth: current_depth,
        });
    }

    let mut path = Vec::new();
    let (path_ms, mut previous) = endpoint.map_or((0.0, None), |(ms, id)| (ms, Some(id)));
    while let Some(id) = previous {
        path.push(id);
        previous = weighted[&id].1;
    }
    path.reverse();
    Ok(GraphDiagnostic {
        nodes,
        wire_count,
        dependency_depth: max_depth,
        error_nodes,
        first_error_candidates: candidates,
        measured_dependency_path: path,
        measured_dependency_path_ms: path_ms,
        cancelled: evaluation.cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Evaluator, NodeTiming};

    fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
        graph.add(kind, [0.0, 0.0]).unwrap()
    }

    #[test]
    fn duplicate_wires_count_once_for_fanout_and_critical_path() {
        let mut g = Graph::default();
        let a = add(&mut g, NodeKind::Number);
        let b = add(&mut g, NodeKind::Add);
        let c = add(&mut g, NodeKind::Multiply);
        g.connect(a, b, "a").unwrap();
        g.connect(a, b, "b").unwrap();
        g.connect(b, c, "a").unwrap();
        let mut e = Evaluator::default().evaluate(&g, &|| false);
        for (id, ms) in [(a, 1.0), (b, 3.0), (c, 2.0)] {
            e.node_timings.insert(
                id,
                NodeTiming {
                    elapsed_ms: ms,
                    cached: false,
                },
            );
        }
        let d = diagnose(&g, &e).unwrap();
        assert_eq!(d.wire_count, 3);
        assert_eq!(d.dependency_depth, 3);
        assert_eq!(d.nodes[0].downstream_nodes, 1);
        assert_eq!(d.measured_dependency_path, vec![a, b, c]);
        assert_eq!(d.measured_dependency_path_ms, 6.0);
    }

    #[test]
    fn failure_candidates_and_disabled_inputs_are_distinct() {
        let mut g = Graph::default();
        let a = add(&mut g, NodeKind::Extrude);
        let b = add(&mut g, NodeKind::Output);
        let c = add(&mut g, NodeKind::Number);
        let d = add(&mut g, NodeKind::Add);
        g.connect(a, b, "geometry").unwrap();
        g.node_mut(c).unwrap().off = true;
        g.connect(c, d, "a").unwrap();
        let e = Evaluator::default().evaluate(&g, &|| false);
        let report = diagnose(&g, &e).unwrap();
        assert_eq!(report.first_error_candidates, vec![a]);
        assert_eq!(report.error_nodes, 2);
        let find = |id| report.nodes.iter().find(|n| n.node_id == id).unwrap();
        assert_eq!(find(b).direct_failed_inputs, vec![a]);
        assert_eq!(find(c).status, NodeStatus::Disabled);
        assert_eq!(find(d).direct_disabled_inputs, vec![c]);
        assert_eq!(find(d).status, NodeStatus::Empty);
        assert!(!find(a).missing_required_inputs.is_empty());
    }

    #[test]
    fn cancelled_or_partial_snapshot_does_not_invent_errors() {
        let mut g = Graph::default();
        add(&mut g, NodeKind::Number);
        let e = Evaluation {
            cancelled: true,
            ..Evaluation::default()
        };
        let report = diagnose(&g, &e).unwrap();
        assert!(report.cancelled);
        assert_eq!(report.nodes[0].status, NodeStatus::NotEvaluated);
        assert_eq!(report.error_nodes, 0);
        assert!(report.measured_dependency_path.is_empty());
    }
}
