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
