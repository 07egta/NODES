//! More exact operations on the vendored SolidCraft kernel, adapted for NODES (see
//! `kernel_bridge` for the conventions: tolerance policy, limits, cancel, Japanese errors):
//! exact surface/surface intersection curves, planar trimming, curved shells, variable-radius
//! fillets, offset surfaces, draft, mirror / arrays, pipes and planar patches, plus NODES-native
//! curve tools (blend curves, curve matching, continuity and curvature analysis).
use crate::curve_geometry::{CurveGeometry, NurbsCurve};
use crate::kernel_bridge::{
    self as kb, BrepValue, R, Stop, brep, kernel, p3, positive, stopped, tol, v3,
};
use crate::model::Curve;
use solidcraft_geom::{Plane, Vec3};
use solidcraft_kernel::{self as sk, Body};

type P3 = [f64; 3];

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> Option<P3> {
    let l = norm(a);
    (l > 1e-300 && l.is_finite()).then(|| scale(a, 1. / l))
}

/// Most points kept on one exact intersection curve before it is fitted.
const MAX_FIT_POINTS: usize = 120;

// ---------------------------------------------------------------------------------------------
// Exact surface/surface intersection
// ---------------------------------------------------------------------------------------------

/// The curves where two closed solids' surfaces cross, from the kernel's boolean: the edges of
/// the intersection whose history says two faces meet there (truck's intersection curves, held
/// on both surfaces by Newton projection). Each is sampled finely and fitted with a cubic NURBS
/// through the samples. An empty list: the solids do not cross.
pub fn exact_intersection(a: &BrepValue, b: &BrepValue, stop: Stop) -> R<Vec<Curve>> {
    let Some(both) = kernel("面どうしの交線", stop, || {
        sk::boolean(a.body(), b.body(), sk::BoolOp::Intersect)
    })?
    else {
        return Ok(Vec::new());
    };
    meet_curves(&both, stop)
}

fn meet_curves(body: &Body, stop: Stop) -> R<Vec<Curve>> {
    let history = body
        .history()
        .ok_or("交線の由来が記録されていません。")?
        .clone();
    let fine = (tol::chord(body.size()) * 0.05).max(1e-4);
    let edges = kernel("交線の取得", stop, || body.edges(fine))?;
    if edges.len() != history.edges.len() {
        return Err("交線の記録と稜線の数が一致しません。".into());
    }
    let mut curves = Vec::new();
    for (edge, prov) in edges.iter().zip(&history.edges) {
        stopped(stop)?;
        let meets = prov
            .origins
            .iter()
            .any(|o| matches!(o, sk::history::EdgeOrigin::Meet { .. }));
        if !meets {
            continue;
        }
        // Even spacing along the curve keeps the interpolation from wiggling where the
        // tessellation left long chords.
        let raw: Vec<P3> = edge.points.iter().map(|p| p3(*p)).collect();
        let mut pts = resample(&raw, MAX_FIT_POINTS);
        // Analytic faces on both sides (planes, cylinders, cones, spheres, tori): pull every
        // sample onto both surfaces by alternating projection.
        if let [fa, fb, ..] = prov.faces.as_slice()
            && let (Some(sa), Some(sb)) = (sk::face_surface(body, *fa), sk::face_surface(body, *fb))
        {
            for p in &mut pts {
                *p = onto_both(&sa, &sb, *p);
            }
        }
        fit_accurate(&pts, FIT_TOLERANCE, 4, &mut curves, stop)?;
    }
    Ok(curves)
}

/// A point on both surfaces near `p`: alternating projections, kept only if they converge.
fn onto_both(a: &sk::FaceSurface, b: &sk::FaceSurface, p: P3) -> P3 {
    let mut x = v3(p);
    for _ in 0..400 {
        let Some(pa) = a.nearest(x) else { return p };
        let Some(pb) = b.nearest(pa) else { return p };
        let step = pb.dist(x);
        x = pb;
        if step < 1e-13 {
            break;
        }
    }
    match (a.nearest(x), b.nearest(x)) {
        (Some(qa), Some(qb)) if qa.dist(x) < 1e-7 && qb.dist(x) < 1e-7 && x.dist(v3(p)) < 0.5 => {
            p3(x)
        }
        _ => p,
    }
}

