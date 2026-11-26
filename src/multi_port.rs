//! Stable port identities and native multi-output components.
//! A wire refers to (node ID, output ID); input IDs are stable string keys.
//! This is a NODES-native implementation, not a Rhino/Grasshopper SDK dependency.
use crate::data_tree as dt;
use crate::engine::{self, Evaluation};
use crate::model::{g, i, DataType, Graph, InputSpec, Node, NodeId, NodeKind, NodeSpec, Tree, Value, MAX_ITEMS, MAX_TOTAL_WEIGHT};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type PortId = u32;
pub const PRIMARY_PORT: PortId = 0;
pub const MAX_WIRES_PER_INPUT: usize = 64;
pub const MAX_DYNAMIC_PORTS: usize = 32;
const MAX_GATE_ID: u32 = 65536;
pub type PortResults = BTreeMap<PortId, Result<Tree, String>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortRef {
    pub node: NodeId,
    pub port: PortId,
}
impl PortRef {
    pub const fn primary(node: NodeId) -> Self { Self { node, port: PRIMARY_PORT } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access { Item, List, Tree, Custom }
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutputPort {
    pub id: PortId,
    pub label: String,
    pub kind: DataType,
    pub access: Access,
}

/// Layout is optional for static components. High-water marks prevent reuse of
/// a removed variable port ID during the life of a document (including reload).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PortLayout {
    pub input_order: Vec<String>,
    pub output_order: Vec<PortId>,
    pub input_labels: BTreeMap<String, String>,
    pub output_labels: BTreeMap<PortId, String>,
    pub next_input: usize,
    pub next_output: PortId,
}
impl PortLayout {
    pub fn is_default(&self) -> bool { self == &Self::default() }
}

static MERGE_BANK: [InputSpec; MAX_DYNAMIC_PORTS] = [
    g("in0", "入力 1", DataType::Any),
    g("in1", "入力 2", DataType::Any),
    g("in2", "入力 3", DataType::Any),
    g("in3", "入力 4", DataType::Any),
    g("in4", "入力 5", DataType::Any),
    g("in5", "入力 6", DataType::Any),
    g("in6", "入力 7", DataType::Any),
    g("in7", "入力 8", DataType::Any),
    g("in8", "入力 9", DataType::Any),
    g("in9", "入力 10", DataType::Any),
    g("in10", "入力 11", DataType::Any),
    g("in11", "入力 12", DataType::Any),
    g("in12", "入力 13", DataType::Any),
    g("in13", "入力 14", DataType::Any),
    g("in14", "入力 15", DataType::Any),
    g("in15", "入力 16", DataType::Any),
    g("in16", "入力 17", DataType::Any),
    g("in17", "入力 18", DataType::Any),
    g("in18", "入力 19", DataType::Any),
    g("in19", "入力 20", DataType::Any),
    g("in20", "入力 21", DataType::Any),
    g("in21", "入力 22", DataType::Any),
    g("in22", "入力 23", DataType::Any),
    g("in23", "入力 24", DataType::Any),
    g("in24", "入力 25", DataType::Any),
    g("in25", "入力 26", DataType::Any),
    g("in26", "入力 27", DataType::Any),
    g("in27", "入力 28", DataType::Any),
    g("in28", "入力 29", DataType::Any),
    g("in29", "入力 30", DataType::Any),
    g("in30", "入力 31", DataType::Any),
    g("in31", "入力 32", DataType::Any),
];
static MERGE_DEFAULT: [InputSpec; 2] = [
    g("in0", "入力 1", DataType::Any), g("in1", "入力 2", DataType::Any)
];

macro_rules! component {
    ($label:literal,$category:literal,$desc:literal,$keywords:literal,$out:ident,[$($input:expr),*]) => {{
        static SPEC: NodeSpec = NodeSpec {
            label: $label, category: $category, description: $desc, keywords: $keywords,
            inputs: &[$($input),*], output: DataType::$out,
        };
        &SPEC
    }};
}
pub(crate) fn spec(kind: NodeKind) -> &'static NodeSpec {
    use NodeKind::*;
    match kind {
        DeconstructPoint => component!("点を分解","曲線","点のXYZを独立した数値ポートに出力。","deconstruct point xyz",Number,[g("point","点",DataType::Point)]),
        DeconstructVector => component!("ベクトルを分解","数値","NODESのXYZ点型ベクトルを数値に分解。","deconstruct vector xyz",Number,[g("vector","ベクトル",DataType::Point)]),
        DeconstructDomain => component!("区間を分解","数値","各枝の始点・終点の2数値を別々に出力。","deconstruct domain interval",Number,[g("domain","区間",DataType::Number)]),
        DeconstructMesh => component!("メッシュを分解","メッシュ","頂点リストと三角形頂点番号列。複数メッシュは別枝。","deconstruct mesh vertices faces",Point,[g("mesh","メッシュ",DataType::Mesh)]),
        Dispatch => component!("リストを振り分け","データ","0は偽、その他の有限数値は真。パターンを繰り返して2リストに分割。","dispatch pattern true false",Any,[g("list","リスト",DataType::Any),g("pattern","パターン",DataType::Number)]),
        SortListPaired => component!("対応リストを整列","データ","数値キーの安定昇順に対応する値を並べ替え。値は任意。","sort list paired keys values",Number,[g("keys","キー",DataType::Number),g("values","値",DataType::Any)]),
        StreamGate => component!("出力を振り分け","データ","0始まりの出力位置を選択してツリーを保持したまま転送。","stream gate route",Any,[i("gate","出力位置",0.,0.,31.),g("stream","データ",DataType::Any)]),
        StreamMerge => {
            static SPEC: NodeSpec = NodeSpec { label: "ストリーム結合", category: "データ",
                description: "可変入力のデータツリーを表示順に結合。未接続入力は空。",
                keywords: "stream merge variable input", inputs: &MERGE_DEFAULT, output: DataType::Any };
            &SPEC
        }
        _ => unreachable!("not a native multi-port kind"),
    }
}

