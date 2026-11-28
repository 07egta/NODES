//! Conservative three-way merge for projects with a known common ancestor.
//! No geometry evaluation, script execution or mutation of source projects.

use crate::model::{Graph, Node, NodeId};
use crate::persistence::Project;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Conflict {
    pub path: String,
    pub reason: String,
}

#[derive(Debug)]
pub struct MergeResult {
    pub project: Option<Project>,
    pub conflicts: Vec<Conflict>,
}

fn equivalent<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    // Serialized representation distinguishes signed zero and nested parameters.
    Ok(serde_json::to_vec(a).map_err(|e| e.to_string())?
        == serde_json::to_vec(b).map_err(|e| e.to_string())?)
}

fn conflict(conflicts: &mut Vec<Conflict>, path: &str, reason: &str) {
    conflicts.push(Conflict {
        path: path.into(),
        reason: reason.into(),
    });
}

fn choose<T: Clone + Serialize>(
    base: &T,
    left: &T,
    right: &T,
    path: &str,
    conflicts: &mut Vec<Conflict>,
) -> Result<T, String> {
    if equivalent(left, right)? || equivalent(base, right)? {
        Ok(left.clone())
    } else if equivalent(base, left)? {
        Ok(right.clone())
    } else {
        conflict(conflicts, path, "both branches changed this field");
        // Placeholder only: never returned as a merged project with conflicts.
        Ok(base.clone())
    }
}

fn merge_map<T: Clone + Serialize>(
    base: &BTreeMap<String, T>,
    left: &BTreeMap<String, T>,
    right: &BTreeMap<String, T>,
    path: &str,
    conflicts: &mut Vec<Conflict>,
) -> Result<BTreeMap<String, T>, String> {
    let keys: BTreeSet<_> = base.keys().chain(left.keys()).chain(right.keys()).collect();
    let mut result = BTreeMap::new();
    for key in keys {
        let field = format!("{path}/{key}");
        if let Some(value) = choose(
            &base.get(key),
            &left.get(key),
            &right.get(key),
            &field,
            conflicts,
        )? {
            result.insert(key.clone(), value.clone());
        }
    }
    Ok(result)
}

fn merge_node(
    base: &Node,
    left: &Node,
    right: &Node,
    conflicts: &mut Vec<Conflict>,
) -> Result<Node, String> {
    let path = format!("nodes/{}", base.id);
    // Kind replacement may change port meaning; do not mix it with edits to
    // the old node even if they happen to have the same serialized key.
    if base.kind != left.kind || base.kind != right.kind {
        return choose(base, left, right, &path, conflicts);
    }
    let mut n = base.clone();
    macro_rules! field {
        ($name:ident) => {
            n.$name = choose(
                &base.$name,
                &left.$name,
                &right.$name,
                &format!("{path}/{}", stringify!($name)),
                conflicts,
            )?;
        };
    }
    field!(name);
    field!(position);
    field!(preview);
    field!(off);
    field!(script);
    field!(cluster);
    n.params = merge_map(
        &base.params,
        &left.params,
        &right.params,
        &format!("{path}/params"),
        conflicts,
    )?;
    n.inputs = merge_map(
        &base.inputs,
        &left.inputs,
        &right.inputs,
        &format!("{path}/inputs"),
        conflicts,
    )?;
    Ok(n)
}

fn index(graph: &Graph) -> BTreeMap<NodeId, &Node> {
    graph.nodes.iter().map(|n| (n.id, n)).collect()
}

