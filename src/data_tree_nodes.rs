//! Single-output graph adapters for the data-tree API.
//! Multiple-output operations remain in the public API until graph wiring has port IDs.
use crate::data_tree as dt;
use crate::engine::cancelled;
use crate::model::*;

/// Validate the reserved, non-executable tree settings shared by native graphs
/// and the structured API. `false` means that this is an ordinary parameter.
pub fn validate_parameter(kind: NodeKind, key: &str, value: f64) -> Result<bool, String> {
    if key == dt::MATCH_PARAM {
        if !supports_matching(kind) {
            return Err("このノードはデータ照合を変更できません。".into());
        }
        dt::MatchMode::from_number(value)?;
        return Ok(true);
    }
    if let Some(port) = key.strip_prefix(dt::PORT_PREFIX) {
        if !supports_transform(kind) || !kind.spec().inputs.iter().any(|s| s.key == port) {
            return Err("データツリー変換のポートが無効です。".into());
        }
        dt::PortTransform::from_number(value)?;
        return Ok(true);
    }
    Ok(false)
}

macro_rules! node {
    ($name:ident,$label:literal,$description:literal,$keywords:literal,$inputs:expr,$output:expr) => {
        static $name: NodeSpec = NodeSpec {
            label: $label,
            category: "データ",
            description: $description,
            keywords: $keywords,
            inputs: $inputs,
            output: $output,
        };
    };
}
const TREE: InputSpec = g("tree", "ツリー", DataType::Any);
const LIST: InputSpec = g("list", "リスト", DataType::Any);
const NUMBERS: InputSpec = g("list", "数値リスト", DataType::Number);
const A: InputSpec = g("a", "データ A", DataType::Any);
const B: InputSpec = g("b", "データ B", DataType::Any);
const LIMIT: f64 = MAX_ITEMS as f64;
node!(
    SIMPLIFY,
    "枝の簡略化",
    "共通のパス成分を取り除きます。",
    "simplify tree 共通 パス",
    &[TREE],
    DataType::Any
);
node!(
    TRIM,
    "枝の階層を縮める",
    "末尾の階層を削り、同じパスのデータを連結します。",
    "trim tree path 階層",
    &[TREE, i("levels", "階層数", 1., 0., 32.)],
    DataType::Any
);
node!(
    PRUNE,
    "短い枝を除く",
    "指定件数未満の枝を取り除きます。",
    "prune tree empty 空枝",
    &[TREE, i("min", "最小件数", 1., 0., LIMIT)],
    DataType::Any
);
node!(
    MERGE,
    "データを結合",
    "同じパスの A と B を連結します。メッシュの結合とは別の処理です。",
    "merge tree data 結合",
    &[A, B],
    DataType::Any
);
node!(
    ENTWINE,
    "データを編み合わせる",
    "入力別の枝番号を先頭に付けます。フラット化は個別に指定できます。",
    "entwine tree prefix 枝",
    &[A, B],
    DataType::Any
);
node!(
    FLIP,
    "ツリーの転置",
    "同じ親を持つ長方形データの行と列を入れ替えます。",
    "flip matrix transpose tree 行列",
    &[TREE],
    DataType::Any
);
node!(
    UNFLATTEN,
    "枝構造を復元",
    "ガイドのパスと件数でデータを再分割します。件数が異なる場合はエラーです。",
    "unflatten tree guide 復元",
    &[A, g("guide", "ガイド", DataType::Any)],
    DataType::Any
);
node!(
    PATH_MAP,
    "パスマッパー",
    "文字 A〜Z と整数だけでパスを置換します。任意コードは実行しません。",
    "path mapper replace paths パス 置換",
    &[TREE],
    DataType::Any
);
node!(
    PARTITION,
    "リスト分割",
    "各枝のリストを指定サイズに分割して子枝に入れます。",
    "partition list chunks 分割",
    &[TREE, i("size", "サイズ", 2., 1., LIMIT)],
    DataType::Any
);
node!(
    BRANCH,
    "指定した枝",
    "パスの整数リストで指定した枝を取得します。",
    "tree branch path 枝 取得",
    &[TREE, g("path", "パスの整数リスト", DataType::Number)],
    DataType::Any
);
node!(
    ITEM,
    "ツリー項目",
    "パスと番号で要素を取得します。",
    "tree item path index 要素",
    &[
        TREE,
        g("path", "パスの整数リスト", DataType::Number),
        i("index", "番号", 0., 0., LIMIT)
    ],
    DataType::Any
);
node!(
    STATS,
    "枝ごとの件数",
    "各パスにその枝の要素数を返します。パスと枝数は値ビューに表示されます。",
    "tree statistics count 枝 件数",
    &[TREE],
    DataType::Number
);
node!(
    CULL_NTH,
    "N番目ごとに間引く",
    "N 番目、2N 番目…の要素を取り除きます。",
    "cull nth 間引き",
    &[LIST, i("n", "間隔", 2., 1., LIMIT)],
    DataType::Any
);
node!(
    CULL_INDEX,
    "番号で間引く",
    "指定した複数の番号の要素を取り除きます。",
    "cull index remove 間引き",
    &[
        LIST,
        i("index", "番号", 0., -LIMIT, LIMIT),
        i("wrap", "折り返す", 1., 0., 1.)
    ],
    DataType::Any
);
node!(
    DUPLICATE,
    "データを複製",
    "リスト全体を指定回数連結します。",
    "duplicate data list 複製",
    &[LIST, i("count", "回数", 2., 0., LIMIT)],
    DataType::Any
);
node!(
    REPEAT,
    "データを繰り返す",
    "リストを繰り返し、指定した要素数で止めます。",
    "repeat data pattern 繰り返し",
    &[LIST, i("count", "全体の件数", 10., 0., LIMIT)],
    DataType::Any
);
node!(
    SUM,
    "合計",
    "各枝の数値を足し合わせます。",
    "mass addition sum 合計",
    &[NUMBERS],
    DataType::Number
);
node!(
    PRODUCT,
    "総積",
    "各枝の数値を掛け合わせます。",
    "mass multiplication product 総積",
    &[NUMBERS],
    DataType::Number
);
node!(
    REPLACE,
    "要素を置換",
    "指定番号の要素を置き換えます。番号リストに値を繰り返して対応させます。",
    "replace items list 置換",
    &[
        LIST,
        i("index", "番号", 0., -LIMIT, LIMIT),
        g("value", "置換するデータ", DataType::Any),
        i("wrap", "折り返す", 0., 0., 1.)
    ],
    DataType::Any
);
node!(
    INSERT,
    "要素を挿入",
    "元リストの位置にデータを挿入します。同じ位置は入力順です。",
    "insert items list 挿入",
    &[
        LIST,
        i("index", "番号", 0., 0., LIMIT),
        g("value", "挿入するデータ", DataType::Any)
    ],
    DataType::Any
);
node!(
    MEMBER,
    "要素の出現位置",
    "完全一致する全番号を返します。",
    "member index item index 検索",
    &[LIST, g("member", "探すデータ", DataType::Any)],
    DataType::Number
);
node!(
    REDUCE,
    "ランダムに間引く",
    "シードで選んだ要素を取り除き、残りの順序を維持します。",
    "random reduce seed 間引き",
    &[
        LIST,
        i("count", "除く件数", 1., 0., LIMIT),
        i("seed", "シード", 1., 0., 100000.)
    ],
    DataType::Any
);
node!(
    JITTER,
    "シャッフル",
    "決定的なシードと割合で順序を変えます。",
    "jitter shuffle seed 乱数",
    &[
        LIST,
        n("amount", "割合", 1., 0., 1., 0.01),
        i("seed", "シード", 1., 0., 100000.)
    ],
    DataType::Any
);

