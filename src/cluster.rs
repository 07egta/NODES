//! Clusters (まとめノード): a group of nodes stored inside the work as one reusable part.
//!
//! * `ClusterDef` lives in `Graph::clusters` (saved in the `.morph` file). It owns a
//!   sub-graph, up to 8 exposed inputs (`in0`..`in7` of the cluster node) and one output node.
//! * A `NodeKind::Cluster` node points at a definition with `Node::cluster`. The engine
//!   evaluates the sub-graph with the same node code (broadcasting and limits included).
//! * Limits: nesting depth 4, no definition cycles, at most 64 definitions, 256 nodes in a
//!   definition and 4096 inner node evaluations per cluster node.
//! * `.morphpart` files hold one definition plus the definitions it needs (JSON, validated,
//!   1 MiB cap). Ids are renumbered on import so parts can move between works.
use crate::engine;
use crate::multi_port::{self, PortResults, PRIMARY_PORT};
use crate::model::*;
use crate::script::MAX_SCRIPT_BYTES;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

pub const MAX_CLUSTER_DEPTH: usize = 4;
pub const MAX_CLUSTER_INPUTS: usize = 8;
pub const MAX_CLUSTER_NODES: usize = 256;
pub const MAX_CLUSTER_DEFS: usize = 64;
pub const MAX_CLUSTER_EVAL_NODES: usize = 4_096;
pub const MAX_PART_BYTES: usize = 1024 * 1024;
pub const PART_FORMAT: &str = "morphlet-part";
pub const PART_VERSION: u32 = 1;
const MAX_NODE_ID: NodeId = 1 << 48;
const VIRTUAL_BASE: NodeId = u64::MAX - 64;

const fn slot(key: &'static str, label: &'static str) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: DataType::Any,
        default: Some(0.),
        min: -1e6,
        max: 1e6,
        step: 0.1,
        integer: false,
    }
}
static CLUSTER_INPUTS: [InputSpec; MAX_CLUSTER_INPUTS] = [
    slot("in0", "入力 1"),
    slot("in1", "入力 2"),
    slot("in2", "入力 3"),
    slot("in3", "入力 4"),
    slot("in4", "入力 5"),
    slot("in5", "入力 6"),
    slot("in6", "入力 7"),
    slot("in7", "入力 8"),
];
pub static CLUSTER_SPEC: NodeSpec = NodeSpec {
    label: "まとめノード",
    category: "データ",
    description: "複数のノードを 1 つにまとめた部品です。使う入力は定義ごとに決まります（最大 8 個）。",
    keywords: "cluster group part まとめ 部品 クラスタ グループ",
    inputs: &CLUSTER_INPUTS,
    output: DataType::Any,
};

/// An exposed input: the unconnected input `key` of inner node `node`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterPort {
    pub node: NodeId,
    pub key: String,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterDef {
    pub id: u64,
    pub name: String,
    /// Inner graph. Its own `clusters` list is always empty; nested `Cluster` nodes refer
    /// to the definitions of the work.
    pub graph: Graph,
    pub inputs: Vec<ClusterPort>,
    pub output: NodeId,
}

// ---------------------------------------------------------------- engine hooks

/// Extra cache-key text: script source, or a fingerprint of the definitions a cluster uses.
pub fn key_extra(graph: &Graph, node: &Node) -> String {
    match node.kind {
        NodeKind::Script | NodeKind::Expression | NodeKind::PathMapper => node.script.clone(),
        // Path, modified time and size: editing the file on disk recomputes the node.
        NodeKind::ImportMesh => crate::import::source_key(&node.script),
        NodeKind::Cluster => node
            .cluster
            .map(|id| signature(&graph.clusters, id))
            .unwrap_or_default(),
        _ => String::new(),
    }
}
fn signature(defs: &[ClusterDef], root: u64) -> String {
    let mut out = String::new();
    for id in closure_ids(defs, root) {
        if let Some(def) = find(defs, id) {
            out.push_str(&definition_signature(def));
        }
    }
    out
}

