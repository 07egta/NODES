//! Mesh physics (independent NODES code; textbook dynamic relaxation with kinetic damping,
//! cotangent-weighted mean-curvature flow). No GPL/LGPL code was read.
//!
//! * `relax`: spring-mass relaxation of a triangle mesh (edge springs, gravity, pinned
//!   points, tension-only cables, floor and mesh collision). Used for spring relaxation,
//!   hanging-chain form finding and draping.
//! * `soap_film`: area minimisation with pinned boundary (soap film / minimal surface).
//! * `disk_from_curve`: a triangulated disc spanning a closed curve, to feed `soap_film`.
#![allow(clippy::needless_range_loop)]
pub mod ccd;

use crate::engine::cancelled;
use crate::mesh_tools::{
    P, Stop, add, boundary_flags, cross, dot, edge_map, len, mul, raw_check, sub, tick,
    vertex_normals,
};
use crate::model::Mesh;

pub const MAX_PHYSICS_VERTICES: usize = 60_000;
pub const MAX_ITERATIONS: usize = 5_000;
const WORK_BUDGET: usize = 300_000_000;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub iterations: usize,
    pub stiffness: f64,
    pub rest_scale: f64,
    /// Downward force on each free vertex.
    pub gravity: f64,
    pub tension_only: bool,
    pub pin_boundary: bool,
    pub floor: Option<f64>,
    pub margin: f64,
}