/// `count` points evenly spaced along a polyline (by length), ends kept.
fn resample(pts: &[P3], count: usize) -> Vec<P3> {
    if pts.len() < 2 || count < 2 {
        return pts.to_vec();
    }
    let mut acc = vec![0.];
    for w in pts.windows(2) {
        if let (Some(a), Some(b)) = (w.first(), w.get(1)) {
            let last = acc.last().copied().unwrap_or(0.);
            acc.push(last + norm(sub(*b, *a)));
        }
    }
    let total = acc.last().copied().unwrap_or(0.);
    if total <= 0. {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(count);
    let mut j = 0usize;
    for i in 0..count {
        let s = total * i as f64 / (count - 1) as f64;
        while j + 2 < acc.len() && acc.get(j + 1).is_some_and(|&x| x < s) {
            j += 1;
        }
        let (a0, a1) = (
            acc.get(j).copied().unwrap_or(0.),
            acc.get(j + 1).copied().unwrap_or(total),
        );
        let t = if a1 > a0 {
            ((s - a0) / (a1 - a0)).clamp(0., 1.)
        } else {
            0.
        };
        let (Some(&p), Some(&q)) = (pts.get(j), pts.get(j + 1)) else {
            continue;
        };
        out.push(add(p, scale(sub(q, p), t)));
    }
    out
}

/// How far a fitted intersection curve may stray from its samples (mm).
pub const FIT_TOLERANCE: f64 = 1e-4;

/// Fit the samples with cubic NURBS pieces that stay within `tolerance` of them: a piece that
/// strays is split in half (up to `depth` times), and the last resort is the samples' polyline.
fn fit_accurate(
    pts: &[P3],
    tolerance: f64,
    depth: usize,
    out: &mut Vec<Curve>,
    stop: Stop,
) -> R<()> {
    stopped(stop)?;
    let fitted = fit_points(pts, stop)?;
    if deviation(&fitted, pts, stop)? <= tolerance {
        out.push(fitted);
        return Ok(());
    }
    if depth == 0 || pts.len() < 8 {
        out.push(kb::curve_value(
            CurveGeometry::nurbs(NurbsCurve::polyline(pts.to_vec(), false)?),
            stop,
        )?);
        return Ok(());
    }
    let mid = pts.len() / 2;
    let (Some(first), Some(second)) = (pts.get(..=mid), pts.get(mid..)) else {
        return Ok(());
    };
    fit_accurate(first, tolerance, depth - 1, out, stop)?;
    fit_accurate(second, tolerance, depth - 1, out, stop)
}

/// Largest distance from the points to the curve (to its fine polyline).
fn deviation(c: &Curve, points: &[P3], stop: Stop) -> R<f64> {
    let dense = kb::sample_curve(c, 24, stop)?;
    let mut worst = 0f64;
    for p in points {
        let mut best = f64::INFINITY;
        for w in dense.windows(2) {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else {
                continue;
            };
            let ab = sub(*b, *a);
            let l2 = dot(ab, ab);
            let t = if l2 > 0. {
                (dot(sub(*p, *a), ab) / l2).clamp(0., 1.)
            } else {
                0.
            };
            best = best.min(norm(sub(*p, add(*a, scale(ab, t)))));
        }
        worst = worst.max(best);
    }
    Ok(worst)
}

/// A curve through points: a cubic NURBS interpolation through at most [`MAX_FIT_POINTS`] of
/// them (evenly picked), a polyline for fewer than four.
pub fn fit_points(points: &[P3], stop: Stop) -> R<Curve> {
    let mut pts: Vec<P3> = Vec::with_capacity(points.len());
    for p in points {
        if pts.last().is_none_or(|q| norm(sub(*p, *q)) > tol::ABS) {
            pts.push(*p);
        }
    }
    let closed = pts.len() > 3
        && pts
            .first()
            .zip(pts.last())
            .is_some_and(|(a, b)| norm(sub(*a, *b)) <= tol::ABS * 10.);
    if pts.len() > MAX_FIT_POINTS {
        let n = pts.len() - 1;
        pts = (0..MAX_FIT_POINTS)
            .map(|i| {
                let k = (i * n) / (MAX_FIT_POINTS - 1);
                pts.get(k).copied().unwrap_or([0.; 3])
            })
            .collect();
    }
    let geometry = if pts.len() >= 4 {
        let nurbs = if closed {
            // Closed: drop the repeated end and use a periodic cubic through the points'
            // control polygon (smooth across the seam).
            let mut ring = pts.clone();
            ring.pop();
            NurbsCurve::interpolate(&pts, 3, stop)
                .or_else(|_| NurbsCurve::uniform(ring, 3, true))?
        } else {
            NurbsCurve::interpolate(&pts, 3, stop)?
        };
        CurveGeometry::nurbs(nurbs)
    } else {
        CurveGeometry::nurbs(NurbsCurve::polyline(pts.clone(), false)?)
    };
    kb::curve_value(geometry, stop)
}

// ---------------------------------------------------------------------------------------------
// Trim, shell, fillet, offset, draft, transforms
// ---------------------------------------------------------------------------------------------

/// Cut a solid with the horizontal plane `z = height` and keep the part below (`keep_above` false)
/// or above. The cut face is exact (the plane).
pub fn trim_by_plane(b: &BrepValue, height: f64, keep_above: bool, stop: Stop) -> R<BrepValue> {
    if !height.is_finite() || height.abs() > kb::limits::MAX_COORDINATE {
        return Err("切る高さが無効です。".into());
    }
    let plane =
        Plane::from_normal(Vec3::new(0., 0., height), Vec3::Z).ok_or("平面を作れません。")?;
    let parts = kernel("平面で切る", stop, || {
        sk::split_by_plane(b.body(), &plane)
    })?;
    let mut best: Option<Body> = None;
    for part in parts {
        let m = kernel("体積", stop, || sk::measure(&part))?;
        let above = m.centroid.z > height;
        if above == keep_above {
            best = Some(match best {
                None => part,
                Some(acc) => kernel("平面で切る", stop, || {
                    sk::boolean(&acc, &part, sk::BoolOp::Union)
                })?
                .unwrap_or(acc),
            });
        }
    }
    match best {
        Some(body) => brep(body, stop),
        None => Err(format!(
            "平面 z = {height} の{}に残る部分がありません。高さを変えてください。",
            if keep_above { "上" } else { "下" }
        )),
    }
}

/// Hollow a solid with curved faces: the face to open is taken away and the sheet left is
/// thickened inward (`thicken`, exact for planes and cylinders, fitted within 0.01 mm otherwise).
pub fn shell_curved(b: &BrepValue, thickness: f64, open: usize, stop: Stop) -> R<BrepValue> {
    let t = positive("厚み", thickness)?;
    if open >= b.face_count() {
        return Err(format!(
            "面番号 {open} はありません（0〜{}）。",
            b.face_count().saturating_sub(1)
        ));
    }
    let sheet = kernel("シェル（面を外す）", stop, || {
        sk::remove_faces(b.body(), &[open])
    })?;
    let solid = kernel("シェル（厚みづけ）", stop, || {
        sk::thicken(&sheet, -t, false)
    })?;
    brep(solid, stop)
}

/// The number of the highest face (the usual one to open for a cup).
pub fn top_face(b: &BrepValue, stop: Stop) -> R<usize> {
    let faces = kernel("面の取得", stop, || {
        b.body().faces(tol::chord(b.body().size()))
    })?;
    faces
        .iter()
        .enumerate()
        .max_by(|x, y| x.1.centroid.z.total_cmp(&y.1.centroid.z))
        .map(|(i, _)| i)
        .ok_or_else(|| "面がありません。".to_string())
}

/// Fillet one straight edge with radius `r0` at its start and `r1` at its end.
pub fn fillet_variable(b: &BrepValue, edge: usize, r0: f64, r1: f64, stop: Stop) -> R<BrepValue> {
    let (r0, r1) = (positive("始点の半径", r0)?, positive("終点の半径", r1)?);
    let edges = kernel("稜線の取得", stop, || {
        b.body().edges(tol::chord(b.body().size()))
    })?;
    let e = edges.get(edge).ok_or_else(|| {
        format!(
            "稜線番号 {edge} はありません（0〜{}）。",
            edges.len().saturating_sub(1)
        )
    })?;
    let start = e.points.first().copied().ok_or("稜線が空です。")?;
    let body = kernel("可変フィレット", stop, || {
        sk::fillet_variable(b.body(), e.mid, start, r0, r1)
    })
    .map_err(|m| format!("{m}（可変フィレットは今のところ直線の稜線に対応しています。）"))?;
    brep(body, stop)
}

/// Every face offset by `distance` along its outward normal (negative: inward), as one sheet.
pub fn offset_surface(b: &BrepValue, distance: f64, stop: Stop) -> R<BrepValue> {
    if !distance.is_finite() || distance.abs() > kb::limits::MAX_COORDINATE {
        return Err("オフセット距離が無効です。".into());
    }
    let faces: Vec<usize> = (0..b.face_count()).collect();
    let body = kernel("オフセット面", stop, || {
        sk::offset_surface(b.body(), &faces, distance, 0.01)
    })?;
    brep(body, stop)
}

/// Tilt the vertical walls by `angle_deg` about their line on `z = 0`, leaning inward going up
/// (a draft for moulds and prints).
pub fn draft(b: &BrepValue, angle_deg: f64, stop: Stop) -> R<BrepValue> {
    if !angle_deg.is_finite() || angle_deg.abs() >= 89. {
        return Err("ドラフト角は -89° より大きく 89° より小さくしてください。".into());
    }
    let faces = kernel("面の取得", stop, || {
        b.body().faces(tol::chord(b.body().size()))
    })?;
    let walls: Vec<Vec3> = faces
        .iter()
        .filter(|f| f.plane_normal.is_some_and(|n| n.z.abs() < 1e-6))
        .map(|f| f.centroid)
        .collect();
    if walls.is_empty() {
        return Err("垂直な側面がありません。ドラフト角は垂直な壁に付けます。".into());
    }
    let neutral = Plane::XY;
    let body = kernel("ドラフト角", stop, || {
        sk::draft(b.body(), &walls, &neutral, Vec3::Z, angle_deg.to_radians())
    })?;
    brep(body, stop)
}

/// Column-major 4x4 matrix of a linear map plus translation.
fn matrix(m: [[f64; 3]; 3], t: P3) -> [[f64; 4]; 4] {
    [
        [m[0][0], m[1][0], m[2][0], 0.],
        [m[0][1], m[1][1], m[2][1], 0.],
        [m[0][2], m[1][2], m[2][2], 0.],
        [t[0], t[1], t[2], 1.],
    ]
}

/// Mirror across the plane `axis = 0` (0 = X, 1 = Y, 2 = Z), optionally keeping the original
/// (joined with a union when they touch, else both bodies in one).
pub fn mirror(b: &BrepValue, axis: usize, keep: bool, stop: Stop) -> R<Vec<BrepValue>> {
    if axis > 2 {
        return Err("軸は 0（X）・1（Y）・2（Z）にしてください。".into());
    }
    let mut m = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    if let Some(row) = m.get_mut(axis)
        && let Some(x) = row.get_mut(axis)
    {
        *x = -1.;
    }
    let mirrored = kernel("ミラー", stop, || {
        sk::transform_matrix(b.body(), matrix(m, [0.; 3]))
    })?;
    let mirrored = brep(mirrored, stop)?;
    if !keep {
        return Ok(vec![mirrored]);
    }
    match kb::boolean(b, &mirrored, kb::Bool::Union, stop) {
        Ok(u) => Ok(vec![u]),
        Err(_) => Ok(vec![b.clone(), mirrored]),
    }
}

/// `count` copies, each moved by `step` from the one before.
pub fn linear_array(b: &BrepValue, step: P3, count: usize, stop: Stop) -> R<Vec<BrepValue>> {
    if !(1..=500).contains(&count) || step.iter().any(|x| !x.is_finite()) {
        return Err("個数は 1〜500、間隔は有限な値にしてください。".into());
    }
    if b.weight().saturating_mul(count) > 40_000_000 {
        return Err("配列が大きすぎます。個数を減らしてください。".into());
    }
    let mats: Vec<[[f64; 4]; 4]> = (1..count)
        .map(|i| {
            matrix(
                [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
                scale(step, i as f64),
            )
        })
        .collect();
    let mut out = vec![b.clone()];
    out.extend(transformed_copies(b, &mats, "配列", stop)?);
    Ok(out)
}

/// `count` copies turned evenly about the Z axis over `angle_deg`.
pub fn polar_array(b: &BrepValue, count: usize, angle_deg: f64, stop: Stop) -> R<Vec<BrepValue>> {
    if !(1..=500).contains(&count) || !angle_deg.is_finite() || angle_deg.abs() > 360. {
        return Err("個数は 1〜500、角度は -360〜360 にしてください。".into());
    }
    if b.weight().saturating_mul(count) > 40_000_000 {
        return Err("配列が大きすぎます。個数を減らしてください。".into());
    }
    let full = (angle_deg.abs() - 360.).abs() < 1e-9;
    let steps = if full || count == 1 {
        count.max(1)
    } else {
        count - 1
    };
    let mats: Vec<[[f64; 4]; 4]> = (1..count)
        .map(|i| {
            let a = (angle_deg * i as f64 / steps as f64).to_radians();
            let (s, c) = a.sin_cos();
            matrix([[c, -s, 0.], [s, c, 0.], [0., 0., 1.]], [0.; 3])
        })
        .collect();
    let mut out = vec![b.clone()];
    out.extend(transformed_copies(b, &mats, "円形配列", stop)?);
    Ok(out)
}

/// Worker threads for independent copies (the machine's cores, at most 8).
fn workers() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 8)
}

