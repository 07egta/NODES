//! Point construction, coordinate conversion and branch-local point lists.
use crate::engine::{cancelled, finite_point, num, sub};
use crate::math_nodes::{Frame, point, stable_sum, xyz};
use crate::model::{DataType as D, InputSpec, NodeKind as K, NodeSpec, Value, g, i, n};
use std::cmp::Ordering;
use std::f64::consts::TAU;

const ORIGIN: InputSpec = g("origin", "平面の原点", D::Point);
const AXIS_X: InputSpec = g("axis_x", "平面の X 方向", D::Point);
const AXIS_Y: InputSpec = g("axis_y", "平面の Y 方向", D::Point);
const AZIMUTH: InputSpec = n("azimuth", "平面内の角度 (rad)", 0.0, -1e12, 1e12, 0.01);
const RADIUS: InputSpec = n("radius", "半径", 1.0, 0.0, 1e10, 0.01);
const POINTS: InputSpec = g("points", "点リスト", D::Point);
const MASK: InputSpec = i("mask", "座標順序 (1=X 2=Y 3=Z)", 123.0, 1.0, 321.0);

macro_rules! spec {
    ($label:literal, $description:literal, $keywords:literal, $output:ident, [$($input:expr),* $(,)?]) => {{
        static SPEC: NodeSpec = NodeSpec {
            label: $label,
            category: "曲線",
            description: $description,
            keywords: $keywords,
            inputs: &[$($input),*],
            output: D::$output,
        };
        Some(&SPEC)
    }};
}

pub(crate) fn spec(kind: K) -> Option<&'static NodeSpec> {
    match kind {
        K::PointPolar => spec!(
            "極座標から点",
            "基準平面の X 方向からの方位角、平面からの仰角、原点からの距離で点を作ります。",
            "point polar spherical coordinates 球面座標 方位角 仰角",
            Point,
            [
                ORIGIN,
                AXIS_X,
                AXIS_Y,
                AZIMUTH,
                n("elevation_angle", "仰角 (rad)", 0.0, -1e12, 1e12, 0.01),
                RADIUS,
            ]
        ),
        K::PointCylindrical => spec!(
            "円柱座標から点",
            "基準平面内の角度と半径、平面の法線に沿った高さで点を作ります。",
            "point cylindrical coordinates 円筒座標 円柱座標",
            Point,
            [
                ORIGIN,
                AXIS_X,
                AXIS_Y,
                AZIMUTH,
                RADIUS,
                n("elevation", "高さ", 0.0, -1e10, 1e10, 0.01),
            ]
        ),
        K::PointToPolar => spec!(
            "点を極座標へ変換",
            "点ごとに方位角 [0, 2π)、仰角 (rad)、原点からの距離をこの順で返します。法線上の方位角と原点の仰角は 0 です。",
            "to polar point spherical coordinates 球面座標 方位角 仰角",
            Number,
            [ORIGIN, AXIS_X, AXIS_Y, g("point", "点", D::Point)]
        ),
        K::PointBarycentric => spec!(
            "重心座標から点",
            "点 A・B・C の重み U・V・W の和が 1 になるように正規化して点を求めます。重みの和が 0 の場合はエラーです。",
            "barycentric point triangle 重心座標 三角形 補間",
            Point,
            [
                g("a", "点 A", D::Point),
                g("b", "点 B", D::Point),
                g("c", "点 C", D::Point),
                n("u", "重み U", 1.0, -1e12, 1e12, 0.01),
                n("v", "重み V", 1.0, -1e12, 1e12, 0.01),
                n("w", "重み W", 1.0, -1e12, 1e12, 0.01),
            ]
        ),
        K::SortPoints => spec!(
            "点を座標順に並べ替え",
            "枝ごとに X、Y、Z の優先順で座標の小さい点から並べます。",
            "sort points xyz lexicographic 点 ソート 並べ替え",
            Point,
            [POINTS]
        ),
        K::CullDuplicatePoints => spec!(
            "重複点を除去",
            "枝の先頭から、すでに残した点からの距離が許容値以下の点を除きます。",
            "cull duplicates duplicate points tolerance 重複点 除去 許容値",
            Point,
            [POINTS, n("tolerance", "許容距離", 0.001, 0.0, 1e10, 0.001)]
        ),
        K::PointsToNumbers => spec!(
            "点リストを数値へ変換",
            "各点から指定した順序で座標を取り出します。123 は XYZ、31 は ZX です。同じ軸の重複はエラーです。",
            "points to numbers coordinate mask 座標 抽出",
            Number,
            [POINTS, MASK]
        ),
        K::NumbersToPoints => spec!(
            "数値リストを点へ変換",
            "指定した順序に数値を割り当てます。123 は XYZ、31 は ZX です。省いた軸は 0、最後の組が不足するとエラーです。",
            "numbers to points coordinate mask 座標 組立",
            Point,
            [g("values", "数値リスト", D::Number), MASK]
        ),
        _ => None,
    }
}

