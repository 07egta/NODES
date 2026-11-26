//! List, formula, randomness, noise and 3D-print helper nodes.
//!
//! These follow the same rules as `engine.rs`: every input may be hostile (NaN is
//! rejected earlier by the input checks, sizes are bounded by the node specs and the
//! engine's budgets), every failure is a Japanese message and nothing here panics.
//! List nodes (`is_list_kind`) receive the whole list of each branch; the other nodes
//! here are evaluated item by item like the older ones.
use crate::engine::{
    as_mesh, cancelled, checked_size, cross, dot, finite_point, length, map_shape, num,
    orient_outward, shape, sub, validate_mesh,
};
use crate::model::*;
use std::f64::consts::TAU;
use std::sync::Arc;

// ------------------------------------------------------------------ specifications

const LIST: InputSpec = g("list", "リスト", DataType::Any);
const NUMBERS: InputSpec = g("values", "数値のリスト", DataType::Number);

static LIST_ITEM: NodeSpec = NodeSpec {
    label: "リスト項目",
    category: "データ",
    description: "リストから番号の位置の要素を取り出します。先頭は 0。番号にリストをつなぐと複数取り出せます。「折り返す」が 1 なら、範囲外の番号は先頭へ戻って数えます（-1 は最後）。",
    keywords: "list item index pick get element リスト 項目 番号 取り出す 要素",
    inputs: &[
        LIST,
        i("index", "番号", 0., -8192., 8192.),
        i("wrap", "折り返す", 1., 0., 1.),
    ],
    output: DataType::Any,
};
static LIST_LENGTH: NodeSpec = NodeSpec {
    label: "リスト長",
    category: "データ",
    description: "ブランチごとに、リストの要素の数を返します。空のリストは 0 です。",
    keywords: "list length count size リスト 長さ 個数 数える",
    inputs: &[LIST],
    output: DataType::Number,
};
static REVERSE_LIST: NodeSpec = NodeSpec {
    label: "反転",
    category: "データ",
    description: "リストの並びを逆にします。",
    keywords: "reverse list flip order リスト 逆順 反転 並び",
    inputs: &[LIST],
    output: DataType::Any,
};
static SORT_LIST: NodeSpec = NodeSpec {
    label: "並べ替え",
    category: "データ",
    description: "数値のリストを小さい順に並べます。大きい順にしたいときは、続けて「反転」をつなぎます。",
    keywords: "sort list order ascending 並べ替え ソート 昇順 小さい順",
    inputs: &[NUMBERS],
    output: DataType::Number,
};
static CULL_PATTERN: NodeSpec = NodeSpec {
    label: "間引きパターン",
    category: "データ",
    description: "パターン（1 で残す、0 で除く）をくり返し当てはめて要素を間引きます。パターンには 1 と 0 のリストをつなぎます。",
    keywords: "cull pattern filter mask remove skip 間引き パターン 除く 残す 交互",
    inputs: &[LIST, i("pattern", "パターン (1:残す 0:除く)", 1., 0., 1.)],
    output: DataType::Any,
};
static SHIFT_LIST: NodeSpec = NodeSpec {
    label: "シフト",
    category: "データ",
    description: "要素の位置を指定した数だけ前へずらします。「折り返す」が 1 ならはみ出した要素を反対側へ回し、0 なら取り除きます。",
    keywords: "shift list offset rotate ずらす シフト 回す 隣",
    inputs: &[
        LIST,
        i("offset", "ずらす数", 1., -8192., 8192.),
        i("wrap", "折り返す", 1., 0., 1.),
    ],
    output: DataType::Any,
};
static CONCAT_LIST: NodeSpec = NodeSpec {
    label: "連結",
    category: "データ",
    description: "リスト A の後ろにリスト B をつなげて 1 つのリストにします。",
    keywords: "concat join append combine list 連結 結合 つなげる 合わせる",
    inputs: &[
        g("a", "リスト A", DataType::Any),
        g("b", "リスト B", DataType::Any),
    ],
    output: DataType::Any,
};
static SUB_LIST: NodeSpec = NodeSpec {
    label: "部分リスト",
    category: "データ",
    description: "開始番号から指定した個数だけ取り出します。リストの終わりを越える分は無視します。",
    keywords: "sub list slice range part 部分 切り出し 一部 スライス",
    inputs: &[
        LIST,
        i("start", "開始番号", 0., 0., 8192.),
        i("count", "個数", 3., 0., 8192.),
    ],
    output: DataType::Any,
};
static BOUNDS: NodeSpec = NodeSpec {
    label: "範囲を調べる",
    category: "数値",
    description: "数値のリストの最小値と最大値を、[最小, 最大] の 2 つの数値として返します。",
    keywords: "bounds domain min max extent 範囲 最小 最大 幅 調べる",
    inputs: &[NUMBERS],
    output: DataType::Number,
};
static REMAP: NodeSpec = NodeSpec {
    label: "再マップ",
    category: "数値",
    description: "数値を元の範囲から新しい範囲へ比例で置き換えます。元の範囲は、0 ならリスト自身の最小〜最大、1 なら指定した値を使います。",
    keywords: "remap map normalize rescale range 再マップ 正規化 置き換え 割合 変換",
    inputs: &[
        NUMBERS,
        n("target_min", "新しい最小", 0., -1e6, 1e6, 0.1),
        n("target_max", "新しい最大", 1., -1e6, 1e6, 0.1),
        i("use_source", "元の範囲 (0:リストから 1:指定)", 0., 0., 1.),
        n("source_min", "元の最小", 0., -1e6, 1e6, 0.1),
        n("source_max", "元の最大", 1., -1e6, 1e6, 0.1),
        i("clamp", "新しい範囲に収める", 0., 0., 1.),
    ],
    output: DataType::Number,
};
static EXPRESSION: NodeSpec = NodeSpec {
    label: "式",
    category: "数値",
    description: "x・y・z・w を使った数式を計算します（例: sin(x) * 10 + y）。式は設定パネルで書きます。使える関数: sin cos tan sqrt abs min max floor ceil round ln exp、記号 + - * / % ^ と pi・e。角度はラジアンです。",
    keywords: "expression formula equation math evaluate 式 数式 計算 関数",
    inputs: &[
        n("x", "x", 1., -1e6, 1e6, 0.1),
        n("y", "y", 0., -1e6, 1e6, 0.1),
        n("z", "z", 0., -1e6, 1e6, 0.1),
        n("w", "w", 0., -1e6, 1e6, 0.1),
    ],
    output: DataType::Number,
};
static RANDOM: NodeSpec = NodeSpec {
    label: "乱数",
    category: "数値",
    description: "最小〜最大の乱数のリストを作ります。同じシードなら毎回同じ数が並ぶので、作品を開き直しても形が変わりません。",
    keywords: "random noise jitter seed 乱数 ランダム シード ばらつき",
    inputs: &[
        i("count", "個数", 10., 0., 512.),
        n("min", "最小", 0., -1e6, 1e6, 0.1),
        n("max", "最大", 1., -1e6, 1e6, 0.1),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Number,
};
static POPULATE: NodeSpec = NodeSpec {
    label: "ランダムな点",
    category: "曲線",
    description: "幅×奥行×高さの箱（XY の中心が原点、底面 Z=0）の中に、決まったシードで点をばらまきます。高さ 0 なら平面上に置きます。",
    keywords: "populate random points scatter seed 点 ばらまく ランダム 散布 配置",
    inputs: &[
        i("count", "個数", 30., 0., 2048.),
        n("width", "幅 (X)", 100., 0., 2000., 1.),
        n("depth", "奥行 (Y)", 100., 0., 2000., 1.),
        n("height", "高さ (Z)", 0., 0., 2000., 1.),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Point,
};
static NOISE: NodeSpec = NodeSpec {
    label: "ノイズ",
    category: "数値",
    description: "位置 (X, Y, Z) からなめらかに変化するノイズの値（およそ -振幅〜+振幅）を返します。模様の大きさを大きくすると、ゆっくり変化します。",
    keywords: "noise perlin gradient smooth random organic ノイズ パーリン ゆらぎ 自然",
    inputs: &[
        n("x", "X", 0., -1e5, 1e5, 1.),
        n("y", "Y", 0., -1e5, 1e5, 1.),
        n("z", "Z", 0., -1e5, 1e5, 1.),
        n("size", "模様の大きさ", 20., 0.1, 10000., 1.),
        n("amp", "振幅", 1., -10000., 10000., 0.1),
        i("seed", "シード", 0., 0., 100000.),
    ],
    output: DataType::Number,
};
static NOISE_DEFORM: NodeSpec = NodeSpec {
    label: "ノイズ変形",
    category: "変形",
    description: "ノイズで頂点を押し出し・押し込みます。方向 0 は Z 軸から外向き（花器向き）、1 は面の向き（メッシュのみ）、2 は Z 方向。細かく分割した形状に使います。",
    keywords: "noise deform displace perlin organic bumpy ノイズ 変形 凸凹 ゆらぎ 押し出し",
    inputs: &[
        g("geometry", "形状", DataType::Geometry),
        n("amp", "振幅", 3., 0., 200., 0.5),
        n("size", "模様の大きさ", 20., 0.1, 2000., 1.),
        i("seed", "シード", 0., 0., 100000.),
        i("mode", "方向 (0:軸から外 1:面の向き 2:Z)", 0., 0., 2.),
    ],
    output: DataType::Geometry,
};
static GRAPH_PROFILE: NodeSpec = NodeSpec {
    label: "グラフで形を決める",
    category: "立体",
    description: "下から上へ並べた半径のリストをなめらかな曲線でつなぎ、底のある器の形（上は開いたまま）を作ります。半径に数列や式のリストをつなぎます。印刷するには「厚みをつける」へ。",
    keywords: "graph mapper profile vase curve radii lathe グラフ 形 輪郭 プロファイル 花器 器 半径",
    inputs: &[
        n("radii", "半径のリスト", 30., 0.5, 1000., 0.5),
        n("height", "高さ", 100., 1., 2000., 1.),
        i("rows", "縦の分割", 48., 2., 256.),
        i("segments", "周の分割", 64., 8., 256.),
        i("top", "上を閉じる", 0., 0., 1.),
    ],
    output: DataType::Mesh,
};
static SHELL: NodeSpec = NodeSpec {
    label: "厚みをつける",
    category: "変形",
    description: "メッシュの面を内側へ厚みの分だけずらした内面を作り、縁をつないで印刷できる壁にします。近似の計算なので、厚みが形の細い部分より大きいと壁が交差します。",
    keywords: "shell thicken offset wall solid print 厚み 肉厚 壁 オフセット 印刷",
    inputs: &[
        g("geometry", "メッシュ", DataType::Mesh),
        n("thickness", "厚み", 2., 0.2, 50., 0.1),
    ],
    output: DataType::Mesh,
};
static OVERHANG_CHECK: NodeSpec = NodeSpec {
    label: "張り出しチェック",
    category: "出力",
    description: "下を向いた面のうち、垂直から許せる角度より寝ている面を調べ、[面積の割合 (%), 面の数, 最大の角度 (度)] の 3 つの数値を返します。造形台に接する底面は数えません。",
    keywords: "overhang check print support angle 張り出し オーバーハング サポート 角度 印刷 検査",
    inputs: &[
        g("geometry", "メッシュ", DataType::Mesh),
        n("angle", "許せる角度（垂直から）", 45., 1., 89., 1.),
    ],
    output: DataType::Number,
};
static DISTANCE: NodeSpec = NodeSpec {
    label: "距離",
    category: "数値",
    description: "2 つの点の間の距離 (mm) を返します。点のリストをつなぐと、順に組にした距離のリストになります。",
    keywords: "distance length between points attractor 距離 長さ 間隔 引力点",
    inputs: &[
        g("a", "点 A", DataType::Point),
        g("b", "点 B", DataType::Point),
    ],
    output: DataType::Number,
};
static POINT_COMPONENT: NodeSpec = NodeSpec {
    label: "点の成分",
    category: "数値",
    description: "点の X・Y・Z のうち 1 つを数値として取り出します。",
    keywords: "deconstruct point component xyz coordinate 点 成分 座標 分解",
    inputs: &[
        g("point", "点", DataType::Point),
        i("axis", "成分 (0:X 1:Y 2:Z)", 0., 0., 2.),
    ],
    output: DataType::Number,
};
static UNKNOWN: NodeSpec = NodeSpec {
    label: "不明なノード",
    category: "データ",
    description: "",
    keywords: "",
    inputs: &[],
    output: DataType::Any,
};

/// ファイル読み込み. The path lives in `Node::script`; the inputs are plain numbers.
static IMPORT_MESH: NodeSpec = NodeSpec {
    label: "ファイル読み込み",
    category: "立体",
    description: "STL・OBJ ファイルの三角形メッシュを読み込みます。「ファイルを選ぶ」か、ウィンドウへのドラッグで指定します。",
    keywords: "import load file stl obj 読み込み ファイル 取り込み 開く 3Dデータ",
    inputs: &[
        n("scale", "倍率", 1., 0.01, 1000., 0.01),
        i("center", "XYを原点へ (0:しない 1:する)", 1., 0., 1.),
        i("bed", "床に置く Z最小=0 (0:しない 1:する)", 1., 0., 1.),
    ],
    output: DataType::Mesh,
};

/// Specification of a node defined in this module.
pub fn spec(kind: NodeKind) -> &'static NodeSpec {
    use NodeKind::*;
    match kind {
        ListItem => &LIST_ITEM,
        ListLength => &LIST_LENGTH,
        ReverseList => &REVERSE_LIST,
        SortList => &SORT_LIST,
        CullPattern => &CULL_PATTERN,
        ShiftList => &SHIFT_LIST,
        ConcatList => &CONCAT_LIST,
        SubList => &SUB_LIST,
        Bounds => &BOUNDS,
        Remap => &REMAP,
        Expression => &EXPRESSION,
        Random => &RANDOM,
        Populate => &POPULATE,
        Noise => &NOISE,
        NoiseDeform => &NOISE_DEFORM,
        GraphProfile => &GRAPH_PROFILE,
        Shell => &SHELL,
        OverhangCheck => &OVERHANG_CHECK,
        Distance => &DISTANCE,
        PointComponent => &POINT_COMPONENT,
        ImportMesh => &IMPORT_MESH,
        _ => &UNKNOWN,
    }
}

/// Nodes that see the whole list of each branch instead of one item at a time.
pub fn is_list_kind(kind: NodeKind) -> bool {
    use NodeKind::*;
    matches!(
        kind,
        ListItem
            | ListLength
            | ReverseList
            | SortList
            | CullPattern
            | ShiftList
            | ConcatList
            | SubList
            | Bounds
            | Remap
            | GraphProfile
    )
}

// ------------------------------------------------------------------ list nodes

fn setting(lists: &[&[Value]], index: usize) -> Result<f64, String> {
    lists
        .get(index)
        .and_then(|l| l.first())
        .ok_or("設定の値がありません。")?
        .number()
}
fn numbers(list: &[Value]) -> Result<Vec<f64>, String> {
    list.iter().map(Value::number).collect()
}
fn too_many(len: usize) -> Result<(), String> {
    if len > MAX_ITEMS {
        Err("リストが長すぎます。要素の数を減らしてください。".into())
    } else {
        Ok(())
    }
}
fn checked_numbers(values: Vec<f64>) -> Result<Vec<Value>, String> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err("計算結果が有限の数値ではありません。".into());
    }
    Ok(values.into_iter().map(Value::Number).collect())
}

