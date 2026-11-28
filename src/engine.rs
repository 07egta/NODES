//! Deterministic, bounded graph evaluation and procedural mesh construction.
//! Cache keys deliberately exclude presentation-only state (name, position, preview).
use crate::data_tree as dt;
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Wall-clock time spent processing one top-level node during this evaluation.
/// Includes input/cache preparation, evaluation or retrieval, resource checks and
/// output insertion. Cluster internals are included; graph validation is excluded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NodeTiming {
    pub elapsed_ms: f64,
    pub cached: bool,
}
impl NodeTiming {
    fn spent(spent: Duration, cached: bool) -> Self {
        Self {
            elapsed_ms: spent.as_secs_f64() * 1000.,
            cached,
        }
    }
    fn since(start: Instant, cached: bool) -> Self {
        Self {
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
            cached,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Evaluation {
    pub outputs: BTreeMap<NodeId, Result<Tree, String>>,
    /// Only visited nodes have timings. Interrupted nodes have partial timings
    /// without cached outputs; disabled nodes have non-cached bookkeeping time.
    pub node_timings: BTreeMap<NodeId, NodeTiming>,
    pub computed: usize,
    pub cached: usize,
    pub elapsed_ms: f64,
    pub cancelled: bool,
}
struct Key {
    kind: NodeKind,
    params: Vec<(String, u64)>,
    inputs: Vec<(String, NodeId, u64)>,
    /// Script text or cluster-definition fingerprint (empty for ordinary nodes).
    extra: crate::cluster::CacheExtra,
}
impl Key {
    fn matches(
        &self,
        node: &Node,
        revisions: &BTreeMap<NodeId, u64>,
        extra: &crate::cluster::CacheExtra,
    ) -> bool {
        self.kind == node.kind
            && &self.extra == extra
            && self.params.iter().map(|(key, bits)| (key, *bits)).eq(node
                .params
                .iter()
                .map(|(key, value)| (key, value.to_bits())))
            && self
                .inputs
                .iter()
                .map(|(key, id, rev)| (key, *id, *rev))
                .eq(node
                    .inputs
                    .iter()
                    .map(|(key, id)| (key, *id, revisions.get(id).copied().unwrap_or(0))))
    }

    fn new(
        node: &Node,
        revisions: &BTreeMap<NodeId, u64>,
        extra: crate::cluster::CacheExtra,
    ) -> Self {
        Self {
            kind: node.kind,
            // Keep exact bit equality for signed-zero-sensitive numeric operations.
            params: node
                .params
                .iter()
                .map(|(key, n)| (key.clone(), n.to_bits()))
                .collect(),
            inputs: node
                .inputs
                .iter()
                .map(|(key, id)| (key.clone(), *id, revisions.get(id).copied().unwrap_or(0)))
                .collect(),
            extra,
        }
    }
}

struct Cache {
    key: Key,
    revision: u64,
    result: Result<Tree, String>,
    weight: usize,
}
#[derive(Default)]
pub struct Evaluator {
    cache: BTreeMap<NodeId, Cache>,
    // Match the previous oldest-revision eviction policy, with logarithmic removal.
    cache_order: BTreeSet<(u64, NodeId)>,
    cache_weight: usize,
    revision: u64,
    cluster_keys: crate::cluster::CacheKeys,
    /// Thread cap override (`None`: all available cores, up to `MAX_THREADS`).
    max_threads: Option<usize>,
}
impl Evaluator {
    /// Limits the worker threads used for one graph level (1 = fully sequential).
    pub fn set_max_threads(&mut self, threads: usize) {
        self.max_threads = Some(threads);
    }
    fn remove_cached(&mut self, id: NodeId) {
        if let Some(old) = self.cache.remove(&id) {
            self.cache_order.remove(&(old.revision, id));
            self.cache_weight = self.cache_weight.saturating_sub(old.weight);
        }
    }

    // Interrupted evaluations may leave entries from several different revisions. Bound
    // retained cache memory independently of the current evaluation's output budget.
    fn store_cached(&mut self, id: NodeId, entry: Cache) {
        self.remove_cached(id);
        if entry.weight > MAX_TOTAL_WEIGHT {
            return;
        }
        while self.cache_weight.saturating_add(entry.weight) > MAX_TOTAL_WEIGHT {
            let Some(&(_, oldest)) = self.cache_order.first() else {
                return;
            };
            self.remove_cached(oldest);
        }
        self.cache_weight = self.cache_weight.saturating_add(entry.weight);
        self.cache_order.insert((entry.revision, id));
        self.cache.insert(id, entry);
    }

    pub fn evaluate(&mut self, graph: &Graph, cancel: &(dyn Fn() -> bool + Sync)) -> Evaluation {
        let start = Instant::now();
        let mut out = Evaluation::default();
        // Latch cancellation: a one-shot callback must not become a cached node error.
        let stopped = AtomicBool::new(false);
        let stop = || {
            if !stopped.load(Ordering::Relaxed) && cancel() {
                stopped.store(true, Ordering::Relaxed);
            }
            stopped.load(Ordering::Relaxed)
        };
        if stop() {
            out.cancelled = true;
            out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
            return out;
        }
        let order = match graph.topological_nodes() {
            Ok(o) => o,
            Err(e) => {
                for n in &graph.nodes {
                    out.outputs.insert(n.id, Err(e.clone()));
                }
                out.cancelled = stop();
                out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
                return out;
            }
        };
        let existing: BTreeSet<_> = graph.nodes.iter().map(|n| n.id).collect();
        let removed: Vec<_> = self
            .cache
            .keys()
            .copied()
            .filter(|id| !existing.contains(id))
            .collect();
        for id in removed {
            self.remove_cached(id);
        }
        self.cluster_keys.begin(graph);
        let threads = if graph.nodes.len() >= PARALLEL_MIN_NODES {
            self.max_threads
                .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
                .clamp(1, MAX_THREADS)
        } else {
            1
        };
        let mut revisions = BTreeMap::new();
        let mut retained = 0usize;
        'levels: for level in levels(&order) {
            if stop() {
                out.cancelled = true;
                break;
            }
            // Phase 1 (sequential): disabled nodes, cache keys and cache hits.
            let mut slots: Vec<Slot> = Vec::with_capacity(level.len());
            for node in level {
                if stop() {
                    out.cancelled = true;
                    break 'levels;
                }
                let id = node.id;
                let node_start = Instant::now();
                // Disabled node: empty output, never cached; revision 0 marks "off" for children.
                if node.off {
                    self.remove_cached(id);
                    revisions.insert(id, 0);
                    out.outputs.insert(id, Ok(Tree::default()));
                    out.node_timings
                        .insert(id, NodeTiming::since(node_start, false));
                    continue;
                }
                let extra = self.cluster_keys.key_for(graph, node);
                let hit = self
                    .cache
                    .get(&id)
                    .filter(|c| c.key.matches(node, &revisions, &extra))
                    .map(|c| (c.result.clone(), c.revision, c.weight));
                let key = if hit.is_some() {
                    None
                } else {
                    Some(Key::new(node, &revisions, extra))
                };
                slots.push(Slot {
                    id,
                    node,
                    key,
                    hit,
                    fresh: None,
                    prep: node_start.elapsed(),
                    compute: Duration::ZERO,
                });
            }
            // Phase 2: compute the misses. Nodes of one level never depend on each other, so
            // they may run in parallel; results are slotted by index, so the outcome does
            // not depend on scheduling.
            let misses: Vec<usize> = (0..slots.len())
                .filter(|&i| slots[i].hit.is_none())
                .collect();
            let outputs = &out.outputs;
            if threads > 1 && misses.len() > 1 {
                let next = AtomicUsize::new(0);
                let results: Vec<Mutex<Option<Computed>>> =
                    misses.iter().map(|_| Mutex::new(None)).collect();
                std::thread::scope(|scope| {
                    for _ in 0..threads.min(misses.len()) {
                        scope.spawn(|| {
                            loop {
                                let k = next.fetch_add(1, Ordering::Relaxed);
                                let (Some(&i), Some(cell)) = (misses.get(k), results.get(k)) else {
                                    break;
                                };
                                if stop() {
                                    break;
                                }
                                let began = Instant::now();
                                let r = crate::cluster::evaluate_any(
                                    graph,
                                    slots[i].node,
                                    outputs,
                                    &stop,
                                );
                                let spent = began.elapsed();
                                // A result computed while cancelling may be a cancel error.
                                let r = if stop() { None } else { Some(r) };
                                if let Ok(mut cell) = cell.lock() {
                                    *cell = Some((r, spent));
                                }
                            }
                        });
                    }
                });
                for (k, cell) in results.into_iter().enumerate() {
                    if let Some((r, spent)) = cell.into_inner().ok().flatten()
                        && let Some(slot) = misses.get(k).and_then(|&i| slots.get_mut(i))
                    {
                        slot.fresh = r;
                        slot.compute = spent;
                    }
                }
            } else {
                for &i in &misses {
                    if stop() {
                        break;
                    }
                    let began = Instant::now();
                    let r = crate::cluster::evaluate_any(graph, slots[i].node, outputs, &stop);
                    slots[i].compute = began.elapsed();
                    if !stop() {
                        slots[i].fresh = Some(r);
                    }
                }
            }
            // Phase 3 (sequential, in level order): revisions, weight budget, cache, outputs.
            for slot in slots {
                let phase3 = Instant::now();
                let id = slot.id;
                let base = slot.prep + slot.compute;
                let (result, revision, is_cached, weight) = match (slot.hit, slot.fresh) {
                    (Some((r, rev, weight)), _) => (r, rev, true, weight),
                    (None, Some(r)) => {
                        self.revision = self.revision.wrapping_add(1).max(1);
                        let weight = r.as_ref().map(Tree::weight).unwrap_or(0);
                        (r, self.revision, false, weight)
                    }
                    (None, None) => {
                        out.cancelled = true;
                        out.node_timings
                            .insert(id, NodeTiming::spent(base + phase3.elapsed(), false));
                        break 'levels;
                    }
                };
                if is_cached {
                    out.cached += 1;
                } else {
                    out.computed += 1;
                }
                if retained.saturating_add(weight) > MAX_TOTAL_WEIGHT {
                    self.remove_cached(id);
                    self.revision = self.revision.wrapping_add(1).max(1);
                    revisions.insert(id, self.revision);
                    out.outputs.insert(
                        id,
                        Err(
                            "グラフ全体の形状が大きすぎます。配列の個数や分割数を減らしてください。"
                                .into(),
                        ),
                    );
                    out.node_timings
                        .insert(id, NodeTiming::spent(base + phase3.elapsed(), is_cached));
                    continue;
                }
                retained = retained.saturating_add(weight);
                if !is_cached && let Some(key) = slot.key {
                    self.store_cached(
                        id,
                        Cache {
                            key,
                            revision,
                            result: result.clone(),
                            weight,
                        },
                    );
                }
                revisions.insert(id, revision);
                out.outputs.insert(id, result);
                out.node_timings
                    .insert(id, NodeTiming::spent(base + phase3.elapsed(), is_cached));
            }
        }
        // Cache-only, disabled-only and empty graphs may never execute a kernel.
        // Observe cancellation before returning even when no fresh slot was missing.
        out.cancelled |= stop();
        out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        out
    }
}

/// Graphs with fewer nodes than this are evaluated on one thread (spawning costs more).
const PARALLEL_MIN_NODES: usize = 8;
const MAX_THREADS: usize = 8;

/// A node computed in phase 2: the result (`None` when cancelled) and the time it took.
type Computed = (Option<Result<Tree, String>>, Duration);

struct Slot<'a> {
    id: NodeId,
    node: &'a Node,
    /// Cache key, built only for nodes that are not cache hits.
    key: Option<Key>,
    /// Cached result, its revision and its weight.
    hit: Option<(Result<Tree, String>, u64, usize)>,
    fresh: Option<Result<Tree, String>>,
    /// Time spent preparing the cache key, and computing the node.
    prep: Duration,
    compute: Duration,
}

/// Groups the topological order into levels: a node's level is one more than its deepest
/// parent, so nodes of one level are independent. Order inside a level follows `order`.
fn levels<'a>(order: &[&'a Node]) -> Vec<Vec<&'a Node>> {
    let mut level_of: BTreeMap<NodeId, usize> = BTreeMap::new();
    let mut out: Vec<Vec<&'a Node>> = Vec::new();
    for &node in order {
        let level = node
            .inputs
            .values()
            .filter_map(|p| level_of.get(p))
            .map(|l| l + 1)
            .max()
            .unwrap_or(0);
        level_of.insert(node.id, level);
        if out.len() <= level {
            out.resize_with(level + 1, Vec::new);
        }
        if let Some(bucket) = out.get_mut(level) {
            bucket.push(node);
        }
    }
    out
}

