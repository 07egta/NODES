//! Bounded, deterministic data-tree operations independent of geometry.
//! GH1-inspired matching; see docs/DATA_TREE_ENGINE.md for deliberate differences.
use crate::model::{MAX_ITEMS, MAX_TOTAL_WEIGHT, Tree, Value};
use std::collections::{BTreeMap, BTreeSet};

pub type Path = Vec<u32>;
pub const MAX_DEPTH: usize = 32;
pub const MATCH_PARAM: &str = "__data_matching";
pub const PORT_PREFIX: &str = "__tree_port:";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchMode {
    Shortest,
    #[default]
    Longest,
    Cross,
}
impl MatchMode {
    pub fn from_number(n: f64) -> Result<Self, String> {
        match n {
            0. => Ok(Self::Longest),
            1. => Ok(Self::Shortest),
            2. => Ok(Self::Cross),
            _ => Err("データ照合方式が無効です。".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PortTransform {
    #[default]
    None,
    Flatten,
    Graft,
    Simplify,
}
impl PortTransform {
    pub fn from_number(n: f64) -> Result<Self, String> {
        match n {
            0. => Ok(Self::None),
            1. => Ok(Self::Flatten),
            2. => Ok(Self::Graft),
            3. => Ok(Self::Simplify),
            _ => Err("データツリー変換が無効です。".into()),
        }
    }
}

/// A missing location and a present null are distinct, even though the current
/// graph Value type does not yet represent null. Useful for future typed ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot<T> {
    Missing,
    Null,
    Item(T),
}

pub fn path_text(path: &[u32]) -> String {
    format!(
        "{{{}}}",
        path.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(";")
    )
}

pub fn validate_path(path: &[u32]) -> Result<(), String> {
    if path.is_empty() || path.len() > MAX_DEPTH || path.iter().any(|&n| n > i32::MAX as u32) {
        return Err("パスは 1〜32 個の非負 32 ビット符号付き整数です。".into());
    }
    Ok(())
}

pub fn validate_topology(tree: &Tree) -> Result<(), String> {
    if tree.branches.len() > MAX_ITEMS
        || tree.item_count() > MAX_ITEMS
        || tree.weight() > MAX_TOTAL_WEIGHT / 2
    {
        return Err("データツリーが容量の上限を超えました。".into());
    }
    for path in tree.branches.keys() {
        validate_path(path)?;
    }
    Ok(())
}

/// Incremental checks happen before cloning, including for geometry payloads.
#[derive(Default)]
struct Builder {
    tree: Tree,
    items: usize,
    weight: usize,
}
impl Builder {
    fn append(&mut self, path: Path, items: &[Value]) -> Result<(), String> {
        validate_path(&path)?;
        let new = !self.tree.branches.contains_key(&path);
        if new && self.tree.branches.len() >= MAX_ITEMS {
            return Err("枝数の上限を超えました。".into());
        }
        let count = self
            .items
            .checked_add(items.len())
            .ok_or("要素数のオーバーフロー")?;
        let mut weight = self.weight;
        if new {
            weight = weight.saturating_add(path.len() + 1);
        }
        for item in items {
            weight = weight.saturating_add(item.weight());
        }
        if count > MAX_ITEMS || weight > MAX_TOTAL_WEIGHT / 2 {
            return Err("データツリーが容量の上限を超えました。".into());
        }
        self.tree
            .branches
            .entry(path)
            .or_default()
            .extend_from_slice(items);
        self.items = count;
        self.weight = weight;
        Ok(())
    }
    fn finish(self) -> Tree {
        self.tree
    }
}

pub fn flatten(tree: &Tree) -> Result<Tree, String> {
    validate_topology(tree)?;
    let mut b = Builder::default();
    b.append(vec![0], &[])?;
    for values in tree.branches.values() {
        b.append(vec![0], values)?;
    }
    Ok(b.finish())
}

pub fn graft(tree: &Tree) -> Result<Tree, String> {
    validate_topology(tree)?;
    let mut b = Builder::default();
    for (path, values) in &tree.branches {
        // NODES deliberately retains explicit empty branches.
        if values.is_empty() {
            b.append(path.clone(), &[])?;
        }
        for (i, value) in values.iter().enumerate() {
            let mut p = path.clone();
            p.push(u32::try_from(i).map_err(|_| "枝番号が大きすぎます。")?);
            // Mixed-depth paths can collide: append, never overwrite.
            b.append(p, std::slice::from_ref(value))?;
        }
    }
    Ok(b.finish())
}

pub fn simplify(tree: &Tree) -> Result<Tree, String> {
    validate_topology(tree)?;
    let Some(first) = tree.branches.keys().next() else {
        return Ok(Tree::default());
    };
    if tree.branches.len() == 1 {
        return remap_paths(tree, |_| Ok(vec![*first.last().unwrap_or(&0)]));
    }
    // Equal-depth paths: remove all invariant columns. Mixed-depth paths:
    // remove only a shared prefix, retaining at least one index per path.
    let min_depth = tree.branches.keys().map(Vec::len).min().unwrap_or(1);
    let same_depth = tree.branches.keys().all(|p| p.len() == first.len());
    if same_depth {
        let keep: Vec<_> = (0..first.len())
            .filter(|&i| tree.branches.keys().any(|p| p[i] != first[i]))
            .collect();
        remap_paths(tree, |p| Ok(keep.iter().map(|&i| p[i]).collect()))
    } else {
        let common = (0..min_depth.saturating_sub(1))
            .take_while(|&i| tree.branches.keys().all(|p| p[i] == first[i]))
            .count();
        remap_paths(tree, |p| Ok(p[common..].to_vec()))
    }
}

pub fn trim(tree: &Tree, levels: usize) -> Result<Tree, String> {
    remap_paths(tree, |p| {
        Ok(p[..p.len().saturating_sub(levels).max(1)].to_vec())
    })
}

pub fn remap_paths(
    tree: &Tree,
    mut f: impl FnMut(&[u32]) -> Result<Path, String>,
) -> Result<Tree, String> {
    validate_topology(tree)?;
    let mut b = Builder::default();
    for (path, items) in &tree.branches {
        b.append(f(path)?, items)?;
    }
    Ok(b.finish())
}

pub fn merge(trees: &[Tree]) -> Result<Tree, String> {
    let mut b = Builder::default();
    for t in trees {
        validate_topology(t)?;
        for (p, v) in &t.branches {
            b.append(p.clone(), v)?;
        }
    }
    Ok(b.finish())
}

pub fn entwine(trees: &[Tree]) -> Result<Tree, String> {
    let mut b = Builder::default();
    for (i, t) in trees.iter().enumerate() {
        validate_topology(t)?;
        let prefix = u32::try_from(i).map_err(|_| "入力数が多すぎます。")?;
        for (p, v) in &t.branches {
            let mut path = vec![prefix];
            path.extend_from_slice(p);
            b.append(path, v)?;
        }
    }
    Ok(b.finish())
}

pub fn branch<'a>(tree: &'a Tree, path: &[u32]) -> Option<&'a [Value]> {
    tree.branches.get(path).map(Vec::as_slice)
}
pub fn item<'a>(tree: &'a Tree, path: &[u32], index: usize) -> Option<&'a Value> {
    branch(tree, path)?.get(index)
}