/// Evaluates one branch of a list node. `lists[k]` is the branch of input `k`.
pub fn run_list(
    kind: NodeKind,
    lists: &[&[Value]],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    let list: &[Value] = lists.first().copied().unwrap_or(&[]);
    let len = list.len();
    match kind {
        ListLength => Ok(vec![Value::Number(len as f64)]),
        ReverseList => Ok(list.iter().rev().cloned().collect()),
        ConcatList => {
            let other: &[Value] = lists.get(1).copied().unwrap_or(&[]);
            too_many(len.saturating_add(other.len()))?;
            Ok(list.iter().chain(other).cloned().collect())
        }
        _ if len == 0 => Ok(Vec::new()),
        ListItem => {
            let wrap = setting(lists, 2)? != 0.;
            let indices = lists.get(1).copied().unwrap_or(&[]);
            too_many(indices.len())?;
            let mut out = Vec::with_capacity(indices.len());
            for value in indices {
                let raw = value.number()?.round() as i64;
                let index = if wrap {
                    raw.rem_euclid(len as i64) as usize
                } else if raw >= 0 && (raw as usize) < len {
                    raw as usize
                } else {
                    return Err(format!(
                        "番号 {raw} はリストの範囲（0 〜 {}）の外です。「折り返す」を 1 にするか番号を直してください。",
                        len - 1
                    ));
                };
                out.push(list.get(index).cloned().ok_or("番号が範囲外です。")?);
            }
            Ok(out)
        }
        SortList => {
            let mut values = numbers(list)?;
            values.sort_by(f64::total_cmp);
            checked_numbers(values)
        }
        CullPattern => {
            let pattern = numbers(lists.get(1).copied().unwrap_or(&[]))?;
            if pattern.is_empty() {
                return Ok(Vec::new());
            }
            Ok(list
                .iter()
                .enumerate()
                .filter(|(k, _)| pattern.get(k % pattern.len()).is_some_and(|p| *p != 0.))
                .map(|(_, v)| v.clone())
                .collect())
        }
        ShiftList => {
            let offset = setting(lists, 1)?.round() as i64;
            let wrap = setting(lists, 2)? != 0.;
            if wrap {
                let start = offset.rem_euclid(len as i64) as usize;
                Ok(list[start..]
                    .iter()
                    .chain(&list[..start])
                    .cloned()
                    .collect())
            } else if offset >= 0 {
                let start = (offset as usize).min(len);
                Ok(list[start..].to_vec())
            } else {
                let cut = (offset.unsigned_abs() as usize).min(len);
                Ok(list[..len - cut].to_vec())
            }
        }
        SubList => {
            let start = (setting(lists, 1)?.max(0.) as usize).min(len);
            let count = setting(lists, 2)?.max(0.) as usize;
            let end = start.saturating_add(count).min(len);
            Ok(list[start..end].to_vec())
        }
        Bounds => {
            let values = numbers(list)?;
            let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            checked_numbers(vec![lo, hi])
        }
        Remap => {
            let values = numbers(list)?;
            let target = [setting(lists, 1)?, setting(lists, 2)?];
            let source = if setting(lists, 3)? != 0. {
                [setting(lists, 4)?, setting(lists, 5)?]
            } else {
                [
                    values.iter().copied().fold(f64::INFINITY, f64::min),
                    values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ]
            };
            let clamp = setting(lists, 6)? != 0.;
            let span = source[1] - source[0];
            let out = values
                .iter()
                .map(|v| {
                    // A zero-width source range has no proportion: use the middle.
                    let mut t = if span.abs() < 1e-12 {
                        0.5
                    } else {
                        (v - source[0]) / span
                    };
                    if clamp {
                        t = t.clamp(0., 1.);
                    }
                    target[0] + (target[1] - target[0]) * t
                })
                .collect();
            checked_numbers(out)
        }
        GraphProfile => {
            let radii = numbers(list)?;
            let mesh = graph_profile(
                &radii,
                setting(lists, 1)?,
                setting(lists, 2)? as usize,
                setting(lists, 3)? as usize,
                setting(lists, 4)? != 0.,
                cancel,
            )?;
            as_mesh(mesh, cancel)
        }
        _ => Err("このノードはリストを扱いません。".into()),
    }
}