pub(crate) fn node_inputs(
    node: &Node,
    outputs: &BTreeMap<NodeId, Result<Tree, String>>,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Tree>, String> {
    let specs = node.kind.spec().inputs;
    let mut inputs = Vec::with_capacity(specs.len());
    for spec in specs {
        let tree = if let Some(source) = node.inputs.get(spec.key) {
            match outputs.get(source) {
                Some(Ok(t)) => t.clone(),
                Some(Err(_)) => {
                    return Err(format!(
                        "「{}」の入力元にエラーがあります。上流の赤いノードを確認してください。",
                        spec.label
                    ));
                }
                None => return Err(format!("「{}」の計算結果がありません。", spec.label)),
            }
        } else if let Some(default) = spec.default {
            Tree::single(Value::Number(
                *node.params.get(spec.key).unwrap_or(&default),
            ))
        } else {
            return Err(format!("「{}」を接続してください。", spec.label));
        };
        let tree = if let Some(&mode) = node.params.get(&format!("{}{}", dt::PORT_PREFIX, spec.key))
        {
            dt::apply_transform(&tree, dt::PortTransform::from_number(mode)?)?
        } else {
            tree
        };
        for (path, values) in &tree.branches {
            for (index, value) in values.iter().enumerate() {
                cancelled(cancel)?;
                let site = || format!("{}[{}]", dt::path_text(path), index);
                if !spec.kind.accepts(value.kind()) {
                    return Err(format!(
                        "「{}」{} には {} が必要です（現在: {}）。",
                        spec.label,
                        site(),
                        spec.kind.label(),
                        value.kind().label()
                    ));
                }
                if spec.default.is_some() {
                    let n = value.number()?;
                    if n < spec.min || n > spec.max {
                        return Err(format!(
                            "「{}」{} は {} 〜 {} の範囲にしてください。",
                            spec.label,
                            site(),
                            spec.min,
                            spec.max
                        ));
                    }
                    if spec.integer && n.fract() != 0.0 {
                        return Err(format!(
                            "「{}」{} は整数にしてください。",
                            spec.label,
                            site()
                        ));
                    }
                }
            }
        }
        inputs.push(tree);
    }
    Ok(inputs)
}

pub(crate) fn evaluate_node(
    node: &Node,
    outputs: &BTreeMap<NodeId, Result<Tree, String>>,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    let inputs = node_inputs(node, outputs, cancel)?;
    if crate::data_tree_nodes::is_tree_kind(node.kind) {
        return checked_tree(
            crate::data_tree_nodes::run_tree(node, &inputs, cancel)?,
            cancel,
        );
    }
    if node.kind == NodeKind::Flatten || node.kind == NodeKind::Graft {
        let t = inputs.first().ok_or("入力がありません。")?;
        let result = if node.kind == NodeKind::Flatten {
            dt::flatten(t)
        } else {
            dt::graft(t)
        }?;
        return checked_tree(result, cancel);
    }
    if node.kind == NodeKind::Output || node.kind == NodeKind::Reroute {
        return checked_tree(
            inputs.into_iter().next().ok_or("形状がありません。")?,
            cancel,
        );
    }
    if node.kind == NodeKind::ExactStepImport {
        return checked_tree(crate::nurbs_nodes::read_step(&node.script, cancel)?, cancel);
    }
    if node.kind == NodeKind::ExactStepExport {
        let bytes = crate::nurbs_nodes::step_text(&inputs, cancel)?.len();
        return checked_tree(Tree::single(Value::Number(bytes as f64)), cancel);
    }
    // 拘束スケッチ: the sketch text is in `script` (see `sketch_nodes`).
    if node.kind == NodeKind::ExactSketch {
        let mode = inputs
            .first()
            .and_then(|t| t.branches.values().flatten().next())
            .map(Value::number)
            .transpose()?
            .unwrap_or(0.);
        return checked_tree(
            crate::sketch_nodes::run(&node.script, mode as usize, cancel)?,
            cancel,
        );
    }
    // ケージ編集: the edit list is in `script` (see `nurbs_nodes::run_cage_edit`).
    if node.kind == NodeKind::SubdCageEdit {
        return checked_tree(
            crate::nurbs_nodes::run_cage_edit(node, &inputs, cancel)?,
            cancel,
        );
    }
    // Gコード nodes work on whole trees of G-code jobs (see `gcode_nodes.rs`).
    if crate::gcode_nodes::is_kind(node.kind) {
        return checked_tree(crate::gcode_nodes::run(node, &inputs, cancel)?, cancel);
    }
    // ファイル読み込み: the path is in `script`; scale, centre and bed come from its inputs.
    if node.kind == NodeKind::ImportMesh {
        let scale = first_number(inputs.first().ok_or("倍率の入力がありません。")?)?;
        let center = first_number(inputs.get(1).ok_or("中心の入力がありません。")?)? != 0.;
        let bed = first_number(inputs.get(2).ok_or("床の入力がありません。")?)? != 0.;
        let mut mesh = crate::import::read_file(&node.script, cancel)?;
        crate::import::place(&mut mesh, scale, center, bed)?;
        let values = as_mesh(mesh, cancel)?;
        return checked_tree(
            Tree {
                branches: BTreeMap::from([(vec![0], values)]),
            },
            cancel,
        );
    }
    // The 式 node parses its text once; every list item then reuses the parsed formula.
    let formula = if node.kind == NodeKind::Expression {
        Some(crate::expr::parse(&node.script)?)
    } else {
        None
    };
    let curve_list = crate::curve_nodes::is_list_kind(node.kind);
    let list_kind = crate::node_access::uses_list_access(node.kind);
    let matching = node
        .params
        .get(dt::MATCH_PARAM)
        .copied()
        .map(dt::MatchMode::from_number)
        .transpose()?;
    let item_mode = matching.unwrap_or(dt::MatchMode::Longest);
    // A single branch is broadcast to the multi-branch input's paths. It does not add an
    // accidental extra root branch when one of the inputs has been grafted.
    let mut paths = BTreeSet::new();
    for tree in inputs
        .iter()
        .filter(|t| t.branches.len() > 1 || t.branches.keys().any(|p| p.as_slice() != [0]))
    {
        paths.extend(tree.branches.keys().cloned());
    }
    if paths.is_empty() {
        for tree in &inputs {
            paths.extend(tree.branches.keys().cloned());
        }
    }
    // Old .morph files omit MATCH_PARAM and keep the original exact-path behaviour.
    // When a matching mode is selected, use ordinal branch matching instead.
    let matched = if matching.is_some() || crate::node_access::uses_ordinal_matching(node.kind) {
        dt::match_branches(&inputs)?
    } else {
        paths
            .into_iter()
            .map(|path| {
                let lists: Vec<&[Value]> = inputs
                    .iter()
                    .map(|t| {
                        t.branches
                            .get(&path)
                            .or_else(|| {
                                if t.branches.len() == 1 {
                                    t.branches.values().next()
                                } else {
                                    None
                                }
                            })
                            .map(Vec::as_slice)
                            .unwrap_or(&[])
                    })
                    .collect();
                dt::BranchMatch { path, lists }
            })
            .collect()
    };
    let mut result = Tree::default();
    let mut total_items = 0usize;
    let mut total_weight = 0usize;
    for dt::BranchMatch { path, lists } in matched {
        cancelled(cancel)?;
        total_weight = total_weight.saturating_add(path.len() + 1);
        if total_weight > MAX_TOTAL_WEIGHT / 2 || result.branches.len() >= MAX_ITEMS {
            return Err("データツリーの枝数または容量が上限を超えました。".into());
        }
        result.branches.entry(path.clone()).or_default();
        let values = (|| -> Result<Vec<Value>, String> {
            // Empty lists are data, including non-leading lists such as cut parameters
            // or edit indices. Only a missing FirstItem scalar setting suppresses a
            // list adapter. SingleItem adapters must validate their exact cardinality.
            let skip = if matches!(node.kind, NodeKind::Merge | NodeKind::Loft) {
                lists.iter().any(|v| v.is_empty())
            } else if list_kind {
                lists.iter().enumerate().any(|(index, values)| {
                    values.is_empty()
                        && crate::node_access::for_input(node.kind, index)
                            == Some(crate::node_access::InputAccess::FirstItem)
                })
            } else {
                lists.iter().any(|v| v.is_empty())
            };
            if skip {
                return Ok(Vec::new());
            }
            let values = if node.kind == NodeKind::Merge {
                vec![Value::Mesh(Arc::new(merge_meshes(lists[0], cancel)?))]
            } else if node.kind == NodeKind::Loft {
                let cap = lists
                    .get(1)
                    .and_then(|v| v.first())
                    .ok_or("端面の設定がありません。")?
                    .number()?
                    != 0.;
                vec![Value::Mesh(Arc::new(loft(lists[0], cap, cancel)?))]
            } else if crate::data_tree_nodes::is_list_kind(node.kind) {
                crate::data_tree_nodes::run_list(
                    node.kind,
                    &lists,
                    MAX_ITEMS.saturating_sub(total_items),
                    (MAX_TOTAL_WEIGHT / 2).saturating_sub(total_weight),
                    cancel,
                )?
            } else if crate::math_nodes::is_list_kind(node.kind) {
                crate::math_nodes::run_list(node.kind, &lists, cancel)?
            } else if node.kind == NodeKind::BioSpaceColonization {
                crate::biomorph::run_colonization(
                    &lists,
                    (MAX_TOTAL_WEIGHT / 2).saturating_sub(total_weight),
                    MAX_ITEMS.saturating_sub(total_items),
                    cancel,
                )?
            } else if crate::generative::is_list(node.kind) {
                crate::generative::run_list(node.kind, &lists, cancel)?
            } else if node.kind == NodeKind::MeshConstruct {
                crate::mesh_fabrication::run_list(&lists, cancel)?
            } else if crate::physics_nodes::is_list_kind(node.kind) {
                crate::physics_nodes::run_list(node.kind, &lists, cancel)?
            } else if crate::nurbs_nodes::is_list_kind(node.kind) {
                crate::nurbs_nodes::run_list(node.kind, &lists, cancel)?
            } else if crate::nodes_shape::is_list_kind(node.kind) {
                crate::nodes_shape::run_list(node.kind, &lists, cancel)?
            } else if curve_list {
                crate::curve_nodes::run_list(node.kind, &lists, cancel)?
            } else if list_kind {
                crate::gh_nodes::run_list(node.kind, &lists, cancel)?
            } else {
                let matches =
                    dt::ItemMatches::new(&lists, item_mode, MAX_ITEMS.saturating_sub(total_items))?;
                let mut values = Vec::new();
                for args in matches {
                    cancelled(cancel)?;
                    preflight_run(
                        node.kind,
                        &args,
                        (MAX_TOTAL_WEIGHT / 2).saturating_sub(total_weight),
                        MAX_ITEMS.saturating_sub(total_items.saturating_add(values.len())),
                    )?;
                    let made = match &formula {
                        Some(f) => vec![Value::Number(f.eval([
                            num(&args, 0)?,
                            num(&args, 1)?,
                            num(&args, 2)?,
                            num(&args, 3)?,
                        ])?)],
                        None => run(node.kind, &args, cancel)?,
                    };
                    if values.len().saturating_add(made.len()) > MAX_ITEMS {
                        return Err(
                            "生成データが多すぎます。数列や配列の個数を減らしてください。".into(),
                        );
                    }
                    for value in &made {
                        total_weight = total_weight.saturating_add(value.weight());
                    }
                    if total_weight > MAX_TOTAL_WEIGHT / 2 {
                        return Err(
                        "1 つのノードの形状が大きすぎます。分割数や配列の個数を減らしてください。"
                            .into(),
                    );
                    }
                    values.extend(made);
                }
                values
            };
            if matches!(node.kind, NodeKind::Merge | NodeKind::Loft) || list_kind {
                for value in &values {
                    total_weight = total_weight.saturating_add(value.weight());
                }
                if total_weight > MAX_TOTAL_WEIGHT / 2 {
                    return Err("生成形状が大きすぎます。断面や分割数を減らしてください。".into());
                }
            }
            total_items = total_items.saturating_add(values.len());
            if total_items > MAX_ITEMS {
                return Err("生成データの個数が上限を超えました。".into());
            }
            Ok(values)
        })()
        .map_err(|e| format!("枝 {}: {e}", dt::path_text(&path)))?;
        result.branches.insert(path, values);
    }
    checked_tree(result, cancel)
}
pub(crate) fn checked_tree(tree: Tree, cancel: &dyn Fn() -> bool) -> Result<Tree, String> {
    if tree.branches.len() > MAX_ITEMS || tree.branches.keys().any(|p| p.len() > 32) {
        return Err(
            "データツリーが大きすぎるか深すぎます。フラット化で階層を整理してください。".into(),
        );
    }
    if tree.item_count() > MAX_ITEMS || tree.weight() > MAX_TOTAL_WEIGHT / 2 {
        return Err("生成データが上限を超えました。分割数や配列の個数を減らしてください。".into());
    }
    for value in tree.branches.values().flatten() {
        cancelled(cancel)?;
        match value {
            Value::Number(v) if !v.is_finite() => {
                return Err("計算結果が有限の数値ではありません。".into());
            }
            Value::Point(p) if !finite_point(*p) => return Err("点の座標が無効です。".into()),
            Value::Curve(c)
                if c.points.len() > MAX_VERTICES || c.points.iter().any(|p| !finite_point(*p)) =>
            {
                return Err("曲線の座標または大きさが無効です。".into());
            }
            Value::Mesh(m) => validate_mesh(m, cancel)?,
            _ => {}
        }
    }
    Ok(tree)
}

fn preflight_run(
    kind: NodeKind,
    args: &[&Value],
    remaining_weight: usize,
    remaining_items: usize,
) -> Result<(), String> {
    let (weight, items) = match kind {
        NodeKind::LinearArray | NodeKind::RadialArray => {
            let count = num(args, 1)? as usize;
            (shape(args)?.weight().saturating_mul(count), count)
        }
        NodeKind::Move
        | NodeKind::Rotate
        | NodeKind::Scale
        | NodeKind::ScaleXyz
        | NodeKind::Twist
        | NodeKind::Wave => (shape(args)?.weight(), 1),
        // A mirror that keeps the original can double the shape.
        NodeKind::Mirror => (shape(args)?.weight().saturating_mul(2), 1),
        NodeKind::Series | NodeKind::Range => {
            let count = num(args, 2)? as usize;
            (count, count)
        }
        NodeKind::GhRange => {
            let count = (num(args, 2)? as usize).saturating_add(1);
            (count, count)
        }
        NodeKind::ConstructDomain => (2, 2),
        NodeKind::RandomNormal => {
            let count = num(args, 0)? as usize;
            (count, count)
        }
        NodeKind::Random | NodeKind::Populate => {
            let count = num(args, 0)? as usize;
            (count, count)
        }
        NodeKind::NoiseDeform
        | NodeKind::MeshFlip
        | NodeKind::MeshSmooth
        | NodeKind::MeshOnBed
        | NodeKind::MeshUnify => (shape(args)?.weight(), 1),
        NodeKind::CurveDivide => {
            let count = num(args, 1)? as usize + 1;
            (count, count)
        }
        // Outer and inner copies plus the stitched rim.
        NodeKind::Shell => (shape(args)?.weight().saturating_mul(3), 1),
        k if crate::mathematical_surfaces::is_kind(k) => {
            crate::mathematical_surfaces::estimate(k, args)?
        }
        k if crate::matter::is_kind(k) => crate::matter::estimate(k, args)?,
        NodeKind::BioCoiledShell => crate::biomorph::estimate_shell(args)?,
        NodeKind::OrigamiMiura | NodeKind::OrigamiMiuraCreases => {
            crate::origami::estimate(kind, args)?
        }
        k if crate::generative::is_kind(k) => crate::generative::estimate(k, args)?,
        k if crate::bio_patterns::is_kind(k) => crate::bio_patterns::estimate(k, args)?,
        k if crate::genesis::is_kind(k) => crate::genesis::estimate(k, args)?,
        k if crate::nodes_shape::is_kind(k) => crate::nodes_shape::estimate(k, args)?,
        k if crate::mechanism_nodes::is_kind(k) => crate::mechanism_nodes::estimate(k, args)?,
        _ => (0, 1),
    };
    if weight > remaining_weight || items > remaining_items {
        return Err("配列や数列の結果が大きすぎます。生成前に停止しました。個数や分割数を減らしてください。".into());
    }
    Ok(())
}
fn first_number(tree: &Tree) -> Result<f64, String> {
    tree.branches
        .values()
        .flatten()
        .next()
        .ok_or("数値を入力してください。")?
        .number()
}
pub(crate) fn cancelled(stop: &dyn Fn() -> bool) -> Result<(), String> {
    if stop() {
        Err("計算を停止しました。".into())
    } else {
        Ok(())
    }
}
pub(crate) fn num(a: &[&Value], i: usize) -> Result<f64, String> {
    a.get(i).ok_or("数値の入力がありません。")?.number()
}
pub(crate) fn shape<'a>(a: &[&'a Value]) -> Result<&'a Value, String> {
    a.first()
        .copied()
        .ok_or_else(|| "形状を接続してください。".into())
}
fn curve(a: &[&Value]) -> Result<Arc<Curve>, String> {
    match shape(a)? {
        Value::Curve(c) => Ok(c.clone()),
        _ => Err("曲線を接続してください。".into()),
    }
}
pub(crate) fn as_mesh(mesh: Mesh, cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    validate_mesh(&mesh, cancel)?;
    Ok(vec![Value::Mesh(Arc::new(mesh))])
}
pub(crate) fn as_curve(points: Vec<[f64; 3]>, closed: bool) -> Result<Vec<Value>, String> {
    if points.iter().any(|p| !finite_point(*p)) {
        return Err("曲線の座標が大きすぎます。".into());
    }
    let geometry = crate::curve_geometry::CurveGeometry::nurbs(
        crate::curve_geometry::NurbsCurve::polyline(points.clone(), closed)?,
    );
    Ok(vec![Value::Curve(Arc::new(Curve {
        points,
        closed,
        geometry: Some(geometry),
    }))])
}

