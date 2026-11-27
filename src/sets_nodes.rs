//! Branch-list components for sequence construction and exact set operations.
use std::collections::{BTreeMap, btree_map::Entry};

use crate::engine::{cancelled, num};
use crate::math_nodes::{Key, key};
use crate::model::{DataType, InputSpec, MAX_ITEMS, NodeKind, NodeSpec, Value, g, i, n};

macro_rules! node {
    ($name:ident, $label:literal, $description:literal, $keywords:literal, $inputs:expr, $output:ident) => {
        static $name: NodeSpec = NodeSpec {
            label: $label,
            category: "データ",
            description: $description,
            keywords: $keywords,
            inputs: $inputs,
            output: DataType::$output,
        };
    };
}

const A: InputSpec = g("a", "集合 A", DataType::Any);
const B: InputSpec = g("b", "集合 B", DataType::Any);
const C: InputSpec = g("c", "集合 C", DataType::Any);

node!(
    FIBONACCI,
    "フィボナッチ数列",
    "最初の2項を指定し、直前の2項の和で数列を作ります。",
    "fibonacci fib sequence 数列",
    &[
        n("a", "第1項", 0., -1e12, 1e12, 0.1),
        n("b", "第2項", 1., -1e12, 1e12, 0.1),
        i("count", "項数", 10., 0., MAX_ITEMS as f64),
    ],
    Number
);
node!(
    STACK,
    "要素ごとに複製",
    "各要素を指定回数並べます。回数のリストを順に繰り返します。複製回数が 0 の要素は出力しません。",
    "stack data item duplicate repeat pattern 個別 複製",
    &[
        g("data", "データ", DataType::Any),
        i("stack", "複製回数のパターン", 2., 0., MAX_ITEMS as f64),
    ],
    Any
);
node!(
    WEAVE,
    "リストを交互に結合",
    "0 と 1 のパターンを繰り返し、指定したリストから順に取り出します。空になったリストは飛ばします。",
    "weave interleave pattern 交互 リスト 結合",
    &[
        g("pattern", "取り出し順 (0 / 1)", DataType::Number),
        g("stream_0", "リスト 0", DataType::Any),
        g("stream_1", "リスト 1", DataType::Any),
    ],
    Any
);
node!(
    SYMMETRIC_DIFFERENCE,
    "集合の対称差",
    "どちらか一方だけにある数値と点を初出順に返します。",
    "set difference (s) symmetric difference exdiff 集合 排他的",
    &[A, B],
    Any
);
node!(
    SUBSET,
    "部分集合を判定",
    "集合 B のすべての数値と点が A に含まれるとき 1、それ以外は 0 を返します。",
    "subset set inclusion superset 部分集合 包含",
    &[A, B],
    Number
);
node!(
    DISJOINT,
    "集合の共通要素なし",
    "数値と点を比較し、集合 A と B に共通する要素がなければ 1、あれば 0 を返します。",
    "disjoint sets overlap 集合 互いに素 共通部分",
    &[A, B],
    Number
);
node!(
    MAJORITY,
    "集合の多数決",
    "3 集合のうち 2 つ以上にある数値と点を初出順に返します。",
    "set majority three sets 集合 多数決",
    &[A, B, C],
    Any
);

pub(crate) fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    use NodeKind::*;
    Some(match kind {
        Fibonacci => &FIBONACCI,
        StackData => &STACK,
        Weave => &WEAVE,
        SetSymmetricDifference => &SYMMETRIC_DIFFERENCE,
        SetSubset => &SUBSET,
        SetDisjoint => &DISJOINT,
        SetMajority => &MAJORITY,
        _ => return None,
    })
}

pub(crate) fn is_list_kind(kind: NodeKind) -> bool {
    kind != NodeKind::Fibonacci && spec(kind).is_some()
}

pub(crate) fn fibonacci(args: &[&Value], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let mut previous = num(args, 0)?;
    let mut current = num(args, 1)?;
    let count = num(args, 2)? as usize;
    let mut series = Vec::with_capacity(count);
    for index in 0..count {
        cancelled(cancel)?;
        let term = match index {
            0 => previous,
            1 => current,
            _ => {
                let next = previous + current;
                previous = current;
                current = next;
                next
            }
        };
        if !term.is_finite() {
            return Err("フィボナッチ数列の項が有限ではありません。".into());
        }
        series.push(Value::Number(term));
    }
    Ok(series)
}