// ------------------------------------------------------------------ item nodes

/// Evaluates one item of a non-list node defined here.
pub fn run_item(
    kind: NodeKind,
    a: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    match kind {
        Random => {
            let count = num(a, 0)? as usize;
            let (lo, hi) = (num(a, 1)?, num(a, 2)?);
            let mut rng = Rng::new(num(a, 3)? as u64);
            checked_numbers(
                (0..count)
                    .map(|_| lo + (hi - lo) * rng.next_f64())
                    .collect(),
            )
        }
        Populate => {
            let count = num(a, 0)? as usize;
            let size = [num(a, 1)?, num(a, 2)?, num(a, 3)?];
            let mut rng = Rng::new(num(a, 4)? as u64);
            let mut out = Vec::with_capacity(count);
            for k in 0..count {
                if k % 256 == 0 {
                    cancelled(cancel)?;
                }
                let p = [
                    size[0] * (rng.next_f64() - 0.5),
                    size[1] * (rng.next_f64() - 0.5),
                    size[2] * rng.next_f64(),
                ];
                if !finite_point(p) {
                    return Err("点の座標が無効です。".into());
                }
                out.push(Value::Point(p));
            }
            Ok(out)
        }
        Noise => {
            let p = [num(a, 0)?, num(a, 1)?, num(a, 2)?];
            let size = num(a, 3)?;
            let amp = num(a, 4)?;
            let seed = num(a, 5)? as u64;
            let v = amp * perlin(p.map(|c| c / size), seed)?;
            checked_numbers(vec![v])
        }
        NoiseDeform => {
            let amp = num(a, 1)?;
            let size = num(a, 2)?;
            let seed = num(a, 3)? as u64;
            let mode = num(a, 4)? as usize;
            Ok(vec![noise_deform(
                shape(a)?,
                amp,
                size,
                seed,
                mode,
                cancel,
            )?])
        }
        Shell => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            as_mesh(shell(m, num(a, 1)?, cancel)?, cancel)
        }
        OverhangCheck => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            checked_numbers(overhang(m, num(a, 1)?, cancel)?.to_vec())
        }
        Distance => {
            let (Some(Value::Point(p)), Some(Value::Point(q))) = (a.first(), a.get(1)) else {
                return Err("点を 2 つ接続してください。".into());
            };
            checked_numbers(vec![length(sub(*p, *q))])
        }
        PointComponent => {
            let Some(Value::Point(p)) = a.first() else {
                return Err("点を接続してください。".into());
            };
            let axis = (num(a, 1)? as usize).min(2);
            checked_numbers(vec![p[axis]])
        }
        _ => Err("このノードは 1 つずつ計算できません。".into()),
    }
}