fn run(kind: NodeKind, a: &[&Value], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    match kind {
        k if crate::mathematical_surfaces::is_kind(k) => {
            crate::mathematical_surfaces::run(k, a, cancel)
        }
        k if crate::curve_nodes::is_curve_kind(k) => crate::curve_nodes::run_item(k, a, cancel),
        k if crate::gh_gaps::is_kind(k) => crate::gh_gaps::run_item(k, a, cancel),
        k if crate::nurbs_nodes::is_kind(k) => crate::nurbs_nodes::run(k, a, cancel),
        k if crate::bio_patterns::is_kind(k) => crate::bio_patterns::run_item(k, a, cancel),
        k if crate::matter::is_kind(k) => crate::matter::run(k, a, cancel),
        k if crate::physics_nodes::is_item_kind(k) => crate::physics_nodes::run_item(k, a, cancel),
        k if crate::mechanism_nodes::is_kind(k) => crate::mechanism_nodes::run(k, a, cancel),
        GenesisScopes | GenesisScopeIds | GenesisRelief => {
            crate::genesis::run_item(kind, a, cancel)
        }
        OrigamiMiura | OrigamiMiuraCreases => crate::origami::run(kind, a, cancel),
        BioCoiledShell => crate::biomorph::run_shell(a, cancel),
        Number => Ok(vec![Value::Number(num(a, 0)?)]),
        Add | Multiply | Divide => {
            let x = num(a, 0)?;
            let y = num(a, 1)?;
            if kind == Divide && y == 0. {
                return Err("ゼロでは割れません。B を 0 以外にしてください。".into());
            }
            let v = match kind {
                Add => x + y,
                Multiply => x * y,
                _ => x / y,
            };
            if !v.is_finite() {
                return Err("計算結果が大きすぎます。".into());
            }
            Ok(vec![Value::Number(v)])
        }
        Subtract | Power | Min | Max | Modulo => {
            let x = num(a, 0)?;
            let y = num(a, 1)?;
            let v = match kind {
                Subtract => x - y,
                Min => x.min(y),
                Max => x.max(y),
                Modulo => {
                    if y == 0. {
                        return Err(
                            "ゼロでの剰余は計算できません。B を 0 以外にしてください。".into()
                        );
                    }
                    x.rem_euclid(y)
                }
                _ => {
                    if x == 0. && y < 0. {
                        return Err(
                            "0 を負の指数でべき乗できません。底か指数を変えてください。".into()
                        );
                    }
                    if x < 0. && (y - y.round()).abs() > 1e-9 {
                        return Err("負の数は整数の指数でだけ累乗できます。".into());
                    }
                    x.powf(y)
                }
            };
            if !v.is_finite() {
                return Err("計算結果が大きすぎます。".into());
            }
            Ok(vec![Value::Number(v)])
        }
        Abs => Ok(vec![Value::Number(num(a, 0)?.abs())]),
        Sine => {
            let x = num(a, 0)?;
            let amp = num(a, 1)?;
            let frequency = num(a, 2)?;
            let phase = num(a, 3)?.to_radians();
            let v = amp * (TAU * frequency * x + phase).sin();
            if !v.is_finite() {
                return Err("計算結果が大きすぎます。".into());
            }
            Ok(vec![Value::Number(v)])
        }
        Range => {
            let start = num(a, 0)?;
            let end = num(a, 1)?;
            let count = num(a, 2)? as usize;
            if count == 0 {
                return Ok(Vec::new());
            }
            if count == 1 {
                return Ok(vec![Value::Number(start)]);
            }
            let span = end - start;
            Ok((0..count)
                .map(|i| {
                    if i + 1 == count {
                        Value::Number(end)
                    } else {
                        Value::Number(start + span * i as f64 / (count - 1) as f64)
                    }
                })
                .collect())
        }
        Series => {
            let start = num(a, 0)?;
            let step = num(a, 1)?;
            let count = num(a, 2)? as usize;
            Ok((0..count)
                .map(|i| Value::Number(start + step * i as f64))
                .collect())
        }
        Point => Ok(vec![Value::Point([num(a, 0)?, num(a, 1)?, num(a, 2)?])]),
        Circle => {
            let r = num(a, 0)?;
            let n = num(a, 1)? as usize;
            let geom = crate::curve_geometry::CurveGeometry::circle([0.; 3], [0., 0., 1.], r)?;
            // Keep the legacy angular sampling and vertex count for existing lofts/exports.
            let points = (0..n)
                .map(|i| {
                    let t = TAU * i as f64 / n as f64;
                    [r * t.cos(), r * t.sin(), 0.]
                })
                .collect();
            Ok(vec![Value::Curve(Arc::new(Curve {
                points,
                closed: true,
                geometry: Some(geom),
            }))])
        }
        Polygon => {
            let r = num(a, 0)?;
            let n = num(a, 1)? as usize;
            as_curve(
                (0..n)
                    .map(|i| {
                        let t = TAU * i as f64 / n as f64;
                        [r * t.cos(), r * t.sin(), 0.]
                    })
                    .collect(),
                true,
            )
        }
        Star => {
            let r = num(a, 0)?;
            let inner = num(a, 1)?;
            let n = num(a, 2)? as usize;
            if inner >= r {
                return Err("内半径は外半径より小さくしてください。".into());
            }
            as_curve(
                (0..n * 2)
                    .map(|i| {
                        let t = TAU * i as f64 / (n * 2) as f64;
                        let radius = if i % 2 == 0 { r } else { inner };
                        [radius * t.cos(), radius * t.sin(), 0.]
                    })
                    .collect(),
                true,
            )
        }
        Rectangle => {
            let x = num(a, 0)? / 2.;
            let y = num(a, 1)? / 2.;
            as_curve(
                vec![[-x, -y, 0.], [x, -y, 0.], [x, y, 0.], [-x, y, 0.]],
                true,
            )
        }
        Helix => {
            let r = num(a, 0)?;
            let h = num(a, 1)?;
            let turns = num(a, 2)?;
            let n = num(a, 3)? as usize;
            let geometry = crate::curve_geometry::CurveGeometry::helix(
                [0.; 3],
                [r, 0., 0.],
                [0., r, 0.],
                [0., 0., h],
                turns,
                0.,
            )?;
            let points = (0..=n)
                .map(|i| {
                    let t = i as f64 / n as f64;
                    let ang = t * turns * TAU;
                    [r * ang.cos(), r * ang.sin(), t * h]
                })
                .collect();
            Ok(vec![Value::Curve(Arc::new(Curve {
                points,
                closed: false,
                geometry: Some(geometry),
            }))])
        }
        Extrude => as_mesh(
            extrude(curve(a)?.as_ref(), num(a, 1)?, num(a, 2)? as usize, cancel)?,
            cancel,
        ),
        Revolve => as_mesh(
            revolve(curve(a)?.as_ref(), num(a, 1)?, num(a, 2)? as usize, cancel)?,
            cancel,
        ),
        Vase => as_mesh(
            vase(
                num(a, 0)?,
                num(a, 1)?,
                num(a, 2)?,
                num(a, 3)? as usize,
                num(a, 4)?,
                num(a, 5)? as usize,
                num(a, 6)? as usize,
                cancel,
            )?,
            cancel,
        ),
        Sphere => as_mesh(
            sphere(num(a, 0)?, num(a, 1)? as usize, num(a, 2)? as usize, cancel)?,
            cancel,
        ),
        Box => as_mesh(box_mesh(num(a, 0)?, num(a, 1)?, num(a, 2)?), cancel),
        Cylinder => as_mesh(
            cylinder(num(a, 0)?, num(a, 1)?, num(a, 2)? as usize, cancel)?,
            cancel,
        ),
        Cone => as_mesh(
            cone(num(a, 0)?, num(a, 1)?, num(a, 2)? as usize, cancel)?,
            cancel,
        ),
        Torus => as_mesh(
            torus(
                num(a, 0)?,
                num(a, 1)?,
                num(a, 2)? as usize,
                num(a, 3)? as usize,
                cancel,
            )?,
            cancel,
        ),
        Plane => as_mesh(
            plane(num(a, 0)?, num(a, 1)?, num(a, 2)? as usize, cancel)?,
            cancel,
        ),
        Move => {
            let d = [num(a, 1)?, num(a, 2)?, num(a, 3)?];
            Ok(vec![map_affine_shape(
                shape(a)?,
                |p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]],
                cancel,
            )?])
        }
        Rotate => {
            let rad = num(a, 1)?.to_radians();
            let axis = num(a, 2)? as usize;
            Ok(vec![map_affine_shape(
                shape(a)?,
                |p| rotate(p, rad, axis),
                cancel,
            )?])
        }
        Scale => {
            let factor = num(a, 1)?;
            Ok(vec![map_affine_shape(
                shape(a)?,
                |p| [p[0] * factor, p[1] * factor, p[2] * factor],
                cancel,
            )?])
        }
        ScaleXyz => {
            let f = [num(a, 1)?, num(a, 2)?, num(a, 3)?];
            Ok(vec![map_affine_shape(
                shape(a)?,
                |p| [p[0] * f[0], p[1] * f[1], p[2] * f[2]],
                cancel,
            )?])
        }
        Mirror => mirror(shape(a)?, num(a, 1)? as usize, num(a, 2)? != 0., cancel),
        Twist => {
            let value = shape(a)?;
            let (lo, hi) = z_bounds(value)?;
            if hi - lo < 1e-9 {
                return Err("ねじりには高さのある形状が必要です。先に押し出してください。".into());
            }
            let angle = num(a, 1)?.to_radians();
            Ok(vec![map_shape(
                value,
                |p| rotate(p, angle * (p[2] - lo) / (hi - lo), 2),
                cancel,
            )?])
        }
        Wave => {
            let amp = num(a, 1)?;
            let wavelength = num(a, 2)?;
            let phase = num(a, 3)?.to_radians();
            Ok(vec![map_shape(
                shape(a)?,
                |p| {
                    [
                        p[0],
                        p[1],
                        p[2] + amp * (TAU * p[0] / wavelength + phase).sin(),
                    ]
                },
                cancel,
            )?])
        }
        LinearArray | RadialArray => {
            let source = shape(a)?;
            let count = num(a, 1)? as usize;
            if source.weight().saturating_mul(count) > MAX_TOTAL_WEIGHT / 2 {
                return Err(
                    "配列が大きすぎます。個数または元の形状の分割数を減らしてください。".into(),
                );
            }
            let mut values = Vec::with_capacity(count);
            for index in 0..count {
                cancelled(cancel)?;
                if index == 0 {
                    values.push(source.clone());
                    continue;
                }
                if kind == LinearArray {
                    let d = [
                        num(a, 2)? * index as f64,
                        num(a, 3)? * index as f64,
                        num(a, 4)? * index as f64,
                    ];
                    values.push(map_affine_shape(
                        source,
                        |p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]],
                        cancel,
                    )?);
                } else {
                    let total = num(a, 2)?;
                    let divisor = if (total - 360.).abs() < 1e-9 {
                        count
                    } else {
                        count.saturating_sub(1).max(1)
                    };
                    let rad = total.to_radians() * index as f64 / divisor as f64;
                    values.push(map_affine_shape(source, |p| rotate(p, rad, 2), cancel)?);
                }
            }
            Ok(values)
        }
        RectGrid | TriangularGrid | HexGrid | RadialGrid | HexPanels | TriPanels | Voronoi2d
        | ConvexHull2d | ClosestPoint | MultiAttractor | CurveAttractor | GridArray
        | PointArray | Rotate3d | Shear | Taper | FractalTree | CylinderWrap | Delaunay2d
        | PopulateMesh | CurveArray | NoiseField | LifeGrid | TruchetPattern | AsanohaPattern
        | ShippoPattern | PowerDiagram2d | LloydRelax2d => {
            crate::generative::run_item(kind, a, cancel)
        }
        Random | Populate | Noise | NoiseDeform | Shell | OverhangCheck | Distance
        | PointComponent => crate::gh_nodes::run_item(kind, a, cancel),
        ArcCurve | Polyline | SmoothCurve | OffsetCurve | PointOnCurve | DivideCurve | Pipe
        | Sweep | SmoothMesh | Subdivide | WeldMesh | UnifyNormals | MeshInfo | TaperScale
        | Bend | GridArray3d | Gyroid | RevolvePoints => {
            crate::nodes_shape::run_item(kind, a, cancel)
        }
        PlaneSurface
        | NurbsPanel
        | BilinearSurface
        | CylinderSurface
        | FrustumSurface
        | SphereSurface
        | TorusSurface
        | EvaluateSurface
        | SurfaceDirection
        | SurfaceNormalLine
        | SurfaceArea
        | SurfaceClosestPoint
        | SurfaceIsoCurve
        | DivideSurface
        | SurfaceMesh
        | BrepBox
        | BrepVolume
        | BrepArea
        | BrepIntersection
        | BrepMove
        | BrepFaces
        | BrepEdges
        | BrepVertices
        | BrepMesh
        | NurbsVase
        | RuledSurface
        | SurfaceExtrude
        | BrepUnion
        | BrepDifference
        | BrepOffset
        | SurfaceGaussianCurvature
        | SurfaceMeanCurvature
        | SurfacePrincipalCurvatures => crate::surface_nodes::run(kind, a, cancel),
        MeshArea | MeshVolume | MeshCenter | MeshDimensions | MeshMass | MeshAudit
        | MeshVertices | MeshFaces | MeshNormals | MeshEdges | MeshBoundary | MeshClosest
        | MeshFlip | MeshWeldExact | MeshUnweld | MeshUnify | MeshSmooth | MeshSubdivide
        | MeshCatmullClark | MeshOnBed | MeshDeleteFace | MeshSelfContact | MeshThicknessSample
        | MeshBedFit | MeshCompact | MeshNonManifold => {
            crate::mesh_fabrication::run_item(kind, a, cancel)
        }
        MeshConstruct | Loft | Merge | Flatten | Graft | Output | Reroute | Script | Cluster
        | ListItem | ListLength | ReverseList | SortList | CullPattern | ShiftList | ConcatList
        | SubList | Bounds | Remap | Expression | GraphProfile | ImportMesh | SimplifyTree
        | TrimTree | PruneTree | MergeTree | EntwineTree | FlipMatrix | UnflattenTree
        | PathMapper | PartitionList | TreeBranch | TreeItem | TreeStatistics | CullNth
        | CullIndex | DuplicateData | RepeatData | MassAddition | MassMultiplication
        | ReplaceItems | InsertItems | MemberIndex | RandomReduce | Jitter => {
            Err("このノードはリスト評価が必要です。".into())
        }
        other => crate::math_nodes::run_item(other, a, cancel),
    }
}