pub fn optional_input(kind: NodeKind, key: &str) -> bool {
    kind == NodeKind::StreamMerge || (kind == NodeKind::SortListPaired && key == "values")
}
pub fn input_access(kind: NodeKind, index: usize) -> Access {
    match crate::data_tree_nodes::access_label(kind,index) {
        "List" => Access::List, "Tree" => Access::Tree, "Custom" => Access::Custom, _ => Access::Item
    }
}
fn declared_outputs(kind: NodeKind) -> Vec<OutputPort> {
    use Access::*;
    use DataType::*;
    use NodeKind as K;
    let raw: &[(PortId, &str, DataType, Access)] = match kind {
        K::DeconstructPoint | K::DeconstructVector => &[(0,"X",Number,Item),(1,"Y",Number,Item),(2,"Z",Number,Item)],
        K::DeconstructDomain => &[(0,"開始",Number,Item),(1,"終了",Number,Item)],
        K::CurveEvaluate => &[(0,"点",Point,Item),(1,"単位接線",Point,Item)],
        K::EvaluateSurface => &[(0,"点",Point,Item),(1,"法線",Point,Item),(2,"U微分",Point,Item),(3,"V微分",Point,Item)],
        K::DeconstructMesh => &[(0,"頂点",Point,List),(1,"面の頂点番号",Number,List)],
        K::Dispatch => &[(0,"真 A",Any,List),(1,"偽 B",Any,List)],
        K::SortListPaired => &[(0,"キー K",Number,List),(1,"値 A",Any,List)],
        K::StreamGate => &[(0,"出力 0",Any,Tree),(1,"出力 1",Any,Tree)],
        K::StreamMerge => &[(0,"結合",Any,Tree)],
        _ => return vec![OutputPort { id: PRIMARY_PORT, label: "出力".into(), kind: kind.spec().output,
            access: Custom }],
    };
    raw.iter().map(|&(id,label,kind,access)| OutputPort { id, label: label.into(), kind, access }).collect()
}