// ------------------------------------------------------------------ randomness & noise

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
/// SplitMix64: tiny, deterministic on every platform, good enough for shapes.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(mix(seed.wrapping_add(0x5EED)))
    }
    /// Uniform in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        (mix(self.0) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6. - 15.) + 10.)
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn gradient(cell: [i64; 3], seed: u64, d: [f64; 3]) -> f64 {
    let h = mix(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (cell[0] as u64).wrapping_mul(0x8DA6_B343)
        ^ (cell[1] as u64).wrapping_mul(0xD816_3841)
        ^ (cell[2] as u64).wrapping_mul(0xCB1A_B31F));
    // The twelve edge directions of a cube, as in classic gradient noise.
    let [x, y, z] = d;
    match h % 12 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        _ => -y - z,
    }
}
/// Deterministic 3D gradient (Perlin-style) noise, roughly in `[-1, 1]`.
pub fn perlin(p: [f64; 3], seed: u64) -> Result<f64, String> {
    if p.iter().any(|c| !c.is_finite() || c.abs() > 1e12) {
        return Err("ノイズの位置が大きすぎます。模様の大きさを大きくしてください。".into());
    }
    let base = p.map(|c| c.floor());
    let cell = base.map(|c| c as i64);
    let f = [p[0] - base[0], p[1] - base[1], p[2] - base[2]];
    let u = f.map(fade);
    let corner = |dx: i64, dy: i64, dz: i64| {
        gradient(
            [
                cell[0].wrapping_add(dx),
                cell[1].wrapping_add(dy),
                cell[2].wrapping_add(dz),
            ],
            seed,
            [f[0] - dx as f64, f[1] - dy as f64, f[2] - dz as f64],
        )
    };
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u[0]);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u[0]);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u[0]);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u[0]);
    let v = lerp(lerp(x00, x10, u[1]), lerp(x01, x11, u[1]), u[2]);
    Ok(v.clamp(-1., 1.))
}