pub(crate) fn finite_point(p: [f64; 3]) -> bool {
    p.into_iter().all(|v| v.is_finite() && v.abs() <= 1e9)
}
pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn length(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
pub(crate) fn validate_mesh(m: &Mesh, cancel: &dyn Fn() -> bool) -> Result<(), String> {
    if m.vertices.len() > MAX_VERTICES || m.triangles.len() > MAX_TRIANGLES {
        return Err("メッシュが上限を超えています。分割数を減らしてください。".into());
    }
    for chunk in m.vertices.chunks(512) {
        cancelled(cancel)?;
        if chunk.iter().any(|p| !finite_point(*p)) {
            return Err("メッシュに無効な座標があります。".into());
        }
    }
    for (index, t) in m.triangles.iter().enumerate() {
        if index % 512 == 0 {
            cancelled(cancel)?;
        }
        let [Some(a), Some(b), Some(c)] = t.map(|i| m.vertices.get(i as usize)) else {
            return Err("メッシュの頂点番号が無効です。".into());
        };
        if length(cross(sub(*b, *a), sub(*c, *a))) < 1e-12 {
            return Err("面積がゼロの面ができました。寸法や分割数を調整してください。".into());
        }
    }
    Ok(())
}
pub(crate) fn map_shape(
    v: &Value,
    f: impl Fn([f64; 3]) -> [f64; 3],
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let map = |points: &[[f64; 3]]| -> Result<Vec<[f64; 3]>, String> {
        let mut out = Vec::with_capacity(points.len());
        for (i, p) in points.iter().enumerate() {
            if i % 256 == 0 {
                cancelled(cancel)?;
            }
            let q = f(*p);
            if !finite_point(q) {
                return Err("変形後の座標が大きすぎます。値を小さくしてください。".into());
            }
            out.push(q);
        }
        Ok(out)
    };
    match v {
        Value::Point(p) => {
            let q = f(*p);
            if !finite_point(q) {
                return Err("変形後の座標が無効です。".into());
            }
            Ok(Value::Point(q))
        }
        Value::Curve(c) => Ok(Value::Curve(Arc::new(Curve {
            points: map(&c.points)?,
            closed: c.closed,
            // Generic map may be nonlinear (twist, wave, noise). Never retain stale precision.
            geometry: None,
        }))),
        Value::Mesh(m) => {
            let mesh = Mesh {
                vertices: map(&m.vertices)?,
                triangles: m.triangles.clone(),
            };
            validate_mesh(&mesh, cancel)?;
            Ok(Value::Mesh(Arc::new(mesh)))
        }
        Value::Number(_) => Err("点・曲線・メッシュのいずれかを接続してください。".into()),
        Value::Surface(_) | Value::Solid(_) | Value::Brep(_) | Value::Cage(_) => Err(
            "精密曲面・B-rep・ケージは専用ノードで変換するか、明示的にメッシュ化してください。"
                .into(),
        ),
        Value::Gcode(_) => Err("点・曲線・メッシュのいずれかを接続してください。".into()),
    }
}
pub(crate) fn map_affine_shape(
    v: &Value,
    f: impl Fn([f64; 3]) -> [f64; 3],
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let mut mapped = map_shape(v, &f, cancel)?;
    if let (Value::Curve(before), Value::Curve(after)) = (v, &mut mapped)
        && let Some(g) = &before.geometry
    {
        Arc::make_mut(after).geometry = Some(g.affine(&f)?);
    }
    Ok(mapped)
}
fn rotate(p: [f64; 3], r: f64, axis: usize) -> [f64; 3] {
    let (s, c) = r.sin_cos();
    match axis {
        0 => [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c],
        1 => [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c],
        _ => [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]],
    }
}
fn z_bounds(v: &Value) -> Result<(f64, f64), String> {
    let points: &[[f64; 3]] = match v {
        Value::Point(p) => std::slice::from_ref(p),
        Value::Curve(c) => &c.points,
        Value::Mesh(m) => &m.vertices,
        _ => return Err("形状を接続してください。".into()),
    };
    if points.is_empty() {
        return Err("形状に頂点がありません。".into());
    }
    Ok(points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p[2]), b.max(p[2]))
        }))
}
pub(crate) fn checked_size(vertices: usize, triangles: usize) -> Result<(), String> {
    if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
        Err("形状が大きすぎます。分割数を減らしてください。".into())
    } else {
        Ok(())
    }
}
fn cylinder(
    radius: f64,
    height: f64,
    segments: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if segments < 3 {
        return Err("周の分割は 3 以上にしてください。".into());
    }
    checked_size(
        segments.saturating_mul(2).saturating_add(2),
        segments.saturating_mul(4),
    )?;
    let mut m = Mesh::default();
    for z in [0., height] {
        cancelled(cancel)?;
        for j in 0..segments {
            let angle = TAU * j as f64 / segments as f64;
            m.vertices
                .push([radius * angle.cos(), radius * angle.sin(), z]);
        }
    }
    let bottom = (segments * 2) as u32;
    m.vertices.push([0., 0., 0.]);
    let top = bottom + 1;
    m.vertices.push([0., 0., height]);
    for j in 0..segments {
        let a = j as u32;
        let b = ((j + 1) % segments) as u32;
        let c = a + segments as u32;
        let d = b + segments as u32;
        m.triangles.extend([[a, b, c], [b, d, c]]);
        m.triangles.push([bottom, b, a]);
        m.triangles.push([top, c, d]);
    }
    Ok(m)
}
fn cone(
    radius: f64,
    height: f64,
    segments: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if segments < 3 {
        return Err("周の分割は 3 以上にしてください。".into());
    }
    checked_size(segments.saturating_add(2), segments.saturating_mul(2))?;
    cancelled(cancel)?;
    let mut m = Mesh::default();
    for j in 0..segments {
        let angle = TAU * j as f64 / segments as f64;
        m.vertices
            .push([radius * angle.cos(), radius * angle.sin(), 0.]);
    }
    let apex = segments as u32;
    m.vertices.push([0., 0., height]);
    let center = apex + 1;
    m.vertices.push([0., 0., 0.]);
    for j in 0..segments {
        let a = j as u32;
        let b = ((j + 1) % segments) as u32;
        m.triangles.push([a, b, apex]);
        m.triangles.push([center, b, a]);
    }
    Ok(m)
}
fn torus(
    major: f64,
    minor: f64,
    segments: usize,
    tube: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if segments < 3 || tube < 3 {
        return Err("分割数は 3 以上にしてください。".into());
    }
    if minor >= major {
        return Err("副半径は主半径より小さくしてください。".into());
    }
    let count = segments.saturating_mul(tube);
    checked_size(count, count.saturating_mul(2))?;
    let mut m = Mesh::default();
    for i in 0..segments {
        cancelled(cancel)?;
        let u = TAU * i as f64 / segments as f64;
        for j in 0..tube {
            let v = TAU * j as f64 / tube as f64;
            let ring = major + minor * v.cos();
            m.vertices
                .push([ring * u.cos(), ring * u.sin(), minor * v.sin()]);
        }
    }
    let index = |i: usize, j: usize| ((i % segments) * tube + (j % tube)) as u32;
    for i in 0..segments {
        for j in 0..tube {
            let a = index(i, j);
            let b = index(i + 1, j);
            let c = index(i, j + 1);
            let d = index(i + 1, j + 1);
            m.triangles.extend([[a, b, c], [b, d, c]]);
        }
    }
    Ok(m)
}
fn plane(
    width: f64,
    depth: f64,
    divisions: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if divisions < 1 {
        return Err("分割数は 1 以上にしてください。".into());
    }
    let side = divisions.saturating_add(1);
    checked_size(
        side.saturating_mul(side),
        divisions.saturating_mul(divisions).saturating_mul(2),
    )?;
    let mut m = Mesh::default();
    for iy in 0..side {
        cancelled(cancel)?;
        for ix in 0..side {
            m.vertices.push([
                -width / 2. + width * ix as f64 / divisions as f64,
                -depth / 2. + depth * iy as f64 / divisions as f64,
                0.,
            ]);
        }
    }
    for iy in 0..divisions {
        for ix in 0..divisions {
            let a = (iy * side + ix) as u32;
            let b = a + 1;
            let c = a + side as u32;
            let d = c + 1;
            m.triangles.extend([[a, b, c], [b, d, c]]);
        }
    }
    Ok(m)
}
fn reflect(p: [f64; 3], axis: usize) -> [f64; 3] {
    let mut q = p;
    let k = axis.min(2);
    q[k] = -q[k];
    q
}
// A reflection reverses the triangle winding, so the mirrored copy swaps two indices to
// keep outward normals. With `keep`, a mesh becomes one merged mesh (the two halves are
// not welded); points and curves return the original and the mirrored copy.
fn mirror(
    source: &Value,
    axis: usize,
    keep: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    match source {
        Value::Mesh(m) => {
            let mut mesh = Mesh::default();
            if keep {
                checked_size(
                    m.vertices.len().saturating_mul(2),
                    m.triangles.len().saturating_mul(2),
                )?;
                mesh.vertices.extend(&m.vertices);
                mesh.triangles.extend(&m.triangles);
            }
            let offset = mesh.vertices.len() as u32;
            for (i, p) in m.vertices.iter().enumerate() {
                if i % 256 == 0 {
                    cancelled(cancel)?;
                }
                mesh.vertices.push(reflect(*p, axis));
            }
            mesh.triangles.extend(
                m.triangles
                    .iter()
                    .map(|t| [t[0] + offset, t[2] + offset, t[1] + offset]),
            );
            as_mesh(mesh, cancel)
        }
        Value::Number(_) => Err("点・曲線・メッシュのいずれかを接続してください。".into()),
        other => {
            let mirrored = map_affine_shape(other, |p| reflect(p, axis), cancel)?;
            if keep {
                Ok(vec![other.clone(), mirrored])
            } else {
                Ok(vec![mirrored])
            }
        }
    }
}
pub(crate) fn box_mesh(w: f64, d: f64, h: f64) -> Mesh {
    let (x, y) = (w / 2., d / 2.);
    Mesh {
        vertices: vec![
            [-x, -y, 0.],
            [x, -y, 0.],
            [x, y, 0.],
            [-x, y, 0.],
            [-x, -y, h],
            [x, -y, h],
            [x, y, h],
            [-x, y, h],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 4],
            [1, 5, 4],
            [1, 2, 5],
            [2, 6, 5],
            [2, 3, 6],
            [3, 7, 6],
            [3, 0, 7],
            [0, 4, 7],
        ],
    }
}
fn sphere(
    radius: f64,
    segments: usize,
    rings: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    checked_size(segments * (rings - 1) + 2, segments * (rings - 1) * 2)?;
    let mut m = Mesh {
        vertices: vec![[0., 0., radius]],
        triangles: Vec::new(),
    };
    for row in 1..rings {
        cancelled(cancel)?;
        let p = PI * row as f64 / rings as f64;
        for j in 0..segments {
            let a = TAU * j as f64 / segments as f64;
            m.vertices.push([
                radius * p.sin() * a.cos(),
                radius * p.sin() * a.sin(),
                radius * p.cos(),
            ]);
        }
    }
    let south = m.vertices.len() as u32;
    m.vertices.push([0., 0., -radius]);
    for j in 0..segments {
        m.triangles
            .push([0, (1 + j) as u32, (1 + (j + 1) % segments) as u32]);
    }
    for row in 0..rings - 2 {
        for j in 0..segments {
            let a = (1 + row * segments + j) as u32;
            let b = (1 + row * segments + (j + 1) % segments) as u32;
            let c = a + segments as u32;
            let d = b + segments as u32;
            m.triangles.extend([[a, c, b], [b, c, d]]);
        }
    }
    let last = 1 + (rings - 2) * segments;
    for j in 0..segments {
        m.triangles
            .push([south, (last + (j + 1) % segments) as u32, (last + j) as u32]);
    }
    Ok(m)
}