fn at(v: &[P], i: usize) -> P {
    v.get(i).copied().unwrap_or([0.; 3])
}
fn finite(p: P) -> bool {
    p.iter().all(|x| x.is_finite() && x.abs() <= 1e9)
}
fn extent(v: &[P]) -> f64 {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in v {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if v.is_empty() {
        return 1.;
    }
    len(sub(hi, lo)).max(1e-9)
}
fn check_size(m: &Mesh, iterations: usize, edges: usize) -> Result<(), String> {
    if m.vertices.len() > MAX_PHYSICS_VERTICES {
        return Err(
            "物理計算の頂点数が上限（6万）を超えています。メッシュを粗くしてください。".into(),
        );
    }
    if iterations == 0 || iterations > MAX_ITERATIONS {
        return Err("反復回数は 1〜5000 にしてください。".into());
    }
    if iterations.saturating_mul(edges + m.vertices.len()) > WORK_BUDGET {
        return Err(
            "反復回数×メッシュの大きさが大きすぎます。反復かメッシュを減らしてください。".into(),
        );
    }
    Ok(())
}
/// Pinned flags from the boundary and anchor points. An anchor pins its nearest vertex
/// and moves it onto the anchor point.
fn pins(
    v: &mut [P],
    boundary: &[bool],
    pin_boundary: bool,
    anchors: &[P],
    stop: Stop,
) -> Result<Vec<bool>, String> {
    let mut pinned = if pin_boundary {
        boundary.to_vec()
    } else {
        vec![false; v.len()]
    };
    if anchors.len().saturating_mul(v.len()) > 100_000_000 {
        return Err("固定点が多すぎます。".into());
    }
    for (k, a) in anchors.iter().enumerate() {
        tick(k, stop)?;
        if !finite(*a) {
            return Err("固定点の座標が無効です。".into());
        }
        let mut best: Option<(f64, usize)> = None;
        for (i, p) in v.iter().enumerate() {
            let d = len(sub(*p, *a));
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        if let Some((_, i)) = best {
            v[i] = *a;
            pinned[i] = true;
        }
    }
    Ok(pinned)
}

/// Dynamic relaxation. Returns the relaxed mesh (same triangles).
pub fn relax(
    m: &Mesh,
    anchors: &[P],
    cfg: &Config,
    collider: Option<&Mesh>,
    stop: Stop,
) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if m.triangles.is_empty() {
        return Err("面のないメッシュです。".into());
    }
    if !(cfg.stiffness.is_finite() && cfg.stiffness > 0.)
        || !cfg.rest_scale.is_finite()
        || cfg.rest_scale < 0.
        || !cfg.gravity.is_finite()
        || !cfg.margin.is_finite()
        || cfg.margin < 0.
        || cfg
            .floor
            .is_some_and(|z| !z.is_finite() || (z + cfg.margin).abs() > 1e9)
    {
        return Err("物理の設定が範囲外です。".into());
    }
    let edge_table = edge_map(&m.triangles, stop)?;
    check_size(m, cfg.iterations, edge_table.len())?;
    let boundary = boundary_flags(m.vertices.len(), &edge_table);
    let mut x = m.vertices.clone();
    let springs: Vec<(usize, usize, f64)> = edge_table
        .keys()
        .map(|&(a, b)| {
            let l = len(sub(at(&x, a as usize), at(&x, b as usize)));
            (a as usize, b as usize, l * cfg.rest_scale)
        })
        .collect();
    let pinned = pins(&mut x, &boundary, cfg.pin_boundary, anchors, stop)?;
    let mut degree = vec![0usize; x.len()];
    for &(a, b, _) in &springs {
        degree[a] += 1;
        degree[b] += 1;
    }
    let mass: Vec<f64> = degree
        .iter()
        .map(|&d| cfg.stiffness * d.max(1) as f64)
        .collect();
    let collider = match collider {
        Some(c) => {
            if cfg.margin <= 0. {
                return Err("CCD のすき間は正の値にしてください。".into());
            }
            Some(ccd::StaticMesh::new(c, stop)?)
        }
        None => None,
    };
    let mut collision_budget = ccd::Budget::new(WORK_BUDGET);
    let scale = extent(&x);
    let max_step = scale * 0.05;
    let tol = scale * 1e-8;
    let dt = 0.9;
    let mut vel = vec![[0.; 3]; x.len()];
    let mut force = vec![[0.; 3]; x.len()];
    let mut prev_energy = f64::MAX;
    let mut calm = 0;
    for _ in 0..cfg.iterations {
        cancelled(stop)?;
        force.fill([0.; 3]);
        for &(a, b, rest) in &springs {
            let d = sub(x[b], x[a]);
            let l = len(d);
            if l < 1e-12 {
                continue;
            }
            let ext = l - rest;
            if cfg.tension_only && ext < 0. {
                continue;
            }
            let f = mul(d, cfg.stiffness * ext / l);
            force[a] = add(force[a], f);
            force[b] = sub(force[b], f);
        }
        let mut energy = 0.;
        for i in 0..x.len() {
            if pinned[i] {
                continue;
            }
            force[i][2] -= cfg.gravity;
            let acc = mul(force[i], 1. / mass[i]);
            vel[i] = add(vel[i], mul(acc, dt));
            energy += mass[i] * dot(vel[i], vel[i]);
        }
        if energy < prev_energy && prev_energy != f64::MAX {
            // Kinetic damping: the energy peak has passed, so restart from rest.
            vel.fill([0.; 3]);
        }
        prev_energy = energy;
        let mut moved = 0f64;
        for i in 0..x.len() {
            if pinned[i] {
                continue;
            }
            let mut step = mul(vel[i], dt);
            let l = len(step);
            if l > max_step {
                step = mul(step, max_step / l);
                vel[i] = mul(step, 1. / dt);
            }
            let start = x[i];
            let mut target = add(start, step);
            let mut floor_hit = None;
            if let Some(z0) = cfg.floor {
                let floor = z0 + cfg.margin;
                if collider.is_some() && start[2] < floor {
                    return Err("CCD: 頂点が床の下から開始しています。".into());
                }
                if target[2] < floor {
                    if collider.is_some() {
                        let t = (floor - start[2]) / (target[2] - start[2]);
                        target = add(start, mul(step, t));
                    }
                    target[2] = floor;
                    floor_hit = Some(floor);
                }
            }
            if let Some(c) = &collider
                && let Some(hit) =
                    c.sweep(start, target, cfg.margin, &mut collision_budget, stop)?
            {
                target = add(start, mul(sub(target, start), hit.toi));
                // Inelastic, frictionless first contact. The unspent displacement
                // is not applied without another sweep; relaxation resumes next iteration.
                let vn = dot(vel[i], hit.normal);
                if vn < 0. {
                    vel[i] = sub(vel[i], mul(hit.normal, vn));
                }
            }
            if floor_hit.is_some_and(|floor| target[2] <= floor) {
                vel[i][2] = vel[i][2].max(0.);
            }
            x[i] = target;
            moved = moved.max(len(sub(target, start)));
        }
        if x.iter().any(|p| !finite(*p)) {
            return Err("計算が発散しました。荷重を下げるか反復を減らしてください。".into());
        }
        if moved < tol {
            calm += 1;
            if calm >= 5 {
                break;
            }
        } else {
            calm = 0;
        }
    }
    Ok(Mesh {
        vertices: x,
        triangles: m.triangles.clone(),
    })
}

/// Mirror the mesh in the plane z = `top` and keep the faces facing the same way
/// (hanging chain model turned upside down: a pure compression shell).
pub fn mirror_z(m: &Mesh, top: f64) -> Mesh {
    Mesh {
        vertices: m
            .vertices
            .iter()
            .map(|p| [p[0], p[1], 2. * top - p[2]])
            .collect(),
        triangles: m.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect(),
    }
}
/// The highest z of the pinned vertices (or of all vertices when nothing is pinned).
pub fn support_height(
    m: &Mesh,
    anchors: &[P],
    pin_boundary: bool,
    stop: Stop,
) -> Result<f64, String> {
    let edges = edge_map(&m.triangles, stop)?;
    let boundary = boundary_flags(m.vertices.len(), &edges);
    let mut top = f64::MIN;
    if pin_boundary {
        for (i, p) in m.vertices.iter().enumerate() {
            if boundary[i] {
                top = top.max(p[2]);
            }
        }
    }
    for a in anchors {
        top = top.max(a[2]);
    }
    if top == f64::MIN {
        top = m.vertices.iter().map(|p| p[2]).fold(f64::MIN, f64::max);
    }
    Ok(top)
}

/// Area minimisation by cotangent-weighted averaging (discrete mean-curvature flow with a
/// convex update, so it cannot blow up). `pressure` pushes along the normals (a bubble).
pub fn soap_film(
    m: &Mesh,
    anchors: &[P],
    iterations: usize,
    strength: f64,
    pressure: f64,
    pin_boundary: bool,
    stop: Stop,
) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if m.triangles.is_empty() {
        return Err("面のないメッシュです。".into());
    }
    if !(0.01..=1.).contains(&strength) || !pressure.is_finite() {
        return Err("ソープフィルムの設定が範囲外です。".into());
    }
    let edge_table = edge_map(&m.triangles, stop)?;
    check_size(m, iterations, edge_table.len())?;
    let boundary = boundary_flags(m.vertices.len(), &edge_table);
    let mut x = m.vertices.clone();
    let pinned = pins(&mut x, &boundary, pin_boundary, anchors, stop)?;
    if !pinned.iter().any(|&p| p) {
        return Err("固定点か「境界を固定」が必要です。".into());
    }
    let ids: Vec<(u32, u32)> = edge_table.keys().copied().collect();
    let index = |a: u32, b: u32| ids.binary_search(&(a.min(b), a.max(b))).ok();
    let mut tri_edges: Vec<[usize; 3]> = Vec::with_capacity(m.triangles.len());
    for t in &m.triangles {
        let (Some(e0), Some(e1), Some(e2)) =
            (index(t[0], t[1]), index(t[1], t[2]), index(t[2], t[0]))
        else {
            return Err("辺の索引が壊れています。".into());
        };
        tri_edges.push([e0, e1, e2]);
    }
    let scale = extent(&x);
    let tol = scale * 1e-9;
    let mut weight = vec![0f64; ids.len()];
    let mut calm = 0;
    for _ in 0..iterations {
        cancelled(stop)?;
        weight.fill(0.);
        for (t, te) in m.triangles.iter().zip(&tri_edges) {
            // The cotangent of the corner opposite an edge weights that edge.
            for k in 0..3 {
                let (p, q, r) = (
                    at(&x, t[k] as usize),
                    at(&x, t[(k + 1) % 3] as usize),
                    at(&x, t[(k + 2) % 3] as usize),
                );
                let (u, w) = (sub(q, p), sub(r, p));
                let sin = len(cross(u, w));
                let cot = if sin > 1e-14 { dot(u, w) / sin } else { 0. };
                // edge (k+1, k+2) is opposite corner k
                weight[te[(k + 1) % 3]] += 0.5 * cot;
            }
        }
        let normals = if pressure != 0. {
            vertex_normals(&x, &m.triangles)
        } else {
            Vec::new()
        };
        let mut sum_w = vec![0f64; x.len()];
        let mut sum_p = vec![[0.; 3]; x.len()];
        for (k, &(a, b)) in ids.iter().enumerate() {
            let w = weight[k].clamp(1e-3, 50.);
            let (a, b) = (a as usize, b as usize);
            sum_w[a] += w;
            sum_w[b] += w;
            sum_p[a] = add(sum_p[a], mul(x[b], w));
            sum_p[b] = add(sum_p[b], mul(x[a], w));
        }
        let mut next = x.clone();
        let mut moved = 0f64;
        for i in 0..x.len() {
            if pinned[i] || sum_w[i] <= 0. {
                continue;
            }
            let avg = mul(sum_p[i], 1. / sum_w[i]);
            let mut q = add(x[i], mul(sub(avg, x[i]), strength));
            if let Some(n) = normals.get(i) {
                q = add(q, mul(*n, pressure));
            }
            moved = moved.max(len(sub(q, x[i])));
            next[i] = q;
        }
        x = next;
        if x.iter().any(|p| !finite(*p)) {
            return Err("計算が発散しました。膨らみを下げてください。".into());
        }
        if moved < tol {
            calm += 1;
            if calm >= 5 {
                break;
            }
        } else {
            calm = 0;
        }
    }
    Ok(Mesh {
        vertices: x,
        triangles: m.triangles.clone(),
    })
}