fn normalized(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = length(v);
    (l > 1e-12 && l.is_finite()).then(|| [v[0] / l, v[1] / l, v[2] / l])
}
/// Per-vertex normals (`None` where unusable) and per-face normals.
type VertexNormals = (Vec<Option<[f64; 3]>>, Vec<[f64; 3]>);
/// Angle-weighted vertex normals; `None` where a vertex has no usable face.
fn vertex_normals(
    m: &Mesh,
    cancel: &dyn Fn() -> bool,
) -> Result<VertexNormals, String> {
    let mut sums = vec![[0.; 3]; m.vertices.len()];
    let mut faces = Vec::with_capacity(m.triangles.len());
    for (k, t) in m.triangles.iter().enumerate() {
        if k % 512 == 0 {
            cancelled(cancel)?;
        }
        let [Some(a), Some(b), Some(c)] = t.map(|i| m.vertices.get(i as usize).copied()) else {
            return Err("メッシュの頂点番号が無効です。".into());
        };
        let normal = normalized(cross(sub(b, a), sub(c, a))).unwrap_or([0.; 3]);
        faces.push(normal);
        for (corner, (p, q, r)) in [(a, b, c), (b, c, a), (c, a, b)].into_iter().enumerate() {
            let (Some(e1), Some(e2)) = (normalized(sub(q, p)), normalized(sub(r, p))) else {
                continue;
            };
            let angle = dot(e1, e2).clamp(-1., 1.).acos();
            let index = t[corner] as usize;
            if let Some(s) = sums.get_mut(index) {
                for axis in 0..3 {
                    s[axis] += normal[axis] * angle;
                }
            }
        }
    }
    Ok((sums.into_iter().map(normalized).collect(), faces))
}