impl Node {
    pub fn sources(&self, key: &str) -> Vec<PortRef> {
        self.port_links.get(key).cloned().unwrap_or_else(|| self.inputs.get(key)
            .map(|&n| vec![PortRef::primary(n)]).unwrap_or_default())
    }
    pub fn has_link(&self, key: &str) -> bool {
        self.inputs.contains_key(key) || self.port_links.contains_key(key)
    }
    pub fn all_links(&self) -> BTreeMap<String, Vec<PortRef>> {
        let mut links: BTreeMap<_,_> = self.inputs.iter().map(|(k,&n)| (k.clone(),vec![PortRef::primary(n)])).collect();
        links.extend(self.port_links.iter().map(|(k,v)| (k.clone(),v.clone())));
        links
    }
    pub(crate) fn set_sources(&mut self, key: &str, sources: Vec<PortRef>) {
        self.inputs.remove(key);
        self.port_links.remove(key);
        match sources.as_slice() {
            [] => {}
            [p] if p.port == PRIMARY_PORT => { self.inputs.insert(key.into(),p.node); }
            _ => { self.port_links.insert(key.into(),sources); }
        }
    }
    /// Display order. Fixed evaluators use their declaration order instead.
    pub fn input_ports(&self) -> Vec<&'static InputSpec> {
        if self.port_layout.input_order.is_empty() { return self.kind.spec().inputs.iter().collect(); }
        let bank = if self.kind == NodeKind::StreamMerge { &MERGE_BANK[..] } else { self.kind.spec().inputs };
        self.port_layout.input_order.iter().filter_map(|k| bank.iter().find(|p| p.key == k)).collect()
    }
    pub fn evaluation_inputs(&self) -> Vec<&'static InputSpec> {
        if self.kind == NodeKind::StreamMerge { self.input_ports() }
        else { self.kind.spec().inputs.iter().collect() }
    }
    pub fn input_spec(&self, key: &str) -> Option<&'static InputSpec> {
        self.input_ports().into_iter().find(|p| p.key == key)
    }
    pub fn input_name<'a>(&'a self, port: &InputSpec) -> &'a str {
        self.port_layout.input_labels.get(port.key).map(String::as_str).unwrap_or(port.label)
    }
    pub fn output_ports(&self) -> Vec<OutputPort> {
        let mut ports = declared_outputs(self.kind);
        if !self.port_layout.output_order.is_empty() {
            ports = self.port_layout.output_order.iter().filter_map(|&id| {
                declared_outputs(self.kind).into_iter().find(|p| p.id == id)
                    .or_else(|| (self.kind == NodeKind::StreamGate).then(|| OutputPort { id, label: format!("出力 {id}"), kind: DataType::Any, access: Access::Tree }))
            }).collect();
        }
        for p in &mut ports {
            if let Some(label) = self.port_layout.output_labels.get(&p.id) { p.label = label.clone(); }
        }
        ports
    }
    pub fn output_port(&self, id: PortId) -> Option<OutputPort> {
        self.output_ports().into_iter().find(|p| p.id == id)
    }
    pub(crate) fn validate_ports(&self) -> Result<(), String> {
        let layout = &self.port_layout;
        let input_ids: BTreeSet<_> = self.input_ports().iter().map(|p| p.key).collect();
        let default_inputs: BTreeSet<_> = self.kind.spec().inputs.iter().map(|p| p.key).collect();
        if !layout.input_order.is_empty() && (input_ids.len() != layout.input_order.len() ||
            input_ids.is_empty() || input_ids.len() > MAX_DYNAMIC_PORTS) {
            return Err("入力ポート ID が無効です。".into());
        }
        if self.kind == NodeKind::StreamMerge {
            let next = layout.next_input.max(2);
            if next > MAX_DYNAMIC_PORTS || layout.next_input == 1 ||
                input_ids.iter().any(|key| !MERGE_BANK[..next].iter().any(|p| p.key == *key)) {
                return Err("可変入力の ID が無効です。".into());
            }
        } else if input_ids != default_inputs || layout.next_input != 0 {
            return Err("固定入力の変更が無効です。".into());
        }
        let outputs: BTreeSet<_> = self.output_ports().iter().map(|p| p.id).collect();
        let default_outputs: BTreeSet<_> = declared_outputs(self.kind).iter().map(|p| p.id).collect();
        if !outputs.contains(&PRIMARY_PORT) || outputs.len() > MAX_DYNAMIC_PORTS ||
            (!layout.output_order.is_empty() && outputs.len() != layout.output_order.len()) {
            return Err("出力ポート ID が無効です。".into());
        }
        if self.kind == NodeKind::StreamGate {
            let next = layout.next_output.max(2);
            if next > MAX_GATE_ID || layout.next_output == 1 || outputs.iter().any(|&p| p >= next) {
                return Err("可変出力の ID が無効です。".into());
            }
        } else if outputs != default_outputs || layout.next_output != 0 {
            return Err("固定出力の変更が無効です。".into());
        }
        let valid_name = |s: &String| !s.trim().is_empty() && s.len() <= 128 && !s.chars().any(char::is_control);
        if layout.input_labels.iter().any(|(k,v)| !input_ids.contains(k.as_str()) || !valid_name(v)) ||
            layout.output_labels.iter().any(|(k,v)| !outputs.contains(k) || !valid_name(v)) {
            return Err("ポート名が無効です。".into());
        }
        for (key, sources) in &self.port_links {
            if self.inputs.contains_key(key) || sources.is_empty() || sources.len() > MAX_WIRES_PER_INPUT ||
                sources.iter().copied().collect::<BTreeSet<_>>().len() != sources.len() {
                return Err("重複または不正な入力ワイヤーがあります。".into());
            }
        }
        Ok(())
    }
}

