//! Standard Grasshopper-style components that were still missing: roots and powers,
//! more logic gates, polar points, polyline area and explode, and two transforms.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! Every node works on one item at a time and uses only the existing value types.
use crate::engine::{as_curve, cancelled, cross, dot, map_affine_shape, num, shape, sub};
use crate::model::{Curve, DataType, NodeKind, NodeSpec, Value, g, i, n};
use std::sync::Arc;

const BIG: f64 = 1e12;
const EPS: f64 = 1e-12;

pub fn is_kind(kind: NodeKind) -> bool {
    use NodeKind::*;
    matches!(
        kind,
        MathSquareRoot
            | MathSquare
            | MathCube
            | MathCubeRoot
            | MathLn
            | MathLog10
            | GateNand
            | GateNor
            | GateXnor
            | DomainIncludes
            | PointPolar
            | PointCylindrical
            | CurveArea
            | CurveAreaCentroid
            | CurveExplode
            | RotateDirection
            | ProjectToZ
    )
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

pub fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    use NodeKind::*;
    match kind {
        MathSquareRoot => node_spec!(
            "平方根",
            "数値",
            "値の平方根。負の数は実数にならないのでエラーにします。",
            "square root sqrt 平方根 ルート",
            DataType::Number,
            [n("x", "値", 4., 0., BIG, 0.01)]
        ),
        MathSquare => node_spec!(
            "二乗",
            "数値",
            "値を 2 回かけます。",
            "square power 二乗 平方",
            DataType::Number,
            [n("x", "値", 2., -BIG, BIG, 0.01)]
        ),
        MathCube => node_spec!(
            "三乗",
            "数値",
            "値を 3 回かけます。",
            "cube power 三乗 立方",
            DataType::Number,
            [n("x", "値", 2., -BIG, BIG, 0.01)]
        ),
        MathCubeRoot => node_spec!(
            "立方根",
            "数値",
            "値の立方根。負の数も扱えます。",
            "cube root cbrt 立方根",
            DataType::Number,
            [n("x", "値", 8., -BIG, BIG, 0.01)]
        ),
        MathLn => node_spec!(
            "自然対数",
            "数値",
            "底が e の対数。値は 0 より大きい数にします。",
            "ln natural log 自然対数",
            DataType::Number,
            [n("x", "値", 1., 1e-9, BIG, 0.01)]
        ),
        MathLog10 => node_spec!(
            "常用対数",
            "数値",
            "底が 10 の対数。値は 0 より大きい数にします。",
            "log10 common log 常用対数",
            DataType::Number,
            [n("x", "値", 10., 1e-9, BIG, 0.01)]
        ),
        GateNand => node_spec!(
            "論理 NAND",
            "数値",
            "A と B がどちらも 1 のときだけ 0、それ以外は 1。",
            "nand gate logic 論理 否定論理積",
            DataType::Number,
            [i("a", "A (0/1)", 1., 0., 1.), i("b", "B (0/1)", 1., 0., 1.)]
        ),
        GateNor => node_spec!(
            "論理 NOR",
            "数値",
            "A と B がどちらも 0 のときだけ 1、それ以外は 0。",
            "nor gate logic 論理 否定論理和",
            DataType::Number,
            [i("a", "A (0/1)", 0., 0., 1.), i("b", "B (0/1)", 0., 0., 1.)]
        ),
        GateXnor => node_spec!(
            "論理 XNOR",
            "数値",
            "A と B が同じなら 1、違えば 0。",
            "xnor gate logic equal 論理 一致",
            DataType::Number,
            [i("a", "A (0/1)", 1., 0., 1.), i("b", "B (0/1)", 1., 0., 1.)]
        ),
        DomainIncludes => node_spec!(
            "範囲に含まれるか",
            "数値",
            "値が最小と最大の間（両端を含む）なら 1、外なら 0。最小と最大が逆でも使えます。",
            "includes domain inside range 範囲 含む 内側",
            DataType::Number,
            [
                n("x", "値", 0.5, -BIG, BIG, 0.01),
                n("min", "最小", 0., -BIG, BIG, 0.01),
                n("max", "最大", 1., -BIG, BIG, 0.01)
            ]
        ),
        PointPolar => node_spec!(
            "極座標の点",
            "数値",
            "原点を中心に、水平角（X 軸から反時計回り）・仰角・半径から点を作ります。ほかの場所へは「移動」で動かします。",
            "point polar spherical azimuth elevation 極座標 球座標 角度",
            DataType::Point,
            [
                n("angle", "水平角 (度)", 0., -3600., 3600., 1.),
                n("elevation", "仰角 (度)", 0., -90., 90., 1.),
                n("radius", "半径", 10., 0., BIG, 0.1)
            ]
        ),
        PointCylindrical => node_spec!(
            "円柱座標の点",
            "数値",
            "原点を中心に、水平角・半径・高さ Z から点を作ります。",
            "point cylindrical polar angle radius height 円柱座標 角度",
            DataType::Point,
            [
                n("angle", "水平角 (度)", 0., -3600., 3600., 1.),
                n("radius", "半径", 10., 0., BIG, 0.1),
                n("height", "高さ Z", 0., -BIG, BIG, 0.1)
            ]
        ),
        CurveArea => node_spec!(
            "曲線の面積",
            "曲線",
            "閉じた平面の曲線で囲まれた面積。曲線の表示用の点列（折れ線）から求めるので、なめらかな曲線では近似です。",
            "curve area polygon area 面積 閉曲線",
            DataType::Number,
            [g("curve", "曲線", DataType::Curve)]
        ),
        CurveAreaCentroid => node_spec!(
            "曲線の面積重心",
            "曲線",
            "閉じた平面の曲線で囲まれた面の重心。折れ線として計算します。",
            "area centroid center of mass 重心 面積重心 閉曲線",
            DataType::Point,
            [g("curve", "曲線", DataType::Curve)]
        ),
        CurveExplode => node_spec!(
            "曲線を線分に分解",
            "曲線",
            "折れ線を 1 本ずつの線分に分けます。閉じた曲線は最後の辺も出します。なめらかな曲線は表示用の点列で分けます。",
            "explode segments polyline break 分解 線分 折れ線",
            DataType::Curve,
            [g("curve", "曲線", DataType::Curve)]
        ),
        RotateDirection => node_spec!(
            "向きを合わせて回転",
            "変形",
            "原点を中心に、元の向きが先の向きに重なるように回転します。向きは XYZ の成分で指定し、長さは使いません。",
            "rotate direction align vector orient 向き 回転 合わせる",
            DataType::Geometry,
            [
                g("geometry", "形状", DataType::Geometry),
                n("fx", "元の向き X", 0., -BIG, BIG, 0.1),
                n("fy", "元の向き Y", 0., -BIG, BIG, 0.1),
                n("fz", "元の向き Z", 1., -BIG, BIG, 0.1),
                n("tx", "先の向き X", 1., -BIG, BIG, 0.1),
                n("ty", "先の向き Y", 0., -BIG, BIG, 0.1),
                n("tz", "先の向き Z", 0., -BIG, BIG, 0.1)
            ]
        ),
        ProjectToZ => node_spec!(
            "水平面へ投影",
            "変形",
            "すべての点を、指定した高さ Z の水平面へ真上・真下から押しつぶします。",
            "project plane flatten xy z 投影 平面 平らにする",
            DataType::Geometry,
            [
                g("geometry", "形状", DataType::Geometry),
                n("z", "高さ Z", 0., -BIG, BIG, 0.1)
            ]
        ),
        _ => None,
    }
}