/// Transformed copies of a body, meshed in parallel: one batch per core, the cancel flag
/// checked between batches (a kernel call itself cannot be interrupted).
pub fn transformed_copies(
    b: &BrepValue,
    mats: &[[[f64; 4]; 4]],
    what: &str,
    stop: Stop,
) -> R<Vec<BrepValue>> {
    let never = || false;
    let mut out = Vec::with_capacity(mats.len());
    for batch in mats.chunks(workers()) {
        stopped(stop)?;
        let results: Vec<R<BrepValue>> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|m| {
                    let never = &never;
                    scope.spawn(move || {
                        let body = kernel(what, never, || sk::transform_matrix(b.body(), *m))?;
                        brep(body, never)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        Err(format!("{what}: 計算中に内部エラーが起きました。"))
                    })
                })
                .collect()
        });
        for r in results {
            out.push(r?);
        }
    }
    Ok(out)
}

/// A round pipe of `radius` along a rail: an exact circle at the rail's start, swept.
pub fn pipe(rail: &Curve, radius: f64, stop: Stop) -> R<BrepValue> {
    let r = positive("半径", radius)?;
    let pts = kb::sample_curve(rail, 6, stop)?;
    let (Some(&start), Some(&next)) = (pts.first(), pts.get(1)) else {
        return Err("レール曲線が短すぎます。".into());
    };
    let tangent = unit(sub(next, start)).ok_or("レールの向きを決められません。")?;
    let circle = CurveGeometry::circle(start, tangent, r)?;
    let profile = kb::curve_value(circle, stop)?;
    kb::sweep1(&profile, rail, stop)
}

