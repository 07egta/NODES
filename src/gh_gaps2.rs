//! Second batch of standard Grasshopper-style components that were still missing:
//! a few maths, set and list operations, point and curve helpers, two transforms,
//! mesh face queries, small display helpers and surface corner / boundary queries.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! Every node uses only the existing value types. Curves made here are plain
//! polylines (no exact geometry), and display helpers are ordinary curves.
use crate::engine::{
    cancelled, cross, dot, finite_point, length, map_affine_shape, num, shape, sub,
};
use crate::model::{Curve, DataType, MAX_ITEMS, Mesh, NodeKind as K, NodeSpec, Value, g, i, n};
use crate::surface_kernel::Surface;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

const BIG: f64 = 1e12;
const EPS: f64 = 1e-12;
/// Largest Fibonacci count whose terms stay exact in an f64 when the seeds are 0 and 1.
const MAX_FIBONACCI: f64 = 78.;
/// Joining is quadratic in the number of curves, so it is bounded.
const MAX_JOIN: usize = 512;

const ITEM_KINDS: &[K] = &[
    K::MathIntDivide,
    K::MathSimilar,
    K::MathPi,
    K::SetFibonacci,
    K::PointMidpoint,
    K::CurveSimplify,
    K::CurveClose,
    K::MoveAway,
    K::CenterToOrigin,
    K::MeshFaceAreas,
    K::MeshFaceCenters,
    K::MeshSplitParts,
    K::DisplayArrow,
    K::DisplayBoundingBox,
    K::DisplayAxes,
    K::SurfaceCorners,
    K::SurfaceBoundary,
];

const LIST_KINDS: &[K] = &[
    K::SetSortByKeys,
    K::SetSplitHead,
    K::SetSplitTail,
    K::SetStack,
    K::SetSymmetricDifference,
    K::SetIsSubset,
    K::PointCentroid,
    K::PointSort,
    K::PointCullDuplicates,
    K::CurveJoin,
];

/// Nodes evaluated one matched item at a time.
pub fn is_item_kind(kind: K) -> bool {
    ITEM_KINDS.contains(&kind)
}

/// Nodes that read a whole list per branch.
pub fn is_list_kind(kind: K) -> bool {
    LIST_KINDS.contains(&kind)
}

pub fn is_kind(kind: K) -> bool {
    is_item_kind(kind) || is_list_kind(kind)
}

macro_rules! node_spec {
    ($label:literal, $category:literal, $description:literal, $keywords:literal, $out:expr, [$($input:expr),* $(,)?]) => {{
        static SPEC: NodeSpec = NodeSpec {
            label: $label,
            category: $category,
            description: $description,
            keywords: $keywords,
            inputs: &[$($input),*],
            output: $out,
        };
        Some(&SPEC)
    }};
}

