//! Versioned, local structured graph API for AI and other untrusted graph producers.
//! This is deliberately a capability-restricted subset of the native graph editor:
//! no script, expression, imported file, cluster or external plugin execution.
use crate::engine::{Evaluation, Evaluator};
use crate::model::{DataType, Graph, NodeId, NodeKind, MAX_NODES};
use crate::persistence::{self, Project, MAX_PROJECT_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const API_VERSION: u32 = 1;
pub const MAX_EDITS: usize = 8192;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

pub fn permitted(kind: NodeKind) -> bool {
    // Positive allowlist: new node kinds cannot acquire capabilities accidentally.
    use NodeKind::*;
    matches!(kind, Number | Add | Multiply | Divide | Subtract | Power | Abs |
        Min | Max | Modulo | Sine | Series | Range | Point | Circle | Polygon |
        Star | Rectangle | Helix | Extrude | Loft | Revolve | Vase | Sphere |
        Box | Cylinder | Cone | Torus | Plane | Move | Rotate | Scale | ScaleXyz |
        Mirror | Twist | Wave | LinearArray | RadialArray | Merge | Flatten | Graft |
        Output | ListItem | ListLength | ReverseList | SortList | CullPattern |
        ShiftList | ConcatList | SubList | Bounds | Remap | Random | Populate |
        Noise | NoiseDeform | GraphProfile | Shell | OverhangCheck | Distance | PointComponent |
        MeshArea | MeshVolume | MeshCenter | MeshDimensions | MeshMass | MeshAudit |
        MeshVertices | MeshFaces | MeshNormals | MeshEdges | MeshBoundary | MeshClosest |
        MeshFlip | MeshWeldExact | MeshUnweld | MeshUnify | MeshSmooth | MeshSubdivide |
        MeshCatmullClark | MeshOnBed | MeshDeleteFace | MeshConstruct | MeshSelfContact |
        MeshThicknessSample | MeshCompact | MeshNonManifold)
        || crate::math_nodes::spec(kind).is_some()
        || crate::curve_nodes::is_curve_kind(kind)
        || crate::surface_nodes::KINDS.contains(&kind)
        || crate::generative::is_kind(kind)
        || (crate::data_tree_nodes::spec(kind).is_some() && kind != PathMapper)
}

#[derive(Debug, Serialize)]
pub struct PortInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: DataType,
    pub default: Option<f64>,
    pub min: f64,
    pub max: f64,
    pub integer: bool,
    pub access: &'static str,
    pub access_description: &'static str,
}
#[derive(Debug, Serialize)]
pub struct NodeInfo {
    pub kind: NodeKind,
    pub label: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub inputs: Vec<PortInfo>,
    pub output: DataType,
    pub item_matching_configurable: bool,
    pub default_branch_matching: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Catalog {
    pub api_version: u32,
    pub nodes: Vec<NodeInfo>,
}

pub fn catalog() -> Catalog {
    Catalog {
        api_version: API_VERSION,
        nodes: NodeKind::all().iter().copied().filter(|k| permitted(*k)).map(|kind| {
            let spec = kind.spec();
            NodeInfo { kind, label: spec.label, category: spec.category, description: spec.description,
                item_matching_configurable: crate::node_access::supports_item_matching(kind),
                default_branch_matching: if crate::node_access::uses_ordinal_matching(kind) { "ordinal" } else { "legacy_paths_unless_explicit" },
                output: spec.output, inputs: spec.inputs.iter().enumerate().map(|(index,p)| PortInfo {
                    key: p.key, label: p.label, kind: p.kind, default: p.default,
                    min: p.min, max: p.max, integer: p.integer,
                    access: crate::node_access::for_input(kind,index).map_or("Unknown", |a| a.label()),
                    access_description: crate::node_access::for_input(kind,index).map_or("", |a| a.description()),
                }).collect() }
        }).collect(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Add { alias: String, kind: NodeKind, position: [f32; 2] },
    Connect { from: String, to: String, input: String },
    Set { node: String, input: String, value: f64 },
    Disconnect { to: String, input: String },
    Reset { node: String, input: String },
    Move { node: String, position: [f32; 2] },
    Remove { node: String },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub api_version: u32,
    pub edits: Vec<Edit>,
}

pub fn parse_request(bytes: &[u8]) -> Result<Request, String> {
    if bytes.len() > MAX_REQUEST_BYTES { return Err("graph request exceeds 1 MiB".into()); }
    let request: Request = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if request.api_version != API_VERSION || request.edits.len() > MAX_EDITS {
        return Err("unsupported API version or too many edits".into());
    }
    Ok(request)
}

fn validate_numeric(kind: NodeKind, key: &str, value: f64) -> Result<(), String> {
    let spec = kind.spec().inputs.iter().find(|p| p.key == key && p.default.is_some())
        .ok_or("unknown numeric input")?;
    if !value.is_finite() || value < spec.min || value > spec.max ||
        (spec.integer && value.fract() != 0.0) { return Err("parameter is outside its declared range".into()); }
    Ok(())
}

pub fn validate_safe(graph: &Graph) -> Result<(), String> {
    if !graph.clusters.is_empty() { return Err("clusters not allowed in untrusted API graphs".into()); }
    graph.validate()?;
    for node in &graph.nodes {
        if !permitted(node.kind) || !node.script.is_empty() || node.cluster.is_some() {
            return Err("untrusted graph contains an executable or unsupported node".into());
        }
        for (key, value) in &node.params { validate_numeric(node.kind, key, *value)?; }
    }
    Ok(())
}

pub fn build(request: &Request) -> Result<Project, String> {
    let mut project = Project::new("API graph", Graph::default());
    apply(&mut project.graph, request)?;
    project.validate()?;
    Ok(project)
}

/// Transactional: caller's graph is unchanged if any operation fails.
/// Existing nodes can be addressed by their decimal native ID; new aliases must
/// be unique within the request and contain ASCII letters, digits, '_' or '-'.
/// Returned aliases refer only to nodes still present after the transaction.
pub fn apply(graph: &mut Graph, request: &Request) -> Result<BTreeMap<String, NodeId>, String> {
    if request.api_version != API_VERSION || request.edits.len() > MAX_EDITS {
        return Err("unsupported API version or too many edits".into());
    }
    validate_safe(graph)?;
    let mut candidate = graph.clone();
    let mut names: BTreeMap<String, NodeId> = candidate.nodes.iter().map(|n| (n.id.to_string(), n.id)).collect();
    let mut allocated: BTreeSet<_> = names.keys().cloned().collect();
    let mut touched = BTreeSet::new();
    for edit in &request.edits {
        match edit {
            Edit::Add { alias, kind, position } => {
                if alias.is_empty() || alias.len() > 64 || alias.bytes().all(|b| b.is_ascii_digit()) ||
                    !alias.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')) ||
                    allocated.contains(alias) || !permitted(*kind) {
                    return Err("invalid, duplicate or unsupported node alias/kind".into());
                }
                if candidate.nodes.len() >= MAX_NODES { return Err("too many nodes".into()); }
                let id = candidate.add(*kind, *position)?;
                allocated.insert(alias.clone());
                names.insert(alias.clone(), id);
            }
            Edit::Connect { from, to, input } => {
                let from = *names.get(from).ok_or("unknown source alias")?;
                let to = *names.get(to).ok_or("unknown target alias")?;
                // A legacy spelling and the canonical key identify the same port.
                let kind = candidate.node(to).ok_or("missing target node")?.kind;
                let key = kind.canonical_input_key(input);
                if !touched.insert((to, key.to_owned())) {
                    return Err("duplicate input connection edit".into());
                }
                candidate.connect(from, to, key)?;
            }
            Edit::Set { node, input, value } => {
                let id = *names.get(node).ok_or("unknown parameter alias")?;
                let n = candidate.node_mut(id).ok_or("missing node")?;
                validate_numeric(n.kind, input, *value)?;
                n.params.insert(input.clone(), *value);
            }
            Edit::Disconnect { to, input } => {
                let id = *names.get(to).ok_or("unknown target alias")?;
                let n = candidate.node(id).ok_or("missing target node")?;
                let key = n.kind.canonical_input_key(input);
                if !n.inputs.contains_key(key) {
                    return Err("input is not connected".into());
                }
                if !touched.insert((id, key.to_owned())) {
                    return Err("duplicate input connection edit".into());
                }
                candidate.disconnect(id, key);
            }
            Edit::Reset { node, input } => {
                let id = *names.get(node).ok_or("unknown parameter alias")?;
                let n = candidate.node_mut(id).ok_or("missing node")?;
                let key = n.kind.canonical_input_key(input);
                let default = n.kind.spec().inputs.iter()
                    .find(|p| p.key == key)
                    .and_then(|p| p.default)
                    .ok_or("input has no numeric default")?;
                n.params.insert(key.to_owned(), default);
            }
            Edit::Move { node, position } => {
                let id = *names.get(node).ok_or("unknown node alias")?;
                if !position.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) {
                    return Err("node position is outside the canvas range".into());
                }
                candidate.node_mut(id).ok_or("missing node")?.position = *position;
            }
            Edit::Remove { node } => {
                let id = *names.get(node).ok_or("unknown node alias")?;
                // Graph::remove cleans dependent wires. Keep unrelated annotations
                // unchanged, including pre-existing empty groups.
                candidate.remove(id);
                candidate.groups.retain_mut(|group| {
                    let was_member = group.nodes.contains(&id);
                    group.nodes.retain(|&member| member != id);
                    !was_member || !group.nodes.is_empty()
                });
                names.retain(|_, value| *value != id);
            }
        }
    }
    validate_safe(&candidate)?;
    *graph = candidate;
    Ok(names)
}

pub fn evaluate(graph: &Graph, cancel: &dyn Fn() -> bool) -> Result<Evaluation, String> {
    validate_safe(graph)?;
    Ok(Evaluator::default().evaluate(graph, cancel))
}

pub fn save(graph: &Graph, destination: &Path) -> Result<(), String> {
    validate_safe(graph)?;
    persistence::save(destination, &Project::new("API graph", graph.clone()))
}

pub fn build_file(input: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(input).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() { return Err("request must be a regular file".into()); }
    let mut bytes = Vec::new();
    file.take((MAX_REQUEST_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let project = build(&parse_request(&bytes)?)?;
    // Save only after successful evaluation. The evaluator has its own geometry and item budgets.
    let evaluation = evaluate(&project.graph, &|| false)?;
    if evaluation.cancelled { return Err("evaluation cancelled".into()); }
    if let Some(error) = evaluation.outputs.values().find_map(|r| r.as_ref().err()) {
        return Err(format!("graph evaluation failed: {error}"));
    }
    save(&project.graph, destination)
}

pub fn load_safe(path: &Path) -> Result<Project, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() { return Err("project must be a regular file".into()); }
    let mut bytes = Vec::new();
    file.take((MAX_PROJECT_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PROJECT_BYTES { return Err("project exceeds size limit".into()); }
    let p: Project = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    p.validate()?;
    validate_safe(&p.graph)?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Value;

    const SAMPLE: &[u8] = include_bytes!("../examples/interop/ai-add.json");

    #[test]
    fn build_and_evaluate_addition() {
        let project = build(&parse_request(SAMPLE).unwrap()).unwrap();
        let result = evaluate(&project.graph, &|| false).unwrap();
        assert_eq!(result.outputs[&2].as_ref().unwrap().branches.values().flatten().next(),
            Some(&Value::Number(12.0)));
        assert!(catalog().nodes.iter().all(|n| permitted(n.kind)));
    }

    #[test]
    fn untrusted_capabilities_and_versions_are_rejected() {
        for kind in ["script", "import_mesh", "cluster", "expression"] {
            let s = format!(r#"{{"api_version":1,"edits":[{{"op":"add","alias":"bad","kind":"{kind}","position":[0,0]}}]}}"#);
            assert!(build(&parse_request(s.as_bytes()).unwrap()).is_err());
        }
        assert!(parse_request(br#"{"api_version":2,"edits":[]}"#).is_err());
        assert!(parse_request(&vec![b' '; MAX_REQUEST_BYTES+1]).is_err());
    }

    #[test]
    fn transactional_editing_disconnect_reset_move_and_remove() {
        let mut graph = build(&parse_request(SAMPLE).unwrap()).unwrap().graph;
        crate::annotations::add_group(&mut graph, &[0, 1], "sources").unwrap();
        crate::annotations::add_group(&mut graph, &[1], "second").unwrap();
        let original_next_id = graph.next_id;
        let default = NodeKind::Number.spec().inputs.iter()
            .find(|p| p.key == "value").unwrap().default.unwrap();
        let request = parse_request(br#"{"api_version":1,"edits":[
            {"op":"disconnect","to":"2","input":"a"},
            {"op":"reset","node":"0","input":"value"},
            {"op":"move","node":"0","position":[125,-50]},
            {"op":"remove","node":"1"}
        ]}"#).unwrap();
        let aliases = apply(&mut graph, &request).unwrap();
        assert_eq!(aliases.keys().map(String::as_str).collect::<Vec<_>>(), vec!["0", "2"]);
        assert_eq!(graph.node(0).unwrap().params["value"], default);
        assert_eq!(graph.node(0).unwrap().position, [125., -50.]);
        assert!(graph.node(1).is_none());
        assert!(graph.node(2).unwrap().inputs.is_empty());
        assert_eq!(graph.next_id, original_next_id);
        assert_eq!(graph.groups.len(), 1);
        assert_eq!(graph.groups[0].nodes, vec![0]);
        assert!(validate_safe(&graph).is_ok());
    }

    #[test]
    fn destructive_and_invalid_edits_roll_back() {
        let mut graph = build(&parse_request(SAMPLE).unwrap()).unwrap().graph;
        let before = graph.clone();
        for bytes in [
            br#"{"api_version":1,"edits":[{"op":"remove","node":"0"},{"op":"move","node":"0","position":[0,0]}]}"#.as_slice(),
            br#"{"api_version":1,"edits":[{"op":"remove","node":"1"},{"op":"disconnect","to":"2","input":"b"}]}"#.as_slice(),
            br#"{"api_version":1,"edits":[{"op":"reset","node":"0","input":"no_such_port"}]}"#.as_slice(),
            br#"{"api_version":1,"edits":[{"op":"move","node":"0","position":[1000001,0]}]}"#.as_slice(),
            br#"{"api_version":1,"edits":[{"op":"add","alias":"new","kind":"number","position":[0,0]},{"op":"remove","node":"new"},{"op":"add","alias":"new","kind":"number","position":[0,0]}]}"#.as_slice(),
        ] {
            assert!(apply(&mut graph, &parse_request(bytes).unwrap()).is_err());
            assert_eq!(graph, before);
        }
    }

    #[test]
    fn duplicate_wire_edits_are_rejected() {
        let mut graph = build(&parse_request(SAMPLE).unwrap()).unwrap().graph;
        let before = graph.clone();
        let request = parse_request(br#"{"api_version":1,"edits":[
            {"op":"disconnect","to":"2","input":"a"},
            {"op":"connect","from":"1","to":"2","input":"a"}
        ]}"#).unwrap();
        assert!(apply(&mut graph, &request).is_err());
        assert_eq!(graph, before);
    }

    #[test]
    fn failed_edits_are_atomic() {
        let mut graph = build(&parse_request(SAMPLE).unwrap()).unwrap().graph;
        let before = graph.clone();
        let invalid = parse_request(br#"{"api_version":1,"edits":[
            {"op":"set","node":"0","input":"value","value":1},
            {"op":"connect","from":"2","to":"2","input":"a"}]}"#).unwrap();
        assert!(apply(&mut graph, &invalid).is_err());
        assert_eq!(graph, before);
        let large = parse_request(br#"{"api_version":1,"edits":[
            {"op":"set","node":"0","input":"value","value":1e99}]}"#).unwrap();
        assert!(apply(&mut graph, &large).is_err());
        assert_eq!(graph, before);
    }
}