pub(crate) fn is_list_kind(kind: K) -> bool {
    matches!(
        kind,
        K::SortPoints | K::CullDuplicatePoints | K::PointsToNumbers | K::NumbersToPoints
    )
}

pub(crate) fn run_item(kind: K, args: &[&Value]) -> Result<Vec<Value>, String> {
    if kind == K::PointBarycentric {
        let [a, b, c] = [xyz(args, 0)?, xyz(args, 1)?, xyz(args, 2)?];
        let mut weights = [num(args, 3)?, num(args, 4)?, num(args, 5)?];
        let largest = weights.into_iter().map(f64::abs).fold(0.0, f64::max);
        // Keep products representable when every supplied weight is very small.
        if largest > 0.0 && largest < f64::MIN_POSITIVE.sqrt() {
            weights = weights.map(|weight| weight / largest);
        }
        let total = stable_sum(&weights);
        if total == 0.0 {
            return Err("重みの和は 0 にできません。".into());
        }
        return point(std::array::from_fn(|axis| {
            stable_sum(&[
                a[axis] * weights[0],
                b[axis] * weights[1],
                c[axis] * weights[2],
            ]) / total
        }));
    }

    let frame = Frame::new(xyz(args, 0)?, xyz(args, 1)?, xyz(args, 2)?)?;
    match kind {
        K::PointPolar => {
            let (sin_elevation, cos_elevation) = num(args, 4)?.sin_cos();
            let radius = num(args, 5)?;
            let horizontal_radius = radius * cos_elevation;
            let (sin_azimuth, cos_azimuth) = num(args, 3)?.sin_cos();
            point(frame.world([
                horizontal_radius * cos_azimuth,
                horizontal_radius * sin_azimuth,
                radius * sin_elevation,
            ]))
        }
        K::PointCylindrical => {
            let (sin_azimuth, cos_azimuth) = num(args, 3)?.sin_cos();
            let radius = num(args, 4)?;
            point(frame.world([radius * cos_azimuth, radius * sin_azimuth, num(args, 5)?]))
        }
        K::PointToPolar => {
            let [x, y, z] = frame.local(xyz(args, 3)?);
            let horizontal = x.hypot(y);
            let radius = horizontal.hypot(z);
            let azimuth = if horizontal == 0.0 {
                0.0
            } else {
                let angle = y.atan2(x).rem_euclid(TAU);
                if angle == TAU { 0.0 } else { angle }
            };
            let elevation = if radius == 0.0 {
                0.0
            } else {
                z.atan2(horizontal)
            };
            Ok([azimuth, elevation, radius].map(Value::Number).to_vec())
        }
        _ => Err("未対応の点ノードです。".into()),
    }
}

fn coordinate_axes(mask: f64) -> Result<&'static [usize], String> {
    let axes: &[usize] = match mask as u16 {
        1 => &[0],
        2 => &[1],
        3 => &[2],
        12 => &[0, 1],
        13 => &[0, 2],
        21 => &[1, 0],
        23 => &[1, 2],
        31 => &[2, 0],
        32 => &[2, 1],
        123 => &[0, 1, 2],
        132 => &[0, 2, 1],
        213 => &[1, 0, 2],
        231 => &[1, 2, 0],
        312 => &[2, 0, 1],
        321 => &[2, 1, 0],
        _ => return Err("座標順序には 1 (X)、2 (Y)、3 (Z) を重複なしで指定してください。".into()),
    };
    Ok(axes)
}