/// A planar face filling a closed planar curve (a patch).
pub fn planar_patch(c: &Curve, stop: Stop) -> R<BrepValue> {
    let (plane, lp) = kb::profile(c, None, stop)?;
    let region = solidcraft_geom::Region2 {
        outer: lp,
        holes: Vec::new(),
    };
    let body = kernel("パッチ", stop, || sk::patch_region(&plane, &region))?;
    brep(body, stop)
}

// ---------------------------------------------------------------------------------------------
// Curve tools (NODES-native, exact NURBS)
// ---------------------------------------------------------------------------------------------

fn geometry(c: &Curve) -> R<CurveGeometry> {
    match &c.geometry {
        Some(g) => Ok(g.clone()),
        None => Ok(CurveGeometry::nurbs(NurbsCurve::polyline(
            c.points.clone(),
            c.closed,
        )?)),
    }
}

/// Position, unit tangent and curvature vector at normalized parameter `t`.
fn frame(g: &CurveGeometry, t: f64) -> R<(P3, P3, P3)> {
    let [p, d1, d2] = g.derivatives(t)?;
    let speed2 = dot(d1, d1);
    let tangent = unit(d1).ok_or("曲線の向きを決められません（速さが 0）。")?;
    let k = scale(sub(d2, scale(tangent, dot(d2, tangent))), 1. / speed2);
    Ok((p, tangent, k))
}

/// How two curves meet: the end of `a` against the start of `b`. Returns the continuity level
/// reached (0 = G0, 1 = G1, 2 = G2, -1 = apart), the gap (mm), the tangent angle (degrees) and
/// the curvature difference (1/mm).
pub fn continuity(a: &Curve, b: &Curve) -> R<[f64; 4]> {
    let (ga, gb) = (geometry(a)?, geometry(b)?);
    let (pa, ta, ka) = frame(&ga, 1.)?;
    let (pb, tb, kb_) = frame(&gb, 0.)?;
    let gap = norm(sub(pa, pb));
    let angle = dot(ta, tb).clamp(-1., 1.).acos().to_degrees();
    let dk = norm(sub(ka, kb_));
    let size = [pa, pb].iter().fold(1f64, |m, p| m.max(norm(*p)));
    let level = if gap > tol::ABS.max(size * 1e-9) {
        -1.
    } else if angle > 0.01 {
        0.
    } else if dk > 1e-4 * (1. + norm(ka)) {
        1.
    } else {
        2.
    };
    Ok([level, gap, angle, dk])
}

