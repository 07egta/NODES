//! Declarative, capability-restricted node packs backed by native .morphpart definitions.
//! Packs contain data only. No DLLs, subprocesses, scripts, network, or WASM engine.
//! Registration is disabled by default. Disabling prevents new placements; existing
//! instances are embedded native cluster snapshots and remain usable.
use crate::cluster::{self, PartFile, MAX_CLUSTER_DEFS, MAX_CLUSTER_DEPTH, MAX_CLUSTER_EVAL_NODES,
    MAX_CLUSTER_NODES, MAX_PART_BYTES, PART_FORMAT, PART_VERSION};
use crate::graph_api;
use crate::model::{DataType, Graph, NodeId, NodeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const PACK_API_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: usize = 16 * 1024;
const MAX_PACKS: usize = 32;
const MAX_TOTAL_PACK_BYTES: usize = 16 * 1024 * 1024;
type Res<T> = Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u32,
    pub id: String,
    pub version: [u16; 3],
    pub name: String,
    pub description: String,
    pub description_ja: String,
    /// Inert UI icon identifier, not a path or SVG.
    pub icon: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PackPort {
    pub key: String,
    pub label: String,
    pub kind: DataType,
    pub default: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PackInfo {
    pub manifest: Manifest,
    pub enabled: bool,
    pub inputs: Vec<PackPort>,
    pub output: DataType,
}
struct Entry {
    info: PackInfo,
    part: Vec<u8>,
}
#[derive(Default)]
pub struct Registry {
    entries: BTreeMap<String, Entry>,
    bytes: usize,
}

fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b|
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
fn text_ok(s: &str, limit: usize) -> bool {
    s.len() <= limit && !s.trim().is_empty() && !s.chars().any(|c|
        c.is_control() && c != '\n' && c != '\t')
}
fn parse_manifest(bytes: &[u8]) -> Res<Manifest> {
    if bytes.len() > MAX_MANIFEST_BYTES { return Err("pack manifest too large".into()); }
    let m: Manifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if m.api_version != PACK_API_VERSION || !identifier(&m.id) || !text_ok(&m.name, 256) ||
        !text_ok(&m.description, 4096) || !text_ok(&m.description_ja, 4096) ||
        m.icon.as_ref().is_some_and(|s| !identifier(s)) {
        return Err("invalid pack manifest or unsupported API version".into());
    }
    Ok(m)
}

/// Preflight nested part expansion before calling existing recursive cluster validation.
/// Reject unknown/new executable node kinds, non-finite/out-of-range parameters and
/// any external capability. Memoize expansion count/depth to bound validation cost.
fn checked_part(bytes: &[u8]) -> Res<PartFile> {
    if bytes.len() > MAX_PART_BYTES { return Err("part exceeds 1 MiB".into()); }
    let file: PartFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if file.format != PART_FORMAT || file.version != PART_VERSION ||
        file.clusters.is_empty() || file.clusters.len() > MAX_CLUSTER_DEFS {
        return Err("unsupported part format or definition count".into());
    }
    let mut defs = BTreeMap::new();
    for def in &file.clusters {
        if defs.insert(def.id, def).is_some() || def.id == u64::MAX ||
            def.graph.nodes.is_empty() || def.graph.nodes.len() > MAX_CLUSTER_NODES ||
            !def.graph.clusters.is_empty() {
            return Err("invalid pack definition".into());
        }
        crate::annotations::validate(&def.graph)?;
        def.graph.topological_order()?;
        for node in &def.graph.nodes {
            if !node.script.is_empty() {
                return Err("pack contains script or file path".into());
            }
            if node.kind == NodeKind::Cluster {
                if node.cluster.is_none() { return Err("missing nested cluster definition".into()); }
            } else if !graph_api::permitted(node.kind) || node.cluster.is_some() {
                return Err("pack contains forbidden node kind".into());
            }
            for (key, v) in &node.params {
                graph_api::validate_numeric(node.kind, key, *v)?;
            }
        }
    }
    fn expansion(id: u64, defs: &BTreeMap<u64, &cluster::ClusterDef>,
        stack: &mut BTreeSet<u64>, memo: &mut BTreeMap<u64, (usize, usize)>) -> Res<(usize, usize)> {
        if stack.len() >= MAX_CLUSTER_DEPTH || stack.contains(&id) {
            return Err("pack cluster cycle or depth limit".into());
        }
        if let Some(&(count, depth)) = memo.get(&id) {
            if stack.len() + depth > MAX_CLUSTER_DEPTH { return Err("pack cluster depth limit".into()); }
            return Ok((count, depth));
        }
        let def = defs.get(&id).ok_or("missing nested cluster")?;
        stack.insert(id);
        let mut count = def.graph.nodes.len();
        let mut depth = 1;
        for n in def.graph.nodes.iter().filter(|n| n.kind == NodeKind::Cluster) {
            let (c,d) = expansion(n.cluster.ok_or("missing child")?, defs, stack, memo)?;
            count = count.checked_add(c).ok_or("pack expansion overflow")?;
            depth = depth.max(d + 1);
            if count > MAX_CLUSTER_EVAL_NODES { return Err("pack expansion limit".into()); }
        }
        stack.remove(&id);
        memo.insert(id, (count,depth));
        Ok((count,depth))
    }
    let mut memo = BTreeMap::new();
    expansion(file.root, &defs, &mut BTreeSet::new(), &mut memo)?;
    if memo.len() != defs.len() { return Err("pack contains unreachable definitions".into()); }
    // Original part validator also checks exposed inputs, output and graph invariants.
    cluster::parse_part(bytes)
}

fn info(m: Manifest, file: &PartFile) -> Res<PackInfo> {
    let root = file.clusters.iter().find(|d| d.id == file.root).ok_or("missing root")?;
    let mut inputs = Vec::new();
    for (i,p) in root.inputs.iter().enumerate() {
        let node = root.graph.node(p.node).ok_or("missing exposed node")?;
        let spec = node.kind.spec().inputs.iter().find(|s| s.key == p.key)
            .ok_or("missing exposed input")?;
        inputs.push(PackPort { key: format!("in{i}"), label: p.label.clone(),
            kind: spec.kind, default: cluster::port_default(root,p) });
    }
    let output = root.graph.node(root.output).ok_or("missing output")?.kind.spec().output;
    Ok(PackInfo { manifest: m, enabled: false, inputs, output })
}

impl Registry {
    /// Register or replace a pack with a strictly newer version. Updates reset enablement.
    /// Failed registration leaves old entries and enabled state unchanged.
    pub fn register(&mut self, manifest: &[u8], part: &[u8]) -> Res<()> {
        let m = parse_manifest(manifest)?;
        let previous = self.entries.get(&m.id);
        if previous.is_some_and(|p| p.info.manifest.version >= m.version) {
            return Err("duplicate or older pack version".into());
        }
        if previous.is_none() && self.entries.len() >= MAX_PACKS {
            return Err("too many registered packs".into());
        }
        let old_bytes = previous.map_or(0, |p| p.part.len());
        let total = self.bytes.saturating_sub(old_bytes).saturating_add(part.len());
        if total > MAX_TOTAL_PACK_BYTES { return Err("pack registry size limit".into()); }
        let validated = checked_part(part)?;
        let id = m.id.clone();
        let info = info(m, &validated)?;
        self.entries.insert(id, Entry { info, part: part.to_vec() });
        self.bytes = total;
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &PackInfo> {
        self.entries.values().map(|p| &p.info)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Res<()> {
        self.entries.get_mut(id).ok_or("unknown pack")?.info.enabled = enabled;
        Ok(())
    }

    /// Place an immutable native snapshot, without evaluating it or loading any code.
    pub fn instantiate(&self, id: &str, graph: &mut Graph, position: [f32;2]) -> Res<NodeId> {
        let entry = self.entries.get(id).ok_or("unknown pack")?;
        if !entry.info.enabled { return Err("pack is disabled".into()); }
        // Existing native import uses consecutive IDs; preclude wraparound on hostile graphs.
        if graph.clusters.iter().map(|d| d.id).max().unwrap_or(0) >
            u64::MAX - MAX_CLUSTER_DEFS as u64 - 1 {
            return Err("cluster ID space exhausted".into());
        }
        let mut candidate = graph.clone();
        let def = cluster::import_part(&mut candidate, &entry.part)?;
        let node = cluster::instantiate(&mut candidate, def, position)?;
        candidate.validate()?;
        *graph = candidate;
        Ok(node)
    }
}

fn read_limited(path: &Path, max: usize) -> Res<Vec<u8>> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("pack input must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take((max+1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > max { return Err("pack input exceeds size limit".into()); }
    Ok(bytes)
}

/// Console import is an explicit one-shot enablement and placement into a fresh work.
pub fn instantiate_file(manifest: &Path, part: &Path, destination: &Path) -> Res<()> {
    let mut registry = Registry::default();
    registry.register(&read_limited(manifest, MAX_MANIFEST_BYTES)?, &read_limited(part, MAX_PART_BYTES)?)?;
    let id = registry.list().next().ok_or("empty pack registry")?.manifest.id.clone();
    registry.set_enabled(&id, true)?;
    let mut graph = Graph::default();
    registry.instantiate(&id, &mut graph, [100.,100.])?;
    crate::persistence::save(destination, &crate::persistence::Project::new("Node pack",graph))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Value;
    const MANIFEST: &[u8] = include_bytes!("../examples/interop/offset-pack.json");
    const PART: &[u8] = include_bytes!("../examples/interop/offset-pack.morphpart");
    const ID: &str = "nodes.example.offset";

    #[test]
    fn explicit_enable_and_evaluate_snapshot() {
        let mut reg = Registry::default();
        reg.register(MANIFEST, PART).unwrap();
        let mut graph = Graph::default();
        assert!(!reg.list().next().unwrap().enabled);
        assert!(reg.instantiate(ID,&mut graph,[0.,0.]).is_err());
        assert!(graph.nodes.is_empty());
        reg.set_enabled(ID,true).unwrap();
        let id = reg.instantiate(ID,&mut graph,[0.,0.]).unwrap();
        reg.set_enabled(ID,false).unwrap();
        assert!(reg.instantiate(ID,&mut graph,[0.,0.]).is_err());
        let eval = crate::engine::Evaluator::default().evaluate(&graph,&||false);
        assert_eq!(eval.outputs[&id].as_ref().unwrap().branches.values().flatten().next(),
            Some(&Value::Number(12.)));
        assert_eq!(reg.list().next().unwrap().inputs[0].default, Some(7.));
    }

    #[test]
    fn malicious_or_invalid_pack_cannot_replace_existing() {
        let mut reg = Registry::default();
        reg.register(MANIFEST,PART).unwrap();
        reg.set_enabled(ID,true).unwrap();
        let update = std::str::from_utf8(MANIFEST).unwrap().replace("[1,0,0]","[2,0,0]");
        let mut file: PartFile = serde_json::from_slice(PART).unwrap();
        file.clusters[0].graph.nodes[0].kind = NodeKind::Script;
        assert!(reg.register(update.as_bytes(),&serde_json::to_vec(&file).unwrap()).is_err());
        assert!(reg.register(MANIFEST,PART).is_err());
        assert!(reg.list().next().unwrap().enabled);
        assert_eq!(reg.list().next().unwrap().manifest.version,[1,0,0]);
        let wrong = std::str::from_utf8(MANIFEST).unwrap().replace("\"api_version\":1","\"api_version\":2");
        assert!(reg.register(wrong.as_bytes(),PART).is_err());
        assert!(reg.register(update.as_bytes(),&vec![b' '; MAX_PART_BYTES+1]).is_err());
    }

    #[test]
    fn cyclic_nested_parts_are_rejected_before_recursive_validation() {
        let mut file: PartFile = serde_json::from_slice(PART).unwrap();
        let root = file.root;
        let def = &mut file.clusters[0];
        let id = def.graph.add(NodeKind::Cluster,[0.,0.]).unwrap();
        def.graph.node_mut(id).unwrap().cluster = Some(root);
        assert!(checked_part(&serde_json::to_vec(&file).unwrap()).is_err());
    }
}