pub fn statistics(tree: &Tree) -> Vec<(Path, usize)> {
    tree.branches
        .iter()
        .map(|(p, v)| (p.clone(), v.len()))
        .collect()
}

pub fn prune_empty(tree: &Tree) -> Result<Tree, String> {
    validate_topology(tree)?;
    let mut b = Builder::default();
    for (p, v) in &tree.branches {
        if !v.is_empty() {
            b.append(p.clone(), v)?;
        }
    }
    Ok(b.finish())
}

pub fn unflatten(data: &Tree, guide: &Tree) -> Result<Tree, String> {
    validate_topology(data)?;
    validate_topology(guide)?;
    if data.item_count() != guide.item_count() {
        return Err("ガイドとデータの要素数が一致しません。".into());
    }
    let data = flatten(data)?;
    let all = &data.branches[&vec![0]];
    let mut offset = 0;
    let mut b = Builder::default();
    for (p, v) in &guide.branches {
        let end = offset + v.len();
        b.append(p.clone(), &all[offset..end])?;
        offset = end;
    }
    Ok(b.finish())
}

/// Transpose a rectangular tree. Ragged data are rejected instead of silently dropped.
pub fn flip_matrix(tree: &Tree) -> Result<Tree, String> {
    validate_topology(tree)?;
    let Some((first_path, first)) = tree.branches.first_key_value() else {
        return Ok(Tree::default());
    };
    let width = first.len();
    if tree.branches.iter().any(|(p, v)| {
        p.len() != first_path.len()
            || p[..p.len() - 1] != first_path[..first_path.len() - 1]
            || v.len() != width
    }) {
        return Err("同じ親パスの長方形データだけを転置できます。".into());
    }
    if width == 0 {
        return Ok(tree.clone());
    }
    let mut b = Builder::default();
    for col in 0..width {
        let mut p = first_path[..first_path.len() - 1].to_vec();
        p.push(col as u32);
        for row in tree.branches.values() {
            b.append(p.clone(), &row[col..col + 1])?;
        }
    }
    Ok(b.finish())
}