fn number(v: f64) -> Result<Vec<Value>, String> {
    if v.is_finite() {
        Ok(vec![Value::Number(v)])
    } else {
        Err("計算結果が大きすぎます。".into())
    }
}

fn point(p: [f64; 3]) -> Result<Vec<Value>, String> {
    if p.iter().all(|v| v.is_finite()) {
        Ok(vec![Value::Point(p)])
    } else {
        Err("計算結果が大きすぎます。".into())
    }
}

fn flag(v: bool) -> Result<Vec<Value>, String> {
    number(if v { 1. } else { 0. })
}

fn bit(v: f64) -> Result<bool, String> {
    if v == 0. {
        Ok(false)
    } else if v == 1. {
        Ok(true)
    } else {
        Err("論理値は 0 または 1 です。".into())
    }
}

fn curve_of<'a>(a: &[&'a Value]) -> Result<&'a Arc<Curve>, String> {
    match a.first().copied() {
        Some(Value::Curve(c)) => Ok(c),
        _ => Err("曲線を接続してください。".into()),
    }
}

fn closed_points(c: &Curve) -> Result<&[[f64; 3]], String> {
    if !c.closed || c.points.len() < 3 {
        return Err("閉じた曲線（3 点以上）を接続してください。".into());
    }
    Ok(&c.points)
}