/// A blend curve from the end of `a` to the start of `b`: a cubic Bézier with matching tangents
/// (G1) or a quintic with matching tangents and curvature (G2). `bulge` scales the tangents.
pub fn blend_curve(a: &Curve, b: &Curve, g2: bool, bulge: f64, stop: Stop) -> R<Curve> {
    if !(bulge.is_finite() && bulge > 0.01 && bulge <= 10.) {
        return Err("ふくらみは 0.01〜10 にしてください。".into());
    }
    let (pa, ta, ka) = frame(&geometry(a)?, 1.)?;
    let (pb, tb, kb_) = frame(&geometry(b)?, 0.)?;
    let d = norm(sub(pb, pa));
    if d <= tol::ABS {
        return Err("2 本の曲線の端がすでに重なっています。".into());
    }
    let speed = d * bulge;
    let points = if g2 {
        let (v0, v1) = (scale(ta, speed), scale(tb, speed));
        let (a0, a1) = (scale(ka, speed * speed), scale(kb_, speed * speed));
        let p1 = add(pa, scale(v0, 0.2));
        let p2 = add(sub(scale(p1, 2.), pa), scale(a0, 0.05));
        let p4 = sub(pb, scale(v1, 0.2));
        let p3 = add(sub(scale(p4, 2.), pb), scale(a1, 0.05));
        vec![pa, p1, p2, p3, p4, pb]
    } else {
        vec![
            pa,
            add(pa, scale(ta, speed / 3.)),
            sub(pb, scale(tb, speed / 3.)),
            pb,
        ]
    };
    kb::curve_value(CurveGeometry::nurbs(NurbsCurve::bezier(points)?), stop)
}

/// Change the end of `a` so that it meets the start of `b` with G0, G1 or G2 continuity, by
/// moving the last one, two or three control points of `a` (a non-rational, clamped NURBS).
pub fn match_curve(a: &Curve, b: &Curve, level: usize, stop: Stop) -> R<Curve> {
    let ga = geometry(a)?;
    let n = ga
        .as_nurbs()
        .ok_or("合わせる曲線は NURBS にしてください（「曲線を再構築」を通すと確実です）。")?;
    if ga.domain() != n.domain() {
        return Err("向きを反転した・切り出した曲線は、先に「曲線を再構築」してください。".into());
    }
    let p = n.degree();
    let mut pts = n.control_points();
    let weights = n.weights();
    let knots = n.knots().to_vec();
    if weights.iter().any(|w| (w - 1.).abs() > 1e-12) {
        return Err(
            "重み付き（有理）曲線は合わせられません。「曲線を再構築」を通してください。".into(),
        );
    }
    let count = pts.len();
    if level > 2 || count < level + 2 || p < level.max(1) {
        return Err("制御点が足りません。G2 には制御点 4 個以上・次数 2 以上が必要です。".into());
    }
    let last = count - 1;
    let (pb, tb, kb_) = frame(&geometry(b)?, 0.)?;
    let k = |i: usize| knots.get(i).copied().unwrap_or(0.);
    // Native end derivative of `a` now; its length is kept.
    let [_, d1, _] = n.derivatives(n.domain()[1])?;
    let speed = norm(d1).max(tol::ABS);
    let set = |pts: &mut Vec<P3>, i: usize, v: P3| {
        if let Some(slot) = pts.get_mut(i) {
            *slot = v;
        }
    };
    set(&mut pts, last, pb);
    if level >= 1 {
        let q1 = scale(tb, speed);
        let span = k(last + p) - k(last);
        if span <= 0. {
            return Err("曲線の端のノットが重なっています。".into());
        }
        let p_prev = sub(pb, scale(q1, span / p as f64));
        set(&mut pts, last - 1, p_prev);
        if level >= 2 {
            let c2 = scale(kb_, speed * speed);
            let span2 = k(last + p - 1) - k(last);
            let span3 = k(last + p - 1) - k(last - 1);
            if span2 <= 0. || span3 <= 0. || p < 2 {
                return Err("曲線の端のノットが重なっています。".into());
            }
            let q0 = sub(q1, scale(c2, span2 / (p - 1) as f64));
            set(&mut pts, last - 2, sub(p_prev, scale(q0, span3 / p as f64)));
        }
    }
    let curve = NurbsCurve::new(p, pts, weights, knots)?;
    kb::curve_value(CurveGeometry::nurbs(curve), stop)
}

/// A curvature comb: at `count` points, a line from the curve along the curvature vector,
/// `scale_mm` long per 1/mm of curvature (Rhino's CurvatureGraph).
pub fn curvature_comb(c: &Curve, count: usize, scale_mm: f64, stop: Stop) -> R<Vec<Curve>> {
    if !(2..=1000).contains(&count) || !(scale_mm.is_finite() && scale_mm > 0.) {
        return Err("本数は 2〜1000、倍率は 0 より大きくしてください。".into());
    }
    let g = geometry(c)?;
    let mut out = Vec::with_capacity(count + 1);
    let mut tips = Vec::with_capacity(count);
    for i in 0..count {
        stopped(stop)?;
        let t = i as f64 / (count - 1) as f64;
        let (p, _, k) = frame(&g, t)?;
        let tip = sub(p, scale(k, scale_mm));
        tips.push(tip);
        out.push(Curve {
            points: vec![p, tip],
            closed: false,
            geometry: None,
        });
    }
    out.push(Curve {
        points: tips,
        closed: false,
        geometry: None,
    });
    Ok(out)
}

