//! Bounded approximate geometric diagnostics, distinct from index topology.
//! These tests do not certify printability or exact intersection predicates.
use crate::engine::{cancelled, cross, dot, length, sub, validate_mesh};
use crate::mesh_fabrication::signed_volume;
use crate::model::Mesh;

type P = [f64; 3];
const TOL: f64 = 1e-8; // model units (mm in the printing workflow)
const MAX_PAIRS: usize = 4_000_000;
const MAX_RAY_TESTS: usize = 4_000_000;
fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: P, s: f64) -> P {
    a.map(|x| x * s)
}
fn tri(m: &Mesh, i: usize) -> [P; 3] {
    m.triangles[i].map(|v| m.vertices[v as usize])
}
fn edges(t: [P; 3]) -> [P; 3] {
    [sub(t[1], t[0]), sub(t[2], t[1]), sub(t[0], t[2])]
}
fn axis_separates(a: [P; 3], b: [P; 3], axis: P) -> bool {
    let l = length(axis);
    if l <= 1e-30 {
        return false;
    }
    let n = mul(axis, 1. / l);
    let origin = a[0];
    let (mut al, mut ah) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut bl, mut bh) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in a {
        let x = dot(sub(v, origin), n);
        al = al.min(x);
        ah = ah.max(x);
    }
    for v in b {
        let x = dot(sub(v, origin), n);
        bl = bl.min(x);
        bh = bh.max(x);
    }
    ah + TOL < bl || bh + TOL < al
}
fn contact(a: [P; 3], b: [P; 3]) -> bool {
    let ea = edges(a);
    let eb = edges(b);
    let na = cross(ea[0], ea[1]);
    let nb = cross(eb[0], eb[1]);
    if axis_separates(a, b, na) || axis_separates(a, b, nb) {
        return false;
    }
    // Edge cross products cover skew triangles. In-plane axes cover coplanar triangles.
    for x in ea {
        if axis_separates(a, b, cross(na, x)) {
            return false;
        }
        for y in eb {
            if axis_separates(a, b, cross(x, y)) {
                return false;
            }
        }
    }
    for y in eb {
        if axis_separates(a, b, cross(nb, y)) {
            return false;
        }
    }
    true
}
#[derive(Clone, Copy)]
struct Box3 {
    lo: P,
    hi: P,
    face: usize,
}
fn aabb(t: [P; 3], face: usize) -> Box3 {
    let mut lo = t[0];
    let mut hi = t[0];
    for v in &t[1..] {
        for j in 0..3 {
            lo[j] = lo[j].min(v[j]);
            hi[j] = hi[j].max(v[j]);
        }
    }
    Box3 { lo, hi, face }
}
/// Count approximate contacting triangle pairs without shared vertex indices.
/// Returns an error instead of a partial count if the broad-phase budget is exhausted.
/// Contacts at edges/points count, even between otherwise separate bodies.
pub fn nonadjacent_contacts(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<usize, String> {
    validate_mesh(m, stop)?;
    let mut boxes = Vec::with_capacity(m.triangles.len());
    for i in 0..m.triangles.len() {
        if i % 512 == 0 {
            cancelled(stop)?;
        }
        boxes.push(aabb(tri(m, i), i));
    }
    // Pick the broad-phase axis with the largest span (simple heuristic).
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for b in &boxes {
        for j in 0..3 {
            lo[j] = lo[j].min(b.lo[j]);
            hi[j] = hi[j].max(b.hi[j]);
        }
    }
    let axis = (0..3)
        .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
        .unwrap_or(0);
    boxes.sort_unstable_by(|a, b| a.lo[axis].total_cmp(&b.lo[axis]).then(a.face.cmp(&b.face)));
    cancelled(stop)?;
    let (mut pairs, mut hits) = (0usize, 0usize);
    for (i, a) in boxes.iter().enumerate() {
        if i % 512 == 0 {
            cancelled(stop)?;
        }
        for b in &boxes[i + 1..] {
            if b.lo[axis] > a.hi[axis] + TOL {
                break;
            }
            pairs += 1;
            if pairs % 512 == 0 {
                cancelled(stop)?;
            }
            if pairs > MAX_PAIRS {
                return Err("自己接触検査の計算上限です。結果は未判定です。".into());
            }
            if (0..3).any(|j| a.hi[j] + TOL < b.lo[j] || b.hi[j] + TOL < a.lo[j]) {
                continue;
            }
            let u = m.triangles[a.face];
            let v = m.triangles[b.face];
            if u.iter().any(|x| v.contains(x)) {
                continue;
            }
            if contact(tri(m, a.face), tri(m, b.face)) {
                hits += 1;
            }
        }
    }
    Ok(hits)
}
fn ray_triangle(origin: P, d: P, t: [P; 3]) -> Option<f64> {
    let e1 = sub(t[1], t[0]);
    let e2 = sub(t[2], t[0]);
    let h = cross(d, e2);
    let det = dot(e1, h);
    let scale = length(e1) * length(e2);
    if det.abs() <= scale * 1e-12 {
        return None;
    }
    let s = sub(origin, t[0]);
    let u = dot(s, h) / det;
    if !(-1e-10..=1. + 1e-10).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(d, q) / det;
    if v < -1e-10 || u + v > 1. + 1e-10 {
        return None;
    }
    let dist = dot(e2, q) / det;
    (dist > TOL).then_some(dist)
}
/// Minimum of sampled inward normal ray exit distances, not global minimum wall thickness.
/// Face-index sampling is deterministic but not area-weighted. Very thin, oblique or
/// unsampled walls can be missed. No result on missing or wrongly oriented exits.
pub fn sampled_thickness(
    m: &Mesh,
    requested: usize,
    stop: &dyn Fn() -> bool,
) -> Result<(f64, usize), String> {
    if requested == 0 || requested > 512 {
        return Err("サンプル数は1〜512です。".into());
    }
    if signed_volume(m, stop)? <= 0. {
        return Err("正の向き付き体積が必要です。".into());
    }
    let n = m.triangles.len();
    let samples = requested.min(n);
    if samples.saturating_mul(n) > MAX_RAY_TESTS {
        return Err("肉厚推定の計算上限です。サンプル数を減らしてください。".into());
    }
    let mut minimum = f64::INFINITY;
    for k in 0..samples {
        cancelled(stop)?;
        let i = k * n / samples;
        let t = tri(m, i);
        let origin = mul(add(add(t[0], t[1]), t[2]), 1. / 3.);
        let normal = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let dir = mul(normal, -1. / length(normal));
        let mut nearest = f64::INFINITY;
        let mut exit = false;
        for j in 0..n {
            if j % 512 == 0 {
                cancelled(stop)?;
            }
            if j == i {
                continue;
            }
            let u = tri(m, j);
            if let Some(d) = ray_triangle(origin, dir, u)
                && d < nearest
            {
                nearest = d;
                exit = dot(cross(sub(u[1], u[0]), sub(u[2], u[0])), dir) > 0.;
            }
        }
        if !nearest.is_finite() || !exit {
            return Err("肉厚の出口を確定できません。法線や形状を確認してください。".into());
        }
        minimum = minimum.min(nearest);
    }
    Ok((minimum, samples))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn no() -> bool {
        false
    }
    #[test]
    fn crossing_and_coplanar() {
        let mut m = Mesh {
            vertices: vec![
                [0., 0., 0.],
                [2., 0., 0.],
                [0., 2., 0.],
                [0.5, 0.5, -1.],
                [0.5, 0.5, 1.],
                [1., 1., 0.],
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
        };
        assert_eq!(nonadjacent_contacts(&m, &no).unwrap(), 1);
        m.vertices[3] = [0.5, 0.5, 0.];
        m.vertices[4] = [1., 0.5, 0.];
        m.vertices[5] = [0.5, 1., 0.];
        assert_eq!(nonadjacent_contacts(&m, &no).unwrap(), 1);
        for p in &mut m.vertices[3..] {
            p[2] += 1.;
        }
        assert_eq!(nonadjacent_contacts(&m, &no).unwrap(), 0);
    }
    #[test]
    fn coplanar_aabbs_overlap_but_triangles_do_not() {
        let m = Mesh {
            vertices: vec![
                [0., 0., 0.],
                [2., 0., 0.],
                [0., 2., 0.],
                [1.2, 1.2, 0.],
                [2., 1.2, 0.],
                [1.2, 2., 0.],
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
        };
        assert_eq!(nonadjacent_contacts(&m, &no).unwrap(), 0);
    }
    #[test]
    fn slab() {
        let m = Mesh {
            vertices: vec![
                [0., 0., 0.],
                [10., 0., 0.],
                [10., 10., 0.],
                [0., 10., 0.],
                [0., 0., 0.8],
                [10., 0., 0.8],
                [10., 10., 0.8],
                [0., 10., 0.8],
            ],
            triangles: vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ],
        };
        let (w, n) = sampled_thickness(&m, 12, &no).unwrap();
        assert_eq!(n, 12);
        assert!((w - 0.8).abs() < 1e-9);
        assert_eq!(nonadjacent_contacts(&m, &no).unwrap(), 0);
    }
}

/// Conservative projected-area check for a linear vertex motion from the source
/// to an offset skin. The projected signed double area is quadratic in time:
/// test both endpoints and its interior minimum, not just the final face.
/// This is a floating-point diagnostic, not a certified immersion predicate.
pub(crate) fn guard_offset_sweep(
    source: &Mesh,
    moved: &[P],
    stop: &dyn Fn() -> bool,
) -> Result<(), String> {
    if moved.len() != source.vertices.len() {
        return Err("オフセットの頂点数が一致しません。".into());
    }
    for (i, face) in source.triangles.iter().enumerate() {
        if i.is_multiple_of(512) {
            cancelled(stop)?;
        }
        let [a, b, c] = face.map(|v| source.vertices[v as usize]);
        let [p, q, r] = face.map(|v| moved[v as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let (du, dv) = (sub(sub(q, p), u), sub(sub(r, p), v));
        let n = cross(u, v);
        let area2 = dot(n, n);
        let linear = dot(n, add(cross(du, v), cross(u, dv))) / area2;
        let quadratic = dot(n, cross(du, dv)) / area2;
        let end = 1.0 + linear + quadratic;
        let mut minimum = 1.0_f64.min(end);
        if quadratic > 0.0 {
            let t = -linear / (2.0 * quadratic);
            if t > 0.0 && t < 1.0 {
                minimum = minimum.min(1.0 + t * (linear + t * quadratic));
            }
        }
        if !area2.is_finite()
            || area2 <= 0.0
            || !linear.is_finite()
            || !quadratic.is_finite()
            || !end.is_finite()
            || minimum <= 1e-10
        {
            return Err(format!(
                "面 {i} のオフセット途中で投影面積が消失または反転する可能性があります。距離・厚みを小さくするか、曲率や鋭い角を調整してください。"
            ));
        }
    }
    Ok(())
}

/// An open sheet is allowed; invalid index topology and unusable vertices are not.
pub(crate) fn guard_offset_source(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<(), String> {
    validate_mesh(m, stop)?;
    if m.triangles.is_empty() {
        return Err("面のないメッシュです。".into());
    }
    let a = crate::mesh_fabrication::audit(m, stop)?;
    if a.non_manifold_edges != 0
        || a.non_manifold_vertices != 0
        || a.orientation_conflicts != 0
        || a.duplicate_faces != 0
        || a.degenerate_faces != 0
        || a.isolated_vertices != 0
    {
        return Err("厚み付け・オフセットの入力に非多様体、向きの不一致、重複面または孤立頂点があります。元のメッシュを修正してください。".into());
    }
    guard_offset_contacts(m, stop)
}

/// Bounded approximate global contact check. Exhaustion is an error, never a pass.
pub(crate) fn guard_offset_contacts(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<(), String> {
    let hits = nonadjacent_contacts(m, stop)?;
    if hits != 0 {
        return Err(format!(
            "共有頂点のない面の自己接触候補が {hits} 組あります。元の形状の隙間を広げるか、距離・厚みを小さくしてください。"
        ));
    }
    Ok(())
}

/// Necessary checks for an oriented closed shell; not a thickness certificate.
pub(crate) fn guard_closed_shell(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<(), String> {
    let a = crate::mesh_fabrication::audit(m, stop)?;
    if !a.closed_oriented() || a.isolated_vertices != 0 {
        return Err("厚み付け後のメッシュが閉じた向きのそろった多様体ではありません。厚みや元の面を調整してください。".into());
    }
    let volume = signed_volume(m, stop)?;
    if !volume.is_finite() || volume <= 0.0 {
        return Err("厚み付け後の符号付き体積が正ではありません。".into());
    }
    guard_offset_contacts(m, stop)
}

#[cfg(test)]
mod omega93_tests {
    use super::*;

    #[test]
    fn projected_area_detects_double_flip_even_with_positive_end_area() {
        let source = Mesh {
            vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            triangles: vec![[0, 1, 2]],
        };
        let moved = [[0., 0., 0.], [-1., 0., 0.], [0., -1., 0.]];
        assert!(guard_offset_sweep(&source, &moved, &|| false).is_err());
        let translated = [[0., 0., 1.], [1., 0., 1.], [0., 1., 1.]];
        assert!(guard_offset_sweep(&source, &translated, &|| false).is_ok());
        assert!(guard_offset_sweep(&source, &translated, &|| true).is_err());
    }
}