fn definition_signature(def: &ClusterDef) -> String {
    let mut out = serde_json::to_string(def).unwrap_or_default();
    out.push('\n');
    // The definition JSON contains paths, not external resource state.
    for node in &def.graph.nodes {
        if node.kind == NodeKind::ImportMesh && !node.off {
            let source = crate::import::source_key(&node.script);
            out.push_str(&serde_json::to_string(&source).unwrap_or_default());
            out.push('\n');
        }
    }
    out
}

/// Exact cache identities without copying complete definition JSON into every instance.
/// The token is assigned only after comparing the full definition signature (not a hash).
#[derive(Clone, PartialEq)]
pub(crate) enum CacheExtra {
    Text(String),
    Cluster(Option<Arc<[(u64, Option<u128>)]>>),
}

struct DefinitionRevision {
    signature: String,
    revision: u128,
}

/// Shared by one evaluator. A definition is serialized at most once per evaluation,
/// regardless of how often it occurs in nested clusters or on the canvas.
#[derive(Default)]
pub(crate) struct CacheKeys {
    definitions: BTreeMap<u64, DefinitionRevision>,
    checked: BTreeSet<u64>,
    roots: BTreeMap<u64, Arc<[(u64, Option<u128>)]>>,
    revision: u128,
}

impl CacheKeys {
    pub(crate) fn begin(&mut self, graph: &Graph) {
        self.checked.clear();
        self.roots.clear();
        let existing: BTreeSet<_> = graph.clusters.iter().map(|d| d.id).collect();
        self.definitions.retain(|id, _| existing.contains(id));
    }

    pub(crate) fn key_for(&mut self, graph: &Graph, node: &Node) -> CacheExtra {
        if node.kind != NodeKind::Cluster {
            return CacheExtra::Text(key_extra(graph, node));
        }
        let Some(root) = node.cluster else {
            return CacheExtra::Cluster(None);
        };
        if let Some(key) = self.roots.get(&root) {
            return CacheExtra::Cluster(Some(Arc::clone(key)));
        }
        let mut dependencies = Vec::new();
        for id in closure_ids(&graph.clusters, root) {
            if self.checked.insert(id) {
                if let Some(def) = find(&graph.clusters, id) {
                    let signature = definition_signature(def);
                    if !self.definitions.get(&id).is_some_and(|old| old.signature == signature) {
                        self.revision = self.revision.wrapping_add(1);
                        self.definitions.insert(id, DefinitionRevision {
                            signature,
                            revision: self.revision,
                        });
                    }
                } else {
                    self.definitions.remove(&id);
                }
            }
            dependencies.push((id, self.definitions.get(&id).map(|d| d.revision)));
        }
        let key: Arc<[(u64, Option<u128>)]> = dependencies.into();
        self.roots.insert(root, Arc::clone(&key));
        CacheExtra::Cluster(Some(key))
    }
}

/// Single entry point used by `Evaluator::evaluate` for every node.
pub fn evaluate_any(
    graph: &Graph,
    node: &Node,
    outputs: &BTreeMap<NodeId, Result<Tree, String>>,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    match node.kind {
        NodeKind::Script => crate::script::evaluate_node(node, outputs, cancel),
        NodeKind::Cluster => {
            let budget = Cell::new(MAX_CLUSTER_EVAL_NODES);
            let mut stack = Vec::new();
            eval_cluster_node(
                &graph.clusters,
                node,
                outputs,
                1,
                &mut stack,
                &budget,
                cancel,
            )
        }
        _ => engine::evaluate_node(node, outputs, cancel),
    }
}

/// Port-aware entry point. Legacy script/cluster APIs receive a temporary view
/// of the merged input trees; persistent endpoint IDs never become fake graph IDs.
pub(crate) fn evaluate_ports(
    graph: &Graph, node: &Node, outputs: &engine::Evaluation, cancel: &dyn Fn() -> bool,
) -> PortResults {
    let prepared = if node.port_links.is_empty() { None } else {
        match multi_port::materialize(node, outputs, cancel) {
            Ok(p) => Some(p),
            Err(e) => return multi_port::failed(node, e),
        }
    };
    let (node, data) = prepared.as_ref().map(|(n, d)| (n, d)).unwrap_or((node, &outputs.outputs));
    match node.kind {
        NodeKind::Script | NodeKind::Cluster => BTreeMap::from([(PRIMARY_PORT, evaluate_any(graph, node, data, cancel))]),
        _ => multi_port::evaluate(node, data, cancel),
    }
}

