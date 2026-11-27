//! Typed text operations for Grasshopper-inspired native graphs.
use crate::data_tree::{self as dt, Builder, ItemMatches, MatchMode};
use crate::engine::{cancelled, num};
use crate::model::{DataType, InputSpec, MAX_ITEMS, Node, NodeKind, NodeSpec, Tree, Value, g, i};
use std::collections::BTreeSet;

pub const MAX_LITERAL_BYTES: usize = 8 * 1024;
pub const MAX_TEXT_BYTES: usize = 128 * 1024;
const MAX_TEXT_WORK: usize = 8_000_000;
const TEXT: InputSpec = g("text", "文字列", DataType::Text);
const SEPARATOR: InputSpec = g("separator", "区切り文字", DataType::Text);
const A: InputSpec = g("a", "文字列 A", DataType::Text);
const B: InputSpec = g("b", "文字列 B", DataType::Text);

static TEXT_LITERAL: NodeSpec = NodeSpec {
    label: "文字列",
    category: "文字列",
    description: "ノードを右クリックし、「文字列を編集」で入力した文字列を 1 件のデータとして渡します。",
    keywords: "text literal panel string parameter 値 入力 パネル",
    inputs: &[],
    output: DataType::Text,
};
static CONCATENATE: NodeSpec = NodeSpec {
    label: "文字列の連結",
    category: "文字列",
    description: "対応する文字列 A と B を続けてつなぎます。",
    keywords: "concatenate concat append string 文字列 連結 結合",
    inputs: &[A, B],
    output: DataType::Text,
};
static TEXT_JOIN: NodeSpec = NodeSpec {
    label: "文字列リストの結合",
    category: "文字列",
    description: "枝ごとに文字列リストを結合します。区切り文字を指定すると、要素の間に挿入します。",
    keywords: "text join string list separator 文字列 リスト 結合 区切り",
    inputs: &[g("text", "文字列リスト", DataType::Text), SEPARATOR],
    output: DataType::Text,
};
static TEXT_SPLIT: NodeSpec = NodeSpec {
    label: "文字列の分割",
    category: "文字列",
    description: "指定した各文字で分割します。指定が空なら空白文字で分割します。空の断片も保持します。",
    keywords: "text split string separator 文字列 分割 区切り",
    inputs: &[TEXT, SEPARATOR],
    output: DataType::Text,
};
static TEXT_LENGTH: NodeSpec = NodeSpec {
    label: "文字列の長さ",
    category: "文字列",
    description: "Unicode コードポイントの数を返します。",
    keywords: "text length character count string 文字列 長さ 文字数",
    inputs: &[TEXT],
    output: DataType::Number,
};
static TEXT_TRIM: NodeSpec = NodeSpec {
    label: "文字列の空白除去",
    category: "文字列",
    description: "先頭・末尾から空白文字を除きます。各位置を個別に設定できます。",
    keywords: "text trim string whitespace 文字列 空白 トリム",
    inputs: &[
        TEXT,
        i("start", "先頭", 1., 0., 1.),
        i("end", "末尾", 1., 0., 1.),
    ],
    output: DataType::Text,
};
static TEXT_FRAGMENT: NodeSpec = NodeSpec {
    label: "文字列の切り出し",
    category: "文字列",
    description: "0 から数えた位置から指定数のコードポイントを取り出します。文字数 −1 は末尾まで。",
    keywords: "text fragment substring string slice 文字列 部分 抽出",
    inputs: &[
        TEXT,
        i("start", "開始位置", 0., 0., MAX_TEXT_BYTES as f64),
        i(
            "count",
            "文字数 (−1: 末尾まで)",
            -1.,
            -1.,
            MAX_TEXT_BYTES as f64,
        ),
    ],
    output: DataType::Text,
};
static TEXT_DISTANCE: NodeSpec = NodeSpec {
    label: "文字列の編集距離",
    category: "文字列",
    description: "挿入・削除・置換の最少回数を返します。大文字と小文字の区別を設定できます。",
    keywords: "text distance levenshtein edit string 文字列 編集距離 レーベンシュタイン",
    inputs: &[A, B, i("case", "大文字・小文字を区別", 1., 0., 1.)],
    output: DataType::Number,
};