fn stack_data(
    list: &[Value],
    pattern: &[Value],
    remaining: usize,
    weight: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    let mut used_weight = 0usize;
    for (item, repeat) in list.iter().zip(pattern.iter().cycle()) {
        cancelled(cancel)?;
        let count = repeat.number()? as usize;
        if output.len() + count > remaining.min(MAX_ITEMS) {
            return Err("複製データの件数が上限を超えました。".into());
        }
        used_weight = used_weight.saturating_add(item.weight().saturating_mul(count));
        if used_weight > weight {
            return Err("複製データの容量が上限を超えました。".into());
        }
        output.extend(std::iter::repeat_n(item.clone(), count));
    }
    Ok(output)
}

fn weave(
    pattern: &[Value],
    mut streams: [&[Value]; 2],
    remaining: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut referenced = [false; 2];
    let mut order = Vec::with_capacity(pattern.len());
    for value in pattern {
        cancelled(cancel)?;
        let index = match value.number()? {
            0. => 0,
            1. => 1,
            _ => return Err("取り出し順には 0 と 1 を指定してください。".into()),
        };
        order.push(index);
        referenced[index] = true;
    }
    for (stream, included) in streams.iter_mut().zip(referenced) {
        if !included {
            *stream = &[];
        }
    }
    let count = streams[0].len() + streams[1].len();
    if count > remaining.min(MAX_ITEMS) {
        return Err("結合データの件数が上限を超えました。".into());
    }
    let mut output = Vec::with_capacity(count);
    // With a single remaining stream, each future selection from it is in order.
    // Append the tail once so sparse patterns do not multiply the work.
    for index in order.into_iter().cycle() {
        cancelled(cancel)?;
        if streams[0].is_empty() || streams[1].is_empty() {
            break;
        }
        let (item, rest) = streams[index]
            .split_first()
            .expect("both streams have values");
        output.push(item.clone());
        streams[index] = rest;
    }
    for item in streams.into_iter().flatten() {
        cancelled(cancel)?;
        output.push(item.clone());
    }
    Ok(output)
}

fn set_operation(
    kind: NodeKind,
    lists: &[&[Value]],
    remaining: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut indices = BTreeMap::<Key, usize>::new();
    let mut elements = Vec::<(&Value, u8)>::new();
    for (set, list) in lists.iter().enumerate() {
        for value in *list {
            cancelled(cancel)?;
            let membership = 1 << set;
            match indices.entry(key(value)?) {
                Entry::Vacant(entry) => {
                    entry.insert(elements.len());
                    elements.push((value, membership));
                }
                Entry::Occupied(entry) => elements[*entry.get()].1 |= membership,
            }
        }
    }
    let flag = match kind {
        NodeKind::SetSubset => Some(elements.iter().all(|&(_, membership)| membership != 0b10)),
        NodeKind::SetDisjoint => Some(elements.iter().all(|&(_, membership)| membership != 0b11)),
        _ => None,
    };
    if let Some(flag) = flag {
        return Ok(vec![Value::Number(if flag { 1. } else { 0. })]);
    }
    let mut output = Vec::new();
    for (value, membership) in elements {
        cancelled(cancel)?;
        let set_count = membership.count_ones();
        let include = match kind {
            NodeKind::SetSymmetricDifference => set_count == 1,
            NodeKind::SetMajority => set_count >= 2,
            _ => return Err("集合ノードではありません。".into()),
        };
        if include {
            if output.len() >= remaining.min(MAX_ITEMS) {
                return Err("集合の個数が上限を超えました。".into());
            }
            output.push(value.clone());
        }
    }
    Ok(output)
}

// data_tree_nodes validates linked inputs and checks the resulting resource usage.
pub(crate) fn run_list(
    kind: NodeKind,
    lists: &[&[Value]],
    remaining: usize,
    weight: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    match kind {
        NodeKind::StackData => stack_data(lists[0], lists[1], remaining, weight, cancel),
        NodeKind::Weave => weave(lists[0], [lists[1], lists[2]], remaining, cancel),
        _ => set_operation(kind, lists, remaining, cancel),
    }
}