fn noise_deform(
    value: &Value,
    amp: f64,
    size: f64,
    seed: u64,
    mode: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let offset = |p: [f64; 3], dir: [f64; 3]| -> Result<[f64; 3], String> {
        let d = amp * perlin(p.map(|c| c / size), seed)?;
        let q = [p[0] + dir[0] * d, p[1] + dir[1] * d, p[2] + dir[2] * d];
        if finite_point(q) {
            Ok(q)
        } else {
            Err("変形後の座標が大きすぎます。振幅を小さくしてください。".into())
        }
    };
    let radial = |p: [f64; 3]| {
        let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
        if r < 1e-9 {
            [0.; 3]
        } else {
            [p[0] / r, p[1] / r, 0.]
        }
    };
    match value {
        Value::Mesh(m) => {
            let normals = if mode == 1 {
                Some(vertex_normals(m, cancel)?.0)
            } else {
                None
            };
            let mut vertices = Vec::with_capacity(m.vertices.len());
            for (k, p) in m.vertices.iter().enumerate() {
                if k % 256 == 0 {
                    cancelled(cancel)?;
                }
                let dir = match (&normals, mode) {
                    (Some(n), _) => n.get(k).copied().flatten().unwrap_or([0.; 3]),
                    (None, 2) => [0., 0., 1.],
                    _ => radial(*p),
                };
                vertices.push(offset(*p, dir)?);
            }
            let mesh = Mesh {
                vertices,
                triangles: m.triangles.clone(),
            };
            validate_mesh(&mesh, cancel)?;
            Ok(Value::Mesh(Arc::new(mesh)))
        }
        Value::Number(_) => Err("点・曲線・メッシュのいずれかを接続してください。".into()),
        other => {
            // Points and curves have no faces: "面の向き" falls back to the radial direction.
            let failed = std::cell::Cell::new(false);
            let moved = map_shape(
                other,
                |p| {
                    let dir = if mode == 2 { [0., 0., 1.] } else { radial(p) };
                    offset(p, dir).unwrap_or_else(|_| {
                        failed.set(true);
                        p
                    })
                },
                cancel,
            )?;
            if failed.get() {
                return Err(
                    "ノイズの位置が大きすぎます。模様の大きさを大きくしてください。".into(),
                );
            }
            Ok(moved)
        }
    }
}