/// Offset a planar curve in its plane by `distance` (positive: outward for a closed
/// counter-clockwise curve), through the kernel's exact planar offset of the profile.
pub fn offset_curve(c: &Curve, distance: f64, stop: Stop) -> R<Curve> {
    if !distance.is_finite() || distance.abs() > kb::limits::MAX_COORDINATE {
        return Err("オフセット距離が無効です。".into());
    }
    let g = geometry(c)?;
    let samples = kb::sample_curve(c, 8, stop)?;
    let plane = kb::fit_plane(&samples, Some([0., 0., 1.]))?;
    let normal = p3(plane.normal());
    let count = (samples.len() * 2).clamp(64, 2048);
    let closed = g.closed()?;
    let mut pts = Vec::with_capacity(count);
    for i in 0..count {
        stopped(stop)?;
        let t = if closed {
            i as f64 / count as f64
        } else {
            i as f64 / (count - 1) as f64
        };
        let (p, tangent, _) = frame(&g, t)?;
        let side = unit(cross(tangent, normal)).ok_or("曲線が平面上にありません。")?;
        pts.push(add(p, scale(side, distance)));
    }
    if closed && let Some(&first) = pts.first() {
        pts.push(first);
    }
    fit_points(&pts, stop)
}

// ---------------------------------------------------------------------------------------------
// Analysis meshes and diagnostics
// ---------------------------------------------------------------------------------------------

/// Area-weighted vertex normals of a triangle mesh.
fn vertex_normals(m: &crate::model::Mesh) -> Vec<P3> {
    let mut n = vec![[0.; 3]; m.vertices.len()];
    for t in &m.triangles {
        let p = |i: u32| m.vertices.get(i as usize).copied().unwrap_or([0.; 3]);
        let f = cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])));
        for &i in t {
            if let Some(slot) = n.get_mut(i as usize) {
                *slot = add(*slot, f);
            }
        }
    }
    n.into_iter()
        .map(|v| unit(v).unwrap_or([0., 0., 1.]))
        .collect()
}

/// The parts of each triangle where the linear scalar `f` (given at its corners) lies in a band
/// `[k, k + 0.5)` for an integer k (Sutherland–Hodgman clipping against the band's levels).
fn bands(m: &crate::model::Mesh, f: &[f64], stop: Stop) -> R<crate::model::Mesh> {
    let mut out = crate::model::Mesh::default();
    for t in &m.triangles {
        stopped(stop)?;
        let poly: Vec<(P3, f64)> = t
            .iter()
            .map(|&i| {
                (
                    m.vertices.get(i as usize).copied().unwrap_or([0.; 3]),
                    f.get(i as usize).copied().unwrap_or(0.),
                )
            })
            .collect();
        let lo = poly.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let hi = poly.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        if !(lo.is_finite() && hi.is_finite()) || hi - lo > 1e4 {
            continue;
        }
        let mut k = lo.floor();
        while k <= hi {
            let clip = |poly: Vec<(P3, f64)>, level: f64, keep_above: bool| -> Vec<(P3, f64)> {
                let inside = |v: f64| if keep_above { v >= level } else { v < level };
                let mut res = Vec::with_capacity(poly.len() + 2);
                for i in 0..poly.len() {
                    let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % poly.len())) else {
                        continue;
                    };
                    if inside(a.1) {
                        res.push(a);
                    }
                    if inside(a.1) != inside(b.1) && (b.1 - a.1).abs() > 1e-300 {
                        let s = (level - a.1) / (b.1 - a.1);
                        res.push((add(a.0, scale(sub(b.0, a.0), s)), level));
                    }
                }
                res
            };
            let piece = clip(clip(poly.clone(), k, true), k + 0.5, false);
            if piece.len() >= 3 {
                let base = out.vertices.len() as u32;
                out.vertices.extend(piece.iter().map(|p| p.0));
                for j in 1..piece.len() as u32 - 1 {
                    out.triangles.push([base, base + j, base + j + 1]);
                }
            }
            k += 1.;
        }
        if out.triangles.len() > kb::limits::MAX_TRIANGLES {
            return Err("縞が細かすぎます。本数を減らしてください。".into());
        }
    }
    Ok(out)
}

/// Zebra stripes (isophotes): the dark stripes of the reflection of parallel light bars along
/// `axis` (0 = X, 1 = Y, 2 = Z), as a mesh lifted slightly off the surface. Stripes that break
/// or kink across an edge show where the surfaces are not G1 or G2.
pub fn zebra(b: &BrepValue, stripes: f64, axis: usize, stop: Stop) -> R<crate::model::Mesh> {
    if !(stripes.is_finite() && (1. ..=200.).contains(&stripes)) || axis > 2 {
        return Err("縞の本数は 1〜200、向きは 0〜2 にしてください。".into());
    }
    let m = b.preview();
    let normals = vertex_normals(m);
    let mut dir = [0.; 3];
    if let Some(d) = dir.get_mut(axis) {
        *d = 1.;
    }
    let f: Vec<f64> = normals
        .iter()
        .map(|n| stripes * dot(*n, dir).clamp(-1., 1.).acos() / std::f64::consts::PI)
        .collect();
    let lift = tol::chord(b.body().size()) * 0.5;
    let lifted = crate::model::Mesh {
        vertices: m
            .vertices
            .iter()
            .zip(&normals)
            .map(|(p, n)| add(*p, scale(*n, lift)))
            .collect(),
        triangles: m.triangles.clone(),
    };
    bands(&lifted, &f, stop)
}

/// The surface that is steeper than `angle_deg` from the pull direction +Z (walls that would
/// need a draft for a mould, or overhangs for printing when negative angles are wanted).
pub fn draft_analysis(b: &BrepValue, angle_deg: f64, stop: Stop) -> R<crate::model::Mesh> {
    if !angle_deg.is_finite() || !(0. ..=89.).contains(&angle_deg) {
        return Err("角度は 0〜89° にしてください。".into());
    }
    let m = b.preview();
    let limit = angle_deg.to_radians().sin();
    let mut out = crate::model::Mesh::default();
    for t in &m.triangles {
        stopped(stop)?;
        let p = |i: u32| m.vertices.get(i as usize).copied().unwrap_or([0.; 3]);
        let Some(n) = unit(cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])))) else {
            continue;
        };
        // Draft angle of a face: its tilt from the pull direction, asin(|n·z|).
        if n[2].abs() < limit {
            let base = out.vertices.len() as u32;
            out.vertices.extend([p(t[0]), p(t[1]), p(t[2])]);
            out.triangles.push([base, base + 1, base + 2]);
        }
    }
    if out.triangles.is_empty() {
        return Err(format!("抜き勾配 {angle_deg}° 未満の面はありません。"));
    }
    Ok(out)
}

