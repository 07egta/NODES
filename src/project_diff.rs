//! Read-only comparison of validated native project revisions by stable node ID.
//!
//! This compares stored calculation/presentation fields, not evaluated geometry or
//! graph isomorphism. It is meaningful for revisions that share their node IDs.
//! A missing parameter and an explicitly stored default remain different. IEEE
//! bits distinguish signed zero. File callers load through graph_api::load_safe;
//! its existing legacy-input normalization precedes this comparison.
use crate::graph_api;
use crate::model::{Graph, Node, NodeId, NodeKind};
use crate::persistence::Project;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const DIFF_SCHEMA_VERSION: u32 = 1;
pub const CHANGE_PAGE_SIZE: usize = 256;
pub const MAX_FIELD_CHANGES: usize = 64;
pub const IMPACT_PREVIEW_SIZE: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub struct ProjectDiff {
    pub schema_version: u32,
    pub identity_basis: &'static str,
    pub comparison_basis: &'static str,
    pub evaluation_performed: bool,
    pub before: ProjectSummary,
    pub after: ProjectSummary,
    pub has_changes: bool,
    pub summary: DiffSummary,
    pub metadata: MetadataChanges,
    /// Node records are ordered by stable ID, across all three change kinds.
    pub node_changes: Vec<NodeChange>,
    pub offset: usize,
    pub page_size: usize,
    /// Exact added + removed + modified node count; excludes project metadata.
    pub total_change_count: usize,
    pub next_offset: Option<usize>,
    /// True when later node records remain after this page. Use next_offset.
    pub truncated: bool,
    pub potential_dependency_impact: PotentialImpact,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub schema_version: u32,
    pub node_count: usize,
}
impl From<&Project> for ProjectSummary {
    fn from(project: &Project) -> Self {
        Self {
            name: project.name.clone(),
            schema_version: project.schema_version,
            node_count: project.graph.nodes.len(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DiffSummary {
    pub added_count: usize,
    pub removed_count: usize,
    pub modified_count: usize,
    /// These two modified-node categories can overlap.
    pub calculation_modified_count: usize,
    pub presentation_modified_count: usize,
    pub presentation_only_count: usize,
    pub unchanged_count: usize,
    pub metadata_changed: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MetadataChanges {
    pub name: bool,
    pub pan: bool,
    pub zoom: bool,
    pub next_id: bool,
    pub groups: bool,
    pub notes: bool,
}
impl MetadataChanges {
    fn any(&self) -> bool {
        self.name || self.pan || self.zoom || self.next_id || self.groups || self.notes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

#[derive(Clone, Debug, Serialize)]
pub struct NodeChange {
    pub node_id: NodeId,
    pub change: ChangeKind,
    /// Existence changes set both flags; modified-node counts distinguish actual
    /// field changes on nodes present in both revisions.
    pub calculation_changed: bool,
    pub presentation_changed: bool,
    pub before: Option<NodeSummary>,
    pub after: Option<NodeSummary>,
    pub calculation: CalculationChanges,
    pub presentation: PresentationChanges,
}

/// Small stored-field summary. No geometry, script, definition or evaluation dump.
#[derive(Clone, Debug, Serialize)]
pub struct NodeSummary {
    pub name: String,
    pub kind: NodeKind,
    pub off: bool,
    pub position: [f32; 2],
    pub preview: bool,
}
impl From<&Node> for NodeSummary {
    fn from(node: &Node) -> Self {
        Self {
            name: node.name.clone(),
            kind: node.kind,
            off: node.off,
            position: node.position,
            preview: node.preview,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CalculationChanges {
    pub kind: bool,
    pub off: bool,
    pub parameters: Vec<ParameterChange>,
    pub parameter_change_count: usize,
    pub parameters_truncated: bool,
    pub inputs: Vec<InputChange>,
    pub input_change_count: usize,
    pub inputs_truncated: bool,
    pub field_preview_limit: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct PresentationChanges {
    pub name: bool,
    pub position: bool,
    pub preview: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParameterChange {
    pub key: String,
    /// None means the map entry is absent, including an unstored default.
    pub before: Option<StoredNumber>,
    pub after: Option<StoredNumber>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StoredNumber {
    pub value: f64,
    /// Fixed-width hexadecimal IEEE-754 bits preserve +0/-0 for JSON consumers.
    pub ieee754_hex: String,
}
impl From<f64> for StoredNumber {
    fn from(value: f64) -> Self {
        Self {
            value,
            ieee754_hex: format!("{:016x}", value.to_bits()),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct InputChange {
    pub key: String,
    pub before: Option<NodeId>,
    pub after: Option<NodeId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PotentialImpact {
    pub meaning: &'static str,
    pub graph: &'static str,
    pub includes_seeds: bool,
    pub seed_count: usize,
    pub total_count: usize,
    pub node_ids: Vec<NodeId>,
    pub preview_limit: usize,
    pub truncated: bool,
}

/// Validate and compare two in-memory projects. Does not evaluate or modify either
/// input. An offset at or beyond the end returns an empty terminal page.
pub fn compare(before: &Project, after: &Project, offset: usize) -> Result<ProjectDiff, String> {
    for (name, project) in [("before", before), ("after", after)] {
        project
            .validate()
            .map_err(|error| format!("{name} project: {error}"))?;
        graph_api::validate_safe(&project.graph)
            .map_err(|error| format!("{name} project: {error}"))?;
    }
    let previous: BTreeMap<_, _> = before
        .graph
        .nodes
        .iter()
        .map(|node| (node.id, node))
        .collect();
    let current: BTreeMap<_, _> = after
        .graph
        .nodes
        .iter()
        .map(|node| (node.id, node))
        .collect();
    let ids: BTreeSet<_> = previous.keys().chain(current.keys()).copied().collect();
    let metadata = MetadataChanges {
        name: before.name != after.name,
        pan: !same_position(before.pan, after.pan),
        zoom: before.zoom.to_bits() != after.zoom.to_bits(),
        next_id: before.graph.next_id != after.graph.next_id,
        groups: before.graph.groups != after.graph.groups,
        notes: before.graph.notes.len() != after.graph.notes.len()
            || before
                .graph
                .notes
                .iter()
                .zip(&after.graph.notes)
                .any(|(a, b)| a.text != b.text || !same_position(a.position, b.position)),
    };
    let mut summary = DiffSummary {
        metadata_changed: metadata.any(),
        ..Default::default()
    };
    let mut node_changes = Vec::with_capacity(CHANGE_PAGE_SIZE);
    let mut total_change_count = 0;
    let mut seeds = BTreeSet::new();
    let page_end = offset.saturating_add(CHANGE_PAGE_SIZE);
    for id in ids {
        let old = previous.get(&id).copied();
        let new = current.get(&id).copied();
        let (kind, calculation, presentation) = match (old, new) {
            (None, Some(_)) => {
                summary.added_count += 1;
                seeds.insert(id);
                (ChangeKind::Added, true, true)
            }
            (Some(_), None) => {
                summary.removed_count += 1;
                (ChangeKind::Removed, true, true)
            }
            (Some(a), Some(b)) => {
                let calculation = a.kind != b.kind
                    || a.off != b.off
                    || !same_parameters(&a.params, &b.params)
                    || a.inputs != b.inputs;
                let presentation = a.name != b.name
                    || !same_position(a.position, b.position)
                    || a.preview != b.preview;
                if !calculation && !presentation {
                    summary.unchanged_count += 1;
                    continue;
                }
                summary.modified_count += 1;
                summary.calculation_modified_count += usize::from(calculation);
                summary.presentation_modified_count += usize::from(presentation);
                summary.presentation_only_count += usize::from(presentation && !calculation);
                if calculation {
                    seeds.insert(id);
                }
                (ChangeKind::Modified, calculation, presentation)
            }
            (None, None) => unreachable!("the ID is in the union of both maps"),
        };
        if (offset..page_end).contains(&total_change_count) {
            node_changes.push(node_change(id, kind, calculation, presentation, old, new));
        }
        total_change_count += 1;
    }
    let next_offset = (page_end < total_change_count).then_some(page_end);
    Ok(ProjectDiff {
        schema_version: DIFF_SCHEMA_VERSION,
        identity_basis: "native_node_id",
        comparison_basis: "stored_project_fields_after_legacy_input_normalization",
        evaluation_performed: false,
        before: ProjectSummary::from(before),
        after: ProjectSummary::from(after),
        has_changes: total_change_count > 0 || metadata.any(),
        summary,
        metadata,
        node_changes,
        offset,
        page_size: CHANGE_PAGE_SIZE,
        total_change_count,
        next_offset,
        truncated: next_offset.is_some(),
        potential_dependency_impact: impact(&after.graph, seeds),
    })
}

fn same_position(a: [f32; 2], b: [f32; 2]) -> bool {
    a.into_iter()
        .zip(b)
        .all(|(a, b)| a.to_bits() == b.to_bits())
}

fn same_parameters(a: &BTreeMap<String, f64>, b: &BTreeMap<String, f64>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((ka, a), (kb, b))| ka == kb && a.to_bits() == b.to_bits())
}

fn node_change(
    node_id: NodeId,
    change: ChangeKind,
    calculation_changed: bool,
    presentation_changed: bool,
    before: Option<&Node>,
    after: Option<&Node>,
) -> NodeChange {
    let mut parameters = Vec::new();
    let mut parameter_change_count = 0;
    let keys: BTreeSet<_> = before
        .into_iter()
        .flat_map(|node| node.params.keys())
        .chain(after.into_iter().flat_map(|node| node.params.keys()))
        .collect();
    for key in keys {
        let old = before.and_then(|node| node.params.get(key)).copied();
        let new = after.and_then(|node| node.params.get(key)).copied();
        if old.map(f64::to_bits) == new.map(f64::to_bits) {
            continue;
        }
        parameter_change_count += 1;
        if parameters.len() < MAX_FIELD_CHANGES {
            parameters.push(ParameterChange {
                key: key.clone(),
                before: old.map(StoredNumber::from),
                after: new.map(StoredNumber::from),
            });
        }
    }
    let mut inputs = Vec::new();
    let mut input_change_count = 0;
    let keys: BTreeSet<_> = before
        .into_iter()
        .flat_map(|node| node.inputs.keys())
        .chain(after.into_iter().flat_map(|node| node.inputs.keys()))
        .collect();
    for key in keys {
        let old = before.and_then(|node| node.inputs.get(key)).copied();
        let new = after.and_then(|node| node.inputs.get(key)).copied();
        if old == new {
            continue;
        }
        input_change_count += 1;
        if inputs.len() < MAX_FIELD_CHANGES {
            inputs.push(InputChange {
                key: key.clone(),
                before: old,
                after: new,
            });
        }
    }
    NodeChange {
        node_id,
        change,
        calculation_changed,
        presentation_changed,
        before: before.map(NodeSummary::from),
        after: after.map(NodeSummary::from),
        calculation: CalculationChanges {
            kind: before.map(|node| node.kind) != after.map(|node| node.kind),
            off: before.map(|node| node.off) != after.map(|node| node.off),
            parameters_truncated: parameters.len() < parameter_change_count,
            parameters,
            parameter_change_count,
            inputs_truncated: inputs.len() < input_change_count,
            inputs,
            input_change_count,
            field_preview_limit: MAX_FIELD_CHANGES,
        },
        presentation: PresentationChanges {
            name: before.map(|node| &node.name) != after.map(|node| &node.name),
            position: before.map(|node| node.position.map(f32::to_bits))
                != after.map(|node| node.position.map(f32::to_bits)),
            preview: before.map(|node| node.preview) != after.map(|node| node.preview),
        },
    }
}

fn impact(graph: &Graph, seeds: BTreeSet<NodeId>) -> PotentialImpact {
    let seed_count = seeds.len();
    let mut downstream: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for node in &graph.nodes {
        for source in node.inputs.values() {
            downstream.entry(*source).or_default().push(node.id);
        }
    }
    let mut reached = seeds;
    let mut queue: VecDeque<_> = reached.iter().copied().collect();
    while let Some(id) = queue.pop_front() {
        for &child in downstream.get(&id).into_iter().flatten() {
            if reached.insert(child) {
                queue.push_back(child);
            }
        }
    }
    let total_count = reached.len();
    let node_ids: Vec<_> = reached.into_iter().take(IMPACT_PREVIEW_SIZE).collect();
    PotentialImpact {
        meaning: "potential_dependency_impact",
        graph: "after",
        includes_seeds: true,
        seed_count,
        total_count,
        truncated: total_count > node_ids.len(),
        node_ids,
        preview_limit: IMPACT_PREVIEW_SIZE,
    }
}