/// Half the sum of cross products: the vector area of a closed polygon.
fn area_vector(points: &[[f64; 3]]) -> [f64; 3] {
    let origin = points[0];
    let mut sum = [0.; 3];
    for (index, p) in points.iter().enumerate() {
        let q = points[(index + 1) % points.len()];
        // Local coordinates avoid cancellation of large world-space products.
        let c = cross(sub(*p, origin), sub(q, origin));
        sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
    }
    sum.map(|v| v * 0.5)
}

fn length(v: [f64; 3]) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}

/// Rotation (axis, cos, sin) that carries direction `from` onto direction `to`.
fn alignment(from: [f64; 3], to: [f64; 3]) -> Result<([f64; 3], f64, f64), String> {
    let (lf, lt) = (length(from), length(to));
    if lf < EPS || lt < EPS {
        return Err("向きの長さが 0 です。XYZ のどれかを 0 以外にしてください。".into());
    }
    let f = from.map(|v| v / lf);
    let t = to.map(|v| v / lt);
    let axis = cross(f, t);
    let sin = length(axis);
    let cos = dot(f, t);
    if sin > EPS {
        return Ok((axis.map(|v| v / sin), cos, sin));
    }
    if cos > 0. {
        return Ok(([0., 0., 1.], 1., 0.));
    }
    // Opposite directions: turn half a circle about any axis perpendicular to `f`.
    let helper = if f[0].abs() < 0.9 {
        [1., 0., 0.]
    } else {
        [0., 1., 0.]
    };
    let perpendicular = cross(f, helper);
    let l = length(perpendicular);
    Ok((perpendicular.map(|v| v / l), -1., 0.))
}