pub fn spec(kind: K) -> Option<&'static NodeSpec> {
    match kind {
        K::MathIntDivide => node_spec!(
            "整数の割り算",
            "数値",
            "A ÷ B の商の整数部分（0 の方向へ切り捨て）。B が 0 のときはエラーです。",
            "integer division quotient truncate 整数 商 割り算",
            DataType::Number,
            [
                n("a", "A", 7., -BIG, BIG, 1.),
                n("b", "B", 2., -BIG, BIG, 1.)
            ]
        ),
        K::MathSimilar => node_spec!(
            "ほぼ等しい",
            "数値",
            "A と B の差が許容差以内なら 1、それより大きければ 0。小数の誤差を許して比べたいときに使います。",
            "similarity equal tolerance approximately 近似 等しい 許容差",
            DataType::Number,
            [
                n("a", "A", 1., -BIG, BIG, 0.01),
                n("b", "B", 1., -BIG, BIG, 0.01),
                n("tolerance", "許容差", 0.001, 0., BIG, 0.001)
            ]
        ),
        K::MathPi => node_spec!(
            "円周率 π の倍数",
            "数値",
            "円周率 π に倍率をかけた値。1 で π、2 で 2π（一周のラジアン）。",
            "pi constant multiplier 円周率 パイ 定数",
            DataType::Number,
            [n("factor", "倍率", 1., -BIG, BIG, 0.01)]
        ),
        K::SetFibonacci => node_spec!(
            "フィボナッチ数列",
            "データ",
            "最初の 2 つの値から、直前 2 つの和を次の値にして並べたリスト。0 と 1 から始めると 0, 1, 1, 2, 3, 5…。",
            "fibonacci sequence series seed フィボナッチ 数列",
            DataType::Number,
            [
                n("a", "最初の値 A", 0., -1e6, 1e6, 1.),
                n("b", "次の値 B", 1., -1e6, 1e6, 1.),
                i("count", "個数", 10., 1., MAX_FIBONACCI)
            ]
        ),
        K::SetSortByKeys => node_spec!(
            "キーで並べ替え",
            "データ",
            "キー（数値）の小さい順に値を並べ替えます。キーと値は同じ個数にします。同じキーは元の順序を保ちます。",
            "sort list keys order by 並べ替え キー 順序",
            DataType::Any,
            [
                g("values", "値のリスト", DataType::Any),
                g("keys", "キーのリスト", DataType::Number)
            ]
        ),
        K::SetSplitHead => node_spec!(
            "リストの前側",
            "データ",
            "リストを指定した番号で二つに分けたときの、前側（先頭から数えて指定個数まで）。個数がリストより多ければ全部です。",
            "split list head first take 分割 先頭 前側",
            DataType::Any,
            [
                g("list", "リスト", DataType::Any),
                i("index", "分割位置", 3., 0., MAX_ITEMS as f64)
            ]
        ),
        K::SetSplitTail => node_spec!(
            "リストの後側",
            "データ",
            "リストを指定した番号で二つに分けたときの、後側（分割位置から最後まで）。「リストの前側」と同じ分割位置を使うと、二つで元のリストになります。",
            "split list tail rest skip 分割 後側 残り",
            DataType::Any,
            [
                g("list", "リスト", DataType::Any),
                i("index", "分割位置", 3., 0., MAX_ITEMS as f64)
            ]
        ),
        K::SetStack => node_spec!(
            "要素ごとに繰り返す",
            "データ",
            "リストの各要素を、回数リストの数だけ続けて並べます（スタック）。回数が足りなければ先頭から繰り返し使います。",
            "stack data repeat each pattern スタック 繰り返し 要素 回数",
            DataType::Any,
            [
                g("list", "リスト", DataType::Any),
                g("counts", "回数のリスト", DataType::Number)
            ]
        ),
        K::SetSymmetricDifference => node_spec!(
            "集合の対称差",
            "データ",
            "A と B のどちらか一方だけにある値。重複は除き、A の分、B の分の順に並べます。数値と点のみ。",
            "symmetric difference set xor 対称差 集合",
            DataType::Any,
            [
                g("a", "リスト A", DataType::Any),
                g("b", "リスト B", DataType::Any)
            ]
        ),
        K::SetIsSubset => node_spec!(
            "部分集合か",
            "データ",
            "B のすべての値が A に含まれていれば 1、そうでなければ 0。B が空なら 1。数値と点のみ。",
            "subset contains all included 部分集合 含む 集合",
            DataType::Number,
            [
                g("a", "リスト A（全体）", DataType::Any),
                g("b", "リスト B（部分）", DataType::Any)
            ]
        ),
        K::PointMidpoint => node_spec!(
            "中点",
            "数値",
            "2 つの点のちょうど真ん中の点。",
            "midpoint middle average two points 中点 中間",
            DataType::Point,
            [
                g("a", "点 A", DataType::Point),
                g("b", "点 B", DataType::Point)
            ]
        ),
        K::PointCentroid => node_spec!(
            "点群の重心",
            "数値",
            "点のリストの平均位置（重心）。点が無ければ何も出しません。",
            "average centroid center of points mean 重心 平均 点群",
            DataType::Point,
            [g("points", "点のリスト", DataType::Point)]
        ),
        K::PointSort => node_spec!(
            "点を並べ替え",
            "数値",
            "点を座標の小さい順に並べます。基準の軸で比べ、同じなら次の軸（X→Y→Z→X…）で比べます。",
            "sort points x y z order 点 並べ替え 座標",
            DataType::Point,
            [
                g("points", "点のリスト", DataType::Point),
                i("axis", "基準の軸 (0:X 1:Y 2:Z)", 0., 0., 2.)
            ]
        ),
        K::PointCullDuplicates => node_spec!(
            "重複点を除く",
            "数値",
            "許容距離以内にある点を 1 つにまとめます。先に出てきた点を残し、順序は保ちます。",
            "cull duplicate points remove same 重複 点 除く",
            DataType::Point,
            [
                g("points", "点のリスト", DataType::Point),
                n("tolerance", "許容距離", 0.001, 0.000001, 1000000., 0.001)
            ]
        ),
        K::CurveSimplify => node_spec!(
            "曲線の簡略化",
            "曲線",
            "折れ線の点を、元の形から許容距離以上ずれない範囲で減らします（ダグラス・ポイカー法）。結果は折れ線で、なめらかな曲線は表示用の点列から作ります。",
            "simplify curve polyline reduce douglas peucker 簡略化 点を減らす",
            DataType::Curve,
            [
                g("curve", "曲線", DataType::Curve),
                n("tolerance", "許容距離", 0.1, 0.000001, 1000000., 0.01)
            ]
        ),
        K::CurveJoin => node_spec!(
            "曲線をつなぐ",
            "曲線",
            "端点どうしが許容距離以内にある曲線をつないで長い折れ線にします。向きが逆でも自動でそろえ、一周すれば閉じた曲線にします。曲線は 512 本まで。",
            "join curves connect polyline 結合 つなぐ 曲線",
            DataType::Curve,
            [
                g("curves", "曲線のリスト", DataType::Curve),
                n("tolerance", "許容距離", 0.001, 0.000001, 1000000., 0.001)
            ]
        ),
        K::CurveClose => node_spec!(
            "曲線を閉じる",
            "曲線",
            "開いた折れ線の終点と始点をつないで閉じた曲線にします。すでに閉じていればそのまま通します。3 点以上が必要です。",
            "close curve loop seal 閉じる 閉曲線",
            DataType::Curve,
            [g("curve", "曲線", DataType::Curve)]
        ),
        K::MoveAway => node_spec!(
            "点から遠ざける",
            "変形",
            "形状を、基準点から見て形状の中心がある向きへ、指定した距離だけ動かします。中心は外側の箱の中心です。",
            "move away from point emitter push 遠ざける 離す 移動",
            DataType::Geometry,
            [
                g("geometry", "形状", DataType::Geometry),
                g("from", "基準点", DataType::Point),
                n("distance", "距離", 10., -BIG, BIG, 0.1)
            ]
        ),
        K::CenterToOrigin => node_spec!(
            "原点へ寄せる",
            "変形",
            "形状の外側の箱の中心を原点に合わせます。「底面を Z=0」を選ぶと、XY は中心を合わせて、底の高さを 0 にします（印刷物を床に置く形）。",
            "center origin align bounding box drop ground 中心 原点 底面 寄せる",
            DataType::Geometry,
            [
                g("geometry", "形状", DataType::Geometry),
                i("mode", "0:箱の中心 1:底面を Z=0", 0., 0., 1.)
            ]
        ),
        K::MeshFaceAreas => node_spec!(
            "面ごとの面積",
            "立体",
            "メッシュの三角形 1 枚ずつの面積を、面の順に並べたリスト。面の数は 16384 まで。",
            "mesh face area list triangle 面積 三角形 メッシュ",
            DataType::Number,
            [g("mesh", "メッシュ", DataType::Mesh)]
        ),
        K::MeshFaceCenters => node_spec!(
            "面の中心点",
            "立体",
            "メッシュの三角形 1 枚ずつの中心（3 頂点の平均）を、面の順に並べた点のリスト。面の数は 16384 まで。",
            "mesh face center centroid points triangle 面 中心 メッシュ",
            DataType::Point,
            [g("mesh", "メッシュ", DataType::Mesh)]
        ),
        K::MeshSplitParts => node_spec!(
            "メッシュを部品ごとに分ける",
            "立体",
            "頂点を共有してつながっている部分ごとに、メッシュを別々のメッシュに分けます。離れた部品が複数あるとき便利です。",
            "disjoint mesh split parts islands components 分離 部品 連結",
            DataType::Mesh,
            [g("mesh", "メッシュ", DataType::Mesh)]
        ),
        K::DisplayArrow => node_spec!(
            "ベクトルの矢印",
            "曲線",
            "始点からベクトルの向きに伸びる矢印を、線で描きます。ベクトルは点の XYZ として渡します。見やすくするための表示用の線です。",
            "vector display arrow draw preview ベクトル 矢印 表示",
            DataType::Curve,
            [
                g("origin", "始点", DataType::Point),
                g("vector", "ベクトル (XYZ)", DataType::Point),
                n("scale", "倍率", 1., 0.001, BIG, 0.1)
            ]
        ),
        K::DisplayBoundingBox => node_spec!(
            "外側の箱の枠",
            "曲線",
            "形状をちょうど囲む、軸に平行な箱の枠線を 6 本の曲線（下の輪・上の輪・縦 4 本）で描きます。大きさの確認用です。",
            "bounding box frame wire display 外接 箱 枠 表示",
            DataType::Curve,
            [g("geometry", "形状", DataType::Geometry)]
        ),
        K::DisplayAxes => node_spec!(
            "座標軸の表示",
            "曲線",
            "指定した点から X・Y・Z の 3 本の軸線を描きます。向きの確認用です。",
            "axes plane display coordinate xyz 座標軸 軸 表示",
            DataType::Curve,
            [
                g("origin", "原点", DataType::Point),
                n("length", "軸の長さ", 20., 0.001, BIG, 0.5)
            ]
        ),
        K::SurfaceCorners => node_spec!(
            "曲面の四隅",
            "精密曲面",
            "曲面の四隅の点を (0,0)・(1,0)・(1,1)・(0,1) の順に返します。",
            "surface corners vertices points 曲面 四隅 角",
            DataType::Point,
            [g("surface", "曲面", DataType::Surface)]
        ),
        K::SurfaceBoundary => node_spec!(
            "曲面の境界線",
            "精密曲面",
            "曲面のふちを一周する閉じた折れ線。各辺を指定した数に分けた点でつなぎます。",
            "surface boundary edge outline border 曲面 境界 ふち 外周",
            DataType::Curve,
            [
                g("surface", "曲面", DataType::Surface),
                i("steps", "1 辺の分割数", 32., 4., 256.)
            ]
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------- shared helpers

fn number(v: f64) -> Result<Vec<Value>, String> {
    if v.is_finite() {
        Ok(vec![Value::Number(v)])
    } else {
        Err("計算結果が大きすぎます。".into())
    }
}

fn flag(v: bool) -> Result<Vec<Value>, String> {
    number(if v { 1. } else { 0. })
}

fn point(p: [f64; 3]) -> Result<Vec<Value>, String> {
    if finite_point(p) {
        Ok(vec![Value::Point(p)])
    } else {
        Err("計算結果が大きすぎます。".into())
    }
}

fn pt(a: &[&Value], at: usize) -> Result<[f64; 3], String> {
    match a.get(at).copied() {
        Some(Value::Point(p)) => Ok(*p),
        _ => Err("点を接続してください。".into()),
    }
}

fn curve_arg<'a>(a: &[&'a Value]) -> Result<&'a Arc<Curve>, String> {
    match a.first().copied() {
        Some(Value::Curve(c)) => Ok(c),
        _ => Err("曲線を接続してください。".into()),
    }
}

