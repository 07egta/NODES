//! Compare two validated saved projects without evaluating geometry or scripts.
//! The report uses stable node IDs, and conservatively tracks downstream impact.

use crate::model::{Graph, Node, NodeId};
use crate::persistence::Project;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Evaluation,
    Presentation,
    Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Project,
    Graph,
    Node,
}

#[derive(Debug, Serialize)]
pub struct Change {
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    pub field: String,
    pub class: Class,
    pub operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub changed: bool,
    pub change_count: usize,
    pub changes: Vec<Change>,
    pub evaluation_roots: Vec<NodeId>,
    pub affected_before: Vec<NodeId>,
    pub affected_after: Vec<NodeId>,
    pub evaluation_performed: bool,
}

impl Report {
    fn record(
        &mut self,
        scope: Scope,
        node_id: Option<NodeId>,
        field: &str,
        class: Class,
        before: Option<Value>,
        after: Option<Value>,
    ) {
        let operation = match (&before, &after) {
            (None, Some(_)) => "add",
            (Some(_), None) => "remove",
            _ => "change",
        };
        self.changes.push(Change {
            scope,
            node_id,
            field: field.into(),
            class,
            operation,
            before,
            after,
        });
    }
}

fn nodes(graph: &Graph) -> BTreeMap<NodeId, &Node> {
    graph.nodes.iter().map(|node| (node.id, node)).collect()
}

fn bits(values: &[f32; 2]) -> [u32; 2] {
    values.map(f32::to_bits)
}

fn compare_node(report: &mut Report, old: &Node, new: &Node) {
    let id = Some(old.id);
    let mut record = |field: &str, class: Class, before: Option<Value>, after: Option<Value>| {
        report.record(Scope::Node, id, field, class, before, after);
    };
    use Class::{Evaluation, Presentation};
    if old.kind != new.kind {
        record(
            "kind",
            Evaluation,
            Some(json!(old.kind)),
            Some(json!(new.kind)),
        );
    }
    if old.name != new.name {
        record(
            "name",
            Presentation,
            Some(json!(old.name)),
            Some(json!(new.name)),
        );
    }
    if bits(&old.position) != bits(&new.position) {
        record(
            "position",
            Presentation,
            Some(json!(old.position)),
            Some(json!(new.position)),
        );
    }
    if old.preview != new.preview {
        record(
            "preview",
            Presentation,
            Some(json!(old.preview)),
            Some(json!(new.preview)),
        );
    }
    if old.off != new.off {
        record("off", Evaluation, Some(json!(old.off)), Some(json!(new.off)));
    }
    if old.cluster != new.cluster {
        record(
            "cluster",
            Evaluation,
            Some(json!(old.cluster)),
            Some(json!(new.cluster)),
        );
    }
    // Script bodies can contain private paths or user code. Do not echo them.
    if old.script != new.script {
        record("script", Evaluation, None, None);
    }
    let keys: BTreeSet<_> = old.params.keys().chain(new.params.keys()).collect();
    for key in keys {
        let before = old.params.get(key).copied();
        let after = new.params.get(key).copied();
        // The evaluator keys numerical inputs by IEEE bits, including signed zero.
        if before.map(f64::to_bits) != after.map(f64::to_bits) {
            record(
                &format!("params.{key}"),
                Evaluation,
                before.map(|value| json!(value)),
                after.map(|value| json!(value)),
            );
        }
    }
    let keys: BTreeSet<_> = old.inputs.keys().chain(new.inputs.keys()).collect();
    for key in keys {
        let before = old.inputs.get(key);
        let after = new.inputs.get(key);
        if before != after {
            record(
                &format!("inputs.{key}"),
                Evaluation,
                before.map(|value| json!(value)),
                after.map(|value| json!(value)),
            );
        }
    }
}