pub fn run_item(
    kind: NodeKind,
    a: &[&Value],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    cancelled(stop)?;
    let x = || num(a, 0);
    let y = || num(a, 1);
    match kind {
        MathSquareRoot => {
            let v = x()?;
            if v < 0. {
                return Err("負の数の平方根は実数ではありません。".into());
            }
            number(v.sqrt())
        }
        MathSquare => number(x()? * x()?),
        MathCube => {
            let v = x()?;
            number(v * v * v)
        }
        MathCubeRoot => number(x()?.cbrt()),
        MathLn => {
            let v = x()?;
            if v <= 0. {
                return Err("対数の値は 0 より大きい数にしてください。".into());
            }
            number(v.ln())
        }
        MathLog10 => {
            let v = x()?;
            if v <= 0. {
                return Err("対数の値は 0 より大きい数にしてください。".into());
            }
            number(v.log10())
        }
        GateNand => {
            let (a, b) = (bit(x()?)?, bit(y()?)?);
            flag(!(a && b))
        }
        GateNor => {
            let (a, b) = (bit(x()?)?, bit(y()?)?);
            flag(!(a || b))
        }
        GateXnor => flag(bit(x()?)? == bit(y()?)?),
        DomainIncludes => {
            let (v, p, q) = (x()?, y()?, num(a, 2)?);
            flag((p.min(q)..=p.max(q)).contains(&v))
        }
        PointPolar => {
            let (angle, elevation, radius) = (x()?.to_radians(), y()?.to_radians(), num(a, 2)?);
            point([
                radius * elevation.cos() * angle.cos(),
                radius * elevation.cos() * angle.sin(),
                radius * elevation.sin(),
            ])
        }
        PointCylindrical => {
            let (angle, radius, height) = (x()?.to_radians(), y()?, num(a, 2)?);
            point([radius * angle.cos(), radius * angle.sin(), height])
        }
        CurveArea => {
            let c = curve_of(a)?;
            let points = closed_points(c)?;
            number(length(area_vector(points)))
        }
        CurveAreaCentroid => {
            let c = curve_of(a)?;
            let points = closed_points(c)?;
            let normal = area_vector(points);
            let l = length(normal);
            if l < EPS {
                return Err("面積が 0 です。平面上の閉じた曲線を接続してください。".into());
            }
            let unit = normal.map(|v| v / l);
            let origin = points[0];
            let mut total = 0.;
            let mut sum = [0.; 3];
            for pair in points.windows(2) {
                cancelled(stop)?;
                let t = dot(cross(sub(pair[0], origin), sub(pair[1], origin)), unit) * 0.5;
                total += t;
                for (k, slot) in sum.iter_mut().enumerate() {
                    *slot += ((pair[0][k] - origin[k]) + (pair[1][k] - origin[k])) / 3. * t;
                }
            }
            if total.abs() < EPS {
                return Err("面積が 0 です。平面上の閉じた曲線を接続してください。".into());
            }
            point(std::array::from_fn(|k| origin[k] + sum[k] / total))
        }
        CurveExplode => {
            let c = curve_of(a)?;
            let points = &c.points;
            let mut out = Vec::new();
            let mut push = |p: [f64; 3], q: [f64; 3]| -> Result<(), String> {
                if length(sub(q, p)) > EPS {
                    out.extend(as_curve(vec![p, q], false)?);
                }
                Ok(())
            };
            for pair in points.windows(2) {
                cancelled(stop)?;
                push(pair[0], pair[1])?;
            }
            if c.closed
                && let (Some(last), Some(first)) = (points.last(), points.first())
            {
                push(*last, *first)?;
            }
            Ok(out)
        }
        RotateDirection => {
            let from = [num(a, 1)?, num(a, 2)?, num(a, 3)?];
            let to = [num(a, 4)?, num(a, 5)?, num(a, 6)?];
            let (k, cos, sin) = alignment(from, to)?;
            let rotate = move |p: [f64; 3]| {
                let kp = cross(k, p);
                let along = dot(k, p) * (1. - cos);
                [
                    p[0] * cos + kp[0] * sin + k[0] * along,
                    p[1] * cos + kp[1] * sin + k[1] * along,
                    p[2] * cos + kp[2] * sin + k[2] * along,
                ]
            };
            Ok(vec![map_affine_shape(shape(a)?, rotate, stop)?])
        }
        ProjectToZ => {
            let z = y()?;
            Ok(vec![map_affine_shape(
                shape(a)?,
                move |p| [p[0], p[1], z],
                stop,
            )?])
        }
        _ => Err("このノードは未対応です。".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(kind: NodeKind, values: &[Value]) -> Result<Vec<Value>, String> {
        let refs: Vec<&Value> = values.iter().collect();
        run_item(kind, &refs, &|| false)
    }
    fn nums(values: &[f64]) -> Vec<Value> {
        values.iter().map(|v| Value::Number(*v)).collect()
    }
    fn one(result: Result<Vec<Value>, String>) -> f64 {
        match result.as_deref() {
            Ok([Value::Number(v)]) => *v,
            _ => f64::NAN,
        }
    }
    fn square() -> Value {
        Value::Curve(Arc::new(Curve {
            points: vec![[0., 0., 0.], [4., 0., 0.], [4., 2., 0.], [0., 2., 0.]],
            closed: true,
            geometry: None,
        }))
    }

    #[test]
    fn every_kind_has_a_spec_and_nothing_else_does() {
        for kind in NodeKind::all() {
            assert_eq!(is_kind(*kind), spec(*kind).is_some(), "{kind:?}");
        }
    }

    #[test]
    fn powers_roots_and_logs() {
        assert_eq!(one(run(NodeKind::MathSquareRoot, &nums(&[9.]))), 3.);
        assert!(run(NodeKind::MathSquareRoot, &nums(&[-1.])).is_err());
        assert_eq!(one(run(NodeKind::MathSquare, &nums(&[-3.]))), 9.);
        assert_eq!(one(run(NodeKind::MathCube, &nums(&[-2.]))), -8.);
        assert!((one(run(NodeKind::MathCubeRoot, &nums(&[-27.]))) + 3.).abs() < 1e-12);
        assert!((one(run(NodeKind::MathLn, &nums(&[std::f64::consts::E]))) - 1.).abs() < 1e-12);
        assert!((one(run(NodeKind::MathLog10, &nums(&[1000.]))) - 3.).abs() < 1e-12);
        assert!(run(NodeKind::MathLn, &nums(&[0.])).is_err());
    }

    #[test]
    fn gates_and_range_test() {
        assert_eq!(one(run(NodeKind::GateNand, &nums(&[1., 1.]))), 0.);
        assert_eq!(one(run(NodeKind::GateNand, &nums(&[1., 0.]))), 1.);
        assert_eq!(one(run(NodeKind::GateNor, &nums(&[0., 0.]))), 1.);
        assert_eq!(one(run(NodeKind::GateNor, &nums(&[0., 1.]))), 0.);
        assert_eq!(one(run(NodeKind::GateXnor, &nums(&[0., 0.]))), 1.);
        assert_eq!(one(run(NodeKind::GateXnor, &nums(&[1., 0.]))), 0.);
        assert!(run(NodeKind::GateNand, &nums(&[2., 0.])).is_err());
        assert_eq!(one(run(NodeKind::DomainIncludes, &nums(&[1., 0., 1.]))), 1.);
        assert_eq!(one(run(NodeKind::DomainIncludes, &nums(&[1., 2., 0.]))), 1.);
        assert_eq!(one(run(NodeKind::DomainIncludes, &nums(&[3., 0., 1.]))), 0.);
    }

    #[test]
    fn polar_and_cylindrical_points() {
        let near = |got: Result<Vec<Value>, String>, want: [f64; 3]| match got.as_deref() {
            Ok([Value::Point(p)]) => p.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-9),
            _ => false,
        };
        assert!(near(
            run(NodeKind::PointPolar, &nums(&[90., 0., 2.])),
            [0., 2., 0.]
        ));
        assert!(near(
            run(NodeKind::PointPolar, &nums(&[0., 90., 2.])),
            [0., 0., 2.]
        ));
        assert!(near(
            run(NodeKind::PointCylindrical, &nums(&[180., 3., 5.])),
            [-3., 0., 5.]
        ));
    }

    #[test]
    fn polygon_area_centroid_and_explode() {
        let c = [square()];
        assert!((one(run(NodeKind::CurveArea, &c)) - 8.).abs() < 1e-12);
        match run(NodeKind::CurveAreaCentroid, &c).as_deref() {
            Ok([Value::Point(p)]) => {
                assert!((p[0] - 2.).abs() < 1e-9 && (p[1] - 1.).abs() < 1e-9 && p[2].abs() < 1e-9);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(run(NodeKind::CurveExplode, &c).map(|v| v.len()), Ok(4));
        let open = Value::Curve(Arc::new(Curve {
            points: vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.]],
            closed: false,
            geometry: None,
        }));
        assert!(run(NodeKind::CurveArea, std::slice::from_ref(&open)).is_err());
        assert_eq!(
            run(NodeKind::CurveExplode, std::slice::from_ref(&open)).map(|v| v.len()),
            Ok(2)
        );
    }

    #[test]
    fn alignment_carries_one_direction_onto_the_other() {
        for (from, to) in [
            ([0., 0., 1.], [1., 0., 0.]),
            ([1., 2., 3.], [-3., 0.5, 2.]),
            ([0., 0., 1.], [0., 0., -1.]),
            ([1., 0., 0.], [5., 0., 0.]),
        ] {
            let (k, cos, sin) = alignment(from, to).unwrap_or(([0.; 3], 1., 0.));
            let kp = cross(k, from);
            let along = dot(k, from) * (1. - cos);
            let r = [
                from[0] * cos + kp[0] * sin + k[0] * along,
                from[1] * cos + kp[1] * sin + k[1] * along,
                from[2] * cos + kp[2] * sin + k[2] * along,
            ];
            let (lr, lt) = (length(r), length(to));
            let ok = (0..3).all(|j| (r[j] / lr - to[j] / lt).abs() < 1e-9);
            assert!(ok, "{from:?} -> {to:?} gave {r:?}");
        }
        assert!(alignment([0.; 3], [1., 0., 0.]).is_err());
    }

    #[test]
    fn projection_flattens_a_point_cloud_curve() {
        let lifted = Value::Curve(Arc::new(Curve {
            points: vec![[0., 0., 3.], [1., 1., 5.]],
            closed: false,
            geometry: None,
        }));
        let result = run(NodeKind::ProjectToZ, &[lifted, Value::Number(1.)]);
        match result.as_deref() {
            Ok([Value::Curve(c)]) => assert!(c.points.iter().all(|p| p[2] == 1.)),
            other => panic!("unexpected {other:?}"),
        }
    }
}