fn mesh_arg<'a>(a: &[&'a Value]) -> Result<&'a Arc<Mesh>, String> {
    match a.first().copied() {
        Some(Value::Mesh(m)) => Ok(m),
        _ => Err("メッシュを接続してください。".into()),
    }
}

fn surface_arg<'a>(a: &[&'a Value]) -> Result<&'a Surface, String> {
    match a.first().copied() {
        Some(Value::Surface(s)) => Ok(s.geometry()),
        _ => Err("曲面を接続してください。".into()),
    }
}

/// A plain polyline curve (no exact geometry), with the coordinates checked.
fn plain_curve(points: Vec<[f64; 3]>, closed: bool) -> Result<Value, String> {
    if points.len() < 2 {
        return Err("曲線には 2 点以上が必要です。".into());
    }
    if points.iter().any(|p| !finite_point(*p)) {
        return Err("曲線の座標が大きすぎます。".into());
    }
    Ok(Value::Curve(Arc::new(Curve {
        points,
        closed,
        geometry: None,
    })))
}

fn shape_points(v: &Value) -> Result<&[[f64; 3]], String> {
    let points: &[[f64; 3]] = match v {
        Value::Point(p) => std::slice::from_ref(p),
        Value::Curve(c) => &c.points,
        Value::Mesh(m) => &m.vertices,
        _ => return Err("点・曲線・メッシュのいずれかを接続してください。".into()),
    };
    if points.is_empty() {
        Err("形状に頂点がありません。".into())
    } else {
        Ok(points)
    }
}

/// Lowest and highest corner of the axis-parallel box around the points.
fn bounds(points: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in points {
        lo = std::array::from_fn(|k| lo[k].min(p[k]));
        hi = std::array::from_fn(|k| hi[k].max(p[k]));
    }
    (lo, hi)
}

fn too_many(len: usize) -> Result<(), String> {
    if len > MAX_ITEMS {
        Err("リストが長すぎます。要素の数を減らしてください。".into())
    } else {
        Ok(())
    }
}

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

fn points_of(list: &[Value]) -> Result<Vec<[f64; 3]>, String> {
    list.iter()
        .map(|v| match v {
            Value::Point(p) => Ok(*p),
            _ => Err("点のリストを接続してください。".to_string()),
        })
        .collect()
}

/// Exact-equality key for numbers and points (the only values sets compare).
fn set_key(v: &Value) -> Result<[u64; 4], String> {
    let bits = |x: f64| if x == 0. { 0 } else { x.to_bits() };
    match v {
        Value::Number(x) if x.is_finite() => Ok([0, bits(*x), 0, 0]),
        Value::Point(p) if p.iter().all(|c| c.is_finite()) => {
            Ok([1, bits(p[0]), bits(p[1]), bits(p[2])])
        }
        _ => Err("集合の比較は数値と点のみ対応します。".into()),
    }
}

fn distance_to_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let l2 = dot(ab, ab);
    if l2 < EPS * EPS {
        return length(ap);
    }
    let t = (dot(ap, ab) / l2).clamp(0., 1.);
    length(sub(ap, ab.map(|c| c * t)))
}