pub fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    use NodeKind::*;
    Some(match kind {
        Text => &TEXT_LITERAL,
        Concatenate => &CONCATENATE,
        TextJoin => &TEXT_JOIN,
        TextSplit => &TEXT_SPLIT,
        TextLength => &TEXT_LENGTH,
        TextTrim => &TEXT_TRIM,
        TextFragment => &TEXT_FRAGMENT,
        TextDistance => &TEXT_DISTANCE,
        _ => return None,
    })
}

pub fn default_input(kind: NodeKind, key: &str) -> Option<&'static str> {
    (matches!(kind, NodeKind::TextJoin | NodeKind::TextSplit) && key == "separator").then_some("")
}

/// Prefix of at most `max_bytes`, ending on a character boundary.
pub fn utf8_prefix(text: &str, max_bytes: usize) -> &str {
    let mut end = text.len().min(max_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

pub fn validate_literal(text: &str) -> Result<(), String> {
    if text.len() > MAX_LITERAL_BYTES {
        return Err(format!(
            "文字列の入力は {MAX_LITERAL_BYTES} バイトまでです。"
        ));
    }
    Ok(())
}

pub(crate) fn validate_text(text: &str) -> Result<(), String> {
    validate_text_len(text.len())
}

fn validate_text_len(bytes: usize) -> Result<(), String> {
    if bytes > MAX_TEXT_BYTES {
        return Err(format!("文字列の結果は {MAX_TEXT_BYTES} バイトまでです。"));
    }
    Ok(())
}

fn charge(remaining: &mut usize, cost: usize) -> Result<(), String> {
    *remaining = remaining
        .checked_sub(cost)
        .ok_or("文字列の処理量が上限を超えました。入力を短くしてください。")?;
    Ok(())
}

pub(crate) fn evaluate(
    node: &Node,
    inputs: &[Tree],
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    if node.kind == Text {
        validate_literal(&node.script)?;
        return Ok(Tree::single(Value::Text(node.script.as_str().into())));
    }

    let mut output = Builder::default();
    let mut work = MAX_TEXT_WORK;
    let branches = dt::match_branches(inputs)?;
    if node.kind == TextJoin {
        if inputs[0].branches.is_empty() {
            return Ok(Tree::default());
        }
        for dt::BranchMatch { path, lists } in branches {
            cancelled(cancel)?;
            let result = (|| {
                output.append(path.clone(), &[])?;
                let parts = lists[0]
                    .iter()
                    .map(Value::text)
                    .collect::<Result<Vec<_>, _>>()?;
                for separator in lists[1] {
                    cancelled(cancel)?;
                    let joined = join(&parts, separator.text()?, &mut work)?;
                    output.append(path.clone(), &[joined])?;
                }
                Ok::<_, String>(())
            })();
            result.map_err(|error| format!("枝 {}: {error}", dt::path_text(&path)))?;
        }
        return Ok(output.finish());
    }

    let mode = node
        .params
        .get(dt::MATCH_PARAM)
        .copied()
        .map(MatchMode::from_number)
        .transpose()?
        .unwrap_or_default();
    let mut items_remaining = MAX_ITEMS;
    let mut matches = Vec::with_capacity(branches.len());
    for dt::BranchMatch { path, lists } in branches {
        cancelled(cancel)?;
        let tuples = ItemMatches::new(&lists, mode, items_remaining)
            .map_err(|error| format!("枝 {}: {error}", dt::path_text(&path)))?;
        items_remaining -= tuples.cardinality();
        matches.push((path, tuples));
    }
    // A uniform child index keeps the split groups distinct at mixed path depths.
    let grouped = node.kind == TextSplit && matches.iter().any(|(_, m)| m.cardinality() > 1);
    for (path, tuples) in matches {
        cancelled(cancel)?;
        let result = (|| {
            if tuples.cardinality() == 0 {
                return output.append(result_path(&path, grouped, 0), &[]);
            }
            for (index, args) in tuples.enumerate() {
                cancelled(cancel)?;
                for value in &args {
                    charge(&mut work, value.weight())?;
                }
                let values = run_item(node.kind, &args, &mut work, cancel)?;
                output.append(result_path(&path, grouped, index), &values)?;
            }
            Ok(())
        })();
        result.map_err(|error: String| format!("枝 {}: {error}", dt::path_text(&path)))?;
    }
    Ok(output.finish())
}

fn result_path(path: &[u32], grouped: bool, index: usize) -> Vec<u32> {
    let mut result = path.to_vec();
    if grouped {
        result.push(index as u32);
    }
    result
}

fn join(parts: &[&str], separator: &str, remaining: &mut usize) -> Result<Value, String> {
    let bytes = parts.iter().map(|part| part.len()).sum::<usize>()
        + separator.len() * parts.len().saturating_sub(1);
    validate_text_len(bytes)?;
    charge(remaining, bytes + parts.len() + 1)?;
    Ok(Value::Text(parts.join(separator).into()))
}

fn run_item(
    kind: NodeKind,
    args: &[&Value],
    work: &mut usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    let text = args[0].text()?;
    let result = match kind {
        Concatenate => join(&[text, args[1].text()?], "", work)?,
        TextSplit => {
            let separators: BTreeSet<_> = args[1].text()?.chars().collect();
            let pieces = text.split(|ch: char| {
                if separators.is_empty() {
                    ch.is_whitespace()
                } else {
                    separators.contains(&ch)
                }
            });
            let mut result = Vec::new();
            for piece in pieces {
                if result.len() == MAX_ITEMS {
                    return Err("文字列の分割数が上限を超えました。".into());
                }
                cancelled(cancel)?;
                result.push(Value::Text(piece.into()));
            }
            return Ok(result);
        }
        TextLength => Value::Number(text.chars().count() as f64),
        TextTrim => {
            let result = match (num(args, 1)? == 1., num(args, 2)? == 1.) {
                (true, true) => text.trim(),
                (true, false) => text.trim_start(),
                (false, true) => text.trim_end(),
                (false, false) => text,
            };
            Value::Text(result.into())
        }
        TextFragment => {
            let start = num(args, 1)? as usize;
            let count = num(args, 2)?;
            let tail = fragment_tail(text, start)?;
            let fragment = if count >= 0. {
                &tail[..tail
                    .char_indices()
                    .nth(count as usize)
                    .map_or(tail.len(), |(i, _)| i)]
            } else {
                tail
            };
            Value::Text(fragment.into())
        }
        TextDistance => {
            Value::Number(
                distance(text, args[1].text()?, num(args, 2)? == 1., work, cancel)? as f64,
            )
        }
        _ => return Err("文字列ノードを実行できません。".into()),
    };
    Ok(vec![result])
}

fn fragment_tail(text: &str, start: usize) -> Result<&str, String> {
    let mut offsets = text.char_indices().map(|(i, _)| i).chain([text.len()]);
    let offset = offsets
        .nth(start)
        .ok_or("開始位置が文字列の長さを超えています。")?;
    Ok(&text[offset..])
}

fn distance(
    a: &str,
    b: &str,
    case_sensitive: bool,
    remaining: &mut usize,
    cancel: &dyn Fn() -> bool,
) -> Result<usize, String> {
    let (lower_a, lower_b);
    let (a, b) = if case_sensitive {
        (a, b)
    } else {
        lower_a = a.to_lowercase();
        lower_b = b.to_lowercase();
        (lower_a.as_str(), lower_b.as_str())
    };
    let (len_a, len_b) = (a.chars().count(), b.chars().count());
    charge(remaining, len_a.saturating_mul(len_b))?;
    if len_a.min(len_b) == 0 {
        return Ok(len_a.max(len_b));
    }
    let (short, long, n) = if len_a <= len_b {
        (a, b, len_a)
    } else {
        (b, a, len_b)
    };
    let short: Vec<_> = short.chars().collect();
    let mut row: Vec<_> = (0..=n).collect();
    for (line, x) in long.chars().enumerate() {
        cancelled(cancel)?;
        let mut diagonal = row[0];
        row[0] = line + 1;
        for (column, y) in short.iter().enumerate() {
            let above = row[column + 1];
            row[column + 1] = (row[column].min(above) + 1).min(diagonal + usize::from(x != *y));
            diagonal = above;
        }
    }
    Ok(row[n])
}