/// Walk a reverse adjacency list once. Iteration is bounded by graph size,
/// and cycles cannot occur in a validated project.
fn affected(graph: &Graph, roots: &BTreeSet<NodeId>) -> Vec<NodeId> {
    let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    let ids: BTreeSet<_> = graph.nodes.iter().map(|node| node.id).collect();
    for node in &graph.nodes {
        for source in node.inputs.values() {
            children.entry(*source).or_default().push(node.id);
        }
    }
    let mut visited: BTreeSet<_> = roots.intersection(&ids).copied().collect();
    let mut pending: VecDeque<_> = visited.iter().copied().collect();
    while let Some(id) = pending.pop_front() {
        if let Some(next) = children.get(&id) {
            for &child in next {
                if visited.insert(child) {
                    pending.push_back(child);
                }
            }
        }
    }
    visited.into_iter().collect()
}

/// Compare native v1 projects. The content-only mode retains evaluation
/// changes, not canvas presentation or allocator metadata. No files are modified.
pub fn compare(before: &Project, after: &Project, content_only: bool) -> Result<Report, String> {
    before.validate()?;
    after.validate()?;
    let mut report = Report::default();
    use Class::{Evaluation, Metadata, Presentation};
    if before.name != after.name {
        report.record(
            Scope::Project,
            None,
            "name",
            Presentation,
            Some(json!(before.name)),
            Some(json!(after.name)),
        );
    }
    if bits(&before.pan) != bits(&after.pan) {
        report.record(
            Scope::Project,
            None,
            "pan",
            Presentation,
            Some(json!(before.pan)),
            Some(json!(after.pan)),
        );
    }
    if before.zoom.to_bits() != after.zoom.to_bits() {
        report.record(
            Scope::Project,
            None,
            "zoom",
            Presentation,
            Some(json!(before.zoom)),
            Some(json!(after.zoom)),
        );
    }
    let a = &before.graph;
    let b = &after.graph;
    if a.next_id != b.next_id {
        report.record(
            Scope::Graph,
            None,
            "next_id",
            Metadata,
            Some(json!(a.next_id)),
            Some(json!(b.next_id)),
        );
    }
    if a.groups != b.groups {
        report.record(Scope::Graph, None, "groups", Presentation, None, None);
    }
    if a.notes != b.notes {
        report.record(Scope::Graph, None, "notes", Presentation, None, None);
    }
    // JSON serialization preserves signed-zero distinctions in nested parameters.
    // Do not report cluster source or scripts: just mark dependent instances.
    let clusters_changed = serde_json::to_vec(&a.clusters).map_err(|e| e.to_string())?
        != serde_json::to_vec(&b.clusters).map_err(|e| e.to_string())?;
    if clusters_changed {
        report.record(Scope::Graph, None, "clusters", Evaluation, None, None);
    }
    let old = nodes(a);
    let new = nodes(b);
    let common_order = |graph: &Graph| {
        graph
            .nodes
            .iter()
            .map(|node| node.id)
            .filter(|id| old.contains_key(id) && new.contains_key(id))
            .collect::<Vec<_>>()
    };
    let old_order = common_order(a);
    let new_order = common_order(b);
    if old_order != new_order {
        report.record(
            Scope::Graph,
            None,
            "relative_node_order",
            Presentation,
            Some(json!(old_order)),
            Some(json!(new_order)),
        );
    }
    let ids: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    for id in ids {
        match (old.get(&id), new.get(&id)) {
            (Some(x), Some(y)) => compare_node(&mut report, x, y),
            (Some(x), None) => report.record(
                Scope::Node,
                Some(id),
                "node",
                Evaluation,
                Some(json!(x.kind)),
                None,
            ),
            (None, Some(y)) => report.record(
                Scope::Node,
                Some(id),
                "node",
                Evaluation,
                None,
                Some(json!(y.kind)),
            ),
            (None, None) => unreachable!(),
        }
    }
    let mut roots: BTreeSet<_> = report
        .changes
        .iter()
        .filter(|change| change.class == Evaluation)
        .filter_map(|change| change.node_id)
        .collect();
    if clusters_changed {
        roots.extend(
            a.nodes
                .iter()
                .chain(&b.nodes)
                .filter(|node| node.cluster.is_some())
                .map(|node| node.id),
        );
    }
    report.affected_before = affected(a, &roots);
    report.affected_after = affected(b, &roots);
    report.evaluation_roots = roots.into_iter().collect();
    if content_only {
        report.changes.retain(|change| change.class == Evaluation);
    }
    report.change_count = report.changes.len();
    report.changed = report.change_count != 0;
    Ok(report)
}