/// Marks the points that stay after Douglas-Peucker simplification. Iterative, so
/// a long curve cannot overflow the stack.
fn douglas_peucker(
    points: &[[f64; 3]],
    tolerance: f64,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<bool>, String> {
    let count = points.len();
    let mut keep = vec![false; count];
    if count == 0 {
        return Ok(keep);
    }
    let last = count - 1;
    keep[0] = true;
    keep[last] = true;
    let mut stack = vec![(0usize, last)];
    while let Some((start, end)) = stack.pop() {
        cancelled(stop)?;
        if end <= start + 1 {
            continue;
        }
        let (a, b) = (points[start], points[end]);
        let mut far = 0.;
        let mut at = None;
        for (k, p) in points.iter().enumerate().take(end).skip(start + 1) {
            let d = distance_to_segment(*p, a, b);
            if d > far {
                far = d;
                at = Some(k);
            }
        }
        if far > tolerance
            && let Some(k) = at
        {
            keep[k] = true;
            stack.push((start, k));
            stack.push((k, end));
        }
    }
    Ok(keep)
}

/// Chains open curves whose end points meet within `tolerance`.
#[allow(clippy::needless_range_loop)]
fn join_curves(
    list: &[Value],
    tolerance: f64,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if list.len() > MAX_JOIN {
        return Err("つなぐ曲線が多すぎます（512 本まで）。".into());
    }
    let mut out = Vec::new();
    let mut pieces: Vec<Vec<[f64; 3]>> = Vec::new();
    for v in list {
        let Value::Curve(c) = v else {
            return Err("曲線のリストを接続してください。".into());
        };
        if c.closed {
            out.push(v.clone());
        } else if c.points.len() >= 2 {
            pieces.push(c.points.clone());
        }
    }
    let close = |p: [f64; 3], q: [f64; 3]| length(sub(p, q)) <= tolerance;
    let mut used = vec![false; pieces.len()];
    for start in 0..pieces.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut chain = pieces[start].clone();
        loop {
            cancelled(stop)?;
            let mut grown = false;
            for k in 0..pieces.len() {
                if used[k] {
                    continue;
                }
                let piece = &pieces[k];
                let (Some(&ps), Some(&pe)) = (piece.first(), piece.last()) else {
                    continue;
                };
                let (Some(&cs), Some(&ce)) = (chain.first(), chain.last()) else {
                    break;
                };
                if close(ce, ps) {
                    chain.extend(piece.iter().skip(1).copied());
                } else if close(ce, pe) {
                    chain.extend(piece.iter().rev().skip(1).copied());
                } else if close(cs, pe) {
                    let mut head: Vec<[f64; 3]> = piece.clone();
                    head.pop();
                    head.extend(chain.iter().copied());
                    chain = head;
                } else if close(cs, ps) {
                    let mut head: Vec<[f64; 3]> = piece.iter().rev().copied().collect();
                    head.pop();
                    head.extend(chain.iter().copied());
                    chain = head;
                } else {
                    continue;
                }
                used[k] = true;
                grown = true;
            }
            if !grown {
                break;
            }
        }
        let closed = match (chain.first(), chain.last()) {
            (Some(&f), Some(&l)) => chain.len() >= 4 && close(f, l),
            _ => false,
        };
        if closed {
            chain.pop();
        }
        out.push(plain_curve(chain, closed)?);
    }
    Ok(out)
}

fn triangle_corners(mesh: &Mesh, t: [u32; 3]) -> Result<[[f64; 3]; 3], String> {
    let get = |index: u32| {
        mesh.vertices
            .get(index as usize)
            .copied()
            .ok_or_else(|| "面が存在しない頂点を参照しています。".to_string())
    };
    Ok([get(t[0])?, get(t[1])?, get(t[2])?])
}

fn find_root(parent: &mut [usize], start: usize) -> usize {
    let mut x = start;
    while let Some(&p) = parent.get(x) {
        if p == x {
            break;
        }
        let grand = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(x) {
            *slot = grand;
        }
        x = grand;
    }
    x
}

/// Splits a mesh into the groups of triangles that share vertices.
fn split_parts(mesh: &Mesh, stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let count = mesh.vertices.len();
    if mesh
        .triangles
        .iter()
        .any(|t| t.iter().any(|&v| v as usize >= count))
    {
        return Err("面が存在しない頂点を参照しています。".into());
    }
    let mut parent: Vec<usize> = (0..count).collect();
    for (index, t) in mesh.triangles.iter().enumerate() {
        if index % 1024 == 0 {
            cancelled(stop)?;
        }
        let a = find_root(&mut parent, t[0] as usize);
        for &v in &t[1..] {
            let b = find_root(&mut parent, v as usize);
            if a != b
                && let Some(slot) = parent.get_mut(b)
            {
                *slot = a;
            }
        }
    }
    // Groups in the order of their first triangle.
    let mut group_of_root: HashMap<usize, usize> = HashMap::new();
    let mut groups: Vec<Vec<[u32; 3]>> = Vec::new();
    for (index, t) in mesh.triangles.iter().enumerate() {
        if index % 1024 == 0 {
            cancelled(stop)?;
        }
        let root = find_root(&mut parent, t[0] as usize);
        let slot = *group_of_root.entry(root).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        if let Some(group) = groups.get_mut(slot) {
            group.push(*t);
        }
    }
    too_many(groups.len())?;
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        cancelled(stop)?;
        let mut remap: HashMap<u32, u32> = HashMap::new();
        let mut vertices = Vec::new();
        let mut triangles = Vec::with_capacity(group.len());
        for t in group {
            let mut mapped = [0u32; 3];
            for (slot, &old) in mapped.iter_mut().zip(t.iter()) {
                *slot =
                    match remap.get(&old) {
                        Some(&new) => new,
                        None => {
                            let new = u32::try_from(vertices.len())
                                .map_err(|_| "頂点が多すぎます。".to_string())?;
                            vertices.push(mesh.vertices.get(old as usize).copied().ok_or_else(
                                || "面が存在しない頂点を参照しています。".to_string(),
                            )?);
                            remap.insert(old, new);
                            new
                        }
                    };
            }
            triangles.push(mapped);
        }
        out.push(Value::Mesh(Arc::new(Mesh {
            vertices,
            triangles,
        })));
    }
    Ok(out)
}

fn fibonacci(a: &[&Value]) -> Result<Vec<Value>, String> {
    let (mut p, mut q) = (num(a, 0)?, num(a, 1)?);
    let count = num(a, 2)?;
    if !(0. ..=MAX_FIBONACCI).contains(&count) {
        return Err("個数は 1 〜 78 にしてください。".into());
    }
    let mut out = Vec::new();
    for _ in 0..count as usize {
        if !p.is_finite() {
            return Err("計算結果が大きすぎます。".into());
        }
        out.push(Value::Number(p));
        let next = p + q;
        p = q;
        q = next;
    }
    Ok(out)
}

// ---------------------------------------------------------------- item nodes