impl Graph {
    /// Replace or append a wire. A failed type/cycle check leaves the node intact.
    pub fn connect_port(&mut self, source: PortRef, to: NodeId, input: &str, append: bool) -> Result<(), String> {
        let out = self.node(source.node).and_then(|n| n.output_port(source.port)).ok_or("接続元ポートがありません。")?;
        let target = self.node(to).ok_or("接続先ノードがありません。")?;
        let spec = target.input_spec(input).ok_or("接続先ポートがありません。")?;
        if !spec.kind.accepts(out.kind) { return Err(format!("{} には {} を接続できません。", spec.kind.label(),out.kind.label())); }
        let legacy = target.inputs.get(input).copied();
        let explicit = target.port_links.get(input).cloned();
        let mut sources = if append { target.sources(input) } else { Vec::new() };
        if !sources.contains(&source) { sources.push(source); }
        if sources.len() > MAX_WIRES_PER_INPUT { return Err("入力ワイヤー数が上限を超えました。".into()); }
        self.node_mut(to).ok_or("接続先がありません。")?.set_sources(input,sources);
        if let Err(e) = self.topological_order() {
            let n = self.node_mut(to).ok_or("接続先がありません。")?;
            n.set_sources(input,Vec::new());
            if let Some(v) = legacy { n.inputs.insert(input.into(),v); }
            if let Some(v) = explicit { n.port_links.insert(input.into(),v); }
            return Err(e);
        }
        Ok(())
    }
    pub fn disconnect_port(&mut self, source: PortRef, to: NodeId, input: &str) -> Result<(), String> {
        let node = self.node_mut(to).ok_or("接続先がありません。")?;
        let mut sources = node.sources(input);
        if !sources.contains(&source) { return Err("接続がありません。".into()); }
        sources.retain(|s| *s != source);
        node.set_sources(input,sources);
        Ok(())
    }
    fn edit_ports(&mut self, id: NodeId, edit: impl FnOnce(&mut Node) -> Result<(),String>) -> Result<(),String> {
        let old = self.node(id).ok_or("ノードがありません。")?.clone();
        let result = edit(self.node_mut(id).ok_or("ノードがありません。")?).and_then(|_| self.validate());
        if result.is_err() { *self.node_mut(id).ok_or("ノードがありません。")? = old; }
        result
    }
    pub fn reorder_inputs(&mut self, id: NodeId, order: Vec<String>) -> Result<(),String> {
        let n = self.node(id).ok_or("ノードがありません。")?;
        let old: BTreeSet<_> = n.input_ports().iter().map(|p| p.key.to_owned()).collect();
        if old.len() != order.len() || old != order.iter().cloned().collect() { return Err("入力の並べ替えが無効です。".into()); }
        self.edit_ports(id, |n| { n.port_layout.input_order = order; Ok(()) })
    }
    pub fn reorder_outputs(&mut self, id: NodeId, order: Vec<PortId>) -> Result<(),String> {
        let n = self.node(id).ok_or("ノードがありません。")?;
        let old: BTreeSet<_> = n.output_ports().iter().map(|p| p.id).collect();
        if old.len() != order.len() || old != order.iter().copied().collect() { return Err("出力の並べ替えが無効です。".into()); }
        self.edit_ports(id, |n| { n.port_layout.output_order = order; Ok(()) })
    }
    pub fn rename_input(&mut self, id: NodeId, key: &str, name: String) -> Result<(),String> {
        self.edit_ports(id, |n| { if n.input_spec(key).is_none() { return Err("入力がありません。".into()); }
            n.port_layout.input_labels.insert(key.into(),name); Ok(()) })
    }
    pub fn rename_output(&mut self, id: NodeId, port: PortId, name: String) -> Result<(),String> {
        self.edit_ports(id, |n| { if n.output_port(port).is_none() { return Err("出力がありません。".into()); }
            n.port_layout.output_labels.insert(port,name); Ok(()) })
    }
    pub fn add_variable_input(&mut self, id: NodeId) -> Result<String,String> {
        let n = self.node(id).ok_or("ノードがありません。")?;
        if n.kind != NodeKind::StreamMerge { return Err("可変入力ノードではありません。".into()); }
        let next = n.port_layout.next_input.max(2);
        let key = MERGE_BANK.get(next).ok_or("可変入力 ID の上限です。")?.key.to_owned();
        let mut order: Vec<_> = n.input_ports().iter().map(|p| p.key.to_owned()).collect();
        order.push(key.clone());
        self.edit_ports(id, |n| { n.port_layout.input_order = order; n.port_layout.next_input = next+1; Ok(()) })?;
        Ok(key)
    }
    pub fn remove_variable_input(&mut self, id: NodeId, key: &str) -> Result<(),String> {
        let n = self.node(id).ok_or("ノードがありません。")?;
        if n.kind != NodeKind::StreamMerge || n.input_spec(key).is_none() || n.input_ports().len() <= 1 || n.has_link(key) {
            return Err("入力を削除できません。接続を先に切断してください。".into());
        }
        let order = n.input_ports().iter().filter(|p| p.key != key).map(|p| p.key.to_owned()).collect();
        self.edit_ports(id, |n| {
            n.port_layout.input_order = order;
            n.port_layout.next_input = n.port_layout.next_input.max(2);
            n.port_layout.input_labels.remove(key);
            n.params.remove(&format!("{}{}",dt::PORT_PREFIX,key));
            Ok(())
        })
    }
    pub fn add_variable_output(&mut self, id: NodeId) -> Result<PortId,String> {
        let n = self.node(id).ok_or("ノードがありません。")?;
        if n.kind != NodeKind::StreamGate || n.output_ports().len() >= MAX_DYNAMIC_PORTS {
            return Err("可変出力を追加できません。".into());
        }
        let next = n.port_layout.next_output.max(2);
        if next >= MAX_GATE_ID { return Err("可変出力 ID の上限です。".into()); }
        let mut order: Vec<_> = n.output_ports().iter().map(|p| p.id).collect();
        order.push(next);
        self.edit_ports(id, |n| { n.port_layout.output_order = order; n.port_layout.next_output = next+1; Ok(()) })?;
        Ok(next)
    }
    pub fn remove_variable_output(&mut self, id: NodeId, port: PortId) -> Result<(),String> {
        let n = self.node(id).ok_or("ノードがありません。")?;
        if n.kind != NodeKind::StreamGate || port == PRIMARY_PORT || n.output_port(port).is_none() ||
            self.nodes.iter().any(|n| n.port_links.values().flatten().any(|s| s.node == id && s.port == port)) {
            return Err("出力を削除できません。接続を先に切断してください。".into());
        }
        let order = n.output_ports().iter().filter(|p| p.id != port).map(|p| p.id).collect();
        self.edit_ports(id, |n| {
            n.port_layout.output_order = order;
            n.port_layout.next_output = n.port_layout.next_output.max(2);
            n.port_layout.output_labels.remove(&port);
            Ok(())
        })
    }
}