#[allow(clippy::too_many_arguments)]
fn vase(
    radius: f64,
    height: f64,
    thickness: f64,
    lobes: usize,
    amplitude: f64,
    segments: usize,
    layers: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if thickness >= height * 0.5 || thickness >= radius * 0.65 * (1. - amplitude) {
        return Err("肉厚が大きすぎます。高さの半分・最小半径より十分小さくしてください。".into());
    }
    if lobes > 0 && segments < lobes * 4 {
        return Err("周の分割を波の数の 4 倍以上にしてください。".into());
    }
    let ring_size = (layers + 1) * segments;
    checked_size(ring_size * 2 + 2, layers * segments * 4 + segments * 4)?;
    let profile = |z: f64, a: f64| {
        let t = z / height;
        radius
            * (0.82 + 0.18 * (PI * t).sin() + 0.12 * t)
            * (1.
                + if lobes == 0 {
                    0.
                } else {
                    amplitude * (lobes as f64 * a).cos()
                })
    };
    let mut m = Mesh::default();
    for inner in [false, true] {
        for row in 0..=layers {
            cancelled(cancel)?;
            let t = row as f64 / layers as f64;
            let z = if inner {
                thickness + (height - thickness) * t
            } else {
                height * t
            };
            for j in 0..segments {
                let a = TAU * j as f64 / segments as f64;
                let r = profile(z, a) - if inner { thickness } else { 0. };
                m.vertices.push([r * a.cos(), r * a.sin(), z]);
            }
        }
    }
    for row in 0..layers {
        for j in 0..segments {
            let a = (row * segments + j) as u32;
            let b = (row * segments + (j + 1) % segments) as u32;
            let c = a + segments as u32;
            let d = b + segments as u32;
            m.triangles.extend([[a, b, c], [b, d, c]]);
            let o = ring_size as u32;
            m.triangles
                .extend([[a + o, c + o, b + o], [b + o, c + o, d + o]]);
        }
    }
    let bottom = m.vertices.len() as u32;
    m.vertices.push([0., 0., 0.]);
    let inner_bottom = m.vertices.len() as u32;
    m.vertices.push([0., 0., thickness]);
    for j in 0..segments {
        let next = (j + 1) % segments;
        m.triangles.push([bottom, next as u32, j as u32]);
        m.triangles.push([
            inner_bottom,
            (ring_size + j) as u32,
            (ring_size + next) as u32,
        ]);
        let a = (layers * segments + j) as u32;
        let b = (layers * segments + next) as u32;
        let c = a + ring_size as u32;
        let d = b + ring_size as u32;
        m.triangles.extend([[a, b, c], [b, d, c]]);
    }
    Ok(m)
}