pub fn run_item(kind: K, a: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    match kind {
        K::MathIntDivide => {
            let (x, y) = (num(a, 0)?, num(a, 1)?);
            if y == 0. {
                return Err("0 では割れません。B を 0 以外にしてください。".into());
            }
            number((x / y).trunc())
        }
        K::MathSimilar => {
            let (x, y, tolerance) = (num(a, 0)?, num(a, 1)?, num(a, 2)?);
            flag((x - y).abs() <= tolerance)
        }
        K::MathPi => number(num(a, 0)? * std::f64::consts::PI),
        K::SetFibonacci => fibonacci(a),
        K::PointMidpoint => {
            let (p, q) = (pt(a, 0)?, pt(a, 1)?);
            point(std::array::from_fn(|k| p[k] * 0.5 + q[k] * 0.5))
        }
        K::CurveClose => {
            let c = curve_arg(a)?;
            if c.closed {
                return Ok(vec![Value::Curve(c.clone())]);
            }
            let mut points = c.points.clone();
            let repeated = match (points.first(), points.last()) {
                (Some(&f), Some(&l)) => points.len() > 1 && length(sub(l, f)) <= EPS,
                _ => false,
            };
            if repeated {
                points.pop();
            }
            if points.len() < 3 {
                return Err("閉じるには 3 点以上の曲線が必要です。".into());
            }
            Ok(vec![plain_curve(points, true)?])
        }
        K::CurveSimplify => {
            let c = curve_arg(a)?;
            let tolerance = num(a, 1)?;
            let mut points = c.points.clone();
            if c.closed
                && let Some(&first) = points.first()
            {
                points.push(first);
            }
            let keep = douglas_peucker(&points, tolerance, stop)?;
            let mut out: Vec<[f64; 3]> = points
                .iter()
                .zip(&keep)
                .filter(|(_, k)| **k)
                .map(|(p, _)| *p)
                .collect();
            if c.closed {
                out.pop();
            }
            if out.len() < if c.closed { 3 } else { 2 } {
                return Err("許容距離が大きすぎて、曲線が残りません。小さくしてください。".into());
            }
            Ok(vec![plain_curve(out, c.closed)?])
        }
        K::MoveAway => {
            let v = shape(a)?;
            let from = pt(a, 1)?;
            let distance = num(a, 2)?;
            let (lo, hi) = bounds(shape_points(v)?);
            let center: [f64; 3] = std::array::from_fn(|k| lo[k] * 0.5 + hi[k] * 0.5);
            let direction = sub(center, from);
            let l = length(direction);
            if l < EPS {
                return Err("基準点が形状の中心と重なっていて、向きが決まりません。".into());
            }
            let d: [f64; 3] = direction.map(|c| c / l * distance);
            Ok(vec![map_affine_shape(
                v,
                move |p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]],
                stop,
            )?])
        }
        K::CenterToOrigin => {
            let v = shape(a)?;
            let ground = num(a, 1)? == 1.;
            let (lo, hi) = bounds(shape_points(v)?);
            let center: [f64; 3] = std::array::from_fn(|k| lo[k] * 0.5 + hi[k] * 0.5);
            let shift = [
                -center[0],
                -center[1],
                if ground { -lo[2] } else { -center[2] },
            ];
            Ok(vec![map_affine_shape(
                v,
                move |p| [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]],
                stop,
            )?])
        }
        K::MeshFaceAreas | K::MeshFaceCenters => {
            let mesh = mesh_arg(a)?;
            too_many(mesh.triangles.len())?;
            let mut out = Vec::with_capacity(mesh.triangles.len());
            for (index, t) in mesh.triangles.iter().enumerate() {
                if index % 1024 == 0 {
                    cancelled(stop)?;
                }
                let [p, q, r] = triangle_corners(mesh, *t)?;
                if kind == K::MeshFaceAreas {
                    let area = length(cross(sub(q, p), sub(r, p))) * 0.5;
                    if !area.is_finite() {
                        return Err("計算結果が大きすぎます。".into());
                    }
                    out.push(Value::Number(area));
                } else {
                    out.push(Value::Point(std::array::from_fn(|k| {
                        (p[k] + q[k] + r[k]) / 3.
                    })));
                }
            }
            Ok(out)
        }
        K::MeshSplitParts => split_parts(mesh_arg(a)?, stop),
        K::DisplayArrow => {
            let (origin, vector, scale) = (pt(a, 0)?, pt(a, 1)?, num(a, 2)?);
            let l = length(vector);
            if l < EPS {
                return Err("ベクトルの長さが 0 です。".into());
            }
            let dir = vector.map(|c| c / l);
            let full = l * scale;
            let tip: [f64; 3] = std::array::from_fn(|k| origin[k] + dir[k] * full);
            let head = full * 0.2;
            let helper = if dir[2].abs() < 0.9 {
                [0., 0., 1.]
            } else {
                [1., 0., 0.]
            };
            let side = cross(dir, helper);
            let sl = length(side);
            if sl < EPS {
                return Err("矢じりの向きを決められません。".into());
            }
            let side = side.map(|c| c / sl * head * 0.4);
            let back: [f64; 3] = std::array::from_fn(|k| tip[k] - dir[k] * head);
            let left: [f64; 3] = std::array::from_fn(|k| back[k] + side[k]);
            let right: [f64; 3] = std::array::from_fn(|k| back[k] - side[k]);
            Ok(vec![plain_curve(
                vec![origin, tip, left, tip, right],
                false,
            )?])
        }
        K::DisplayBoundingBox => {
            let (lo, hi) = bounds(shape_points(shape(a)?)?);
            let corner = |x: bool, y: bool, z: bool| {
                [
                    if x { hi[0] } else { lo[0] },
                    if y { hi[1] } else { lo[1] },
                    if z { hi[2] } else { lo[2] },
                ]
            };
            let ring = |z: bool| {
                vec![
                    corner(false, false, z),
                    corner(true, false, z),
                    corner(true, true, z),
                    corner(false, true, z),
                ]
            };
            let mut out = vec![
                plain_curve(ring(false), true)?,
                plain_curve(ring(true), true)?,
            ];
            for (x, y) in [(false, false), (true, false), (true, true), (false, true)] {
                out.push(plain_curve(
                    vec![corner(x, y, false), corner(x, y, true)],
                    false,
                )?);
            }
            Ok(out)
        }
        K::DisplayAxes => {
            let (origin, size) = (pt(a, 0)?, num(a, 1)?);
            let mut out = Vec::with_capacity(3);
            for axis in 0..3 {
                let mut end = origin;
                if let Some(c) = end.get_mut(axis) {
                    *c += size;
                }
                out.push(plain_curve(vec![origin, end], false)?);
            }
            Ok(out)
        }
        K::SurfaceCorners => {
            let s = surface_arg(a)?;
            let mut out = Vec::with_capacity(4);
            for (u, v) in [(0., 0.), (1., 0.), (1., 1.), (0., 1.)] {
                out.push(Value::Point(s.evaluate(u, v)?.point));
            }
            Ok(out)
        }
        K::SurfaceBoundary => {
            let s = surface_arg(a)?;
            let steps = num(a, 1)? as usize;
            if !(4..=256).contains(&steps) {
                return Err("分割数は 4 〜 256 にしてください。".into());
            }
            let mut points = Vec::with_capacity(steps * 4);
            for edge in 0..4 {
                for k in 0..steps {
                    cancelled(stop)?;
                    let t = k as f64 / steps as f64;
                    let (u, v) = match edge {
                        0 => (t, 0.),
                        1 => (1., t),
                        2 => (1. - t, 1.),
                        _ => (0., 1. - t),
                    };
                    points.push(s.evaluate(u, v)?.point);
                }
            }
            Ok(vec![plain_curve(points, true)?])
        }
        _ => Err("このノードは未対応です。".into()),
    }
}