/// Materialize only the connected input trees for a legacy evaluator. Distinct
/// output ports of the same node are assigned different transient IDs.
pub(crate) fn materialize(node: &Node, outputs: &Evaluation, cancel: &dyn Fn() -> bool)
    -> Result<(Node, BTreeMap<NodeId, Result<Tree,String>>),String>
{
    let mut adapted = node.clone();
    adapted.inputs.clear();
    adapted.port_links.clear();
    let mut trees = BTreeMap::new();
    let mut total = 0usize;
    for (index,(key,sources)) in node.all_links().into_iter().enumerate() {
        let mut source_trees = Vec::new();
        for source in sources {
            engine::cancelled(cancel)?;
            let value = outputs.output(source).ok_or("入力元ポートの結果がありません。")?;
            let tree = value.as_ref().map_err(|e| format!("入力 {key}: {e}"))?;
            total = total.saturating_add(tree.weight());
            if total > MAX_TOTAL_WEIGHT { return Err("入力ワイヤーのデータ量が上限を超えました。".into()); }
            source_trees.push(tree.clone());
        }
        let merged = if source_trees.len() == 1 { source_trees.pop().unwrap() } else { dt::merge(&source_trees)? };
        let virtual_id = index as NodeId;
        adapted.inputs.insert(key,virtual_id);
        trees.insert(virtual_id,Ok(merged));
    }
    Ok((adapted,trees))
}