fn find(defs: &[ClusterDef], id: u64) -> Option<&ClusterDef> {
    defs.iter().find(|d| d.id == id)
}

/// Default value of an exposed input if it is a number slot (None for geometry slots).
pub fn port_default(def: &ClusterDef, port: &ClusterPort) -> Option<f64> {
    let node = def.graph.node(port.node)?;
    let spec = node.input_spec(&port.key)?;
    let default = spec.default?;
    Some(node.params.get(&port.key).copied().unwrap_or(default))
}

fn eval_cluster_node(
    defs: &[ClusterDef],
    node: &Node,
    outputs: &BTreeMap<NodeId, Result<Tree, String>>,
    depth: usize,
    stack: &mut Vec<u64>,
    budget: &Cell<usize>,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    if depth > MAX_CLUSTER_DEPTH {
        return Err(format!(
            "まとめノードの入れ子が深すぎます（上限 {MAX_CLUSTER_DEPTH} 段）。"
        ));
    }
    let id = node
        .cluster
        .ok_or("このまとめノードには定義がありません。")?;
    let def = find(defs, id).ok_or("まとめノードの定義が見つかりません。")?;
    if stack.contains(&id) {
        return Err("まとめノードが自分自身を含んでいます（循環）。".into());
    }
    let mut slots = Vec::with_capacity(def.inputs.len());
    for (k, port) in def.inputs.iter().enumerate() {
        let key = format!("in{k}");
        let tree = if let Some(source) = node.inputs.get(&key) {
            match outputs.get(source) {
                Some(Ok(t)) => t.clone(),
                Some(Err(_)) => {
                    return Err(format!(
                        "「{}」の入力元にエラーがあります。上流の赤いノードを確認してください。",
                        port.label
                    ));
                }
                None => return Err(format!("「{}」の計算結果がありません。", port.label)),
            }
        } else if let Some(default) = port_default(def, port) {
            Tree::single(Value::Number(
                node.params.get(&key).copied().unwrap_or(default),
            ))
        } else {
            return Err(format!("「{}」を接続してください。", port.label));
        };
        slots.push(tree);
    }
    stack.push(id);
    let result = run_def(defs, def, slots, depth, stack, budget, cancel);
    stack.pop();
    result
}

fn run_def(
    defs: &[ClusterDef],
    def: &ClusterDef,
    slots: Vec<Tree>,
    depth: usize,
    stack: &mut Vec<u64>,
    budget: &Cell<usize>,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    let order = def.graph.topological_nodes()?;
    // Exposed inputs are fed through "virtual" source ids so lists/trees keep working.
    let mut local = engine::Evaluation::default();
    for (k, tree) in slots.into_iter().enumerate() {
        local.outputs.insert(VIRTUAL_BASE + k as u64, Ok(tree));
    }
    let mut weight = 0usize;
    for original in order {
        if cancel() {
            return Err("計算を停止しました。".into());
        }
        let left = budget.get();
        if left == 0 {
            return Err(format!(
                "まとめノードの計算量が上限（内部ノード {MAX_CLUSTER_EVAL_NODES} 個分）を超えました。"
            ));
        }
        budget.set(left - 1);
        let id = original.id;
        // Match the top-level evaluator: disabled nodes produce an empty tree
        // without evaluating scripts, geometry, or nested clusters.
        if original.off {
            local.put_component(id, multi_port::empty(original));
            continue;
        }
        let mut node = original.clone();
        for (k, port) in def.inputs.iter().enumerate() {
            if port.node == id {
                node.inputs
                    .insert(port.key.clone(), VIRTUAL_BASE + k as u64);
            }
        }
        let prepared = if node.port_links.is_empty() { None } else {
            Some(multi_port::materialize(&node, &local, cancel)?)
        };
        let (node, inputs) = prepared.as_ref().map(|(n, d)| (n, d)).unwrap_or((&node, &local.outputs));
        let results = match node.kind {
            NodeKind::Script => BTreeMap::from([(PRIMARY_PORT, crate::script::evaluate_node(node, inputs, cancel))]),
            NodeKind::Cluster => BTreeMap::from([(PRIMARY_PORT, eval_cluster_node(defs, node, inputs, depth + 1, stack, budget, cancel))]),
            _ => multi_port::evaluate(node, inputs, cancel),
        };
        if let Some(Err(e)) = results.get(&PRIMARY_PORT) {
            return Err(format!("部品「{}」の「{}」: {e}", def.name, node.name));
        }
        weight = weight.saturating_add(multi_port::result_weight(&results));
        if weight > MAX_TOTAL_WEIGHT {
            return Err("まとめノードの中間結果が大きすぎます。".into());
        }
        local.put_component(id, results);
    }
    match local.outputs.remove(&def.output) {
        Some(result) => result,
        None => Err("まとめノードの出力ノードが見つかりません。".into()),
    }
}

