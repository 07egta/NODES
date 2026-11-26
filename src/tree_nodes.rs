//! Explicit tree operations. These preserve source paths and do not use implicit list matching.
use crate::engine::cancelled;
use crate::model::{DataType, MAX_ITEMS, NodeKind, NodeSpec, Tree, Value, g, i};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const CULL_INDEX: NodeSpec = NodeSpec {
    label: "指定番号を除く",
    category: "データ",
    description: "枝ごとに、0 始まりの番号を指定して要素を除きます。残りの順番と枝はそのままです。",
    keywords: "cull index remove items 番号で削除 要素を除く 間引き",
    inputs: &[
        g("list", "リスト", DataType::Any),
        i(
            "index",
            "除く番号",
            0.,
            -(MAX_ITEMS as f64),
            MAX_ITEMS as f64,
        ),
        i("wrap", "折り返す", 0., 0., 1.),
    ],
    output: DataType::Any,
};

pub(crate) const MERGE_TREES: NodeSpec = NodeSpec {
    label: "パスでツリーを合流",
    category: "データ",
    description: "パスが同じ枝だけを A・B の順につなぎ、違う枝や空の枝はそのまま残します。",
    keywords: "merge trees union paths append path ツリー 合流 パス 別の枝",
    inputs: &[
        g("a", "ツリー A", DataType::Any),
        g("b", "ツリー B", DataType::Any),
    ],
    output: DataType::Any,
};

pub(crate) const TREE_BRANCH: NodeSpec = NodeSpec {
    label: "パスで枝を取り出す",
    category: "データ",
    description: "パスを示す整数リストで枝を 1 本選びます。例: 数値リスト 0, 2 は {0;2}。元のパスは変えません。",
    keywords: "tree branch exact path select extract 枝 抽出 パス ツリー",
    inputs: &[
        g("tree", "ツリー", DataType::Any),
        i("path", "パスの数値リスト", 0., 0., u32::MAX as f64),
    ],
    output: DataType::Any,
};

pub(crate) fn evaluate(
    kind: NodeKind,
    inputs: &[Tree],
    stop: &dyn Fn() -> bool,
) -> Option<Result<Tree, String>> {
    Some(match (kind, inputs) {
        (NodeKind::CullIndex, [data, ids, wrap]) => cull(data, ids, wrap, stop),
        (NodeKind::MergeTrees, [a, b]) => merge(a, b, stop),
        (NodeKind::TreeBranch, [data, index]) => branch(data, index, stop),
        (NodeKind::CullIndex | NodeKind::MergeTrees | NodeKind::TreeBranch, _) => {
            Err("ツリー操作の入力が不足しています。".into())
        }
        _ => return None,
    })
}

fn merge(a: &Tree, b: &Tree, stop: &dyn Fn() -> bool) -> Result<Tree, String> {
    if a.item_count().saturating_add(b.item_count()) > MAX_ITEMS {
        return Err("合流する要素の個数が上限を超えています。".into());
    }
    let mut output = Tree::default();
    for input in [a, b] {
        for (path, values) in &input.branches {
            cancelled(stop)?;
            output
                .branches
                .entry(path.clone())
                .or_default()
                .extend(values.iter().cloned());
            if output.branches.len() > MAX_ITEMS {
                return Err("合流する枝の個数が上限を超えています。".into());
            }
        }
    }
    Ok(output)
}

fn branch(data: &Tree, index: &Tree, stop: &dyn Fn() -> bool) -> Result<Tree, String> {
    cancelled(stop)?;
    let ListPath(values) = ListPath::from_tree(index)?;
    let path = values
        .iter()
        .map(|value| value.number().map(|v| v.round() as u32))
        .collect::<Result<Vec<_>, _>>()?;
    let found = data
        .branches
        .get(&path)
        .ok_or_else(|| format!("パス {} に枝がありません。", path_text(&path)))?;
    Ok(Tree {
        branches: BTreeMap::from([(path, found.clone())]),
    })
}

struct ListPath<'a>(&'a [Value]);
impl<'a> ListPath<'a> {
    fn from_tree(tree: &'a Tree) -> Result<Self, String> {
        let mut branches = tree.branches.values();
        let Some(values) = branches.next().filter(|v| (1..=32).contains(&v.len())) else {
            return Err("パスには 1〜32 個の整数を 1 本のリストで指定してください。".into());
        };
        if branches.next().is_some() {
            return Err("パスの入力は 1 本のリストにしてください。".into());
        }
        Ok(Self(values))
    }
}