// ------------------------------------------------------------------ graph profile

/// Catmull-Rom through evenly spaced values; `t` in `[0, 1]`.
pub fn catmull_rom(values: &[f64], t: f64) -> f64 {
    let n = values.len();
    match n {
        0 => 0.,
        1 => values[0],
        _ => {
            let u = t.clamp(0., 1.) * (n - 1) as f64;
            let k = (u.floor() as usize).min(n - 2);
            let f = u - k as f64;
            let at = |i: isize| values[i.clamp(0, n as isize - 1) as usize];
            let k = k as isize;
            let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
            0.5 * (2. * p1
                + (-p0 + p2) * f
                + (2. * p0 - 5. * p1 + 4. * p2 - p3) * f * f
                + (-p0 + 3. * p1 - 3. * p2 + p3) * f * f * f)
        }
    }
}

fn graph_profile(
    radii: &[f64],
    height: f64,
    rows: usize,
    segments: usize,
    top: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if radii.is_empty() {
        return Err("半径のリストが空です。".into());
    }
    if rows < 2 || segments < 3 {
        return Err("分割数が少なすぎます。".into());
    }
    let ring = rows.saturating_add(1).saturating_mul(segments);
    checked_size(
        ring.saturating_add(2),
        rows.saturating_mul(segments)
            .saturating_mul(2)
            .saturating_add(segments.saturating_mul(2)),
    )?;
    let mut m = Mesh::default();
    for row in 0..=rows {
        cancelled(cancel)?;
        let t = row as f64 / rows as f64;
        let r = catmull_rom(radii, t);
        if !(r.is_finite() && r >= 0.5) {
            return Err("グラフの曲線が半径 0.5 mm より細くなる所があります。隣り合う値の差を小さくしてください。".into());
        }
        for j in 0..segments {
            let angle = TAU * j as f64 / segments as f64;
            m.vertices
                .push([r * angle.cos(), r * angle.sin(), height * t]);
        }
    }
    let s = segments as u32;
    for row in 0..rows as u32 {
        for j in 0..s {
            let a = row * s + j;
            let b = row * s + (j + 1) % s;
            m.triangles.extend([[a, b, a + s], [b, b + s, a + s]]);
        }
    }
    let bottom = m.vertices.len() as u32;
    m.vertices.push([0., 0., 0.]);
    for j in 0..s {
        m.triangles.push([bottom, (j + 1) % s, j]);
    }
    if top {
        let center = m.vertices.len() as u32;
        m.vertices.push([0., 0., height]);
        let last = rows as u32 * s;
        for j in 0..s {
            m.triangles.push([center, last + j, last + (j + 1) % s]);
        }
    }
    Ok(m)
}

// ------------------------------------------------------------------ shell