// ---------------------------------------------------------------- UI helpers

/// Inputs of `node` that are real: a cluster node only has as many as its definition.
pub fn real_input_count(graph: &Graph, node: &Node) -> usize {
    match node.kind {
        NodeKind::Cluster => node
            .cluster
            .and_then(|id| find(&graph.clusters, id))
            .map_or(0, |def| def.inputs.len()),
        _ => node.input_ports().len(),
    }
}
pub fn input_label(graph: &Graph, node: &Node, index: usize, spec: &InputSpec) -> String {
    if node.kind == NodeKind::Cluster
        && let Some(def) = node.cluster.and_then(|id| find(&graph.clusters, id))
        && let Some(port) = def.inputs.get(index)
    {
        return port.label.clone();
    }
    node.input_name(spec).to_string()
}
/// The Script node's shape input `g` may stay unconnected.
pub fn input_is_optional(node: &Node, spec: &InputSpec) -> bool {
    (node.kind == NodeKind::Script && spec.key == "g") || multi_port::optional_input(node.kind,spec.key)
}
/// True when an unconnected input cannot use a slider value (a shape must be connected).
pub fn input_needs_link(graph: &Graph, node: &Node, index: usize, spec: &InputSpec) -> bool {
    if spec.default.is_none() {
        return !input_is_optional(node, spec);
    }
    if node.kind == NodeKind::Cluster
        && let Some(def) = node.cluster.and_then(|id| find(&graph.clusters, id))
        && let Some(port) = def.inputs.get(index)
    {
        return port_default(def, port).is_none();
    }
    false
}

// ---------------------------------------------------------------- validation

pub fn validate_graph(graph: &Graph) -> Result<(), String> {
    let defs = &graph.clusters;
    if defs.len() > MAX_CLUSTER_DEFS {
        return Err(format!(
            "まとめノードの定義は {MAX_CLUSTER_DEFS} 個までです。"
        ));
    }
    let mut ids = BTreeSet::new();
    for def in defs {
        if !ids.insert(def.id) {
            return Err("まとめノードの定義 ID が重複しています。".into());
        }
        validate_def(def)?;
    }
    check_nodes(&graph.nodes, defs)?;
    let mut depths = BTreeMap::new();
    for def in defs {
        check_nodes(&def.graph.nodes, defs)?;
        def_depth_cached(defs, def.id, &mut Vec::new(), &mut depths)?;
    }
    Ok(())
}
fn check_nodes(nodes: &[Node], defs: &[ClusterDef]) -> Result<(), String> {
    for node in nodes {
        if node.script.len() > MAX_SCRIPT_BYTES {
            return Err(format!(
                "「{}」のスクリプトが長すぎます（上限 {MAX_SCRIPT_BYTES} バイト）。",
                node.name
            ));
        }
        if node.kind == NodeKind::Cluster {
            let id = node
                .cluster
                .ok_or_else(|| format!("「{}」に部品の定義がありません。", node.name))?;
            if find(defs, id).is_none() {
                return Err(format!("「{}」の部品の定義が見つかりません。", node.name));
            }
        }
    }
    Ok(())
}
fn validate_def(def: &ClusterDef) -> Result<(), String> {
    let context = |e: String| format!("部品「{}」: {e}", def.name);
    if def.name.len() > 512 || def.name.chars().any(|c| c == '\0') {
        return Err(context("名前が長すぎるか、使えない文字があります。".into()));
    }
    if !def.graph.clusters.is_empty() {
        return Err(context("内部のグラフに定義を入れ子にできません。".into()));
    }
    let nodes = &def.graph.nodes;
    if nodes.is_empty() || nodes.len() > MAX_CLUSTER_NODES {
        return Err(context(format!(
            "ノード数は 1〜{MAX_CLUSTER_NODES} 個にしてください。"
        )));
    }
    if def.graph.next_id > MAX_NODE_ID || nodes.iter().any(|n| n.id >= MAX_NODE_ID) {
        return Err(context("ノード ID が大きすぎます。".into()));
    }
    def.graph.topological_order().map_err(context)?;
    if def.graph.node(def.output).is_none() {
        return Err(context("出力ノードが見つかりません。".into()));
    }
    if def.inputs.len() > MAX_CLUSTER_INPUTS {
        return Err(context(format!(
            "公開する入力は {MAX_CLUSTER_INPUTS} 個までです。"
        )));
    }
    let mut seen = BTreeSet::new();
    for port in &def.inputs {
        let node = def
            .graph
            .node(port.node)
            .ok_or_else(|| context("公開する入力のノードが見つかりません。".into()))?;
        let exists = node.input_spec(&port.key).is_some();
        if !exists
            || node.has_link(&port.key)
            || !seen.insert((port.node, port.key.clone()))
            || port.label.len() > 512
        {
            return Err(context("公開する入力の指定が正しくありません。".into()));
        }
    }
    Ok(())
}
/// Nesting depth of a definition (1 for a flat one). Errors on cycles, missing definitions
/// and anything deeper than `MAX_CLUSTER_DEPTH`.
pub fn def_depth(defs: &[ClusterDef], id: u64, stack: &mut Vec<u64>) -> Result<usize, String> {
    def_depth_cached(defs, id, stack, &mut BTreeMap::new())
}