pub fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    use NodeKind::*;
    Some(match kind {
        SimplifyTree => &SIMPLIFY,
        TrimTree => &TRIM,
        PruneTree => &PRUNE,
        MergeTree => &MERGE,
        EntwineTree => &ENTWINE,
        FlipMatrix => &FLIP,
        UnflattenTree => &UNFLATTEN,
        PathMapper => &PATH_MAP,
        PartitionList => &PARTITION,
        TreeBranch => &BRANCH,
        TreeItem => &ITEM,
        TreeStatistics => &STATS,
        CullNth => &CULL_NTH,
        CullIndex => &CULL_INDEX,
        DuplicateData => &DUPLICATE,
        RepeatData => &REPEAT,
        MassAddition => &SUM,
        MassMultiplication => &PRODUCT,
        ReplaceItems => &REPLACE,
        InsertItems => &INSERT,
        MemberIndex => &MEMBER,
        RandomReduce => &REDUCE,
        Jitter => &JITTER,
        _ => return None,
    })
}

pub fn is_tree_kind(kind: NodeKind) -> bool {
    use NodeKind::*;
    matches!(
        kind,
        SimplifyTree
            | TrimTree
            | PruneTree
            | MergeTree
            | EntwineTree
            | FlipMatrix
            | UnflattenTree
            | PathMapper
            | PartitionList
            | TreeBranch
            | TreeItem
            | TreeStatistics
    )
}
pub fn is_list_kind(kind: NodeKind) -> bool {
    (spec(kind).is_some() && !is_tree_kind(kind)) || crate::sets_nodes::is_list_kind(kind)
}