pub(crate) fn failed(node: &Node, error: String) -> PortResults {
    let mut out: PortResults = node.output_ports().into_iter().map(|p| (p.id,Err(error.clone()))).collect();
    out.entry(PRIMARY_PORT).or_insert(Err(error));
    out
}
pub(crate) fn empty(node: &Node) -> PortResults {
    node.output_ports().into_iter().map(|p| (p.id,Ok(Tree::default()))).collect()
}
pub(crate) fn result_weight(results: &PortResults) -> usize {
    results.values().filter_map(|r| r.as_ref().ok()).fold(0usize,|a,t| a.saturating_add(t.weight()))
}

fn split_native(node: &Node, outputs: &BTreeMap<NodeId,Result<Tree,String>>, stop: &dyn Fn()->bool) -> Result<PortResults,String> {
    use NodeKind::*;
    let inputs = engine::collect_inputs(node,outputs,stop)?;
    let mut trees: BTreeMap<PortId,Tree> = node.output_ports().into_iter().map(|p| (p.id,Tree::default())).collect();
    if node.kind == StreamMerge {
        trees.insert(PRIMARY_PORT,dt::merge(&inputs)?);
    } else if node.kind == StreamGate {
        let mut gates = inputs[0].branches.values().flatten();
        let index = gates.next().ok_or("出力位置を指定してください。")?.number()? as usize;
        if gates.next().is_some() { return Err("出力位置は数値1個で指定してください。".into()); }
        let port = node.output_ports().get(index).ok_or("出力位置が範囲外です。")?.id;
        trees.insert(port,inputs[1].clone());
    } else {
        // The native list policy pairs branches by ordinal using the shared tree
        // matcher. A point/vector/domain/mesh has only one source tree.
        let active: &[Tree] = if node.kind == SortListPaired && !node.has_link("values") { &inputs[..1] } else { &inputs };
        let mut items = 0usize;
        let mut weight = 0usize;
        for dt::BranchMatch { path, lists } in dt::match_branches(active)? {
            engine::cancelled(stop)?;
            let mut emit = |port: PortId, path: Vec<u32>, values: Vec<Value>| -> Result<(),String> {
                items = items.saturating_add(values.len());
                weight = values.iter().fold(weight.saturating_add(path.len()+1),|a,v| a.saturating_add(v.weight()));
                if items > MAX_ITEMS || weight > MAX_TOTAL_WEIGHT / 2 { return Err("複数出力のデータ上限を超えました。".into()); }
                let t = trees.get_mut(&port).ok_or("出力ポートがありません。")?;
                if t.branches.insert(path,values).is_some() { return Err("出力パスが重複しています。".into()); }
                Ok(())
            };
            match node.kind {
                DeconstructPoint | DeconstructVector => {
                    let mut xyz: [Vec<Value>;3] = Default::default();
                    for v in lists[0] {
                        engine::cancelled(stop)?;
                        let Value::Point(p) = v else { return Err("XYZ点型が必要です。".into()); };
                        for k in 0..3 { xyz[k].push(Value::Number(p[k])); }
                    }
                    for (k,v) in xyz.into_iter().enumerate() { emit(k as PortId,path.clone(),v)?; }
                }
                DeconstructDomain => {
                    let [a,b] = lists[0] else { return Err("区間には枝ごとに2数値が必要です。".into()); };
                    emit(0,path.clone(),vec![Value::Number(a.number()?)])?;
                    emit(1,path,vec![Value::Number(b.number()?)])?;
                }
                Dispatch => {
                    let pattern = lists[1].iter().map(|v| v.number().map(|x| x != 0.)).collect::<Result<Vec<_>,_>>()?;
                    let (a,b) = dt::dispatch(lists[0],&pattern)?;
                    emit(0,path.clone(),a)?;
                    emit(1,path,b)?;
                }
                SortListPaired => {
                    let keys = lists[0].iter().map(Value::number).collect::<Result<Vec<_>,_>>()?;
                    let values = lists.get(1).copied();
                    if values.is_some_and(|v| v.len() != keys.len()) { return Err("キーと値の個数が一致しません。".into()); }
                    let mut order: Vec<_> = (0..keys.len()).collect();
                    // Stable tie ordering, including -0 and +0 as equal keys.
                    order.sort_by(|&a,&b| keys[a].partial_cmp(&keys[b]).unwrap_or(std::cmp::Ordering::Equal));
                    emit(0,path.clone(),order.iter().map(|&i| Value::Number(keys[i])).collect())?;
                    emit(1,path,values.map(|v| order.iter().map(|&i| v[i].clone()).collect()).unwrap_or_default())?;
                }
                DeconstructMesh => {
                    if lists[0].is_empty() {
                        emit(0,path.clone(),Vec::new())?;
                        emit(1,path,Vec::new())?;
                    } else {
                        for (j,m) in lists[0].iter().enumerate() {
                            engine::cancelled(stop)?;
                            let mut p = path.clone();
                            // Always append item index to avoid mixed-depth path collisions.
                            if p.len() >= 32 { return Err("メッシュ分解のパスが深すぎます。".into()); }
                            p.push(j as u32);
                            let vertices = crate::mesh_fabrication::run_item(MeshVertices,&[m],stop)?;
                            let faces = crate::mesh_fabrication::run_item(MeshFaces,&[m],stop)?;
                            emit(0,p.clone(),vertices)?;
                            emit(1,p,faces)?;
                        }
                    }
                }
                _ => return Err("未対応の複数出力演算です。".into()),
            }
        }
    }
    let mut checked = BTreeMap::new();
    for (port,tree) in trees { checked.insert(port,Ok(engine::checked_tree(tree,stop)?)); }
    Ok(checked)
}