/// Tolerance and validity check of a B-rep: [closed (1/0), faces, edges, shells, open mesh edges,
/// mesh watertight (1/0), crossed face loops, faces that did not mesh, volume (mm³, 0 if open),
/// chord tolerance used (mm), linear tolerance (mm)].
pub fn diagnose(b: &BrepValue, stop: Stop) -> R<Vec<f64>> {
    let body = b.body();
    let crossed = kernel("診断", stop, || body.crossed_face_loops()).unwrap_or(usize::MAX);
    let open = kb::boundary_edges(b.preview()).len();
    Ok(vec![
        f64::from(u8::from(b.is_closed())),
        b.face_count() as f64,
        b.edge_count() as f64,
        body.shell_count() as f64,
        open as f64,
        f64::from(u8::from(b.mesh_is_watertight())),
        if crossed == usize::MAX {
            -1.
        } else {
            crossed as f64
        },
        body.unmeshed_faces() as f64,
        b.volume().unwrap_or(0.),
        tol::chord(body.size()),
        tol::ABS,
    ])
}

/// A sheet thickened into a closed solid (SolidCraft `thicken`: planes and cylinders exactly,
/// other faces fitted within 0.01 mm).
pub fn thicken(b: &BrepValue, thickness: f64, both: bool, stop: Stop) -> R<BrepValue> {
    if !thickness.is_finite()
        || thickness.abs() <= tol::ABS
        || thickness.abs() > kb::limits::MAX_COORDINATE
    {
        return Err("厚みは 0 以外の有限な値にしてください。".into());
    }
    if b.is_closed() {
        return Err(
            "閉じた立体には厚みをつけられません。中空にするときは「シェル」を使ってください。"
                .into(),
        );
    }
    let body = kernel("厚みをつける", stop, || {
        sk::thicken(b.body(), thickness, both)
    })?;
    brep(body, stop)
}

// ---------------------------------------------------------------------------------------------
// Blend / match surfaces, trimming by any plane, seam analysis, curvature map
// ---------------------------------------------------------------------------------------------

/// A whole-number input (an edge or face number) as an index.
fn whole(name: &str, v: f64) -> R<usize> {
    if v.is_finite() && (0. ..1e9).contains(&v) {
        Ok(v as usize)
    } else {
        Err(format!("{name}は 0 以上の整数にしてください。"))
    }
}

/// 0 → G0 (position), 1 → G1 (tangent plane), 2 or more → G2 (curvature).
fn grade(v: f64) -> sk::Continuity {
    if v < 0.5 {
        sk::Continuity::G0
    } else if v < 1.5 {
        sk::Continuity::G1
    } else {
        sk::Continuity::G2
    }
}

/// A point on edge number `edge` (its middle), the way the kernel picks an edge again.
fn edge_point(b: &BrepValue, edge: f64, stop: Stop) -> R<Vec3> {
    let index = whole("稜線番号", edge)?;
    let edges = kernel("稜線の取得", stop, || {
        b.body().edges(tol::chord(b.body().size()))
    })?;
    edges.get(index).map(|e| e.mid).ok_or_else(|| {
        format!(
            "稜線番号 {index} はありません（0〜{}）。",
            edges.len().saturating_sub(1)
        )
    })
}

/// A blend surface between edge `edge_a` of `a` and edge `edge_b` of `b`. `grades` are 0 (G0),
/// 1 (G1) or 2 (G2) for each side; `crown` is how full the middle is.
pub fn blend_surface(
    a: &BrepValue,
    edge_a: f64,
    b: &BrepValue,
    edge_b: f64,
    grades: (f64, f64),
    crown: f64,
    stop: Stop,
) -> R<BrepValue> {
    if !crown.is_finite() || crown.abs() > 10. {
        return Err("ふくらみは -10〜10 の範囲にしてください。".into());
    }
    let pa = edge_point(a, edge_a, stop)?;
    let pb = edge_point(b, edge_b, stop)?;
    let (body, _gap) = kernel("ブレンド曲面", stop, || {
        sk::blend_between(
            a.body(),
            &[pa],
            b.body(),
            &[pb],
            [grade(grades.0), grade(grades.1)],
            [crown, crown],
            None,
            None,
            33,
            false,
        )
    })?;
    brep(body, stop)
}

/// Rebuild face `face` of the sheet so that its edge nearest `edge` of `target` takes that edge's
/// place and continues the target's surface up to `order` (0 = G0, 1 = G1, 2 = G2).
pub fn match_sheet(
    sheet: &BrepValue,
    face: f64,
    target: &BrepValue,
    edge: f64,
    order: f64,
    stop: Stop,
) -> R<BrepValue> {
    if sheet.is_closed() {
        return Err(
            "曲面のマッチは開いた面（シート）に使います。閉じた立体では「面を外す」などで開いてください。"
                .into(),
        );
    }
    let index = whole("面番号", face)?;
    let faces = kernel("面の取得", stop, || {
        sheet.body().faces(tol::chord(sheet.body().size()))
    })?;
    let at = faces.get(index).map(|f| f.centroid).ok_or_else(|| {
        format!(
            "面番号 {index} はありません（0〜{}）。",
            faces.len().saturating_sub(1)
        )
    })?;
    let on_target = edge_point(target, edge, stop)?;
    let body = kernel("曲面のマッチ", stop, || {
        sk::match_surface(
            sheet.body(),
            &[at],
            target.body(),
            &[on_target],
            grade(order),
        )
    })?;
    brep(body, stop)
}