fn cull(
    data: &Tree,
    indices: &Tree,
    wraps: &Tree,
    stop: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    let mut output = Tree::default();
    for (path, values) in &data.branches {
        cancelled(stop)?;
        if values.is_empty() {
            output.branches.insert(path.clone(), Vec::new());
            continue;
        }
        let idx = matching_control(indices, path)
            .ok_or_else(|| format!("枝 {} に対応する番号の入力がありません。", path_text(path)))?;
        let wrap = match matching_control(wraps, path) {
            Some([value]) => value.number()?.round() != 0.,
            _ => {
                return Err(format!(
                    "枝 {} の「折り返す」は 0 または 1 を 1 個だけ指定してください。",
                    path_text(path)
                ));
            }
        };
        let n = values.len() as i64;
        let mut removed = BTreeSet::new();
        for id in idx {
            cancelled(stop)?;
            let p = id.number()?.round() as i64;
            if wrap {
                removed.insert(p.rem_euclid(n) as usize);
            } else if (0..n).contains(&p) {
                removed.insert(p as usize);
            } else {
                return Err(format!(
                    "枝 {} に番号 {p} はありません（長さ {n}）。",
                    path_text(path)
                ));
            }
        }
        let kept = values
            .iter()
            .enumerate()
            .filter(|(index, _)| !removed.contains(index))
            .map(|(_, value)| value.clone());
        output.branches.insert(path.clone(), kept.collect());
    }
    Ok(output)
}

fn matching_control<'a>(tree: &'a Tree, path: &[u32]) -> Option<&'a [Value]> {
    tree.branches
        .get(path)
        .or_else(|| {
            if tree.branches.len() == 1 {
                tree.branches.get(&[0][..])
            } else {
                None
            }
        })
        .map(Vec::as_slice)
}

fn path_text(path: &[u32]) -> String {
    format!(
        "{{{}}}",
        path.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(";")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tree(branches: &[(&[u32], &[f64])]) -> Tree {
        Tree {
            branches: branches
                .iter()
                .map(|(p, nums)| {
                    (
                        p.to_vec(),
                        nums.iter().copied().map(Value::Number).collect(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn exact_merge_keeps_parent_and_child_empty_paths() {
        let a = tree(&[(&[0], &[5.]), (&[0, 2], &[]), (&[3], &[7.])]);
        let b = tree(&[(&[0], &[9.]), (&[3, 4], &[1.]), (&[5], &[])]);
        assert_eq!(
            merge(&a, &b, &|| false).unwrap(),
            tree(&[
                (&[0], &[5., 9.]),
                (&[0, 2], &[]),
                (&[3], &[7.]),
                (&[3, 4], &[1.]),
                (&[5], &[])
            ])
        );
    }

    #[test]
    fn exact_path_selects_empty_branch_and_reports_a_missing_path() {
        let data = tree(&[(&[0], &[5.]), (&[0, 2], &[]), (&[0, 3], &[9.])]);
        assert_eq!(
            branch(&data, &tree(&[(&[0], &[0., 2.])]), &|| false).unwrap(),
            tree(&[(&[0, 2], &[])])
        );
        assert!(
            branch(&data, &tree(&[(&[0], &[0., 4.])]), &|| false)
                .unwrap_err()
                .contains("{0;4}")
        );
        assert!(branch(&data, &tree(&[(&[0], &[0.]), (&[1], &[2.])]), &|| false).is_err());
    }

    #[test]
    fn cull_deduplicates_wrapped_indices_and_keeps_paths() {
        let data = tree(&[
            (&[3], &[0., 1., 2., 3.]),
            (&[5, 6], &[10., 20., 30.]),
            (&[9], &[]),
        ]);
        let controls = tree(&[(&[0], &[0.999999999, 1., -1.])]);
        assert_eq!(
            cull(&data, &controls, &Tree::single(Value::Number(1.)), &|| {
                false
            })
            .unwrap(),
            tree(&[(&[3], &[0., 2.]), (&[5, 6], &[10.]), (&[9], &[])])
        );
        assert!(
            cull(&data, &controls, &Tree::single(Value::Number(0.)), &|| {
                false
            })
            .unwrap_err()
            .contains("{3}")
        );
        assert!(
            cull(
                &data,
                &tree(&[(&[3], &[0.])]),
                &Tree::single(Value::Number(0.)),
                &|| false
            )
            .unwrap_err()
            .contains("{5;6}")
        );
    }
}