fn extrude(
    curve: &Curve,
    height: f64,
    layers: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if !curve.closed || curve.points.len() < 3 {
        return Err(
            "押し出しには閉じた断面が必要です。円・星形・長方形などを接続してください。".into(),
        );
    }
    let mut points = curve.points.clone();
    let z = points[0][2];
    if points.iter().any(|p| (p[2] - z).abs() > 1e-6) {
        return Err("押し出しの断面は XY 平面に平行にしてください。".into());
    }
    if signed_xy_area(&points) < 0. {
        points.reverse();
    }
    let caps = triangulate_polygon(&points, cancel)?;
    let n = points.len();
    checked_size(n * (layers + 1), 2 * n * layers + 2 * caps.len())?;
    let mut m = Mesh::default();
    for row in 0..=layers {
        cancelled(cancel)?;
        let dz = height * row as f64 / layers as f64;
        m.vertices
            .extend(points.iter().map(|p| [p[0], p[1], p[2] + dz]));
    }
    for row in 0..layers {
        for j in 0..n {
            let a = (row * n + j) as u32;
            let b = (row * n + (j + 1) % n) as u32;
            let c = a + n as u32;
            let d = b + n as u32;
            m.triangles.extend([[a, b, c], [b, d, c]]);
        }
    }
    let last = (layers * n) as u32;
    for [a, b, c] in caps {
        m.triangles.push([c, b, a]);
        m.triangles.push([a + last, b + last, c + last]);
    }
    Ok(m)
}
fn signed_xy_area(points: &[[f64; 3]]) -> f64 {
    (0..points.len())
        .map(|i| {
            let p = points[i];
            let q = points[(i + 1) % points.len()];
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        / 2.
}

fn loft(values: &[Value], cap: bool, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    if values.len() < 2 {
        return Err(
            "ロフトには同じブランチに 2 本以上の断面が必要です。数列と移動で断面を並べてください。"
                .into(),
        );
    }
    let curves: Vec<_> = values
        .iter()
        .map(|v| match v {
            Value::Curve(c) => Ok(c.clone()),
            _ => Err("ロフトには曲線のリストを接続してください。".to_string()),
        })
        .collect::<Result<_, _>>()?;
    let n = curves[0].points.len();
    let closed = curves[0].closed;
    if n < 2
        || curves
            .iter()
            .any(|c| c.points.len() != n || c.closed != closed)
    {
        return Err("ロフトの断面は点数と開閉状態をそろえてください。".into());
    }
    checked_size(n * curves.len(), n * (curves.len() - 1) * 2 + n * 2)?;
    let mut m = Mesh::default();
    for c in &curves {
        cancelled(cancel)?;
        m.vertices.extend(&c.points);
    }
    let edges = if closed { n } else { n - 1 };
    for row in 0..curves.len() - 1 {
        cancelled(cancel)?;
        for j in 0..edges {
            let a = (row * n + j) as u32;
            let b = (row * n + (j + 1) % n) as u32;
            let c = a + n as u32;
            let d = b + n as u32;
            m.triangles.extend([[a, b, c], [b, d, c]]);
        }
    }
    if cap && closed {
        let first = triangulate_polygon(&curves[0].points, cancel)?;
        let last = triangulate_polygon(&curves[curves.len() - 1].points, cancel)?;
        for [a, b, c] in first {
            m.triangles.push([c, b, a]);
        }
        let offset = ((curves.len() - 1) * n) as u32;
        for [a, b, c] in last {
            m.triangles.push([a + offset, b + offset, c + offset]);
        }
        orient_outward(&mut m);
    }
    validate_mesh(&m, cancel)?;
    Ok(m)
}
fn revolve(
    curve: &Curve,
    angle: f64,
    steps: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if curve.points.len() < 2 {
        return Err("回転する断面に 2 点以上が必要です。".into());
    }
    if curve
        .points
        .iter()
        .any(|p| p[1].abs() > 1e-6 || p[0] <= 1e-6)
    {
        return Err("断面を XZ 平面に置き、全ての X 座標を 0 より大きくしてください。移動と X 軸回転を使えます。".into());
    }
    let full = (angle - 360.).abs() < 1e-8;
    let columns = if full { steps } else { steps + 1 };
    let n = curve.points.len();
    checked_size(n * columns, steps * n * 2 + n * 2)?;
    let mut m = Mesh::default();
    for column in 0..columns {
        cancelled(cancel)?;
        let a = angle.to_radians() * column as f64 / steps as f64;
        for p in &curve.points {
            m.vertices.push(rotate(*p, a, 2));
        }
    }
    let edges = if curve.closed { n } else { n - 1 };
    for column in 0..steps {
        cancelled(cancel)?;
        for j in 0..edges {
            let a = (column * n + j) as u32;
            let b = (column * n + (j + 1) % n) as u32;
            let c = (((column + 1) % columns) * n + j) as u32;
            let d = (((column + 1) % columns) * n + (j + 1) % n) as u32;
            m.triangles.extend([[a, b, c], [b, d, c]]);
        }
    }
    if curve.closed && !full {
        let triangles = triangulate_polygon(&curve.points, cancel)?;
        let offset = ((columns - 1) * n) as u32;
        for [a, b, c] in triangles {
            m.triangles.push([c, b, a]);
            m.triangles.push([a + offset, b + offset, c + offset]);
        }
    }
    if curve.closed {
        orient_outward(&mut m);
    }
    validate_mesh(&m, cancel)?;
    Ok(m)
}
pub(crate) fn orient_outward(m: &mut Mesh) {
    let volume = m
        .triangles
        .iter()
        .map(|t| {
            dot(
                m.vertices[t[0] as usize],
                cross(m.vertices[t[1] as usize], m.vertices[t[2] as usize]),
            )
        })
        .sum::<f64>();
    if volume < 0. {
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
    }
}
fn merge_meshes(values: &[Value], cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let mut m = Mesh::default();
    for value in values {
        cancelled(cancel)?;
        let Value::Mesh(part) = value else {
            return Err("メッシュだけを接続してください。".into());
        };
        checked_size(
            m.vertices.len().saturating_add(part.vertices.len()),
            m.triangles.len().saturating_add(part.triangles.len()),
        )?;
        let offset = m.vertices.len() as u32;
        m.vertices.extend(&part.vertices);
        m.triangles.extend(
            part.triangles
                .iter()
                .map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]),
        );
    }
    Ok(m)
}