/// Caller must establish common ancestry. Independently created projects
/// must not be passed here merely because their node IDs match.
pub fn merge(base: &Project, left: &Project, right: &Project) -> Result<MergeResult, String> {
    base.validate()?;
    left.validate()?;
    right.validate()?;
    let mut conflicts = Vec::new();
    let a = index(&base.graph);
    let b = index(&left.graph);
    let c = index(&right.graph);
    let ids: BTreeSet<_> = a.keys().chain(b.keys()).chain(c.keys()).copied().collect();
    let mut merged = BTreeMap::new();
    for id in ids {
        let (x, y, z) = (
            a.get(&id).copied(),
            b.get(&id).copied(),
            c.get(&id).copied(),
        );
        let path = format!("nodes/{id}");
        let node = match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Some(merge_node(x, y, z, &mut conflicts)?),
            (None, Some(_), Some(_)) => {
                // Independent allocations can produce identical IDs and even
                // identical values. Identity cannot be inferred from equality.
                conflict(&mut conflicts, &path, "independent node ID collision");
                None
            }
            _ => choose(&x, &y, &z, &path, &mut conflicts)?.cloned(),
        };
        if let Some(node) = node {
            merged.insert(id, node);
        }
    }
    // Preserve ordering of surviving base nodes. Reject reordering rather than
    // silently undoing it; independently added nodes are appended in ID order.
    for graph in [&left.graph, &right.graph] {
        let actual: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| a.contains_key(&n.id))
            .map(|n| n.id)
            .collect();
        let expected: Vec<_> = base
            .graph
            .nodes
            .iter()
            .filter(|n| actual.contains(&n.id))
            .map(|n| n.id)
            .collect();
        if actual != expected {
            conflict(
                &mut conflicts,
                "graph/node_order",
                "reordering requires explicit resolution",
            );
        }
    }
    let mut project = base.clone();
    project.name = choose(&base.name, &left.name, &right.name, "name", &mut conflicts)?;
    project.pan = choose(&base.pan, &left.pan, &right.pan, "pan", &mut conflicts)?;
    project.zoom = choose(&base.zoom, &left.zoom, &right.zoom, "zoom", &mut conflicts)?;
    project.graph.clusters = choose(
        &base.graph.clusters,
        &left.graph.clusters,
        &right.graph.clusters,
        "graph/clusters",
        &mut conflicts,
    )?;
    project.graph.groups = choose(
        &base.graph.groups,
        &left.graph.groups,
        &right.graph.groups,
        "graph/groups",
        &mut conflicts,
    )?;
    project.graph.notes = choose(
        &base.graph.notes,
        &left.graph.notes,
        &right.graph.notes,
        "graph/notes",
        &mut conflicts,
    )?;
    project.graph.next_id = base
        .graph
        .next_id
        .max(left.graph.next_id)
        .max(right.graph.next_id);
    project.graph.nodes = base
        .graph
        .nodes
        .iter()
        .filter_map(|n| merged.remove(&n.id))
        .collect();
    project.graph.nodes.extend(merged.into_values());
    if conflicts.is_empty() {
        if let Err(error) = project.validate() {
            conflict(&mut conflicts, "graph", &error);
        }
    }
    Ok(MergeResult {
        project: conflicts.is_empty().then_some(project),
        conflicts,
    })
}

/// Explicit stored parameters and incoming wires, not inferred design intent.
#[derive(Debug, Serialize)]
pub struct NodeDna {
    pub id: NodeId,
    pub kind: crate::model::NodeKind,
    pub parameters: BTreeMap<String, f64>,
    pub dependencies: BTreeMap<String, NodeId>,
    pub seed_parameters: BTreeMap<String, f64>,
}

pub fn extract(project: &Project) -> Result<Vec<NodeDna>, String> {
    project.validate()?;
    Ok(project
        .graph
        .nodes
        .iter()
        .map(|n| NodeDna {
            id: n.id,
            kind: n.kind,
            parameters: n.params.clone(),
            dependencies: n.inputs.clone(),
            seed_parameters: n
                .params
                .iter()
                .filter(|(key, _)| key.as_str() == "seed")
                .map(|(key, value)| (key.clone(), *value))
                .collect(),
        })
        .collect())
}

/// Parameter-only revisions store sparse overrides, sharing the original
/// project. Revision IDs are scoped to this history, not global geometry IDs.
#[derive(Clone, Debug, Serialize)]
pub struct ParameterEdit {
    pub node: NodeId,
    pub key: String,
    pub value: f64,
}

#[derive(Debug)]
struct Revision {
    parents: Vec<usize>,
    edits: Vec<ParameterEdit>,
}

#[derive(Debug)]
pub struct History {
    root: Project,
    revisions: Vec<Revision>,
}

impl History {
    pub fn new(root: Project) -> Result<Self, String> {
        root.validate()?;
        Ok(Self {
            root,
            revisions: vec![Revision {
                parents: vec![],
                edits: vec![],
            }],
        })
    }

    pub fn parents(&self, id: usize) -> Option<&[usize]> {
        self.revisions.get(id).map(|r| r.parents.as_slice())
    }

