//! Curve analysis, primitive construction, and polyline editing kernels.
use super::{R, arg, curve, cv, geometry, numbers, polyline, preview, straight};
use crate::curve_geometry::{
    CurveGeometry, NurbsCurve, Point3, add, cross, dot, norm, scale, sub, unit,
};
use crate::engine::{cancelled, num};
use crate::model::{MAX_ITEMS, MAX_TOTAL_WEIGHT, NodeKind, Value};
use std::f64::consts::{PI, TAU};

fn length_between(g: &CurveGeometry, start: f64, end: f64, cancel: &dyn Fn() -> bool) -> R<f64> {
    if !(0. ..=1.).contains(&start) || !(0. ..=1.).contains(&end) {
        return Err("曲線パラメータは0〜1にしてください。".into());
    }
    if start == end {
        return Ok(0.);
    }
    Ok(g.subcurve(start, end)?.length_table(cancel)?.total())
}

pub(super) fn dash_pattern(
    curves: &[Value],
    pattern: &[Value],
    segments: usize,
    gaps: bool,
    cancel: &dyn Fn() -> bool,
) -> R<Vec<Value>> {
    const MAX_INTERVALS: usize = 512;
    let pattern = numbers(pattern)?;
    if pattern.is_empty() || pattern.len() > MAX_INTERVALS || pattern.iter().any(|&x| x <= 0.) {
        return Err("破線パターンは1〜512個の正の長さで指定してください。".into());
    }
    let mut out = Vec::new();
    let mut weight = 0usize;
    for value in curves {
        cancelled(cancel)?;
        let g = geometry(cv(value)?)?;
        let table = g.length_table(cancel)?;
        let total = table.total();
        let mut position = 0.;
        let mut start_t = 0.;
        for (i, &step) in pattern.iter().cycle().take(MAX_INTERVALS).enumerate() {
            if position == total {
                break;
            }
            cancelled(cancel)?;
            let remaining = total - position;
            let end = if step >= remaining - 16. * f64::EPSILON * total {
                total
            } else {
                position + step
            };
            if end <= position {
                return Err("破線の長さが曲線に対して小さすぎます。".into());
            }
            let end_t = table.parameter(&g, end, cancel)?;
            if (i % 2 == 1) == gaps {
                if out.len() >= MAX_ITEMS {
                    return Err("破線の結果が多すぎます。".into());
                }
                let value = preview(g.subcurve(start_t, end_t)?, segments, cancel)?;
                weight = weight.saturating_add(value.weight());
                if weight > MAX_TOTAL_WEIGHT / 2 {
                    return Err("破線の結果が大きすぎます。".into());
                }
                out.push(value);
            }
            start_t = end_t;
            position = end;
        }
        if position < total {
            return Err("破線と間隔の合計が512区間を超えます。".into());
        }
    }
    Ok(out)
}

/// Extract the active degree-one vertices, including trim points and reversed order.
fn polyline_vertices(g: &CurveGeometry, cancel: &dyn Fn() -> bool) -> R<(Vec<Point3>, bool)> {
    if g.as_nurbs().is_none_or(|n| n.degree() != 1) {
        return Err("次数1のポリラインを入力してください。".into());
    }
    let closed = g.closed()?;
    let mut breaks = g.breaks();
    if closed {
        breaks.pop();
    }
    let points: Vec<_> = breaks
        .into_iter()
        .map(|t| {
            cancelled(cancel)?;
            g.point(t)
        })
        .collect::<R<_>>()?;
    if points.len() < if closed { 3 } else { 2 } {
        return Err("ポリラインの頂点が不足しています。".into());
    }
    Ok((points, closed))
}

fn consecutive_edges(
    points: &[Point3],
    closed: bool,
) -> impl Iterator<Item = (Point3, Point3)> + '_ {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len() - usize::from(!closed))
}

fn edge_center(points: &[Point3], closed: bool) -> R<Point3> {
    let length = |a, b| norm(sub(a, b));
    let longest = consecutive_edges(points, closed)
        .map(|(a, b)| length(a, b))
        .fold(0., f64::max);
    if longest == 0. || !longest.is_finite() {
        return Err("辺の長さが無効なため辺中心を計算できません。".into());
    }
    let (mut center, mut total) = (points[0], 0.);
    for (a, b) in consecutive_edges(points, closed) {
        let weight = length(a, b) / longest;
        if weight > 0. {
            total += weight;
            let fraction = weight / total;
            let middle = add(scale(a, 0.5), scale(b, 0.5));
            center = add(scale(center, 1. - fraction), scale(middle, fraction));
        }
    }
    Ok(center)
}