/// Offsets the surface inwards (against the face normals) by `thickness`, keeps the
/// original as the outside and stitches every open edge with a rim. An approximation:
/// it does not detect where the offset surface crosses itself.
pub fn shell(m: &Mesh, thickness: f64, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    if !(thickness.is_finite() && thickness > 0.) {
        return Err("厚みは 0 より大きくしてください。".into());
    }
    let nv = m.vertices.len();
    if nv < 3 || m.triangles.is_empty() {
        return Err("厚みをつけるメッシュに面がありません。".into());
    }
    // A wall thicker than the part itself folds through; refuse the obvious cases.
    for axis in 0..3 {
        let lo = m
            .vertices
            .iter()
            .map(|p| p[axis])
            .fold(f64::INFINITY, f64::min);
        let hi = m
            .vertices
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let extent = hi - lo;
        if extent > 1e-9 && extent <= thickness * 2. {
            return Err("厚みが形の大きさに対して大きすぎます。厚みを小さくしてください。".into());
        }
    }
    // Directed edges: each must appear once; an undirected edge may have at most 2 faces.
    let mut directed = std::collections::BTreeSet::new();
    for (k, t) in m.triangles.iter().enumerate() {
        if k % 512 == 0 {
            cancelled(cancel)?;
        }
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if !directed.insert((a, b)) {
                return Err("面の向きがそろっていないか、3 枚以上の面が 1 つの辺を共有しています。厚みをつけられません。".into());
            }
        }
    }
    let boundary: Vec<(u32, u32)> = directed
        .iter()
        .filter(|(a, b)| !directed.contains(&(*b, *a)))
        .copied()
        .collect();
    checked_size(
        nv.saturating_mul(2),
        m.triangles
            .len()
            .saturating_mul(2)
            .saturating_add(boundary.len().saturating_mul(2)),
    )?;
    let (normals, faces) = vertex_normals(m, cancel)?;
    // Mitre: at a crease the offset along the averaged normal is lengthened so the wall
    // keeps roughly its thickness against every neighbouring face (at most 2x).
    let mut min_dot = vec![1.0f64; nv];
    for (t, f) in m.triangles.iter().zip(&faces) {
        for &i in t {
            let i = i as usize;
            if let (Some(Some(n)), Some(d)) = (normals.get(i), min_dot.get_mut(i)) {
                *d = d.min(dot(*n, *f));
            }
        }
    }
    let mut out = Mesh {
        vertices: m.vertices.clone(),
        triangles: m.triangles.clone(),
    };
    for (k, p) in m.vertices.iter().enumerate() {
        if k % 256 == 0 {
            cancelled(cancel)?;
        }
        let Some(Some(n)) = normals.get(k) else {
            return Err("面のない頂点があるため、厚みの向きを決められません。".into());
        };
        let distance = thickness / min_dot.get(k).copied().unwrap_or(1.).max(0.5);
        let q = [
            p[0] - n[0] * distance,
            p[1] - n[1] * distance,
            p[2] - n[2] * distance,
        ];
        if !finite_point(q) {
            return Err("厚みをつけた座標が無効です。".into());
        }
        out.vertices.push(q);
    }
    let o = nv as u32;
    out.triangles
        .extend(m.triangles.iter().map(|t| [t[0] + o, t[2] + o, t[1] + o]));
    for (a, b) in boundary {
        out.triangles.extend([[b, a, a + o], [b, a + o, b + o]]);
    }
    orient_outward(&mut out);
    validate_mesh(&out, cancel)?;
    Ok(out)
}

// ------------------------------------------------------------------ overhang

/// `[area % of overhanging faces, their count, steepest angle from vertical in degrees]`.
/// Faces lying on the lowest Z (the build plate) are not counted.
pub fn overhang(m: &Mesh, limit_deg: f64, cancel: &dyn Fn() -> bool) -> Result<[f64; 3], String> {
    let lo = m
        .vertices
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    if !lo.is_finite() {
        return Err("メッシュに頂点がありません。".into());
    }
    let limit = limit_deg.clamp(0., 90.).to_radians().sin();
    let (mut total, mut over, mut count, mut steepest) = (0., 0., 0usize, 0.0f64);
    for (k, t) in m.triangles.iter().enumerate() {
        if k % 512 == 0 {
            cancelled(cancel)?;
        }
        let [Some(a), Some(b), Some(c)] = t.map(|i| m.vertices.get(i as usize).copied()) else {
            return Err("メッシュの頂点番号が無効です。".into());
        };
        let normal = cross(sub(b, a), sub(c, a));
        let area = length(normal) / 2.;
        total += area;
        let Some(n) = normalized(normal) else {
            continue;
        };
        let on_plate = [a, b, c].iter().all(|p| (p[2] - lo).abs() < 1e-6);
        let down = -n[2];
        if on_plate || down <= 0. {
            continue;
        }
        let angle = down.clamp(0., 1.).asin().to_degrees();
        if down > limit + 1e-12 {
            over += area;
            count += 1;
            steepest = steepest.max(angle);
        }
    }
    let ratio = if total > 0. { over / total * 100. } else { 0. };
    Ok([ratio, count as f64, steepest])
}