    pub fn materialize(&self, mut id: usize) -> Result<Project, String> {
        let mut chain = Vec::new();
        loop {
            let revision = self.revisions.get(id).ok_or("unknown design revision")?;
            chain.push(revision);
            match revision.parents.first() {
                Some(parent) => id = *parent,
                None => break,
            }
        }
        let mut project = self.root.clone();
        for revision in chain.into_iter().rev() {
            apply(&mut project, &revision.edits)?;
        }
        project.validate()?;
        Ok(project)
    }

    pub fn derive(&mut self, parent: usize, edits: Vec<ParameterEdit>) -> Result<usize, String> {
        let mut project = self.materialize(parent)?;
        apply(&mut project, &edits)?;
        project.validate()?;
        self.append(vec![parent], edits)
    }

    fn append(&mut self, parents: Vec<usize>, edits: Vec<ParameterEdit>) -> Result<usize, String> {
        if self.revisions.len() >= 1024 || edits.len() > crate::model::MAX_NODES {
            return Err("design revision limit reached".into());
        }
        let id = self.revisions.len();
        self.revisions.push(Revision { parents, edits });
        Ok(id)
    }

    fn ancestors(&self, id: usize) -> Result<BTreeSet<usize>, String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            if seen.insert(next) {
                let revision = self.revisions.get(next).ok_or("unknown design revision")?;
                stack.extend(revision.parents.iter().copied());
            }
        }
        Ok(seen)
    }

    /// Only parameter histories are accepted. Multiple maximal common ancestors
    /// require explicit resolution instead of an arbitrary base.
    pub fn combine(
        &mut self,
        left: usize,
        right: usize,
    ) -> Result<Result<usize, Vec<Conflict>>, String> {
        let a = self.ancestors(left)?;
        let b = self.ancestors(right)?;
        let common: BTreeSet<_> = a.intersection(&b).copied().collect();
        let mut nonmaximal = BTreeSet::new();
        for &id in &common {
            nonmaximal.extend(
                self.revisions[id]
                    .parents
                    .iter()
                    .filter(|p| common.contains(p))
                    .copied(),
            );
        }
        let bases: Vec<_> = common.difference(&nonmaximal).copied().collect();
        let [base] = bases.as_slice() else {
            return Err("multiple or missing common ancestors".into());
        };
        let l = self.materialize(left)?;
        let result = merge(&self.materialize(*base)?, &l, &self.materialize(right)?)?;
        let Some(project) = result.project else {
            return Ok(Err(result.conflicts));
        };
        let mut edits = Vec::new();
        for node in &project.graph.nodes {
            let old = l.graph.node(node.id).ok_or("unexpected node change")?;
            for (key, value) in &node.params {
                if old.params.get(key).map(|v| v.to_bits()) != Some(value.to_bits()) {
                    edits.push(ParameterEdit {
                        node: node.id,
                        key: key.clone(),
                        value: *value,
                    });
                }
            }
        }
        self.append(vec![left, right], edits).map(Ok)
    }
}

fn apply(project: &mut Project, edits: &[ParameterEdit]) -> Result<(), String> {
    if edits.len() > crate::model::MAX_NODES {
        return Err("too many parameter edits".into());
    }
    let mut seen = BTreeSet::new();
    for edit in edits {
        if !seen.insert((edit.node, &edit.key)) {
            return Err("duplicate parameter edit".into());
        }
        let node = project.graph.node_mut(edit.node).ok_or("unknown node")?;
        let spec = node
            .kind
            .spec()
            .inputs
            .iter()
            .find(|s| s.key == edit.key && s.default.is_some())
            .ok_or("unknown numeric parameter")?;
        if !edit.value.is_finite()
            || edit.value < spec.min
            || edit.value > spec.max
            || (spec.integer && edit.value.fract() != 0.0)
        {
            return Err("parameter outside declared range".into());
        }
        node.params.insert(edit.key.clone(), edit.value);
    }
    Ok(())
}

/// Provenance matches are never resolved by proximity or by first candidate.
#[derive(Debug, PartialEq, Eq)]
pub enum ReferenceResolution<T> {
    Missing,
    Unique(T),
    Ambiguous(Vec<T>),
}

pub fn resolve_candidates<T: Ord>(
    candidates: impl IntoIterator<Item = T>,
) -> ReferenceResolution<T> {
    let mut unique: Vec<_> = candidates
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    match unique.len() {
        0 => ReferenceResolution::Missing,
        1 => match unique.pop() {
            Some(candidate) => ReferenceResolution::Unique(candidate),
            None => ReferenceResolution::Missing,
        },
        _ => ReferenceResolution::Ambiguous(unique),
    }
}