/// The legacy Script/Cluster evaluator is separate. Tree and list nodes have
/// their own access rules; only ordinary item nodes expose item matching modes.
pub fn supports_transform(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Script | NodeKind::Cluster)
}
pub fn supports_matching(kind: NodeKind) -> bool {
    crate::node_access::supports_item_matching(kind)
}

/// Shared inspector metadata; never infer list access from a data type.
pub fn access_label(kind: NodeKind, index: usize) -> &'static str {
    crate::node_access::for_input(kind, index).map_or("Unknown", |a| a.label())
}

// These adapters are public API entry points as well as evaluator helpers.
// Validate before integer casts or cloning, even when no Graph is involved.
fn validate_port(spec: &InputSpec, values: &[Value]) -> Result<(), String> {
    for value in values {
        if !spec.kind.accepts(value.kind()) {
            return Err(format!(
                "「{}」には {} が必要です。",
                spec.label,
                spec.kind.label()
            ));
        }
        if spec.default.is_some() {
            let number = value.number()?;
            if number < spec.min || number > spec.max {
                return Err(format!(
                    "「{}」は {} 〜 {} の範囲です。",
                    spec.label, spec.min, spec.max
                ));
            }
            if spec.integer && number.fract() != 0. {
                return Err(format!("「{}」は整数にしてください。", spec.label));
            }
        }
    }
    Ok(())
}

fn checked_list(
    values: Vec<Value>,
    remaining: usize,
    weight: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if values.len() > remaining.min(MAX_ITEMS) {
        return Err("生成データの件数が上限を超えました。".into());
    }
    let mut used = 0usize;
    for value in &values {
        cancelled(cancel)?;
        used = used.saturating_add(value.weight());
        if used > weight.min(MAX_TOTAL_WEIGHT / 2) {
            return Err("生成データの容量が上限を超えました。".into());
        }
    }
    Ok(values)
}

fn first(inputs: &[Tree], index: usize) -> Result<f64, String> {
    inputs
        .get(index)
        .and_then(|t| t.branches.values().flatten().next())
        .ok_or("設定の値がありません。")?
        .number()
}
fn input_path(inputs: &[Tree]) -> Result<dt::Path, String> {
    let mut path = Vec::new();
    for value in inputs[1].branches.values().flatten() {
        let n = value.number()?;
        if n.fract() != 0. || n < 0. || n > i32::MAX as f64 {
            return Err("パスの各番号は非負整数です。".into());
        }
        path.push(n as u32);
        if path.len() > dt::MAX_DEPTH {
            return Err("パスが深すぎます。".into());
        }
    }
    dt::validate_path(&path)?;
    Ok(path)
}