// Ear clipping supports concave profiles; a fan would incorrectly fill star-shaped cutouts.
fn triangulate_polygon(
    points: &[[f64; 3]],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<[u32; 3]>, String> {
    let n = points.len();
    if !(3..=2048).contains(&n) {
        return Err("断面の頂点数は 3 〜 2048 にしてください。".into());
    }
    let mut normal = [0.; 3];
    for i in 0..n {
        let p = points[i];
        let q = points[(i + 1) % n];
        normal[0] += (p[1] - q[1]) * (p[2] + q[2]);
        normal[1] += (p[2] - q[2]) * (p[0] + q[0]);
        normal[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    let norm = length(normal);
    if norm < 1e-10 {
        return Err("断面の面積がゼロ、または自己交差しています。".into());
    }
    let origin = points[0];
    let scale = points
        .iter()
        .map(|p| length(sub(*p, origin)))
        .fold(1., f64::max);
    if points
        .iter()
        .any(|p| dot(normal, sub(*p, origin)).abs() / norm > 1e-6 * scale)
    {
        return Err("断面の点が 1 つの平面上にありません。".into());
    }
    let axis = if normal[0].abs() > normal[1].abs() && normal[0].abs() > normal[2].abs() {
        0
    } else if normal[1].abs() > normal[2].abs() {
        1
    } else {
        2
    };
    let p: Vec<[f64; 2]> = points
        .iter()
        .map(|p| match axis {
            0 => [p[1], p[2]],
            1 => [p[2], p[0]],
            _ => [p[0], p[1]],
        })
        .collect();
    let eps = 1e-10 * scale * scale;
    let signed = (0..n)
        .map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1])
        .sum::<f64>();
    let orientation = if signed >= 0. { 1. } else { -1. };
    for i in 0..n {
        if i % 32 == 0 {
            cancelled(cancel)?;
        }
        let next = (i + 1) % n;
        if dist2(p[i], p[next]) < eps * 1e-4 {
            return Err("断面に重複する点があります。".into());
        }
        for j in i + 1..n {
            let jnext = (j + 1) % n;
            if i == j || next == j || jnext == i {
                continue;
            }
            if segments_intersect(p[i], p[next], p[j], p[jnext], eps) {
                return Err("断面が自己交差しています。".into());
            }
        }
    }
    let mut indices: Vec<usize> = (0..n).collect();
    let mut triangles = Vec::with_capacity(n - 2);
    while indices.len() > 3 {
        cancelled(cancel)?;
        let mut ear = None;
        for k in 0..indices.len() {
            if k % 16 == 0 {
                cancelled(cancel)?;
            }
            let a = indices[(k + indices.len() - 1) % indices.len()];
            let b = indices[k];
            let c = indices[(k + 1) % indices.len()];
            if orient2(p[a], p[b], p[c]) * orientation <= eps {
                continue;
            }
            if indices
                .iter()
                .copied()
                .filter(|q| *q != a && *q != b && *q != c)
                .any(|q| point_in_triangle(p[q], p[a], p[b], p[c], orientation, eps))
            {
                continue;
            }
            ear = Some((k, [a as u32, b as u32, c as u32]));
            break;
        }
        let Some((index, t)) = ear else {
            return Err("断面を三角形に分割できません。形状を単純にしてください。".into());
        };
        triangles.push(t);
        indices.remove(index);
    }
    triangles.push([indices[0] as u32, indices[1] as u32, indices[2] as u32]);
    Ok(triangles)
}
fn orient2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}
fn point_in_triangle(
    p: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    orientation: f64,
    eps: f64,
) -> bool {
    orient2(a, b, p) * orientation >= -eps
        && orient2(b, c, p) * orientation >= -eps
        && orient2(c, a, p) * orientation >= -eps
}
fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], eps: f64) -> bool {
    let o = [
        orient2(a, b, c),
        orient2(a, b, d),
        orient2(c, d, a),
        orient2(c, d, b),
    ];
    if ((o[0] > eps && o[1] < -eps) || (o[0] < -eps && o[1] > eps))
        && ((o[2] > eps && o[3] < -eps) || (o[2] < -eps && o[3] > eps))
    {
        return true;
    }
    let on = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        r[0] >= p[0].min(q[0]) - 1e-9
            && r[0] <= p[0].max(q[0]) + 1e-9
            && r[1] >= p[1].min(q[1]) - 1e-9
            && r[1] <= p[1].max(q[1]) + 1e-9
    };
    (o[0].abs() <= eps && on(a, b, c))
        || (o[1].abs() <= eps && on(a, b, d))
        || (o[2].abs() <= eps && on(c, d, a))
        || (o[3].abs() <= eps && on(c, d, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn add(g: &mut Graph, k: NodeKind) -> NodeId {
        g.add(k, [0., 0.]).unwrap()
    }
    fn mesh_of(ev: &Evaluation, id: NodeId) -> Arc<Mesh> {
        let t = ev.outputs[&id].as_ref().unwrap();
        match t.branches.values().flatten().next().unwrap() {
            Value::Mesh(m) => m.clone(),
            v => panic!("mesh expected, got {:?}", v),
        }
    }
    fn closed_positive(m: &Mesh) {
        let mut edges: BTreeMap<(u32, u32), (usize, i32)> = BTreeMap::new();
        for t in &m.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let e = edges.entry((a.min(b), a.max(b))).or_default();
                e.0 += 1;
                e.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(
            edges.values().all(|e| *e == (2, 0)),
            "non-manifold or inconsistent edge"
        );
        let v = m
            .triangles
            .iter()
            .map(|t| {
                dot(
                    m.vertices[t[0] as usize],
                    cross(m.vertices[t[1] as usize], m.vertices[t[2] as usize]),
                ) / 6.
            })
            .sum::<f64>();
        assert!(v > 0., "negative volume {v}");
    }

    #[test]
    fn interrupted_evaluation_also_bounds_unvisited_cached_meshes() {
        let mut graph = Graph::default();
        for _ in 0..24 {
            add(&mut graph, NodeKind::Vase);
        }
        let mut engine = Evaluator::default();
        engine.set_max_threads(1); // call counts below must be deterministic
        assert!(
            engine
                .evaluate(&graph, &|| false)
                .outputs
                .values()
                .all(Result::is_ok)
        );
        for node in graph.nodes.iter_mut().take(8) {
            node.params.insert("segments".into(), 192.);
            node.params.insert("layers".into(), 128.);
        }
        // Count a deterministic eight-node prefix, then interrupt the real graph
        // before its sixteen old cached meshes are visited or replaced.
        let mut prefix = graph.clone();
        prefix.nodes.truncate(8);
        let calls = AtomicUsize::new(0);
        let mut prefix_engine = Evaluator::default();
        prefix_engine.set_max_threads(1);
        let prefix_result = prefix_engine.evaluate(&prefix, &|| {
            calls.fetch_add(1, Ordering::SeqCst);
            false
        });
        assert!(prefix_result.outputs.values().all(Result::is_ok));
        let allowed = calls.load(Ordering::SeqCst);
        drop(prefix_result);
        calls.store(0, Ordering::SeqCst);
        let result = engine.evaluate(&graph, &|| {
            calls.fetch_add(1, Ordering::SeqCst) + 1 > allowed / 2
        });
        assert!(result.cancelled);
        assert!(
            result.computed > 0 && result.computed < 24,
            "{}",
            result.computed
        );
        assert!(engine.cache_weight <= MAX_TOTAL_WEIGHT);
        assert_eq!(engine.cache_order.len(), engine.cache.len());
        for (id, cached) in &engine.cache {
            assert!(engine.cache_order.contains(&(cached.revision, *id)));
            assert_eq!(
                cached.weight,
                cached.result.as_ref().map(Tree::weight).unwrap_or(0)
            );
        }
        assert_eq!(
            engine.cache_weight,
            engine
                .cache
                .values()
                .map(|c| c.result.as_ref().map(Tree::weight).unwrap_or(0))
                .sum::<usize>()
        );
    }

    #[test]
    fn nested_arrays_observe_the_remaining_budget_before_each_expansion() {
        let mut graph = Graph::default();
        let vase = add(&mut graph, NodeKind::Vase);
        let first = add(&mut graph, NodeKind::LinearArray);
        graph
            .node_mut(first)
            .unwrap()
            .params
            .insert("count".into(), 32.);
        let second = add(&mut graph, NodeKind::LinearArray);
        graph
            .node_mut(second)
            .unwrap()
            .params
            .insert("count".into(), 4.);
        graph.connect(vase, first, "geometry").unwrap();
        graph.connect(first, second, "geometry").unwrap();
        let result = Evaluator::default().evaluate(&graph, &|| false);
        assert!(result.outputs[&first].is_ok());
        assert!(
            result.outputs[&second]
                .as_ref()
                .unwrap_err()
                .contains("生成前")
        );
    }

    #[test]
    fn mesh_validation_cooperates_with_cancellation() {
        let mesh = sphere(20., 192, 96, &|| false).unwrap();
        let calls = Cell::new(0usize);
        let error = validate_mesh(&mesh, &|| {
            calls.set(calls.get() + 1);
            calls.get() >= 3
        })
        .unwrap_err();
        assert!(error.contains("停止"));
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn a_one_shot_cancel_signal_is_latched_and_not_cached_as_an_error() {
        let mut graph = Graph::default();
        let id = add(&mut graph, NodeKind::Vase);
        let mut engine = Evaluator::default();
        let calls = AtomicUsize::new(0);
        let result = engine.evaluate(&graph, &|| calls.fetch_add(1, Ordering::SeqCst) + 1 == 10);
        assert!(result.cancelled);
        assert!(result.outputs.is_empty());
        assert!(engine.cache.is_empty());
        assert!(engine.evaluate(&graph, &|| false).outputs[&id].is_ok());
    }
    #[test]
    fn vase_and_twisted_vase_are_watertight() {
        let mut g = Graph::default();
        let v = add(&mut g, NodeKind::Vase);
        let t = add(&mut g, NodeKind::Twist);
        g.connect(v, t, "geometry").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        closed_positive(&mesh_of(&ev, v));
        closed_positive(&mesh_of(&ev, t));
    }
    #[test]
    fn sphere_is_closed_without_duplicate_poles() {
        let mut g = Graph::default();
        let id = add(&mut g, NodeKind::Sphere);
        let ev = Evaluator::default().evaluate(&g, &|| false);
        let m = mesh_of(&ev, id);
        closed_positive(&m);
        assert_eq!(m.vertices.len(), 48 * 23 + 2);
    }
    #[test]
    fn concave_star_extrusion_has_expected_volume() {
        let mut g = Graph::default();
        let s = add(&mut g, NodeKind::Star);
        let e = add(&mut g, NodeKind::Extrude);
        g.connect(s, e, "curve").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        let m = mesh_of(&ev, e);
        closed_positive(&m);
        let volume = m
            .triangles
            .iter()
            .map(|t| {
                dot(
                    m.vertices[t[0] as usize],
                    cross(m.vertices[t[1] as usize], m.vertices[t[2] as usize]),
                ) / 6.
            })
            .sum::<f64>();
        let expected = 7. * 30. * 16. * (PI / 7.).sin() * 40.;
        assert!((volume - expected).abs() < expected * 1e-9);
    }
    #[test]
    fn changed_parameter_only_recomputes_descendants() {
        let mut g = Graph::default();
        let n = add(&mut g, NodeKind::Number);
        let a = add(&mut g, NodeKind::Add);
        let b = add(&mut g, NodeKind::Box);
        g.connect(n, a, "a").unwrap();
        let mut engine = Evaluator::default();
        assert_eq!(engine.evaluate(&g, &|| false).computed, 3);
        g.node_mut(n).unwrap().params.insert("value".into(), 8.);
        let ev = engine.evaluate(&g, &|| false);
        assert_eq!((ev.computed, ev.cached), (2, 1));
        assert!(ev.outputs[&b].is_ok());
    }
    #[test]
    fn moving_renaming_and_hiding_cards_do_not_invalidate_cache() {
        let mut g = Graph::default();
        let id = add(&mut g, NodeKind::Vase);
        let mut engine = Evaluator::default();
        engine.evaluate(&g, &|| false);
        let n = g.node_mut(id).unwrap();
        n.position = [20., 30.];
        n.name = "作品".into();
        n.preview = false;
        let ev = engine.evaluate(&g, &|| false);
        assert_eq!((ev.computed, ev.cached), (0, 1));
    }
    #[test]
    fn error_does_not_leave_previous_geometry_in_output() {
        let mut g = Graph::default();
        let v = add(&mut g, NodeKind::Vase);
        let o = add(&mut g, NodeKind::Output);
        g.connect(v, o, "geometry").unwrap();
        let mut engine = Evaluator::default();
        assert!(engine.evaluate(&g, &|| false).outputs[&o].is_ok());
        g.node_mut(v)
            .unwrap()
            .params
            .insert("thickness".into(), 40.);
        let ev = engine.evaluate(&g, &|| false);
        assert!(ev.outputs[&v].is_err());
        assert!(ev.outputs[&o].is_err());
    }
    #[test]
    fn lists_repeat_last_item_and_empty_branches_survive() {
        let mut g = Graph::default();
        let s = add(&mut g, NodeKind::Series);
        let a = add(&mut g, NodeKind::Add);
        g.node_mut(s).unwrap().params.insert("count".into(), 3.);
        g.connect(s, a, "a").unwrap();
        let mut engine = Evaluator::default();
        let ev = engine.evaluate(&g, &|| false);
        let t = ev.outputs[&a].as_ref().unwrap();
        assert_eq!(
            t.branches[&vec![0]],
            vec![Value::Number(10.), Value::Number(20.), Value::Number(30.)]
        );
        g.node_mut(s).unwrap().params.insert("count".into(), 0.);
        let gr = add(&mut g, NodeKind::Graft);
        g.connect(a, gr, "geometry").unwrap();
        let ev = engine.evaluate(&g, &|| false);
        let t = ev.outputs[&gr].as_ref().unwrap();
        assert_eq!(t.item_count(), 0);
        assert_eq!(t.branches.len(), 1);
    }
    #[test]
    fn graft_broadcast_does_not_add_spurious_root_branch() {
        let mut g = Graph::default();
        let s = add(&mut g, NodeKind::Series);
        g.node_mut(s).unwrap().params.insert("count".into(), 3.);
        let gr = add(&mut g, NodeKind::Graft);
        let a = add(&mut g, NodeKind::Add);
        g.connect(s, gr, "geometry").unwrap();
        g.connect(gr, a, "a").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        let t = ev.outputs[&a].as_ref().unwrap();
        assert_eq!(t.branches.len(), 3);
        assert!(!t.branches.contains_key(&vec![0]));
    }
    #[test]
    fn graft_preserves_value_when_deeper_empty_path_collides() {
        // Construct a valid mixed-depth tree at the evaluator boundary; do not
        // depend on a particular graph's path-matching rules to create it.
        let mut graph = Graph::default();
        let source = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let graft = graph.add(NodeKind::Graft, [0., 0.]).unwrap();
        graph.connect(source, graft, "geometry").unwrap();
        let input = Tree {
            branches: BTreeMap::from([
                (vec![0], vec![Value::Number(42.), Value::Number(17.)]),
                (vec![0, 0], vec![]),
                (vec![3], vec![]),
                (vec![4, 2], vec![Value::Number(8.)]),
            ]),
        };
        let outputs = BTreeMap::from([(source, Ok(input.clone()))]);
        let result = evaluate_node(graph.node(graft).unwrap(), &outputs, &|| false).unwrap();
        assert_eq!(result.item_count(), input.item_count());
        assert_eq!(result.branches[&vec![0, 0]], vec![Value::Number(42.)]);
        assert_eq!(result.branches[&vec![0, 1]], vec![Value::Number(17.)]);
        assert_eq!(result.branches[&vec![3]], Vec::<Value>::new());
        assert_eq!(result.branches[&vec![4, 2, 0]], vec![Value::Number(8.)]);
    }

    #[test]
    fn singleton_grafted_branch_is_not_duplicated_by_constants() {
        let mut g = Graph::default();
        let n = add(&mut g, NodeKind::Number);
        let gr = add(&mut g, NodeKind::Graft);
        let a = add(&mut g, NodeKind::Add);
        g.connect(n, gr, "geometry").unwrap();
        g.connect(gr, a, "a").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        let t = ev.outputs[&a].as_ref().unwrap();
        assert_eq!(t.branches.len(), 1);
        assert_eq!(t.item_count(), 1);
        assert!(t.branches.contains_key(&vec![0, 0]));
    }
    #[test]
    fn separate_radius_and_height_lists_produce_a_closed_loft() {
        let mut g = Graph::default();
        let rs = add(&mut g, NodeKind::Series);
        g.node_mut(rs).unwrap().params.extend([
            ("start".into(), 14.),
            ("step".into(), 3.),
            ("count".into(), 6.),
        ]);
        let zs = add(&mut g, NodeKind::Series);
        g.node_mut(zs).unwrap().params.extend([
            ("start".into(), 0.),
            ("step".into(), 20.),
            ("count".into(), 6.),
        ]);
        let c = add(&mut g, NodeKind::Circle);
        let mv = add(&mut g, NodeKind::Move);
        let l = add(&mut g, NodeKind::Loft);
        g.connect(rs, c, "radius").unwrap();
        g.connect(c, mv, "geometry").unwrap();
        g.connect(zs, mv, "z").unwrap();
        g.connect(mv, l, "curves").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        closed_positive(&mesh_of(&ev, l));
    }
    #[test]
    fn cancelling_does_not_poison_cache() {
        let mut g = Graph::default();
        let id = add(&mut g, NodeKind::Vase);
        let mut engine = Evaluator::default();
        let ev = engine.evaluate(&g, &|| true);
        assert!(ev.cancelled);
        assert!(ev.outputs.is_empty());
        let ev = engine.evaluate(&g, &|| false);
        assert!(ev.outputs[&id].is_ok());
        assert_eq!(ev.computed, 1);
    }
    #[test]
    fn oversized_array_is_rejected_before_allocating() {
        let mut g = Graph::default();
        let v = add(&mut g, NodeKind::Vase);
        let a = add(&mut g, NodeKind::LinearArray);
        g.node_mut(a).unwrap().params.insert("count".into(), 128.);
        g.connect(v, a, "geometry").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        assert!(ev.outputs[&a].is_err());
    }
    #[test]
    fn self_intersecting_profile_is_rejected() {
        let c = Curve {
            points: vec![[0., 0., 0.], [10., 10., 0.], [0., 10., 0.], [10., 0., 0.]],
            closed: true,
            geometry: None,
        };
        assert!(extrude(&c, 10., 1, &|| false).is_err());
    }
    #[test]
    fn bad_division_is_local_to_its_descendants() {
        let mut g = Graph::default();
        let d = add(&mut g, NodeKind::Divide);
        g.node_mut(d).unwrap().params.insert("b".into(), 0.);
        let a = add(&mut g, NodeKind::Add);
        let b = add(&mut g, NodeKind::Box);
        g.connect(d, a, "a").unwrap();
        let ev = Evaluator::default().evaluate(&g, &|| false);
        assert!(ev.outputs[&d].is_err());
        assert!(ev.outputs[&a].is_err());
        assert!(ev.outputs[&b].is_ok());
    }
}