fn area_center(points: &[Point3], closed: bool) -> R<Point3> {
    if !closed {
        return Err("面積重心は閉じたポリラインに対応しています。".into());
    }
    let origin = points[0];
    let extent = points
        .iter()
        .map(|&p| norm(sub(p, origin)))
        .fold(0., f64::max);
    if extent == 0. || !extent.is_finite() {
        return Err("面積重心を定義できません。".into());
    }
    let local: Vec<_> = points
        .iter()
        .map(|&p| sub(p, origin).map(|x| x / extent))
        .collect();
    let normal = consecutive_edges(&local, true)
        .map(|(a, b)| cross(a, b))
        .fold([0.; 3], add);
    let double_area = norm(normal);
    let normal = unit(normal).map_err(|_| "ポリラインの符号付き面積がゼロです。")?;
    if local.iter().any(|&p| dot(p, normal).abs() > 1e-9) {
        return Err("面積重心は平面ポリラインに対応しています。".into());
    }
    let weighted = consecutive_edges(&local, true)
        .map(|(a, b)| scale(add(a, b), dot(cross(a, b), normal)))
        .fold([0.; 3], add);
    let center = add(
        origin,
        scale(weighted.map(|x| x / (3. * double_area)), extent),
    );
    if center.iter().all(|x| x.is_finite()) {
        Ok(center)
    } else {
        Err("面積重心が有限ではありません。".into())
    }
}

fn smooth(
    mut points: Vec<Point3>,
    closed: bool,
    strength: f64,
    iterations: f64,
    cancel: &dyn Fn() -> bool,
) -> R<Value> {
    if !(0. ..=1.).contains(&strength)
        || !(0. ..=64.).contains(&iterations)
        || iterations.fract() != 0.
    {
        return Err("平滑化の強さは0〜1、反復回数は0〜64の整数にしてください。".into());
    }
    let mut next = points.clone();
    let n = points.len();
    let first = usize::from(!closed);
    for _ in 0..iterations as usize {
        for i in first..n - first {
            cancelled(cancel)?;
            let average = add(
                scale(points[(i + n - 1) % n], 0.5),
                scale(points[(i + 1) % n], 0.5),
            );
            next[i] = add(scale(points[i], 1. - strength), scale(average, strength));
        }
        std::mem::swap(&mut points, &mut next);
    }
    if closed {
        let start = points[0];
        let second = points.iter().find(|&&p| p != start);
        let has_three_vertices =
            second.is_some_and(|second| points.iter().any(|p| *p != start && p != second));
        if !has_three_vertices {
            return Err(
                "平滑化後の閉じたポリラインに異なる頂点が3点以上必要です。強さを下げてください。"
                    .into(),
            );
        }
    }
    polyline(points, closed)
}

fn reduce(
    mut points: Vec<Point3>,
    closed: bool,
    tolerance: f64,
    cancel: &dyn Fn() -> bool,
) -> R<Value> {
    if tolerance < 0. {
        return Err("ポリラインの許容距離は0以上にしてください。".into());
    }
    let mut intervals = if closed {
        let (opposite, _) = points
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, &p)| (i, norm(sub(p, points[0]))))
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .ok_or("閉じたポリラインの頂点が不足しています。")?;
        points.push(points[0]);
        vec![(0, opposite), (opposite, points.len() - 1)]
    } else {
        vec![(0, points.len() - 1)]
    };
    let mut keep = vec![false; points.len()];
    let mut remaining_probes = 2_000_000usize;
    while let Some((start, end)) = intervals.pop() {
        cancelled(cancel)?;
        keep[start] = true;
        keep[end] = true;
        remaining_probes = remaining_probes
            .checked_sub(end.saturating_sub(start + 1))
            .ok_or("頂点削減の計算予算を超えました。曲線を分割してください。")?;
        let ab = sub(points[end], points[start]);
        let length = norm(ab);
        let tangent = if length == 0. { [0.; 3] } else { unit(ab)? };
        let mut farthest = None;
        let mut largest = tolerance;
        for (i, &p) in points.iter().enumerate().take(end).skip(start + 1) {
            cancelled(cancel)?;
            let ap = sub(p, points[start]);
            let r = norm(sub(ap, scale(tangent, dot(ap, tangent).clamp(0., length))));
            if !r.is_finite() {
                return Err("頂点と線分の距離が有限ではありません。".into());
            }
            if r > largest {
                farthest = Some(i);
                largest = r;
            }
        }
        if let Some(i) = farthest {
            intervals.extend([(start, i), (i, end)]);
        }
    }
    let reduced: Vec<_> = points
        .into_iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(p))
        .collect();
    if closed && reduced.len() < 4 {
        return Err("閉じたポリラインを保つために許容距離を小さくしてください。".into());
    }
    polyline(reduced, closed)
}