pub fn run_tree(node: &Node, inputs: &[Tree], cancel: &dyn Fn() -> bool) -> Result<Tree, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    if inputs.len() != node.kind.spec().inputs.len() {
        return Err("入力数が一致しません。".into());
    }
    for (spec, input) in node.kind.spec().inputs.iter().zip(inputs) {
        dt::validate_topology(input)?;
        for values in input.branches.values() {
            validate_port(spec, values)?;
        }
    }
    let t = inputs.first().ok_or("ツリーがありません。")?;
    let result = match node.kind {
        SimplifyTree => dt::simplify(t),
        TrimTree => dt::trim(t, first(inputs, 1)? as usize),
        PruneTree => {
            let min = first(inputs, 1)? as usize;
            let mut out = t.clone();
            out.branches.retain(|_, v| v.len() >= min);
            Ok(out)
        }
        MergeTree => dt::merge(inputs),
        EntwineTree => dt::entwine(inputs),
        FlipMatrix => dt::flip_matrix(t),
        UnflattenTree => dt::unflatten(t, &inputs[1]),
        PathMapper => dt::path_mapper(t, &node.script),
        PartitionList => dt::partition(t, first(inputs, 1)? as usize),
        TreeBranch | TreeItem => {
            let p = input_path(inputs)?;
            let v = dt::branch(t, &p).ok_or("指定した枝がありません。")?;
            let items = if node.kind == TreeItem {
                vec![
                    v.get(first(inputs, 2)? as usize)
                        .ok_or("指定した要素がありません。")?
                        .clone(),
                ]
            } else {
                v.to_vec()
            };
            Ok(Tree {
                branches: [(p, items)].into(),
            })
        }
        TreeStatistics => Ok(Tree {
            branches: dt::statistics(t)
                .into_iter()
                .map(|(p, n)| (p, vec![Value::Number(n as f64)]))
                .collect(),
        }),
        _ => Err("ツリーノードではありません。".into()),
    }?;
    cancelled(cancel)?;
    dt::validate_topology(&result)?;
    Ok(result)
}