fn def_depth_cached(
    defs: &[ClusterDef],
    id: u64,
    stack: &mut Vec<u64>,
    memo: &mut BTreeMap<u64, usize>,
) -> Result<usize, String> {
    let too_deep = || format!(
        "まとめノードの入れ子が深すぎます（上限 {MAX_CLUSTER_DEPTH} 段）。"
    );
    if stack.contains(&id) {
        return Err("まとめノードの定義が循環しています。".into());
    }
    // Reject over-deep hostile definitions before descending, not while unwinding.
    if stack.len() >= MAX_CLUSTER_DEPTH {
        return Err(too_deep());
    }
    if let Some(&depth) = memo.get(&id) {
        if stack.len() + depth > MAX_CLUSTER_DEPTH {
            return Err(too_deep());
        }
        return Ok(depth);
    }
    let def = find(defs, id).ok_or("まとめノードの定義が見つかりません。")?;
    stack.push(id);
    let result = (|| -> Result<usize, String> {
        let mut deepest = 0;
        for node in def.graph.nodes.iter().filter(|n| n.kind == NodeKind::Cluster) {
            let child = node.cluster.ok_or("まとめノードの定義がありません。")?;
            deepest = deepest.max(def_depth_cached(defs, child, stack, memo)?);
        }
        Ok(deepest + 1)
    })();
    stack.pop();
    let depth = result?;
    memo.insert(id, depth);
    Ok(depth)
}
/// `root` and every definition it uses, root first. Cycle-safe.
fn closure_ids(defs: &[ClusterDef], root: u64) -> Vec<u64> {
    let mut out = vec![root];
    let mut index = 0;
    while let Some(id) = out.get(index).copied() {
        index += 1;
        if let Some(def) = find(defs, id) {
            for node in def
                .graph
                .nodes
                .iter()
                .filter(|n| n.kind == NodeKind::Cluster)
            {
                if let Some(child) = node.cluster
                    && !out.contains(&child)
                    && out.len() <= MAX_CLUSTER_DEFS
                {
                    out.push(child);
                }
            }
        }
    }
    out
}
fn next_def_id(defs: &[ClusterDef]) -> Result<u64, String> {
    match defs.iter().map(|d| d.id).max() {
        Some(id) => id.checked_add(1).ok_or_else(|| "部品の ID をこれ以上追加できません。".into()),
        None => Ok(1),
    }
}

// ---------------------------------------------------------------- commands