// ---------------------------------------------------------------- list nodes

/// Evaluates one branch of a list node. `lists[k]` is the branch of input `k`.
pub fn run_list(
    kind: K,
    lists: &[&[Value]],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let list: &[Value] = lists.first().copied().unwrap_or(&[]);
    let other: &[Value] = lists.get(1).copied().unwrap_or(&[]);
    match kind {
        K::SetSortByKeys => {
            let keys = numbers(other)?;
            if keys.len() != list.len() {
                return Err("キーの数と値の数が同じではありません。".into());
            }
            let mut order: Vec<usize> = (0..list.len()).collect();
            order.sort_by(|x, y| match (keys.get(*x), keys.get(*y)) {
                (Some(p), Some(q)) => p.total_cmp(q),
                _ => Ordering::Equal,
            });
            Ok(order
                .into_iter()
                .filter_map(|k| list.get(k).cloned())
                .collect())
        }
        K::SetSplitHead => {
            let at = setting(lists, 1)? as usize;
            Ok(list.iter().take(at).cloned().collect())
        }
        K::SetSplitTail => {
            let at = setting(lists, 1)? as usize;
            Ok(list.iter().skip(at).cloned().collect())
        }
        K::SetStack => {
            let counts = numbers(other)?;
            if counts.is_empty() {
                return Err("回数のリストが空です。".into());
            }
            let mut out = Vec::new();
            for (index, item) in list.iter().enumerate() {
                cancelled(stop)?;
                let times = counts
                    .get(index % counts.len())
                    .copied()
                    .ok_or("回数がありません。")?
                    .round();
                if !(0. ..=MAX_ITEMS as f64).contains(&times) {
                    return Err("回数は 0 以上の整数にしてください。".into());
                }
                too_many(out.len().saturating_add(times as usize))?;
                for _ in 0..times as usize {
                    out.push(item.clone());
                }
            }
            Ok(out)
        }
        K::SetSymmetricDifference => {
            let mut in_a = BTreeSet::new();
            for v in list {
                cancelled(stop)?;
                in_a.insert(set_key(v)?);
            }
            let mut in_b = BTreeSet::new();
            for v in other {
                cancelled(stop)?;
                in_b.insert(set_key(v)?);
            }
            let mut seen = BTreeSet::new();
            let mut out = Vec::new();
            for v in list {
                let key = set_key(v)?;
                if !in_b.contains(&key) && seen.insert(key) {
                    out.push(v.clone());
                }
            }
            for v in other {
                let key = set_key(v)?;
                if !in_a.contains(&key) && seen.insert(key) {
                    out.push(v.clone());
                }
            }
            too_many(out.len())?;
            Ok(out)
        }
        K::SetIsSubset => {
            let mut in_a = BTreeSet::new();
            for v in list {
                cancelled(stop)?;
                in_a.insert(set_key(v)?);
            }
            for v in other {
                cancelled(stop)?;
                if !in_a.contains(&set_key(v)?) {
                    return flag(false);
                }
            }
            flag(true)
        }
        K::PointCentroid => {
            let points = points_of(list)?;
            let Some(&origin) = points.first() else {
                return Ok(Vec::new());
            };
            // Averaging offsets from the first point keeps large coordinates accurate.
            let mut sum = [0.; 3];
            for p in &points {
                for (slot, (c, o)) in sum.iter_mut().zip(p.iter().zip(origin.iter())) {
                    *slot += c - o;
                }
            }
            let count = points.len() as f64;
            point(std::array::from_fn(|k| origin[k] + sum[k] / count))
        }
        K::PointSort => {
            let mut points = points_of(list)?;
            let axis = setting(lists, 1)? as usize % 3;
            let order = [axis, (axis + 1) % 3, (axis + 2) % 3];
            points.sort_by(|p, q| {
                order
                    .iter()
                    .map(|&k| p[k].total_cmp(&q[k]))
                    .find(|o| o.is_ne())
                    .unwrap_or(Ordering::Equal)
            });
            Ok(points.into_iter().map(Value::Point).collect())
        }
        K::PointCullDuplicates => {
            let points = points_of(list)?;
            let tolerance = setting(lists, 1)?;
            if !(tolerance.is_finite() && tolerance > 0.) {
                return Err("許容距離は 0 より大きい数にしてください。".into());
            }
            if points.iter().any(|p| !finite_point(*p)) {
                return Err("点の座標が大きすぎます。".into());
            }
            let cell = |p: [f64; 3]| -> [i64; 3] {
                std::array::from_fn(|k| (p[k] / tolerance).floor() as i64)
            };
            let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
            let mut kept: Vec<[f64; 3]> = Vec::new();
            for (index, p) in points.iter().enumerate() {
                if index % 1024 == 0 {
                    cancelled(stop)?;
                }
                let c = cell(*p);
                let mut duplicate = false;
                'search: for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            let near = [c[0] + dx, c[1] + dy, c[2] + dz];
                            let Some(ids) = grid.get(&near) else {
                                continue;
                            };
                            for &id in ids {
                                if let Some(&q) = kept.get(id)
                                    && length(sub(q, *p)) <= tolerance
                                {
                                    duplicate = true;
                                    break 'search;
                                }
                            }
                        }
                    }
                }
                if !duplicate {
                    grid.entry(c).or_default().push(kept.len());
                    kept.push(*p);
                }
            }
            Ok(kept.into_iter().map(Value::Point).collect())
        }
        K::CurveJoin => {
            let tolerance = setting(lists, 1)?;
            join_curves(list, tolerance, stop)
        }
        _ => Err("このノードは未対応です。".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(kind: K, values: &[Value]) -> Result<Vec<Value>, String> {
        let refs: Vec<&Value> = values.iter().collect();
        run_item(kind, &refs, &|| false)
    }
    fn nums(values: &[f64]) -> Vec<Value> {
        values.iter().map(|v| Value::Number(*v)).collect()
    }
    fn pts(values: &[[f64; 3]]) -> Vec<Value> {
        values.iter().map(|p| Value::Point(*p)).collect()
    }
    fn one(result: Result<Vec<Value>, String>) -> f64 {
        match result.as_deref() {
            Ok([Value::Number(v)]) => *v,
            _ => f64::NAN,
        }
    }
    fn polyline(points: &[[f64; 3]], closed: bool) -> Value {
        Value::Curve(Arc::new(Curve {
            points: points.to_vec(),
            closed,
            geometry: None,
        }))
    }
    fn curve_points(v: &Value) -> Vec<[f64; 3]> {
        match v {
            Value::Curve(c) => c.points.clone(),
            _ => Vec::new(),
        }
    }
    fn two_triangles_apart() -> Value {
        Value::Mesh(Arc::new(Mesh {
            vertices: vec![
                [0., 0., 0.],
                [2., 0., 0.],
                [0., 2., 0.],
                [10., 0., 0.],
                [11., 0., 0.],
                [10., 1., 0.],
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
        }))
    }
    fn list(kind: K, lists: &[&[Value]]) -> Result<Vec<Value>, String> {
        run_list(kind, lists, &|| false)
    }

    #[test]
    fn every_kind_has_a_spec_and_nothing_else_does() {
        for kind in K::all() {
            assert_eq!(is_kind(*kind), spec(*kind).is_some(), "{kind:?}");
            assert!(!(is_item_kind(*kind) && is_list_kind(*kind)), "{kind:?}");
        }
        assert_eq!(ITEM_KINDS.len() + LIST_KINDS.len(), 27);
    }

    #[test]
    fn maths_nodes() {
        assert_eq!(one(items(K::MathIntDivide, &nums(&[7., 2.]))), 3.);
        assert_eq!(one(items(K::MathIntDivide, &nums(&[-7., 2.]))), -3.);
        assert!(items(K::MathIntDivide, &nums(&[1., 0.])).is_err());
        assert_eq!(one(items(K::MathSimilar, &nums(&[1., 1.0005, 0.001]))), 1.);
        assert_eq!(one(items(K::MathSimilar, &nums(&[1., 1.1, 0.001]))), 0.);
        let two_pi = one(items(K::MathPi, &nums(&[2.])));
        assert!((two_pi - std::f64::consts::TAU).abs() < 1e-12);
    }

    #[test]
    fn fibonacci_follows_the_seeds() {
        let got = items(K::SetFibonacci, &nums(&[0., 1., 8.]));
        assert_eq!(got, Ok(nums(&[0., 1., 1., 2., 3., 5., 8., 13.])));
        let seeded = items(K::SetFibonacci, &nums(&[2., 1., 4.]));
        assert_eq!(seeded, Ok(nums(&[2., 1., 3., 4.])));
        assert!(items(K::SetFibonacci, &nums(&[0., 1., 79.])).is_err());
    }

    #[test]
    fn sort_by_keys_split_and_stack() {
        let values = nums(&[10., 20., 30., 40.]);
        let keys = nums(&[3., 1., 2., 1.]);
        assert_eq!(
            list(K::SetSortByKeys, &[&values, &keys]),
            Ok(nums(&[20., 40., 30., 10.]))
        );
        assert!(list(K::SetSortByKeys, &[&values, &nums(&[1.])]).is_err());
        let at = nums(&[3.]);
        assert_eq!(
            list(K::SetSplitHead, &[&nums(&[1., 2., 3., 4., 5.]), &at]),
            Ok(nums(&[1., 2., 3.]))
        );
        assert_eq!(
            list(K::SetSplitTail, &[&nums(&[1., 2., 3., 4., 5.]), &at]),
            Ok(nums(&[4., 5.]))
        );
        assert_eq!(
            list(K::SetSplitHead, &[&nums(&[1., 2.]), &nums(&[9.])]),
            Ok(nums(&[1., 2.]))
        );
        assert_eq!(
            list(K::SetStack, &[&nums(&[7., 8., 9.]), &nums(&[2., 0., 1.])]),
            Ok(nums(&[7., 7., 9.]))
        );
        assert!(list(K::SetStack, &[&nums(&[1.]), &[]]).is_err());
        assert!(list(K::SetStack, &[&nums(&[1.]), &nums(&[-1.])]).is_err());
    }

    #[test]
    fn set_algebra() {
        let a = nums(&[1., 2., 3., 3.]);
        let b = nums(&[3., 4., 4., 5.]);
        assert_eq!(
            list(K::SetSymmetricDifference, &[&a, &b]),
            Ok(nums(&[1., 2., 4., 5.]))
        );
        assert_eq!(one(list(K::SetIsSubset, &[&a, &nums(&[1., 3.])])), 1.);
        assert_eq!(one(list(K::SetIsSubset, &[&a, &nums(&[1., 9.])])), 0.);
        assert_eq!(one(list(K::SetIsSubset, &[&a, &[]])), 1.);
        let p = pts(&[[0., 0., 0.]]);
        assert_eq!(one(list(K::SetIsSubset, &[&p, &pts(&[[0., 0., 0.]])])), 1.);
    }

    #[test]
    fn point_helpers() {
        let mid = items(K::PointMidpoint, &pts(&[[0., 0., 0.], [2., 4., 6.]]));
        assert_eq!(mid, Ok(vec![Value::Point([1., 2., 3.])]));
        let cloud = pts(&[[5e8, 0., 0.], [5e8 + 2., 0., 0.], [5e8 + 1., 3., 0.]]);
        match list(K::PointCentroid, &[&cloud]).as_deref() {
            Ok([Value::Point(p)]) => {
                assert!((p[0] - (5e8 + 1.)).abs() < 1e-6 && (p[1] - 1.).abs() < 1e-9);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(list(K::PointCentroid, &[&[]]), Ok(Vec::new()));
        let unsorted = pts(&[[2., 0., 0.], [1., 5., 0.], [1., 2., 0.]]);
        assert_eq!(
            list(K::PointSort, &[&unsorted, &nums(&[0.])]),
            Ok(pts(&[[1., 2., 0.], [1., 5., 0.], [2., 0., 0.]]))
        );
        assert_eq!(
            list(K::PointSort, &[&unsorted, &nums(&[1.])]),
            Ok(pts(&[[2., 0., 0.], [1., 2., 0.], [1., 5., 0.]]))
        );
        let dup = pts(&[
            [0., 0., 0.],
            [0.0004, 0., 0.],
            [1., 0., 0.],
            [1., 0.0009, 0.],
        ]);
        assert_eq!(
            list(K::PointCullDuplicates, &[&dup, &nums(&[0.001])]),
            Ok(pts(&[[0., 0., 0.], [1., 0., 0.]]))
        );
        assert!(list(K::PointCullDuplicates, &[&dup, &nums(&[0.])]).is_err());
    }

    #[test]
    fn curve_close_simplify_and_join() {
        let open = polyline(&[[0., 0., 0.], [4., 0., 0.], [4., 3., 0.]], false);
        let closed = items(K::CurveClose, std::slice::from_ref(&open));
        match closed.as_deref() {
            Ok([Value::Curve(c)]) => assert!(c.closed && c.points.len() == 3),
            other => panic!("unexpected {other:?}"),
        }
        let two = polyline(&[[0., 0., 0.], [1., 0., 0.]], false);
        assert!(items(K::CurveClose, std::slice::from_ref(&two)).is_err());

        let wobbly = polyline(
            &[
                [0., 0., 0.],
                [1., 0.01, 0.],
                [2., -0.01, 0.],
                [3., 0., 0.],
                [3., 5., 0.],
            ],
            false,
        );
        let simplified = items(K::CurveSimplify, &[wobbly, Value::Number(0.1)]);
        match simplified.as_deref() {
            Ok([v]) => assert_eq!(
                curve_points(v),
                vec![[0., 0., 0.], [3., 0., 0.], [3., 5., 0.]]
            ),
            other => panic!("unexpected {other:?}"),
        }
        let ring = polyline(
            &[
                [0., 0., 0.],
                [2., 0., 0.],
                [4., 0., 0.],
                [4., 4., 0.],
                [0., 4., 0.],
            ],
            true,
        );
        match items(K::CurveSimplify, &[ring, Value::Number(0.1)]).as_deref() {
            Ok([Value::Curve(c)]) => assert!(c.closed && c.points.len() == 4),
            other => panic!("unexpected {other:?}"),
        }

        let a = polyline(&[[0., 0., 0.], [1., 0., 0.]], false);
        let b = polyline(&[[2., 0., 0.], [1., 0., 0.]], false);
        let c = polyline(&[[2., 0., 0.], [2., 1., 0.]], false);
        let far = polyline(&[[9., 9., 9.], [9., 9., 10.]], false);
        let joined = list(K::CurveJoin, &[&[a, b, c, far], &nums(&[0.001])]);
        match joined.as_deref() {
            Ok([first, second]) => {
                assert_eq!(curve_points(first).len(), 4);
                assert_eq!(curve_points(second).len(), 2);
            }
            other => panic!("unexpected {other:?}"),
        }
        let sides = [
            polyline(&[[0., 0., 0.], [1., 0., 0.]], false),
            polyline(&[[1., 0., 0.], [1., 1., 0.]], false),
            polyline(&[[1., 1., 0.], [0., 0., 0.]], false),
        ];
        match list(K::CurveJoin, &[&sides, &nums(&[0.001])]).as_deref() {
            Ok([Value::Curve(c)]) => assert!(c.closed && c.points.len() == 3),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn transforms_move_and_center() {
        let line = polyline(&[[2., 0., 0.], [4., 0., 0.]], false);
        let away = items(
            K::MoveAway,
            &[line.clone(), Value::Point([0., 0., 0.]), Value::Number(5.)],
        );
        match away.as_deref() {
            Ok([v]) => assert_eq!(curve_points(v), vec![[7., 0., 0.], [9., 0., 0.]]),
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            items(
                K::MoveAway,
                &[line.clone(), Value::Point([3., 0., 0.]), Value::Number(5.)]
            )
            .is_err()
        );
        let lifted = polyline(&[[2., 2., 5.], [4., 6., 9.]], false);
        match items(K::CenterToOrigin, &[lifted.clone(), Value::Number(0.)]).as_deref() {
            Ok([v]) => assert_eq!(curve_points(v), vec![[-1., -2., -2.], [1., 2., 2.]]),
            other => panic!("unexpected {other:?}"),
        }
        match items(K::CenterToOrigin, &[lifted, Value::Number(1.)]).as_deref() {
            Ok([v]) => assert_eq!(curve_points(v), vec![[-1., -2., 0.], [1., 2., 4.]]),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn mesh_face_queries_and_parts() {
        let mesh = two_triangles_apart();
        assert_eq!(
            items(K::MeshFaceAreas, std::slice::from_ref(&mesh)),
            Ok(nums(&[2., 0.5]))
        );
        match items(K::MeshFaceCenters, std::slice::from_ref(&mesh)).as_deref() {
            Ok([Value::Point(p), Value::Point(q)]) => {
                assert!((p[0] - 2. / 3.).abs() < 1e-12 && (p[1] - 2. / 3.).abs() < 1e-12);
                assert!((q[0] - 31. / 3.).abs() < 1e-12);
            }
            other => panic!("unexpected {other:?}"),
        }
        match items(K::MeshSplitParts, std::slice::from_ref(&mesh)).as_deref() {
            Ok([Value::Mesh(a), Value::Mesh(b)]) => {
                assert_eq!((a.vertices.len(), a.triangles.len()), (3, 1));
                assert_eq!((b.vertices.len(), b.triangles.len()), (3, 1));
                assert_eq!(b.triangles, vec![[0, 1, 2]]);
            }
            other => panic!("unexpected {other:?}"),
        }
        let broken = Value::Mesh(Arc::new(Mesh {
            vertices: vec![[0., 0., 0.]],
            triangles: vec![[0, 1, 2]],
        }));
        assert!(items(K::MeshFaceAreas, std::slice::from_ref(&broken)).is_err());
        assert!(items(K::MeshSplitParts, std::slice::from_ref(&broken)).is_err());
    }

    #[test]
    fn display_helpers_draw_curves() {
        let arrow = items(
            K::DisplayArrow,
            &[
                Value::Point([0., 0., 0.]),
                Value::Point([0., 0., 2.]),
                Value::Number(1.),
            ],
        );
        match arrow.as_deref() {
            Ok([v]) => {
                let p = curve_points(v);
                assert_eq!(p.len(), 5);
                assert_eq!(p[1], [0., 0., 2.]);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            items(
                K::DisplayArrow,
                &[
                    Value::Point([0.; 3]),
                    Value::Point([0.; 3]),
                    Value::Number(1.)
                ]
            )
            .is_err()
        );
        let frame = items(
            K::DisplayBoundingBox,
            &[polyline(&[[0., 0., 0.], [2., 3., 4.]], false)],
        );
        assert_eq!(frame.map(|v| v.len()), Ok(6));
        let axes = items(
            K::DisplayAxes,
            &[Value::Point([1., 1., 1.]), Value::Number(5.)],
        );
        match axes.as_deref() {
            Ok([x, y, z]) => {
                assert_eq!(curve_points(x)[1], [6., 1., 1.]);
                assert_eq!(curve_points(y)[1], [1., 6., 1.]);
                assert_eq!(curve_points(z)[1], [1., 1., 6.]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn surface_corners_and_boundary() {
        let Ok(plane) = Surface::plane(4., 2.) else {
            panic!("plane");
        };
        let value = match crate::surface_kernel::SurfaceValue::new(plane, &|| false) {
            Ok(v) => Value::Surface(Arc::new(v)),
            Err(e) => panic!("{e}"),
        };
        match items(K::SurfaceCorners, std::slice::from_ref(&value)).as_deref() {
            Ok(corners) => assert_eq!(corners.len(), 4),
            other => panic!("unexpected {other:?}"),
        }
        match items(K::SurfaceBoundary, &[value, Value::Number(8.)]).as_deref() {
            Ok([Value::Curve(c)]) => assert!(c.closed && c.points.len() == 32),
            other => panic!("unexpected {other:?}"),
        }
    }
}