fn setting(lists: &[&[Value]], index: usize) -> Result<f64, String> {
    lists
        .get(index)
        .and_then(|v| v.first())
        .ok_or("設定がありません。")?
        .number()
}
fn index_of(n: f64, len: usize, wrap: bool) -> Result<usize, String> {
    if len == 0 {
        return Err("空のリストに番号はありません。".into());
    }
    if !n.is_finite() || n.fract() != 0. || n < -(MAX_ITEMS as f64) || n > MAX_ITEMS as f64 {
        return Err("番号は範囲内の整数にしてください。".into());
    }
    let n = n as i64;
    if wrap {
        Ok(n.rem_euclid(len as i64) as usize)
    } else if n >= 0 && n < len as i64 {
        Ok(n as usize)
    } else {
        Err(format!("番号 {n} は範囲外です。"))
    }
}
fn check_count(n: usize) -> Result<(), String> {
    if n > MAX_ITEMS {
        Err("リストの件数が上限を超えました。".into())
    } else {
        Ok(())
    }
}
fn repeat(
    list: &[Value],
    count: usize,
    remaining: usize,
    weight: usize,
) -> Result<Vec<Value>, String> {
    if list.is_empty() {
        return Ok(Vec::new());
    }
    check_count(count)?;
    if count > remaining {
        return Err("残りのデータ件数が不足しています。".into());
    }
    let cycle_weight = list
        .iter()
        .fold(0usize, |w, v| w.saturating_add(v.weight()));
    let estimate = cycle_weight
        .saturating_mul(count / list.len())
        .saturating_add(
            list[..count % list.len()]
                .iter()
                .fold(0usize, |w, v| w.saturating_add(v.weight())),
        );
    if estimate > weight {
        return Err("複製データの容量が上限を超えました。".into());
    }
    Ok(list.iter().cycle().take(count).cloned().collect())
}
pub fn run_list(
    kind: NodeKind,
    lists: &[&[Value]],
    remaining: usize,
    weight: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    use std::collections::{BTreeMap, BTreeSet};
    cancelled(cancel)?;
    if lists.len() != kind.spec().inputs.len() {
        return Err("入力数が一致しません。".into());
    }
    for (spec, values) in kind.spec().inputs.iter().zip(lists) {
        if values.len() > MAX_ITEMS {
            return Err("入力リストが長すぎます。".into());
        }
        validate_port(spec, values)?;
    }
    let list = lists.first().copied().unwrap_or(&[]);
    let len = list.len();
    let output: Result<Vec<Value>, String> = match kind {
        MassAddition | MassMultiplication => {
            let mut value = if kind == MassAddition { 0. } else { 1. };
            for v in list {
                cancelled(cancel)?;
                let x = v.number()?;
                value = if kind == MassAddition {
                    value + x
                } else {
                    value * x
                };
                if !value.is_finite() {
                    return Err("集計結果が有限ではありません。".into());
                }
            }
            Ok(vec![Value::Number(value)])
        }
        kind if crate::sets_nodes::is_list_kind(kind) => {
            crate::sets_nodes::run_list(kind, lists, remaining, weight, cancel)
        }
        _ if len == 0 && kind != InsertItems => Ok(Vec::new()),
        CullNth => {
            let n = setting(lists, 1)? as usize;
            if n == 0 {
                return Err("間隔は 1 以上です。".into());
            }
            Ok(list
                .iter()
                .enumerate()
                .filter(|(i, _)| (i + 1) % n != 0)
                .map(|(_, v)| v.clone())
                .collect())
        }
        CullIndex => {
            let wrap = setting(lists, 2)? != 0.;
            let indices: BTreeSet<_> = lists[1]
                .iter()
                .map(|v| index_of(v.number()?, len, wrap))
                .collect::<Result<_, String>>()?;
            Ok(list
                .iter()
                .enumerate()
                .filter(|(i, _)| !indices.contains(i))
                .map(|(_, v)| v.clone())
                .collect())
        }
        DuplicateData => repeat(
            list,
            len.checked_mul(setting(lists, 1)? as usize)
                .ok_or("複製件数が大きすぎます。")?,
            remaining,
            weight,
        ),
        RepeatData => repeat(list, setting(lists, 1)? as usize, remaining, weight),
        ReplaceItems => {
            let wrap = setting(lists, 3)? != 0.;
            let mut out = list.to_vec();
            let vals = lists[2];
            if vals.is_empty() {
                return Err("置換データがありません。".into());
            }
            for (i, v) in lists[1].iter().enumerate() {
                let index = index_of(v.number()?, len, wrap)?;
                out[index] = vals[i.min(vals.len() - 1)].clone();
            }
            Ok(out)
        }
        InsertItems => {
            let vals = lists[2];
            if vals.is_empty() {
                return Err("挿入データがありません。".into());
            }
            let count = len
                .checked_add(lists[1].len())
                .ok_or("挿入件数が多すぎます。")?;
            check_count(count)?;
            if count > remaining {
                return Err("挿入件数が上限を超えました。".into());
            }
            let mut insert: BTreeMap<usize, Vec<&Value>> = BTreeMap::new();
            for (i, v) in lists[1].iter().enumerate() {
                let idx = v.number()? as usize;
                if idx > len {
                    return Err("挿入番号が範囲外です。".into());
                }
                insert
                    .entry(idx)
                    .or_default()
                    .push(&vals[i.min(vals.len() - 1)]);
            }
            let mut out = Vec::with_capacity(count);
            for i in 0..=len {
                if let Some(v) = insert.get(&i) {
                    out.extend(v.iter().map(|&v| v.clone()));
                }
                if let Some(v) = list.get(i) {
                    out.push(v.clone());
                }
            }
            Ok(out)
        }
        MemberIndex => {
            // Structural equality of distinct curves/meshes can traverse their
            // payloads. Count that worst-case work, not just the number of pairs.
            let weighted = |values: &[Value]| {
                values.iter().fold(0usize, |used, value| {
                    used.saturating_add(value.weight().max(1))
                })
            };
            let comparisons = weighted(list)
                .saturating_mul(lists[1].len())
                .max(weighted(lists[1]).saturating_mul(len));
            if comparisons > 1_000_000 {
                return Err("検索比較の計算量が上限を超えました。".into());
            }
            let mut out = Vec::new();
            for member in lists[1] {
                for (i, v) in list.iter().enumerate() {
                    cancelled(cancel)?;
                    if v == member {
                        if out.len() >= remaining.min(MAX_ITEMS) {
                            return Err("検索結果が多すぎます。".into());
                        }
                        out.push(Value::Number(i as f64));
                    }
                }
            }
            Ok(out)
        }
        RandomReduce | Jitter => {
            let mut rng = crate::gh_nodes::Rng::new(setting(lists, 2)? as u64);
            let mut indices: Vec<_> = (0..len).collect();
            // Deterministic Fisher-Yates, no OS randomness.
            let swaps = if kind == Jitter {
                (setting(lists, 1)? * len as f64).round() as usize
            } else {
                len
            };
            for i in (0..len).rev().take(swaps) {
                cancelled(cancel)?;
                let j = ((rng.next_f64() * (i + 1) as f64) as usize).min(i);
                indices.swap(i, j);
            }
            if kind == RandomReduce {
                indices.truncate(len.saturating_sub(setting(lists, 1)? as usize));
                indices.sort_unstable();
            }
            Ok(indices.into_iter().map(|i| list[i].clone()).collect())
        }
        _ => Err("リストノードではありません。".into()),
    };
    checked_list(output?, remaining, weight, cancel)
}
