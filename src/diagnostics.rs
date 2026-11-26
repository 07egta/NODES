//! Bounded diagnostics for a graph and its corresponding evaluation snapshot.
//!
//! Error causes are inferred from failed, consumed dependencies, never from localized
//! messages. This is a navigation aid, not a proof that a downstream node has no
//! independent problem: evaluators can stop at the first invalid input. A successful
//! or disabled node breaks the failure chain. Cluster failures stop at the outer node;
//! the evaluator's original message retains any available internal context.
//!
//! Callers must pair the evaluation with the graph revision that produced it. This
//! module does not evaluate nodes, touch files, modify the graph or invalidate caches.
use crate::engine::Evaluation;
use crate::model::{Graph, Node, NodeId, NodeKind};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
/// Exact counts are retained when the display/API relationship lists are shortened.
pub const MAX_REPORTED_ROOTS: usize = 64;
pub const MAX_REPORTED_AFFECTED: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticReport {
    pub schema_version: u32,
    pub summary: DiagnosticSummary,
    /// The evaluator's structural graph validation failed. Such errors are not
    /// attributed to arbitrary nodes even if Evaluation repeats them on every node.
    pub graph_error: Option<String>,
    pub causality: &'static str,
    pub root_failures: Vec<NodeFailure>,
    pub blocked_nodes: Vec<NodeFailure>,
    pub disabled_nodes: Vec<NodeRef>,
    pub unevaluated_nodes: Vec<NodeRef>,
    /// Enabled node visits, including partial visits interrupted by cancellation.
    /// Descending measured time, with node ID as a deterministic tie breaker.
    pub hotspots: Vec<NodeHotspot>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DiagnosticSummary {
    pub node_count: usize,
    /// Number of connected input sockets (two inputs from one source count twice).
    pub connection_count: usize,
    /// Nodes with an output entry, including visited disabled nodes.
    pub completed_count: usize,
    /// Successful enabled nodes. Empty trees are valid successful results.
    pub successful_count: usize,
    pub failed_count: usize,
    pub root_failure_count: usize,
    pub blocked_count: usize,
    pub disabled_count: usize,
    /// Enabled nodes without an output entry; never interpreted as successes.
    pub unevaluated_count: usize,
    pub computed_count: usize,
    pub cached_count: usize,
    pub cancelled: bool,
    /// All nodes have outputs, structural validation succeeded and no cancellation
    /// occurred. A complete evaluation may still contain failures.
    pub complete: bool,
    pub elapsed_ms: f64,
    /// Sum of valid measured enabled-node wall times, including partial visits.
    /// This excludes graph validation and disabled-node bookkeeping.
    pub measured_node_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NodeRef {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
}
impl From<&Node> for NodeRef {
    fn from(node: &Node) -> Self {
        Self {
            id: node.id,
            name: node.name.clone(),
            kind: node.kind,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct NodeFailure {
    pub node: NodeRef,
    /// Original evaluator message, preserved verbatim.
    pub message: String,
    /// Failed upstream roots, sorted by ID, capped at MAX_REPORTED_ROOTS.
    /// Empty for a root failure itself.
    pub upstream_roots: Vec<NodeId>,
    pub upstream_root_count: usize,
    pub upstream_roots_truncated: bool,
    /// Failed descendants of this root only, sorted by ID. For blocked nodes this
    /// remains empty: the report does not materialize all-pairs descendant lists.
    pub affected_nodes: Vec<NodeId>,
    pub affected_count: usize,
    pub affected_nodes_truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NodeHotspot {
    pub node: NodeRef,
    /// Inclusive top-level node wall time, including input/cache preparation,
    /// retrieval or computation, checks and output insertion; clusters are inclusive.
    pub elapsed_ms: f64,
    pub cached: bool,
    /// False for a visited node interrupted before publishing an output.
    pub completed: bool,
    /// Share of summary.measured_node_ms, not CPU utilization or a critical path.
    pub share_percent: f64,
}

// Single roots require no allocation. Multi-root sets share their immutable words
// along chains/fanout; joins only copy if they introduce a new root. The largest
// possible dense matrix is bounded by the graph's existing MAX_NODES limit.
#[derive(Clone)]
enum Roots {
    One(usize),
    Many(Arc<[u64]>),
}
impl Roots {
    fn contains(&self, index: usize) -> bool {
        match self {
            Self::One(i) => *i == index,
            Self::Many(words) => words
                .get(index / 64)
                .is_some_and(|word| word & (1 << (index % 64)) != 0),
        }
    }

    fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (_, Self::One(index)) if self.contains(*index) => self.clone(),
            (Self::One(_), Self::Many(_)) => other.union(self),
            (Self::Many(a), Self::Many(b))
                if Arc::ptr_eq(a, b)
                    || b.iter()
                        .enumerate()
                        .all(|(i, word)| word & !a.get(i).copied().unwrap_or(0) == 0) =>
            {
                self.clone()
            }
            _ => {
                let mut words = match self {
                    Self::One(index) => {
                        let mut words = vec![0; index / 64 + 1];
                        words[index / 64] |= 1 << (index % 64);
                        words
                    }
                    Self::Many(words) => words.to_vec(),
                };
                match other {
                    Self::One(index) => {
                        words.resize(words.len().max(index / 64 + 1), 0);
                        words[index / 64] |= 1 << (index % 64);
                    }
                    Self::Many(other) => {
                        words.resize(words.len().max(other.len()), 0);
                        for (to, from) in words.iter_mut().zip(other.iter()) {
                            *to |= from;
                        }
                    }
                }
                Self::Many(words.into())
            }
        }
    }

    fn for_each(&self, mut f: impl FnMut(usize)) {
        match self {
            Self::One(index) => f(*index),
            Self::Many(words) => {
                for (word_index, word) in words.iter().enumerate() {
                    let mut left = *word;
                    while left != 0 {
                        f(word_index * 64 + left.trailing_zeros() as usize);
                        left &= left - 1;
                    }
                }
            }
        }
    }

    // Pointer identity only groups shared internal allocations for work reduction.
    // It never controls report ordering or serialized values.
    fn key(&self) -> (bool, usize) {
        match self {
            Self::One(index) => (false, *index),
            Self::Many(words) => (true, words.as_ptr() as usize),
        }
    }
}

struct FailureRoots {
    roots: Roots,
    is_root: bool,
}
struct AffectedGroup {
    roots: Roots,
    root_ids: Vec<NodeId>,
    root_count: usize,
    count: usize,
    preview: Vec<NodeId>,
}

/// Analyze one evaluation without recomputing it. Lists are bounded independently
/// of exact counts. Output order depends on IDs/timings, never graph storage order.
pub fn diagnose(graph: &Graph, evaluation: &Evaluation) -> DiagnosticReport {
    let mut nodes: Vec<_> = graph.nodes.iter().collect();
    nodes.sort_unstable_by_key(|node| node.id);
    let mut report = DiagnosticReport {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        summary: DiagnosticSummary {
            node_count: nodes.len(),
            connection_count: nodes.iter().map(|node| node.inputs.len()).sum(),
            computed_count: evaluation.computed,
            cached_count: evaluation.cached,
            cancelled: evaluation.cancelled,
            elapsed_ms: valid_time(evaluation.elapsed_ms).unwrap_or(0.),
            ..Default::default()
        },
        graph_error: None,
        causality: "dependency_inference",
        root_failures: Vec::new(),
        blocked_nodes: Vec::new(),
        disabled_nodes: Vec::new(),
        unevaluated_nodes: Vec::new(),
        hotspots: Vec::new(),
    };
    for node in &nodes {
        let result = evaluation.outputs.get(&node.id);
        report.summary.completed_count += usize::from(result.is_some());
        if node.off {
            report.disabled_nodes.push(NodeRef::from(*node));
            continue;
        }
        match result {
            Some(Ok(_)) => report.summary.successful_count += 1,
            Some(Err(_)) => report.summary.failed_count += 1,
            None => report.unevaluated_nodes.push(NodeRef::from(*node)),
        }
        if let Some(timing) = evaluation.node_timings.get(&node.id)
            && let Some(elapsed_ms) = valid_time(timing.elapsed_ms)
        {
            report.summary.measured_node_ms += elapsed_ms;
            report.hotspots.push(NodeHotspot {
                node: NodeRef::from(*node),
                elapsed_ms,
                cached: timing.cached,
                completed: result.is_some(),
                share_percent: 0.,
            });
        }
    }
    report.summary.disabled_count = report.disabled_nodes.len();
    report.summary.unevaluated_count = report.unevaluated_nodes.len();
    // Real evaluator times cannot overflow this sum; keep the public report JSON
    // finite even for synthetic/stale callers constructing Evaluation manually.
    report.summary.measured_node_ms =
        valid_time(report.summary.measured_node_ms).unwrap_or(f64::MAX);
    if report.summary.measured_node_ms > 0. {
        for hotspot in &mut report.hotspots {
            hotspot.share_percent = hotspot.elapsed_ms / report.summary.measured_node_ms * 100.;
        }
    }
    report.hotspots.sort_unstable_by(|a, b| {
        b.elapsed_ms
            .total_cmp(&a.elapsed_ms)
            .then_with(|| a.node.id.cmp(&b.node.id))
    });
    let order = match graph.topological_nodes() {
        Ok(order) => order,
        Err(error) => {
            report.graph_error = Some(error);
            return report;
        }
    };
    report.summary.complete =
        !evaluation.cancelled && report.summary.completed_count == report.summary.node_count;
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id, index))
        .collect();
    let mut failures: BTreeMap<NodeId, FailureRoots> = BTreeMap::new();
    for node in order {
        if node.off || !evaluation.outputs.get(&node.id).is_some_and(Result::is_err) {
            continue;
        }
        let mut upstream: Option<Roots> = None;
        // Cluster's static schema includes spare sockets that its current definition
        // does not read. Inferring causes through those connections would be false.
        for spec in node
            .kind
            .spec()
            .inputs
            .iter()
            .take(crate::cluster::real_input_count(graph, node))
        {
            if let Some(source) = node.inputs.get(spec.key)
                && let Some(failure) = failures.get(source)
            {
                upstream = Some(match upstream {
                    None => failure.roots.clone(),
                    Some(roots) => roots.union(&failure.roots),
                });
            }
        }
        failures.insert(
            node.id,
            FailureRoots {
                is_root: upstream.is_none(),
                roots: upstream.unwrap_or_else(|| Roots::One(indices[&node.id])),
            },
        );
    }
    let mut root_positions = BTreeMap::new();
    let mut groups: BTreeMap<(bool, usize), AffectedGroup> = BTreeMap::new();
    for (id, failure) in failures {
        let node = nodes[indices[&id]];
        let Some(Err(message)) = evaluation.outputs.get(&id) else {
            continue;
        };
        let mut entry = NodeFailure {
            node: NodeRef::from(node),
            message: message.clone(),
            upstream_roots: Vec::new(),
            upstream_root_count: 0,
            upstream_roots_truncated: false,
            affected_nodes: Vec::new(),
            affected_count: 0,
            affected_nodes_truncated: false,
        };
        if failure.is_root {
            root_positions.insert(indices[&id], report.root_failures.len());
            report.root_failures.push(entry);
        } else {
            let group = groups.entry(failure.roots.key()).or_insert_with(|| {
                let mut root_ids = Vec::new();
                let mut root_count = 0;
                failure.roots.for_each(|index| {
                    root_count += 1;
                    if root_ids.len() < MAX_REPORTED_ROOTS {
                        root_ids.push(nodes[index].id);
                    }
                });
                AffectedGroup {
                    roots: failure.roots.clone(),
                    root_ids,
                    root_count,
                    count: 0,
                    preview: Vec::new(),
                }
            });
            group.count += 1;
            if group.preview.len() < MAX_REPORTED_AFFECTED {
                group.preview.push(id);
            }
            entry.upstream_roots.clone_from(&group.root_ids);
            entry.upstream_root_count = group.root_count;
            entry.upstream_roots_truncated = group.root_count > group.root_ids.len();
            report.blocked_nodes.push(entry);
        }
    }
    // Transpose root membership once per shared root set. A many-root merge followed
    // by a long chain therefore does not visit every root for every descendant.
    for group in groups.values() {
        group.roots.for_each(|index| {
            let root = &mut report.root_failures[root_positions[&index]];
            root.affected_count += group.count;
            merge_preview(&mut root.affected_nodes, &group.preview);
        });
    }
    for root in &mut report.root_failures {
        root.affected_nodes_truncated = root.affected_count > root.affected_nodes.len();
    }
    report.summary.root_failure_count = report.root_failures.len();
    report.summary.blocked_count = report.blocked_nodes.len();
    report
}

fn valid_time(time: f64) -> Option<f64> {
    (time.is_finite() && time >= 0.).then_some(time)
}

/// Keep the smallest IDs across groups, independently of their allocation order.
fn merge_preview(current: &mut Vec<NodeId>, incoming: &[NodeId]) {
    if current.is_empty() {
        current.extend(incoming.iter().take(MAX_REPORTED_AFFECTED));
        return;
    }
    let mut merged = Vec::with_capacity(MAX_REPORTED_AFFECTED.min(current.len() + incoming.len()));
    let (mut a, mut b) = (0, 0);
    while merged.len() < MAX_REPORTED_AFFECTED && (a < current.len() || b < incoming.len()) {
        if b == incoming.len() || (a < current.len() && current[a] < incoming[b]) {
            merged.push(current[a]);
            a += 1;
        } else if a == current.len() || incoming[b] < current[a] {
            merged.push(incoming[b]);
            b += 1;
        } else {
            merged.push(current[a]);
            a += 1;
            b += 1;
        }
    }
    *current = merged;
}