pub(super) fn analyze_or_edit(
    kind: NodeKind,
    g: &CurveGeometry,
    args: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> R<Vec<Value>> {
    use NodeKind::*;
    let value = match kind {
        CurveLengthDomain => {
            Value::Number(length_between(g, num(args, 1)?, num(args, 2)?, cancel)?)
        }
        CurveLengthParameter => {
            let t = num(args, 1)?;
            return Ok(vec![
                Value::Number(length_between(g, 0., t, cancel)?),
                Value::Number(length_between(g, t, 1., cancel)?),
            ]);
        }
        CurveParameterAtLength | CurveTangentAtLength => {
            let table = g.length_table(cancel)?;
            let amount = num(args, 1)?;
            let length = if num(args, 2)? == 0. {
                amount
            } else {
                if !(0. ..=1.).contains(&amount) {
                    return Err("弧長の割合は0〜1にしてください。".into());
                }
                amount * table.total()
            };
            let t = table.parameter(g, length, cancel)?;
            if kind == CurveParameterAtLength {
                Value::Number(t)
            } else {
                Value::Point(g.tangent(t)?)
            }
        }
        CurvePeriodic => Value::Number(f64::from(g.periodic())),
        CurveControlPolygon => {
            let n = g.as_nurbs().ok_or("NURBS 曲線を入力してください。")?;
            let mut controls = n.control_points();
            if n.periodic() {
                controls.truncate(controls.len() - n.degree());
            }
            let closed = n.periodic() || controls.first() == controls.last();
            controls.dedup();
            if closed && controls.len() > 1 && controls.first() == controls.last() {
                controls.pop();
            }
            let [start, end] = g.domain();
            if start > end {
                let first = usize::from(closed && !controls.is_empty());
                controls[first..].reverse();
            }
            polyline(controls, closed)?
        }
        CurveShortestSegment
        | CurveLongestSegment
        | CurvePolygonCenterVertices
        | CurvePolygonCenterEdges
        | CurvePolygonCenterArea
        | CurveSmoothPolyline
        | CurveReducePolyline => {
            let (points, closed) = polyline_vertices(g, cancel)?;
            match kind {
                CurveShortestSegment | CurveLongestSegment => {
                    let lengths = consecutive_edges(&points, closed).map(|(a, b)| norm(sub(a, b)));
                    Value::Number(if kind == CurveShortestSegment {
                        lengths.fold(f64::INFINITY, f64::min)
                    } else {
                        lengths.fold(0., f64::max)
                    })
                }
                CurvePolygonCenterVertices => {
                    let factor = 1. / points.len() as f64;
                    Value::Point(
                        points
                            .into_iter()
                            .map(|p| scale(p, factor))
                            .fold([0.; 3], add),
                    )
                }
                CurvePolygonCenterEdges => Value::Point(edge_center(&points, closed)?),
                CurvePolygonCenterArea => Value::Point(area_center(&points, closed)?),
                CurveSmoothPolyline => {
                    smooth(points, closed, num(args, 1)?, num(args, 2)?, cancel)?
                }
                CurveReducePolyline => reduce(points, closed, num(args, 1)?, cancel)?,
                _ => unreachable!(),
            }
        }
        _ => unreachable!("curve operation dispatch"),
    };
    Ok(vec![value])
}

fn planar_side(a: Point3, b: Point3, c: Point3) -> R<(Point3, Point3)> {
    let along = unit(sub(b, a))?;
    let towards = unit(sub(c, a))?;
    let normal = cross(along, towards);
    if norm(normal) <= 1e-12 {
        return Err("3 点が重複またはほぼ一直線です。".into());
    }
    Ok((along, cross(unit(normal)?, along)))
}

pub(super) fn primitive(kind: NodeKind, args: &[&Value], cancel: &dyn Fn() -> bool) -> R<Value> {
    use NodeKind::*;
    if kind == CurveLineFour {
        let (a, b) = straight(curve(args, 0)?)?;
        let tangent = unit(sub(b, a))?;
        let project = |p| add(a, scale(tangent, dot(sub(p, a), tangent)));
        return polyline(vec![project(arg(args, 1)?), project(arg(args, 2)?)], false);
    }
    let (a, b, c) = (arg(args, 0)?, arg(args, 1)?, arg(args, 2)?);
    match kind {
        CurveRectangleThree => {
            let (_, side) = planar_side(a, b, c)?;
            let offset = scale(side, dot(sub(c, a), side));
            polyline(vec![a, b, add(b, offset), add(a, offset)], true)
        }
        CurvePolygonEdge => {
            let (along, side) = planar_side(a, b, c)?;
            let count = num(args, 3)?;
            if !(3. ..=512.).contains(&count) || count.fract() != 0. {
                return Err("辺数は3〜512の整数にしてください。".into());
            }
            let half = norm(sub(b, a)) / 2.;
            let apothem = half / (PI / count).tan();
            let center = add(a, add(scale(along, half), scale(side, apothem)));
            let x = sub(a, center);
            let y = sub(scale(along, apothem), scale(side, half));
            let mut vertices = vec![a, b];
            for i in 2..count as usize {
                cancelled(cancel)?;
                let (s, c) = (TAU * i as f64 / count).sin_cos();
                vertices.push(add(center, add(scale(x, c), scale(y, s))));
            }
            polyline(vertices, true)
        }
        CurveIncircle => {
            let (along, side) = planar_side(a, b, c)?;
            let ab = sub(b, a);
            let ac = sub(c, a);
            let opposite = [norm(sub(c, b)), norm(ac), norm(ab)];
            let longest = opposite.into_iter().fold(0., f64::max);
            let [wa, wb, wc] = opposite.map(|v| v / longest);
            let perimeter = wa + wb + wc;
            let center = add(a, add(scale(ab, wb / perimeter), scale(ac, wc / perimeter)));
            let radius = (wc / perimeter) * dot(ac, side);
            preview(
                CurveGeometry::circle(center, cross(along, side), radius)?,
                num(args, 3)? as usize,
                cancel,
            )
        }
        CurveInEllipse => {
            planar_side(a, b, c)?;
            let ab = sub(b, a);
            let ac = sub(c, a);
            let center = add(a, add(scale(ab, 1. / 3.), scale(ac, 1. / 3.)));
            // Apply the affine map from an equilateral unit triangle to ABC
            // to its incircle. These two radial vectors need not be orthogonal.
            let x = scale(sub(ab, ac), 1. / (2. * 3_f64.sqrt()));
            let y = add(scale(ab, 1. / 6.), scale(ac, 1. / 6.));
            preview(
                CurveGeometry::arc(center, x, y, 0., TAU)?,
                num(args, 3)? as usize,
                cancel,
            )
        }
        CurveArcSed => preview(
            arc_start_tangent_end(a, b, c)?,
            num(args, 3)? as usize,
            cancel,
        ),
        _ => unreachable!("curve primitive dispatch"),
    }
}

fn arc_start_tangent_end(a: Point3, b: Point3, direction: Point3) -> R<CurveGeometry> {
    let chord = sub(b, a);
    let chord_direction = unit(chord)?;
    let tangent = unit(direction)?;
    let normal = cross(tangent, chord_direction);
    let sine = norm(normal);
    if sine <= 1e-12 {
        return Err("接線に沿う終点から円弧を作成できません。".into());
    }
    let inward = cross(unit(normal)?, tangent);
    let radius = (0.5 * norm(chord)) / sine;
    let sweep = 2. * sine.atan2(dot(chord_direction, tangent));
    let spans = (sweep / (PI / 2.)).ceil() as usize;
    let step = sweep / spans as f64;
    let handle = radius * (step / 2.).tan();
    let weight = (step / 2.).cos();

    // Form the spans relative to the start point so shallow arcs never
    // recover a small endpoint by subtracting two large radial vectors.
    let mut points = vec![a];
    let mut weights = vec![1.];
    let mut knots = vec![0.; 3];
    let mut start = a;
    for i in 1..=spans {
        let angle = (i - 1) as f64 * step;
        let (s, c) = angle.sin_cos();
        let local_tangent = add(scale(tangent, c), scale(inward, s));
        points.push(add(start, scale(local_tangent, handle)));
        if i == spans {
            start = b;
        } else {
            let angle = i as f64 * step;
            let along = radius * angle.sin();
            let half_sine = (angle / 2.).sin();
            let rise = (radius * half_sine) * (2. * half_sine);
            start = add(a, add(scale(tangent, along), scale(inward, rise)));
            knots.extend([i as f64 / spans as f64; 2]);
        }
        points.push(start);
        weights.extend([weight, 1.]);
    }
    knots.extend([1.; 3]);
    Ok(CurveGeometry::nurbs(NurbsCurve::new(
        2, points, weights, knots,
    )?))
}