pub(crate) fn run_list(
    kind: K,
    lists: &[&[Value]],
    remaining_items: usize,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let leading = lists.first().ok_or("入力リストがありません。")?;
    let setting = || {
        lists
            .get(1)
            .and_then(|values| values.first())
            .ok_or("設定値がありません。")?
            .number()
    };
    match kind {
        K::PointsToNumbers => {
            let axes = coordinate_axes(setting()?)?;
            let count = leading.len().saturating_mul(axes.len());
            if count > remaining_items {
                return Err("座標の出力数が上限を超えました。点の個数を減らしてください。".into());
            }
            let mut out = Vec::with_capacity(count);
            for value in *leading {
                cancelled(stop)?;
                let point = xyz(&[value], 0)?;
                out.extend(axes.iter().map(|&axis| Value::Number(point[axis])));
            }
            Ok(out)
        }
        K::NumbersToPoints => {
            let axes = coordinate_axes(setting()?)?;
            let chunks = leading.chunks_exact(axes.len());
            if !chunks.remainder().is_empty() {
                return Err("座標の数を座標順序の軸数の倍数にしてください。".into());
            }
            let mut out = Vec::with_capacity(chunks.len());
            for chunk in chunks {
                cancelled(stop)?;
                let mut point = [0.0; 3];
                for (&axis, value) in axes.iter().zip(chunk) {
                    point[axis] = value.number()?;
                }
                if !finite_point(point) {
                    return Err("座標が許容範囲内ではありません。".into());
                }
                out.push(Value::Point(point));
            }
            Ok(out)
        }
        K::SortPoints | K::CullDuplicatePoints => {
            let mut points = Vec::with_capacity(leading.len());
            for value in *leading {
                cancelled(stop)?;
                points.push(xyz(&[value], 0)?);
            }
            if kind == K::SortPoints {
                points.sort_by(|a, b| {
                    compare(a[0], b[0])
                        .then_with(|| compare(a[1], b[1]))
                        .then_with(|| compare(a[2], b[2]))
                });
            } else {
                points = cull_duplicates(&points, setting()?, stop)?;
            }
            Ok(points.into_iter().map(Value::Point).collect())
        }
        _ => Err("未対応の点リストノードです。".into()),
    }
}

fn compare(a: f64, b: f64) -> Ordering {
    // Input validation guarantees finiteness. Numeric equality includes both signed zeroes.
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

fn cull_duplicates(
    points: &[[f64; 3]],
    tolerance: f64,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<[f64; 3]>, String> {
    if points.is_empty() {
        return Ok(Vec::new());
    }
    // Search by the widest axis, but assign representatives in original input order.
    let mut min = points[0];
    let mut max = points[0];
    for point in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let extents = sub(max, min);
    let axis = (0..3)
        .max_by(|&a, &b| compare(extents[a], extents[b]))
        .unwrap();
    let mut axis_order: Vec<usize> = (0..points.len()).collect();
    axis_order.sort_unstable_by(|&a, &b| compare(points[a][axis], points[b][axis]));

    let mut removed = vec![false; points.len()];
    let mut kept = Vec::new();
    let mut comparisons = 0usize;
    for (index, &point) in points.iter().enumerate() {
        cancelled(stop)?;
        if removed[index] {
            continue;
        }
        kept.push(point);
        let start = axis_order.partition_point(|&j| points[j][axis] - point[axis] < -tolerance);
        for &candidate in &axis_order[start..] {
            if points[candidate][axis] - point[axis] > tolerance {
                break;
            }
            comparisons += 1;
            if comparisons > 1_000_000 {
                return Err("重複点の比較回数が上限を超えました。点を枝に分けてください。".into());
            }
            if comparisons.is_multiple_of(256) {
                cancelled(stop)?;
            }
            if candidate > index && !removed[candidate] {
                let [dx, dy, dz] = sub(points[candidate], point);
                if dx.hypot(dy).hypot(dz) <= tolerance {
                    removed[candidate] = true;
                }
            }
        }
    }
    Ok(kept)
}
