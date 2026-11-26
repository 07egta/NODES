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
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::Arc;

pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
/// Exact counts are retained when the display/API relationship lists are shortened.
pub const MAX_REPORTED_ROOTS: usize = 64;
pub const MAX_REPORTED_AFFECTED: usize = 256;
/// Error messages can contain user script text. Bound each report entry without
/// changing the evaluator's original result or splitting a UTF-8 character.
pub const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 8_192;

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
    /// Original evaluator message, preserved verbatim unless message_truncated.
    pub message: String,
    pub message_truncated: bool,
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
// along chains/fanout; joins only copy if they introduce a new root. Bits address
// dense root ordinals, not all graph nodes, so unrelated nodes or high root IDs do
// not widen every set. The graph's MAX_NODES limit bounds the largest matrix.
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
    // finite even for synthetic callers constructing Evaluation manually. For an
    // overflowing synthetic sum, saturate the reported total but normalize shares
    // in scaled units so they still do not exceed 100% in aggregate.
    if !report.summary.measured_node_ms.is_finite() {
        let largest = report
            .hotspots
            .iter()
            .map(|hotspot| hotspot.elapsed_ms)
            .fold(0., f64::max);
        let scaled_total: f64 = report
            .hotspots
            .iter()
            .map(|hotspot| hotspot.elapsed_ms / largest)
            .sum();
        for hotspot in &mut report.hotspots {
            hotspot.share_percent = (hotspot.elapsed_ms / largest) / scaled_total * 100.;
        }
        report.summary.measured_node_ms = f64::MAX;
    } else if report.summary.measured_node_ms > 0. {
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
    if report.summary.failed_count == 0 {
        // Validation and visit/timing summaries still apply to successful,
        // disabled or interrupted graphs; no failure relationships need indexing.
        return report;
    }
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id, index))
        .collect();
    let failed: Vec<_> = nodes
        .iter()
        .map(|node| !node.off && evaluation.outputs.get(&node.id).is_some_and(Result::is_err))
        .collect();
    // In a valid DAG every consumed parent precedes its child. The propagation
    // pass therefore creates a root exactly when this local test finds no enabled
    // failed parent. Successful, disabled and unevaluated parents all break the
    // failure chain, just as they do in the propagation pass below.
    let mut root_node_ids = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if !failed[index] {
            continue;
        }
        let has_failed_parent = node
            .kind
            .spec()
            .inputs
            .iter()
            .take(crate::cluster::real_input_count(graph, node))
            .any(|spec| {
                node.inputs
                    .get(spec.key)
                    .is_some_and(|source| failed[indices[source]])
            });
        if !has_failed_parent {
            root_node_ids.push(node.id);
        }
    }
    // `nodes` is ID-sorted, so bit iteration still emits root IDs in sorted order
    // even when their topological discovery order is different.
    let root_ordinals: BTreeMap<_, _> = root_node_ids
        .iter()
        .enumerate()
        .map(|(ordinal, &id)| (id, ordinal))
        .collect();
    let mut failures: BTreeMap<NodeId, FailureRoots> = BTreeMap::new();
    for node in order {
        if !failed[indices[&node.id]] {
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
                roots: upstream.unwrap_or_else(|| Roots::One(root_ordinals[&node.id])),
            },
        );
    }
    // Only ordinals set by actual root failures are ever read from this vector.
    // Direct indexing avoids a map lookup for each root/group membership.
    let mut root_positions = vec![0; root_node_ids.len()];
    let mut previews = Vec::new();
    let mut groups: BTreeMap<(bool, usize), AffectedGroup> = BTreeMap::new();
    for (id, failure) in failures {
        let node = nodes[indices[&id]];
        let Some(Err(message)) = evaluation.outputs.get(&id) else {
            continue;
        };
        let (message, message_truncated) = bounded_message(message);
        let mut entry = NodeFailure {
            node: NodeRef::from(node),
            message,
            message_truncated,
            upstream_roots: Vec::new(),
            upstream_root_count: 0,
            upstream_roots_truncated: false,
            affected_nodes: Vec::new(),
            affected_count: 0,
            affected_nodes_truncated: false,
        };
        if failure.is_root {
            root_positions[root_ordinals[&id]] = report.root_failures.len();
            report.root_failures.push(entry);
            previews.push(AffectedPreview::default());
        } else {
            let group = groups.entry(failure.roots.key()).or_insert_with(|| {
                let mut root_ids = Vec::new();
                let mut root_count = 0;
                failure.roots.for_each(|index| {
                    root_count += 1;
                    if root_ids.len() < MAX_REPORTED_ROOTS {
                        root_ids.push(root_node_ids[index]);
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
    // Process groups by their smallest affected ID. Prefix-merge graphs can have
    // many distinct root sets; filling the smallest previews first lets later
    // groups update exact counts without updating already-full lists. Interleaved
    // IDs switch the affected root to a bounded heap, avoiding a full vector copy
    // for each newly admitted smaller ID.
    let mut groups: Vec<_> = groups.values().collect();
    groups.sort_unstable_by_key(|group| group.preview.first().copied());
    for group in groups {
        group.roots.for_each(|index| {
            let position = root_positions[index];
            let root = &mut report.root_failures[position];
            root.affected_count += group.count;
            previews[position].extend(&group.preview);
        });
    }
    for (root, preview) in report.root_failures.iter_mut().zip(previews) {
        root.affected_nodes = preview.finish();
        root.affected_nodes_truncated = root.affected_count > root.affected_nodes.len();
    }
    report.summary.root_failure_count = report.root_failures.len();
    report.summary.blocked_count = report.blocked_nodes.len();
    report
}

fn valid_time(time: f64) -> Option<f64> {
    (time.is_finite() && time >= 0.).then_some(time)
}

fn bounded_message(message: &str) -> (String, bool) {
    if message.len() <= MAX_DIAGNOSTIC_MESSAGE_BYTES {
        return (message.to_owned(), false);
    }
    let mut end = MAX_DIAGNOSTIC_MESSAGE_BYTES - '…'.len_utf8();
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}…", &message[..end]), true)
}

/// Keep the smallest IDs across groups. Group previews are sorted and disjoint:
/// each blocked node belongs to exactly one group. A node omitted from its own
/// group's first K entries cannot enter any root's global first K entries.
enum AffectedPreview {
    Sorted(Vec<NodeId>),
    Heap(BinaryHeap<NodeId>),
}

impl Default for AffectedPreview {
    fn default() -> Self {
        Self::Sorted(Vec::new())
    }
}

impl AffectedPreview {
    fn extend(&mut self, incoming: &[NodeId]) {
        let Some(&first) = incoming.first() else {
            return;
        };
        match self {
            Self::Sorted(current) => {
                if current.len() == MAX_REPORTED_AFFECTED
                    && current.last().is_some_and(|last| first >= *last)
                {
                    return;
                }
                if current.last().is_none_or(|last| *last < first) {
                    current.extend(
                        incoming
                            .iter()
                            .take(MAX_REPORTED_AFFECTED - current.len())
                            .copied(),
                    );
                    return;
                }
                // Reuse the existing allocation and heapify once. A succession of
                // interleaving singleton groups then needs O(log K) replacements,
                // instead of allocating/copying K entries for every singleton.
                let mut heap = BinaryHeap::from(std::mem::take(current));
                Self::extend_heap(&mut heap, incoming);
                *self = Self::Heap(heap);
            }
            Self::Heap(heap) => Self::extend_heap(heap, incoming),
        }
    }

    fn extend_heap(heap: &mut BinaryHeap<NodeId>, incoming: &[NodeId]) {
        for &id in incoming {
            if heap.len() < MAX_REPORTED_AFFECTED {
                heap.push(id);
            } else {
                match heap.peek_mut() {
                    Some(mut largest) if id < *largest => *largest = id,
                    // Incoming IDs are ascending; every later one also misses
                    // the bounded preview once the current ID is too large.
                    _ => break,
                }
            }
        }
    }

    fn finish(self) -> Vec<NodeId> {
        match self {
            Self::Sorted(ids) => ids,
            Self::Heap(heap) => heap.into_sorted_vec(),
        }
    }
}