/// Cut by the plane through `origin` with normal `normal`; keep the normal's side (`keep_normal`)
/// or the other side. The cut face is the exact plane.
pub fn trim_by_any_plane(
    b: &BrepValue,
    origin: P3,
    normal: P3,
    keep_normal: bool,
    stop: Stop,
) -> R<BrepValue> {
    if origin
        .iter()
        .any(|v| !v.is_finite() || v.abs() > kb::limits::MAX_COORDINATE)
    {
        return Err("平面の点が無効です。".into());
    }
    let n = unit(normal).ok_or("平面の法線が 0 です。X・Y・Z のどれかを 0 以外にしてください。")?;
    let plane = Plane::from_normal(v3(origin), v3(n)).ok_or("平面を作れません。")?;
    let (part, _tool) = kernel("平面で切る", stop, || {
        sk::half_space(b.body(), &plane, keep_normal)
    })?;
    match part {
        Some(body) => brep(body, stop),
        None => Err(format!(
            "平面の{}に残る部分がありません。平面の位置か向きを変えてください。",
            if keep_normal {
                "法線側"
            } else {
                "反対側"
            }
        )),
    }
}

/// Measure every two-sided seam between faces. Returns `[seams, smooth, curvature jump, sharp,
/// gap, worst gap mm, worst normal angle deg, worst curvature difference 1/mm, skipped,
/// unmeasured]`; `tolerances` is `[gap mm, angle deg, curvature 1/mm]`.
pub fn seam_report(b: &BrepValue, samples: f64, tolerances: [f64; 3], stop: Stop) -> R<Vec<f64>> {
    if !samples.is_finite() || !(3. ..=257.).contains(&samples) {
        return Err("測定点は 3〜257 にしてください。".into());
    }
    let report = kernel("連続性の解析", stop, || {
        sk::seam_diagnostics(
            b.body(),
            &[],
            samples as usize,
            sk::ContinuityTolerances {
                gap_mm: tolerances[0],
                angle_deg: tolerances[1],
                curvature_per_mm: tolerances[2],
            },
        )
    })?;
    let count = |v: sk::SeamVerdict| report.seams.iter().filter(|s| s.verdict == v).count() as f64;
    Ok(vec![
        report.seams.len() as f64,
        count(sk::SeamVerdict::Smooth),
        count(sk::SeamVerdict::CurvatureJump),
        count(sk::SeamVerdict::Sharp),
        count(sk::SeamVerdict::Gap),
        report
            .seams
            .iter()
            .map(|s| s.gap_mm.value)
            .fold(0., f64::max),
        report
            .seams
            .iter()
            .map(|s| s.normal_angle_deg.value)
            .fold(0., f64::max),
        report
            .seams
            .iter()
            .map(|s| s.curvature_jump_per_mm.value)
            .fold(0., f64::max),
        report.skipped as f64,
        report.unmeasured as f64,
    ])
}

/// Per-vertex Gaussian curvature of the display mesh (angle defect over a third of the
/// surrounding triangle area; open boundary vertices are 0).
fn gaussian_curvature(m: &crate::model::Mesh) -> Vec<f64> {
    let n = m.vertices.len();
    let mut angles = vec![0.; n];
    let mut area = vec![0.; n];
    let mut boundary = vec![false; n];
    let mut edge_use: std::collections::HashMap<(u32, u32), u32> = Default::default();
    for t in &m.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edge_use.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    for (&(a, b), &c) in &edge_use {
        if c == 1 {
            for v in [a, b] {
                if let Some(f) = boundary.get_mut(v as usize) {
                    *f = true;
                }
            }
        }
    }
    let pt = |i: u32| m.vertices.get(i as usize).copied().unwrap_or([0.; 3]);
    for t in &m.triangles {
        let tri_area = 0.5 * norm(cross(sub(pt(t[1]), pt(t[0])), sub(pt(t[2]), pt(t[0]))));
        for k in 0..3 {
            let (i, j, l) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let (u, w) = (sub(pt(j), pt(i)), sub(pt(l), pt(i)));
            let denom = norm(u) * norm(w);
            if let (true, Some(a)) = (denom > 1e-300, angles.get_mut(i as usize)) {
                *a += (dot(u, w) / denom).clamp(-1., 1.).acos();
            }
            if let Some(a) = area.get_mut(i as usize) {
                *a += tri_area / 3.;
            }
        }
    }
    (0..n)
        .map(|i| {
            let (a, s, edge) = (
                angles.get(i).copied().unwrap_or(0.),
                area.get(i).copied().unwrap_or(0.),
                boundary.get(i).copied().unwrap_or(true),
            );
            if edge || s <= 1e-300 {
                0.
            } else {
                (2. * std::f64::consts::PI - a) / s
            }
        })
        .collect()
}

/// Bands of equal Gaussian curvature (`K · size² · gain`) over the display mesh.
pub fn curvature_map(b: &BrepValue, gain: f64, stop: Stop) -> R<crate::model::Mesh> {
    if !gain.is_finite() || gain <= 0. || gain > 1e3 {
        return Err("感度は 0 より大きく 1000 以下にしてください。".into());
    }
    let m = b.preview();
    let size = b.body().size();
    let k = gaussian_curvature(m);
    let f: Vec<f64> = k
        .iter()
        .map(|v| (v * size * size * gain).clamp(-50., 50.))
        .collect();
    bands(m, &f, stop)
}