/// 「まとめる」: replace `ids` by one cluster node. On any error the graph is unchanged.
///
/// * The output is the node that is not used by another selected node (or the only one used
///   outside the selection). Other selected nodes must not be used outside the selection.
/// * Exposed inputs: every input connected from outside or needing a shape connection, plus
///   unconnected number inputs while there is room (8 in total). Numbers that do not fit stay
///   inside with their current values.
pub fn group_nodes(graph: &mut Graph, ids: &[NodeId], name: &str) -> Result<NodeId, String> {
    let backup = graph.clone();
    let result = group_inner(graph, ids, name);
    if result.is_err() {
        *graph = backup;
    }
    result
}

fn group_inner(graph: &mut Graph, ids: &[NodeId], name: &str) -> Result<NodeId, String> {
    let set: BTreeSet<NodeId> = ids.iter().copied().collect();
    if set.is_empty() {
        return Err("まとめるノードを選んでください。".into());
    }
    if set.len() > MAX_CLUSTER_NODES {
        return Err(format!(
            "まとめられるのは {MAX_CLUSTER_NODES} ノードまでです。"
        ));
    }
    if set.iter().any(|id| graph.node(*id).is_none()) {
        return Err("選択したノードが見つかりません。".into());
    }
    if graph.clusters.len() >= MAX_CLUSTER_DEFS {
        return Err(format!("部品の定義は {MAX_CLUSTER_DEFS} 個までです。"));
    }
    // The current exposed cluster interface has a single output. Never collapse
    // a selected multi-output node or a new-style boundary wire into a legacy one.
    if graph.nodes.iter().any(|n| {
        (set.contains(&n.id) && (n.output_ports().len() != 1 || n.kind == NodeKind::StreamMerge || !n.port_links.is_empty()))
        || n.port_links.values().flatten().any(|s| set.contains(&s.node))
    }) {
        return Err("複数ポートを含む選択はまだ部品化できません。配線を保持するため中止しました。".into());
    }
    let name: String = name.trim().chars().take(64).collect();
    let name = if name.is_empty() {
        "部品".to_string()
    } else {
        name
    };

    let mut internal = BTreeSet::new();
    let mut external = BTreeSet::new();
    for n in &graph.nodes {
        for source in n.inputs.values().filter(|s| set.contains(*s)) {
            if set.contains(&n.id) {
                internal.insert(*source);
            } else {
                external.insert(*source);
            }
        }
    }
    let sinks: Vec<NodeId> = set
        .iter()
        .filter(|id| !internal.contains(*id))
        .copied()
        .collect();
    let output = if let [only] = sinks.as_slice() {
        *only
    } else {
        let used_outside: Vec<_> = sinks.iter().filter(|id| external.contains(*id)).collect();
        let outputs_kind: Vec<_> = sinks
            .iter()
            .filter(|id| graph.node(**id).is_some_and(|n| n.kind == NodeKind::Output))
            .collect();
        if let [only] = used_outside.as_slice() {
            **only
        } else if let [only] = outputs_kind.as_slice() {
            **only
        } else {
            return Err(format!(
                "出力にするノードを決められません（行き先のないノードが {} 個あります）。最後のノードだけが残るように選び直してください。",
                sinks.len()
            ));
        }
    };
    if external.iter().any(|id| *id != output) {
        return Err(
            "選択したノードの途中の結果が、選択外のノードにも使われています。出力以外は外へつながないようにしてください。"
                .into(),
        );
    }

    // Exposed inputs, in node/spec order.
    struct Candidate {
        node: NodeId,
        key: String,
        label: String,
        required: bool,
    }
    let mut candidates = Vec::new();
    for id in &set {
        let Some(node) = graph.node(*id) else {
            continue;
        };
        let real = real_input_count(graph, node);
        for (index, spec) in node.kind.spec().inputs.iter().enumerate().take(real) {
            if node.inputs.get(spec.key).is_some_and(|s| set.contains(s)) {
                continue;
            }
            if input_is_optional(node, spec) && !node.inputs.contains_key(spec.key) {
                continue;
            }
            let required =
                node.inputs.contains_key(spec.key) || input_needs_link(graph, node, index, spec);
            candidates.push(Candidate {
                node: *id,
                key: spec.key.to_string(),
                label: format!("{}・{}", node.name, input_label(graph, node, index, spec)),
                required,
            });
        }
    }
    let required = candidates.iter().filter(|c| c.required).count();
    if required > MAX_CLUSTER_INPUTS {
        return Err(format!(
            "外から必要な入力が {required} 個あります。公開できる入力は {MAX_CLUSTER_INPUTS} 個までなので、範囲を小さくしてください。"
        ));
    }
    let mut room = MAX_CLUSTER_INPUTS - required;
    let mut ports = Vec::new();
    for c in candidates {
        if c.required || room > 0 {
            if !c.required {
                room -= 1;
            }
            ports.push(ClusterPort {
                node: c.node,
                key: c.key,
                label: c.label.chars().take(60).collect(),
            });
        }
    }

    let mut inner = Graph::default();
    for id in &set {
        if let Some(original) = graph.node(*id) {
            let mut node = original.clone();
            node.inputs.retain(|_, source| set.contains(&*source));
            inner.nodes.push(node);
        }
    }
    inner.next_id = inner.nodes.iter().map(|n| n.id).max().unwrap_or(0)
        .checked_add(1).ok_or("ノード ID をこれ以上追加できません。")?;
    let def = ClusterDef {
        id: next_def_id(&graph.clusters)?,
        name: name.clone(),
        graph: inner,
        inputs: ports,
        output,
    };
    validate_def(&def)?;
    let mut with_new = graph.clusters.clone();
    with_new.push(def.clone());
    def_depth(&with_new, def.id, &mut Vec::new())?;

    // Rewire. Remember what feeds each exposed slot before the nodes disappear.
    let mut links = Vec::new();
    let mut numbers = Vec::new();
    for (k, port) in def.inputs.iter().enumerate() {
        let key = format!("in{k}");
        match graph.node(port.node).and_then(|n| n.inputs.get(&port.key)) {
            Some(source) => links.push((key, *source)),
            None => {
                if let Some(v) = port_default(&def, port) {
                    numbers.push((key, v));
                }
            }
        }
    }
    let consumers: Vec<(NodeId, String)> = graph
        .nodes
        .iter()
        .filter(|n| !set.contains(&n.id))
        .flat_map(|n| {
            n.inputs
                .iter()
                .filter(|(_, s)| **s == output)
                .map(|(k, _)| (n.id, k.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    let count = set.len() as f32;
    let position = graph
        .nodes
        .iter()
        .filter(|n| set.contains(&n.id))
        .fold([0f32; 2], |a, n| {
            [a[0] + n.position[0] / count, a[1] + n.position[1] / count]
        });
    graph.nodes.retain(|n| !set.contains(&n.id));
    let new_id = graph.add(NodeKind::Cluster, position)?;
    let def_id = def.id;
    graph.clusters.push(def);
    if let Some(node) = graph.node_mut(new_id) {
        node.name = name;
        node.cluster = Some(def_id);
        for (key, source) in links {
            node.inputs.insert(key, source);
        }
        for (key, value) in numbers {
            node.params.insert(key, value);
        }
    }
    for (consumer, key) in consumers {
        if let Some(node) = graph.node_mut(consumer) {
            node.inputs.insert(key, new_id);
        }
    }
    graph.validate().map_err(|e| {
        format!("まとめると配線が循環するなど、成り立たなくなります。間のノードも一緒に選んでください。（{e}）")
    })?;
    Ok(new_id)
}

/// Add a cluster node for an existing definition (e.g. an imported part).
pub fn instantiate(graph: &mut Graph, def_id: u64, position: [f32; 2]) -> Result<NodeId, String> {
    let def = find(&graph.clusters, def_id)
        .ok_or("部品の定義が見つかりません。")?
        .clone();
    let id = graph.add(NodeKind::Cluster, position)?;
    if let Some(node) = graph.node_mut(id) {
        node.name = def.name.clone();
        node.cluster = Some(def_id);
        for (k, port) in def.inputs.iter().enumerate() {
            if let Some(v) = port_default(&def, port) {
                node.params.insert(format!("in{k}"), v);
            }
        }
    }
    Ok(id)
}

// ---------------------------------------------------------------- .morphpart

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartFile {
    pub format: String,
    pub version: u32,
    pub root: u64,
    pub clusters: Vec<ClusterDef>,
}

/// JSON text of a part: the definition and every definition it uses.
pub fn export_part(graph: &Graph, def_id: u64) -> Result<String, String> {
    if find(&graph.clusters, def_id).is_none() {
        return Err("書き出す部品が見つかりません。".into());
    }
    let clusters: Vec<ClusterDef> = closure_ids(&graph.clusters, def_id)
        .into_iter()
        .filter_map(|id| find(&graph.clusters, id).cloned())
        .collect();
    let file = PartFile {
        format: PART_FORMAT.into(),
        version: PART_VERSION,
        root: def_id,
        clusters,
    };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("部品を保存形式に変換できませんでした: {e}"))?;
    text.push('\n');
    if text.len() > MAX_PART_BYTES {
        return Err("部品のファイルが大きすぎます（上限 1 MB）。".into());
    }
    // Reject malformed in-memory definitions rather than exporting an unreadable part.
    parse_part(text.as_bytes())?;
    Ok(text)
}

/// Parse and fully validate part bytes without touching any work.
pub fn parse_part(bytes: &[u8]) -> Result<PartFile, String> {
    if bytes.len() > MAX_PART_BYTES {
        return Err("部品ファイルが大きすぎます。上限は 1 MB です。".into());
    }
    let file: PartFile = serde_json::from_slice(bytes).map_err(|e| {
        let detail: String = e.to_string().chars().take(300).collect();
        format!(
            "部品の内容を読み取れませんでした（{} 行目）: {detail}",
            e.line()
        )
    })?;
    if file.format != PART_FORMAT {
        return Err("これは Morphlet の部品ファイルではありません。".into());
    }
    if file.version != PART_VERSION {
        return Err(format!(
            "この部品は形式 v{} です。このアプリで読める形式は v{PART_VERSION} です。",
            file.version
        ));
    }
    if find(&file.clusters, file.root).is_none() {
        return Err("部品の本体が見つかりません。".into());
    }
    let check = Graph {
        nodes: Vec::new(),
        next_id: 0,
        clusters: file.clusters.clone(),
        ..Graph::default()
    };
    validate_graph(&check)?;
    Ok(file)
}

/// Add the part's definitions to `graph`; returns the new id of the root definition.
/// Use `instantiate` afterwards to place a node. The graph is unchanged on error.
pub fn import_part(graph: &mut Graph, bytes: &[u8]) -> Result<u64, String> {
    let file = parse_part(bytes)?;
    if graph.clusters.len() + file.clusters.len() > MAX_CLUSTER_DEFS {
        return Err(format!("部品の定義は合計 {MAX_CLUSTER_DEFS} 個までです。"));
    }
    let base = next_def_id(&graph.clusters)?;
    // Check the full contiguous allocation before any additions or mutations.
    base.checked_add(file.clusters.len().saturating_sub(1) as u64)
        .ok_or("部品の ID をこれ以上追加できません。")?;
    let map: BTreeMap<u64, u64> = file
        .clusters
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id, base + i as u64))
        .collect();
    let mut incoming = file.clusters;
    for def in &mut incoming {
        def.id = *map.get(&def.id).ok_or("部品の ID を対応づけられません。")?;
        for node in def
            .graph
            .nodes
            .iter_mut()
            .filter(|n| n.kind == NodeKind::Cluster)
        {
            node.cluster = node.cluster.and_then(|old| map.get(&old).copied());
        }
    }
    let root = *map.get(&file.root).ok_or("部品の本体が見つかりません。")?;
    let backup = graph.clone();
    graph.clusters.extend(incoming);
    if let Err(e) = graph.validate() {
        *graph = backup;
        return Err(e);
    }
    Ok(root)
}

pub fn save_part(path: &Path, graph: &Graph, def_id: u64) -> Result<(), String> {
    let text = export_part(graph, def_id)?;
    crate::persistence::atomic_write(path, |file| {
        use std::io::Write;
        file.write_all(text.as_bytes())
    })
}

pub fn load_part(path: &Path, graph: &mut Graph) -> Result<u64, String> {
    let file = File::open(path).map_err(|e| format!("部品を開けませんでした: {e}"))?;
    let mut bytes = Vec::new();
    file.take((MAX_PART_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("部品を読み込めませんでした: {e}"))?;
    import_part(graph, &bytes)
}