pub fn apply_transform(tree: &Tree, mode: PortTransform) -> Result<Tree, String> {
    match mode {
        PortTransform::None => {
            validate_topology(tree)?;
            Ok(tree.clone())
        }
        PortTransform::Flatten => flatten(tree),
        PortTransform::Graft => graft(tree),
        PortTransform::Simplify => simplify(tree),
    }
}

/// Paths and borrowed input lists for one branch invocation. No geometry is cloned.
pub struct BranchMatch<'a> {
    pub path: Path,
    pub lists: Vec<&'a [Value]>,
}

/// GH1 tutorial's ordinal branch matching, repeating the final branch.
/// Topology selection is a NODES policy: greatest branch count, then deepest
/// path, then first port. Exact GH1 mixed-depth topology needs oracle tests.
pub fn match_branches(inputs: &[Tree]) -> Result<Vec<BranchMatch<'_>>, String> {
    if inputs.len() > 64 {
        return Err("入力数の上限を超えました。".into());
    }
    for t in inputs {
        validate_topology(t)?;
    }
    let mut master = None;
    let mut best = (0, 0);
    for (i, t) in inputs.iter().enumerate() {
        let score = (
            t.branches.len(),
            t.branches.keys().map(Vec::len).max().unwrap_or(0),
        );
        if score > best {
            best = score;
            master = Some(i);
        }
    }
    let Some(master) = master else {
        return Ok(Vec::new());
    };
    let branches: Vec<Vec<&[Value]>> = inputs
        .iter()
        .map(|t| t.branches.values().map(Vec::as_slice).collect())
        .collect();
    let mut result = Vec::with_capacity(best.0);
    for (index, path) in inputs[master].branches.keys().enumerate() {
        let lists = branches
            .iter()
            .map(|b| {
                if b.is_empty() {
                    &[][..]
                } else {
                    b[index.min(b.len() - 1)]
                }
            })
            .collect();
        result.push(BranchMatch {
            path: path.clone(),
            lists,
        });
    }
    Ok(result)
}

/// Cardinality is checked before any combination is emitted. No Cartesian matrix
/// is materialised. For Cross, the last port varies fastest (NODES policy).
pub struct ItemMatches<'a> {
    lists: Vec<&'a [Value]>,
    mode: MatchMode,
    next: usize,
    count: usize,
}
impl<'a> ItemMatches<'a> {
    pub fn new(lists: &[&'a [Value]], mode: MatchMode, remaining: usize) -> Result<Self, String> {
        if lists.len() > 64 {
            return Err("入力数の上限を超えました。".into());
        }
        let count = if lists.is_empty() || lists.iter().any(|l| l.is_empty()) {
            0
        } else {
            match mode {
                MatchMode::Longest => lists.iter().map(|l| l.len()).max().unwrap_or(0),
                MatchMode::Shortest => lists.iter().map(|l| l.len()).min().unwrap_or(0),
                MatchMode::Cross => lists.iter().try_fold(1usize, |n, l| {
                    n.checked_mul(l.len())
                        .ok_or("クロス参照の件数が大きすぎます。")
                })?,
            }
        };
        if count > remaining.min(MAX_ITEMS) {
            return Err("データ照合の件数が上限を超えました。".into());
        }
        Ok(Self {
            lists: lists.to_vec(),
            mode,
            next: 0,
            count,
        })
    }
    pub fn cardinality(&self) -> usize {
        self.count
    }
}
impl<'a> Iterator for ItemMatches<'a> {
    type Item = Vec<&'a Value>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let index = self.next;
        self.next += 1;
        if self.mode == MatchMode::Cross {
            let mut n = index;
            let mut args = Vec::with_capacity(self.lists.len());
            for list in self.lists.iter().rev() {
                args.push(&list[n % list.len()]);
                n /= list.len();
            }
            args.reverse();
            Some(args)
        } else {
            Some(
                self.lists
                    .iter()
                    .map(|l| &l[index.min(l.len() - 1)])
                    .collect(),
            )
        }
    }
}

/// Return one tree per input, preserving the same matched paths and item indices.
pub fn cross_reference(inputs: &[Tree]) -> Result<Vec<Tree>, String> {
    if inputs.len() > 64 {
        return Err("入力数の上限を超えました。".into());
    }
    let mut builders: Vec<Builder> = inputs.iter().map(|_| Builder::default()).collect();
    let mut total_weight = 0usize;
    let mut remaining = MAX_ITEMS;
    for m in match_branches(inputs)? {
        let matches = ItemMatches::new(&m.lists, MatchMode::Cross, remaining)?;
        remaining -= matches.cardinality();
        total_weight = total_weight.saturating_add((m.path.len() + 1).saturating_mul(inputs.len()));
        if total_weight > MAX_TOTAL_WEIGHT {
            return Err("クロス参照が大きすぎます。".into());
        }
        for b in &mut builders {
            b.append(m.path.clone(), &[])?;
        }
        for args in matches {
            for (b, v) in builders.iter_mut().zip(args) {
                total_weight = total_weight.saturating_add(v.weight());
                if total_weight > MAX_TOTAL_WEIGHT {
                    return Err("クロス参照が大きすぎます。".into());
                }
                b.append(m.path.clone(), std::slice::from_ref(v))?;
            }
        }
    }
    Ok(builders.into_iter().map(Builder::finish).collect())
}