/// A disc of `rings` rings spanning a closed polyline (fan around the centroid).
/// The outer ring is the curve itself; it is the boundary that `soap_film` pins.
pub fn disk_from_curve(points: &[P], rings: usize, stop: Stop) -> Result<Mesh, String> {
    let mut pts: Vec<P> = points.to_vec();
    if pts.len() > 1 && len(sub(pts[0], pts[pts.len() - 1])) < 1e-9 {
        pts.pop();
    }
    if pts.len() > 400 {
        let stride = pts.len().div_ceil(400);
        pts = pts.into_iter().step_by(stride).collect();
    }
    let n = pts.len();
    if n < 3 || !(2..=64).contains(&rings) {
        return Err("3 点以上の閉じた曲線と、2〜64 段の分割が必要です。".into());
    }
    if pts.iter().any(|p| !finite(*p)) {
        return Err("曲線の座標が無効です。".into());
    }
    if n * rings > 30_000 {
        return Err("曲線の点数×段数が多すぎます。段数を減らしてください。".into());
    }
    let centre = mul(pts.iter().fold([0.; 3], |s, p| add(s, *p)), 1. / n as f64);
    let mut vertices = vec![centre];
    for k in 1..=rings {
        cancelled(stop)?;
        let t = k as f64 / rings as f64;
        for p in &pts {
            vertices.push(add(centre, mul(sub(*p, centre), t)));
        }
    }
    let ring = |k: usize, j: usize| (1 + (k - 1) * n + j % n) as u32;
    let mut triangles = Vec::new();
    for j in 0..n {
        triangles.push([0, ring(1, j), ring(1, j + 1)]);
    }
    for k in 1..rings {
        for j in 0..n {
            let (a, b, c, d) = (
                ring(k, j),
                ring(k, j + 1),
                ring(k + 1, j),
                ring(k + 1, j + 1),
            );
            triangles.push([a, c, d]);
            triangles.push([a, d, b]);
        }
    }
    Ok(Mesh {
        vertices,
        triangles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, s: f64) -> Mesh {
        let w = (n + 1) as u32;
        let mut vertices = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                vertices.push([i as f64 * s, j as f64 * s, 0.]);
            }
        }
        let mut triangles = Vec::new();
        for j in 0..n as u32 {
            for i in 0..n as u32 {
                let a = j * w + i;
                triangles.push([a, a + 1, a + w + 1]);
                triangles.push([a, a + w + 1, a + w]);
            }
        }
        Mesh {
            vertices,
            triangles,
        }
    }
    fn area(m: &Mesh) -> f64 {
        m.triangles
            .iter()
            .map(|t| {
                let p = |i: u32| m.vertices[i as usize];
                0.5 * len(cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0]))))
            })
            .sum()
    }
    const GO: &dyn Fn() -> bool = &|| false;
    fn config() -> Config {
        Config {
            iterations: 600,
            stiffness: 1.,
            rest_scale: 0.,
            gravity: 1.,
            tension_only: true,
            pin_boundary: true,
            floor: None,
            margin: 0.1,
        }
    }

    #[test]
    fn a_pinned_net_sags_under_gravity_and_mirrors_into_a_shell() {
        let g = grid(10, 1.);
        let hung = relax(&g, &[], &config(), None, GO).unwrap();
        let low = hung.vertices.iter().map(|p| p[2]).fold(f64::MAX, f64::min);
        assert!(low < -0.5, "sagged to {low}");
        assert!(
            hung.vertices
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite()))
        );
        // Boundary vertices did not move.
        assert_eq!(hung.vertices[0], g.vertices[0]);
        let top = support_height(&g, &[], true, GO).unwrap();
        let shell = mirror_z(&hung, top);
        let high = shell.vertices.iter().map(|p| p[2]).fold(f64::MIN, f64::max);
        assert!((high + low).abs() < 1e-9);
    }
    #[test]
    fn the_floor_stops_a_falling_mesh() {
        let mut cfg = config();
        cfg.pin_boundary = false;
        cfg.rest_scale = 1.;
        cfg.tension_only = false;
        cfg.floor = Some(-2.);
        let mut g = grid(6, 1.);
        for p in &mut g.vertices {
            p[2] = 3.;
        }
        let m = relax(&g, &[], &cfg, None, GO).unwrap();
        assert!(m.vertices.iter().all(|p| p[2] >= -2. + cfg.margin - 1e-9));
    }
    #[test]
    fn a_collider_holds_the_mesh_up() {
        let mut cfg = config();
        cfg.pin_boundary = false;
        cfg.rest_scale = 1.;
        cfg.tension_only = false;
        cfg.gravity = 0.3;
        let mut cloth = grid(8, 1.);
        for p in &mut cloth.vertices {
            p[2] = 2.;
        }
        let table = grid(8, 1.);
        let m = relax(&cloth, &[], &cfg, Some(&table), GO).unwrap();
        // The swept sphere stays above the two-sided table by its margin.
        assert!(m.vertices.iter().all(|p| p[2] >= cfg.margin - 1e-8));
    }
    #[test]
    fn soap_film_reduces_area_and_keeps_the_rim() {
        let mut g = grid(12, 1.);
        for (i, p) in g.vertices.iter_mut().enumerate() {
            if p[0] > 0. && p[0] < 12. && p[1] > 0. && p[1] < 12. {
                p[2] = ((i * 7) % 5) as f64 * 0.5;
            }
        }
        let before = area(&g);
        let s = soap_film(&g, &[], 300, 0.5, 0., true, GO).unwrap();
        assert!(area(&s) < before);
        assert_eq!(s.vertices[0], g.vertices[0]);
        assert!(soap_film(&g, &[], 10, 0.5, 0., false, GO).is_err());
        assert!(soap_film(&g, &[], 0, 0.5, 0., true, GO).is_err());
    }
    #[test]
    fn pressure_inflates_the_film() {
        let g = grid(8, 1.);
        let s = soap_film(&g, &[], 100, 0.5, 0.05, true, GO).unwrap();
        let hi = s.vertices.iter().map(|p| p[2].abs()).fold(0., f64::max);
        assert!(hi > 0.1);
    }
    #[test]
    fn a_disc_over_a_wavy_curve_becomes_a_minimal_surface() {
        let ring: Vec<[f64; 3]> = (0..48)
            .map(|i| {
                let a = i as f64 / 48. * std::f64::consts::TAU;
                [a.cos() * 10., a.sin() * 10., (2. * a).sin() * 2.]
            })
            .collect();
        let d = disk_from_curve(&ring, 8, GO).unwrap();
        let s = soap_film(&d, &[], 400, 0.5, 0., true, GO).unwrap();
        assert!(area(&s) <= area(&d) + 1e-9);
        assert!(disk_from_curve(&ring[..2], 8, GO).is_err());
    }
    #[test]
    fn limits_and_cancel_are_enforced() {
        let g = grid(4, 1.);
        let mut cfg = config();
        cfg.iterations = 0;
        assert!(relax(&g, &[], &cfg, None, GO).is_err());
        cfg.iterations = 10;
        assert!(relax(&g, &[], &cfg, None, &|| true).is_err());
        cfg.stiffness = f64::NAN;
        assert!(relax(&g, &[], &cfg, None, GO).is_err());
    }
}