/// Existing Curve/Surface primary output semantics are preserved. Secondary
/// solvers report errors per port (e.g. tangent undefined at a stationary point).
pub(crate) fn evaluate(node: &Node, outputs: &BTreeMap<NodeId,Result<Tree,String>>, stop: &dyn Fn()->bool) -> PortResults {
    use NodeKind::*;
    match node.kind {
        CurveEvaluate | EvaluateSurface => {
            let mut result = BTreeMap::from([(PRIMARY_PORT,engine::evaluate_node(node,outputs,stop))]);
            let mut adapted = node.clone();
            if node.kind == CurveEvaluate {
                adapted.kind = CurveTangent;
                result.insert(1,engine::evaluate_node(&adapted,outputs,stop));
            } else {
                adapted.kind = SurfaceDirection;
                for axis in 0..3 {
                    if stop() { return failed(node,"計算を停止しました。".into()); }
                    adapted.params.insert("axis".into(),axis as f64);
                    result.insert(axis+1,engine::evaluate_node(&adapted,outputs,stop));
                }
            }
            if result_weight(&result) > MAX_TOTAL_WEIGHT / 2 { return failed(node,"複数出力の容量が上限を超えました。".into()); }
            result
        }
        DeconstructPoint | DeconstructVector | DeconstructDomain | DeconstructMesh | Dispatch | SortListPaired | StreamGate | StreamMerge =>
            split_native(node,outputs,stop).unwrap_or_else(|e| failed(node,e)),
        _ => BTreeMap::from([(PRIMARY_PORT,engine::evaluate_node(node,outputs,stop))]),
    }
}