/// Path mapper subset: literals and single uppercase variables only.
/// No expression evaluation, scripting, I/O, globbing or external dependencies.
/// Unmatched branches are retained. Destination collisions append in source order.
pub fn path_mapper(tree: &Tree, rule: &str) -> Result<Tree, String> {
    if rule.len() > 1024 {
        return Err("パスマップが長すぎます。".into());
    }
    #[derive(Clone, Copy)]
    enum Token {
        Literal(u32),
        Variable(u8),
    }
    fn parse(s: &str) -> Result<Vec<Token>, String> {
        let s = s
            .trim()
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or("パスは {A;B} 形式です。")?;
        let mut tokens = Vec::new();
        for x in s.split(';') {
            let x = x.trim();
            let raw = x.as_bytes();
            let token = if raw.len() == 1 && raw[0].is_ascii_uppercase() {
                Token::Variable(raw[0])
            } else if !raw.is_empty() && raw.iter().all(u8::is_ascii_digit) {
                let v = x.parse::<u32>().map_err(|_| "パス番号が範囲外です。")?;
                validate_path(&[v])?;
                Token::Literal(v)
            } else {
                return Err("変数 A〜Z と整数だけを使えます。".into());
            };
            tokens.push(token);
            if tokens.len() > MAX_DEPTH {
                return Err("パスが深すぎます。".into());
            }
        }
        Ok(tokens)
    }
    let (left, right) = rule
        .split_once("->")
        .ok_or("パスマップは {A;B}->{B;A} 形式です。")?;
    let src = parse(left)?;
    let dst = parse(right)?;
    let declared: BTreeSet<_> = src
        .iter()
        .filter_map(|t| match t {
            Token::Variable(v) => Some(*v),
            _ => None,
        })
        .collect();
    if dst
        .iter()
        .any(|t| matches!(t, Token::Variable(v) if !declared.contains(v)))
    {
        return Err("出力パスに未定義の変数があります。".into());
    }
    remap_paths(tree, |p| {
        if p.len() != src.len() {
            return Ok(p.to_vec());
        }
        let mut vars = BTreeMap::new();
        for (&index, token) in p.iter().zip(&src) {
            match *token {
                Token::Literal(v) if v != index => return Ok(p.to_vec()),
                Token::Variable(v) => {
                    if vars.insert(v, index).is_some_and(|prev| prev != index) {
                        return Ok(p.to_vec());
                    }
                }
                _ => {}
            }
        }
        Ok(dst
            .iter()
            .map(|t| match *t {
                Token::Literal(v) => v,
                Token::Variable(v) => vars[&v],
            })
            .collect())
    })
}

/// Partition every branch into consecutive chunks, retaining empty branches.
pub fn partition(tree: &Tree, size: usize) -> Result<Tree, String> {
    validate_topology(tree)?;
    if size == 0 {
        return Err("分割サイズは 1 以上です。".into());
    }
    let mut b = Builder::default();
    for (p, v) in &tree.branches {
        if v.is_empty() {
            b.append(p.clone(), &[])?;
        }
        for (i, chunk) in v.chunks(size).enumerate() {
            let mut path = p.clone();
            path.push(i as u32);
            b.append(path, chunk)?;
        }
    }
    Ok(b.finish())
}

/// Two outputs are separate, not flattened or encoded as magical values.
pub fn split_list(list: &[Value], index: usize) -> (&[Value], &[Value]) {
    list.split_at(index.min(list.len()))
}
pub fn dispatch(list: &[Value], pattern: &[bool]) -> Result<(Vec<Value>, Vec<Value>), String> {
    if pattern.is_empty() {
        return Err("パターンが空です。".into());
    }
    if list.len() > MAX_ITEMS {
        return Err("リストが長すぎます。".into());
    }
    let weight = list
        .iter()
        .fold(0usize, |sum, value| sum.saturating_add(value.weight()));
    if weight > MAX_TOTAL_WEIGHT / 2 {
        return Err("リストの容量が上限を超えました。".into());
    }
    let (mut yes, mut no) = (Vec::new(), Vec::new());
    for (i, v) in list.iter().enumerate() {
        if pattern[i % pattern.len()] {
            yes.push(v.clone());
        } else {
            no.push(v.clone());
        }
    }
    Ok((yes, no))
}
