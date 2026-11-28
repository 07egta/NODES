//! Mesh editing tools (independent NODES code, written from the textbook definitions;
//! no GPL/LGPL code was read). Loop subdivision, face splitting, Taubin smoothing,
//! isotropic remeshing, hole filling, offsetting and thickening of indexed triangle meshes.
//!
//! Every function checks the input, honours `stop` (cancel) and refuses results larger
//! than the model limits. Nothing here panics on hostile meshes.
#![allow(clippy::needless_range_loop)]
use crate::engine::cancelled;
use crate::model::{MAX_TRIANGLES, MAX_VERTICES, Mesh};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub type P = [f64; 3];
pub type Stop<'a> = &'a dyn Fn() -> bool;
type Tri = [u32; 3];
type EdgeMap = BTreeMap<(u32, u32), Vec<EdgeUse>>;

pub(crate) fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn mul(a: P, s: f64) -> P {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: P, b: P) -> P {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn len(a: P) -> f64 {
    dot(a, a).sqrt()
}
pub(crate) fn unit(a: P) -> Option<P> {
    let l = len(a);
    (l.is_finite() && l > 1e-300).then(|| mul(a, 1. / l))
}
fn mid(a: P, b: P) -> P {
    mul(add(a, b), 0.5)
}
fn key(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}
fn at(v: &[P], i: u32) -> P {
    v.get(i as usize).copied().unwrap_or([0.; 3])
}
pub(crate) fn tick(i: usize, stop: Stop) -> Result<(), String> {
    if i.is_multiple_of(256) {
        cancelled(stop)?;
    }
    Ok(())
}
fn finite(p: P) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
}

/// One use of an edge by a triangle: the directed edge `from -> to` and the third corner.
#[derive(Clone, Copy, Debug)]
pub struct EdgeUse {
    pub face: usize,
    pub from: u32,
    pub to: u32,
    pub opposite: u32,
}

/// Size, index and coordinate check of a hostile input mesh.
pub fn raw_check(m: &Mesh, stop: Stop) -> Result<(), String> {
    if m.vertices.len() > MAX_VERTICES || m.triangles.len() > MAX_TRIANGLES {
        return Err("メッシュが上限を超えています。".into());
    }
    for (i, p) in m.vertices.iter().enumerate() {
        tick(i, stop)?;
        if !finite(*p) {
            return Err("無効な頂点座標があります。".into());
        }
    }
    let n = m.vertices.len();
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        if t.iter().any(|&v| v as usize >= n) {
            return Err("面の頂点番号が範囲外です。".into());
        }
    }
    Ok(())
}
fn limits(vertices: usize, triangles: usize) -> Result<(), String> {
    if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
        Err("結果が大きすぎます。段数や細かさを減らしてください。".into())
    } else {
        Ok(())
    }
}

pub fn edge_map(tris: &[Tri], stop: Stop) -> Result<EdgeMap, String> {
    let mut edges = EdgeMap::new();
    for (f, t) in tris.iter().enumerate() {
        tick(f, stop)?;
        for k in 0..3 {
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            edges.entry(key(a, b)).or_default().push(EdgeUse {
                face: f,
                from: a,
                to: b,
                opposite: c,
            });
        }
    }
    Ok(edges)
}
/// Vertices on an edge that does not have exactly two faces.
pub fn boundary_flags(count: usize, edges: &EdgeMap) -> Vec<bool> {
    let mut flags = vec![false; count];
    for (&(a, b), uses) in edges {
        if uses.len() != 2 {
            for v in [a, b] {
                if let Some(f) = flags.get_mut(v as usize) {
                    *f = true;
                }
            }
        }
    }
    flags
}
pub fn neighbours(count: usize, edges: &EdgeMap) -> Vec<Vec<u32>> {
    let mut n = vec![Vec::new(); count];
    for &(a, b) in edges.keys() {
        if let Some(l) = n.get_mut(a as usize) {
            l.push(b);
        }
        if let Some(l) = n.get_mut(b as usize) {
            l.push(a);
        }
    }
    n
}
/// Area-weighted vertex normals (zero for vertices without a usable face).
pub fn vertex_normals(v: &[P], tris: &[Tri]) -> Vec<P> {
    let mut n = vec![[0.; 3]; v.len()];
    for t in tris {
        let c = cross(sub(at(v, t[1]), at(v, t[0])), sub(at(v, t[2]), at(v, t[0])));
        for &i in t {
            if let Some(s) = n.get_mut(i as usize) {
                *s = add(*s, c);
            }
        }
    }
    n.into_iter().map(|s| unit(s).unwrap_or([0.; 3])).collect()
}
fn tri_normal(v: &[P], t: Tri) -> P {
    cross(sub(at(v, t[1]), at(v, t[0])), sub(at(v, t[2]), at(v, t[0])))
}
fn edge_len(v: &[P], a: u32, b: u32) -> f64 {
    len(sub(at(v, a), at(v, b)))
}

// ---------------------------------------------------------------- Loop subdivision

/// Loop subdivision (Loop 1987): every triangle becomes four; interior vertices and edge
/// points use the standard masks. Boundary and non-manifold edges stay creases.
pub fn loop_subdivide(m: &Mesh, levels: usize, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if !(1..=4).contains(&levels) {
        return Err("Loop 細分化は 1〜4 段です。".into());
    }
    let mut cur = m.clone();
    for _ in 0..levels {
        cancelled(stop)?;
        cur = loop_level(&cur, stop)?;
    }
    Ok(cur)
}
fn loop_level(m: &Mesh, stop: Stop) -> Result<Mesh, String> {
    let edges = edge_map(&m.triangles, stop)?;
    let nv = m.vertices.len();
    limits(nv + edges.len(), m.triangles.len() * 4)?;
    let mut index: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut edge_pts: Vec<P> = Vec::with_capacity(edges.len());
    let mut ring: Vec<Vec<u32>> = vec![Vec::new(); nv];
    let mut crease: Vec<Vec<u32>> = vec![Vec::new(); nv];
    for (k, (&(a, b), uses)) in edges.iter().enumerate() {
        tick(k, stop)?;
        let (pa, pb) = (at(&m.vertices, a), at(&m.vertices, b));
        let p = match uses.as_slice() {
            [u, w] => add(
                mul(add(pa, pb), 3. / 8.),
                mul(
                    add(at(&m.vertices, u.opposite), at(&m.vertices, w.opposite)),
                    1. / 8.,
                ),
            ),
            _ => mid(pa, pb),
        };
        if let Some(r) = ring.get_mut(a as usize) {
            r.push(b);
        }
        if let Some(r) = ring.get_mut(b as usize) {
            r.push(a);
        }
        if uses.len() != 2 {
            if let Some(r) = crease.get_mut(a as usize) {
                r.push(b);
            }
            if let Some(r) = crease.get_mut(b as usize) {
                r.push(a);
            }
        }
        index.insert((a, b), (nv + edge_pts.len()) as u32);
        edge_pts.push(p);
    }
    let mut vertices: Vec<P> = Vec::with_capacity(nv + edge_pts.len());
    for i in 0..nv {
        tick(i, stop)?;
        let p = at(&m.vertices, i as u32);
        let (r, c) = (&ring[i], &crease[i]);
        let q = if r.is_empty() {
            p
        } else if c.is_empty() {
            let n = r.len() as f64;
            let beta = if r.len() == 3 {
                3. / 16.
            } else {
                let t = 3. / 8. + 0.25 * (std::f64::consts::TAU / n).cos();
                (5. / 8. - t * t) / n
            };
            let sum = r.iter().fold([0.; 3], |s, &j| add(s, at(&m.vertices, j)));
            add(mul(p, 1. - n * beta), mul(sum, beta))
        } else if let [c0, c1] = c.as_slice() {
            add(
                mul(p, 0.75),
                mul(add(at(&m.vertices, *c0), at(&m.vertices, *c1)), 0.125),
            )
        } else {
            p
        };
        vertices.push(q);
    }
    vertices.extend(edge_pts);
    let mut tris = Vec::with_capacity(m.triangles.len() * 4);
    for (i, &[a, b, c]) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        let get = |x: u32, y: u32| {
            index
                .get(&key(x, y))
                .copied()
                .ok_or("辺の索引が壊れています。")
        };
        let (ab, bc, ca) = (get(a, b)?, get(b, c)?, get(c, a)?);
        tris.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
    }
    Ok(Mesh {
        vertices,
        triangles: tris,
    })
}

// ---------------------------------------------------------------- face splitting

fn split_triangle(t: Tri, midpoints: &BTreeMap<(u32, u32), u32>, v: &[P], out: &mut Vec<Tri>) {
    let e = [key(t[0], t[1]), key(t[1], t[2]), key(t[2], t[0])];
    let mk = [
        midpoints.get(&e[0]).copied(),
        midpoints.get(&e[1]).copied(),
        midpoints.get(&e[2]).copied(),
    ];
    match mk.iter().flatten().count() {
        0 => out.push(t),
        1 => {
            let u = mk.iter().position(|m| m.is_some()).unwrap_or(0);
            let (a, b, c) = (t[u], t[(u + 1) % 3], t[(u + 2) % 3]);
            if let Some(m) = mk[u] {
                out.push([a, m, c]);
                out.push([m, b, c]);
            }
        }
        2 => {
            let u = mk.iter().position(|m| m.is_none()).unwrap_or(2);
            let s = (u + 1) % 3;
            let (a, b, c) = (t[s], t[(s + 1) % 3], t[(s + 2) % 3]);
            let (Some(m1), Some(m2)) = (mk[s], mk[(s + 1) % 3]) else {
                out.push(t);
                return;
            };
            out.push([m1, b, m2]);
            if len(sub(at(v, m1), at(v, c))) <= len(sub(at(v, a), at(v, m2))) {
                out.push([a, m1, c]);
                out.push([m1, m2, c]);
            } else {
                out.push([a, m1, m2]);
                out.push([a, m2, c]);
            }
        }
        _ => {
            if let [Some(m0), Some(m1), Some(m2)] = mk {
                let (a, b, c) = (t[0], t[1], t[2]);
                out.extend([[a, m0, m2], [m0, b, m1], [m2, m1, c], [m0, m1, m2]]);
            } else {
                out.push(t);
            }
        }
    }
}
/// Split every triangle into three around its centroid, `levels` times.
pub fn split_centroid(m: &Mesh, levels: usize, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if !(1..=4).contains(&levels) {
        return Err("面分割は 1〜4 段です。".into());
    }
    let mut cur = m.clone();
    for _ in 0..levels {
        cancelled(stop)?;
        limits(
            cur.vertices.len() + cur.triangles.len(),
            cur.triangles.len() * 3,
        )?;
        let mut tris = Vec::with_capacity(cur.triangles.len() * 3);
        for (i, &[a, b, c]) in cur.triangles.iter().enumerate() {
            tick(i, stop)?;
            let centre = cur.vertices.len() as u32;
            let p = mul(
                add(
                    add(at(&cur.vertices, a), at(&cur.vertices, b)),
                    at(&cur.vertices, c),
                ),
                1. / 3.,
            );
            cur.vertices.push(p);
            tris.extend([[a, b, centre], [b, c, centre], [c, a, centre]]);
        }
        cur.triangles = tris;
    }
    Ok(cur)
}
/// Bisect every edge longer than `max_len` (conforming, no T-junctions) until none is.
pub fn split_long_edges(m: &Mesh, max_len: f64, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if !(max_len.is_finite() && max_len > 0.) {
        return Err("最大辺長は正の数にしてください。".into());
    }
    let mut cur = m.clone();
    split_long(&mut cur.vertices, &mut cur.triangles, max_len, stop)?;
    Ok(cur)
}
fn split_long(v: &mut Vec<P>, f: &mut Vec<Tri>, max_len: f64, stop: Stop) -> Result<(), String> {
    for _ in 0..24 {
        cancelled(stop)?;
        let mut marked: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for (i, t) in f.iter().enumerate() {
            tick(i, stop)?;
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if edge_len(v, a, b) > max_len {
                    marked.entry(key(a, b)).or_insert(0);
                }
            }
        }
        if marked.is_empty() {
            return Ok(());
        }
        limits(v.len() + marked.len(), f.len().saturating_mul(4))?;
        for (&(a, b), id) in marked.iter_mut() {
            *id = v.len() as u32;
            let p = mid(at(v, a), at(v, b));
            v.push(p);
        }
        let mut out = Vec::with_capacity(f.len() * 2);
        for (i, &t) in f.iter().enumerate() {
            tick(i, stop)?;
            split_triangle(t, &marked, v, &mut out);
        }
        *f = out;
    }
    Err("辺の分割が収束しません。最大辺長を大きくしてください。".into())
}

// ---------------------------------------------------------------- Taubin smoothing

/// Taubin lambda|mu smoothing: a shrink step followed by an inflate step, so the shape
/// keeps its volume. `strength` is lambda; mu follows from a 0.1 pass-band frequency.
pub fn taubin(
    m: &Mesh,
    iterations: usize,
    strength: f64,
    fix_boundary: bool,
    stop: Stop,
) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if iterations == 0 || iterations > 200 || !(0.01..=0.95).contains(&strength) {
        return Err("平滑化の設定が範囲外です。".into());
    }
    let lambda = strength;
    let mu = 1. / (0.1 - 1. / lambda);
    let edges = edge_map(&m.triangles, stop)?;
    let nb = neighbours(m.vertices.len(), &edges);
    let fixed = if fix_boundary {
        boundary_flags(m.vertices.len(), &edges)
    } else {
        vec![false; m.vertices.len()]
    };
    // Adjacency and boundary flags own their data; the edge map is no longer needed.
    drop(edges);
    let mut v = m.vertices.clone();
    // Both buffers start with the original fixed/isolated vertices. Every other
    // vertex is overwritten on each pass, so neither buffer needs a full copy.
    let mut next = v.clone();
    for _ in 0..iterations {
        for factor in [lambda, mu] {
            cancelled(stop)?;
            for (i, ns) in nb.iter().enumerate() {
                tick(i, stop)?;
                if ns.is_empty() || fixed[i] {
                    continue;
                }
                let avg = mul(
                    ns.iter().fold([0.; 3], |s, &j| add(s, at(&v, j))),
                    1. / ns.len() as f64,
                );
                next[i] = add(v[i], mul(sub(avg, v[i]), factor));
            }
            std::mem::swap(&mut v, &mut next);
        }
    }
    drop(next);
    Ok(Mesh {
        vertices: v,
        triangles: m.triangles.clone(),
    })
}

// ---------------------------------------------------------------- spatial grid

pub fn closest_on_triangle(p: P, a: P, b: P, c: P) -> P {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0. && d2 <= 0. {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0. && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0. && d1 >= 0. && d3 <= 0. && d1 - d3 != 0. {
        return add(a, mul(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0. && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0. && d2 >= 0. && d6 <= 0. && d2 - d6 != 0. {
        return add(a, mul(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0. && d4 - d3 >= 0. && d5 - d6 >= 0. && (d4 - d3) + (d5 - d6) != 0. {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, mul(sub(c, b), w));
    }
    let sum = va + vb + vc;
    if sum.abs() < 1e-300 {
        return a;
    }
    add(a, add(mul(ab, vb / sum), mul(ac, vc / sum)))
}

/// Uniform grid over the triangles of a mesh, for nearest-point queries that only need to
/// look at the 3x3x3 block of cells around the query.
pub struct TriGrid {
    cell: f64,
    origin: P,
    cells: HashMap<(i64, i64, i64), Vec<u32>>,
    verts: Vec<P>,
    tris: Vec<Tri>,
}
/// A nearest-point hit: the point on the surface, the face normal and the distance.
pub struct Hit {
    pub point: P,
    pub normal: P,
    pub distance: f64,
}
impl TriGrid {
    pub fn new(m: &Mesh, min_cell: f64, stop: Stop) -> Result<Self, String> {
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for p in &m.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if m.vertices.is_empty() || m.triangles.is_empty() {
            return Err("空のメッシュです。".into());
        }
        let extent = (0..3).map(|k| hi[k] - lo[k]).fold(0., f64::max);
        let cell = min_cell.max(extent / 200.).max(1e-9);
        let mut grid = Self {
            cell,
            origin: lo,
            cells: HashMap::new(),
            verts: m.vertices.clone(),
            tris: m.triangles.clone(),
        };
        let mut inserted = 0usize;
        for (f, t) in m.triangles.iter().enumerate() {
            tick(f, stop)?;
            let ps = t.map(|i| at(&m.vertices, i));
            let (mut a, mut b) = ([i64::MAX; 3], [i64::MIN; 3]);
            for p in ps {
                let c = grid.cell_of(p);
                for k in 0..3 {
                    a[k] = a[k].min(c[k]);
                    b[k] = b[k].max(c[k]);
                }
            }
            for x in a[0]..=b[0] {
                for y in a[1]..=b[1] {
                    for z in a[2]..=b[2] {
                        inserted += 1;
                        if inserted > 4_000_000 {
                            return Err("衝突判定の格子が大きすぎます。".into());
                        }
                        grid.cells.entry((x, y, z)).or_default().push(f as u32);
                    }
                }
            }
        }
        Ok(grid)
    }
    fn cell_of(&self, p: P) -> [i64; 3] {
        let mut c = [0i64; 3];
        for k in 0..3 {
            c[k] = ((p[k] - self.origin[k]) / self.cell)
                .floor()
                .clamp(-1e12, 1e12) as i64;
        }
        c
    }
    pub fn cell_size(&self) -> f64 {
        self.cell
    }
    /// Nearest surface point among the triangles in the 3x3x3 cells around `p`.
    pub fn nearest(&self, p: P) -> Option<Hit> {
        let c = self.cell_of(p);
        let mut best: Option<Hit> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(list) = self.cells.get(&(c[0] + dx, c[1] + dy, c[2] + dz)) else {
                        continue;
                    };
                    for &f in list {
                        let Some(t) = self.tris.get(f as usize) else {
                            continue;
                        };
                        let [a, b, cc] = t.map(|i| at(&self.verts, i));
                        let q = closest_on_triangle(p, a, b, cc);
                        let d = len(sub(p, q));
                        if best.as_ref().is_none_or(|h| d < h.distance) {
                            let normal = unit(cross(sub(b, a), sub(cc, a))).unwrap_or([0., 0., 1.]);
                            best = Some(Hit {
                                point: q,
                                normal,
                                distance: d,
                            });
                        }
                    }
                }
            }
        }
        best
    }
}

// ---------------------------------------------------------------- isotropic remeshing

/// Isotropic remeshing toward `target` edge length (0 = the current mean edge length).
/// Splits long edges, collapses short ones, flips toward valence 6 and smooths inside the
/// tangent plane, projecting back onto the input surface. Boundary vertices stay in place.
pub fn remesh(m: &Mesh, target: f64, iterations: usize, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if m.triangles.is_empty() {
        return Err("面のないメッシュです。".into());
    }
    if iterations == 0 || iterations > 10 || !target.is_finite() || target < 0. {
        return Err("再メッシュの設定が範囲外です。".into());
    }
    let mut area = 0.;
    let mut total = 0.;
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        area += 0.5 * len(tri_normal(&m.vertices, *t));
        for k in 0..3 {
            total += edge_len(&m.vertices, t[k], t[(k + 1) % 3]);
        }
    }
    let mean = total / (3. * m.triangles.len() as f64);
    let l = if target > 0. { target } else { mean };
    if !(l.is_finite() && l > 1e-9) {
        return Err("辺の長さを決められません。".into());
    }
    if area / (0.433 * l * l) > MAX_TRIANGLES as f64 / 2. {
        return Err("辺の長さが短すぎて面が多くなりすぎます。長くしてください。".into());
    }
    // A cell far larger than the input's own edges would put every triangle in a few
    // cells and make each projection scan them all (quadratic time).
    let grid = TriGrid::new(m, (l * 2.).min(mean * 16.), stop)?;
    let (hi, lo) = (l * 4. / 3., l * 4. / 5.);
    let mut v = m.vertices.clone();
    let mut f = m.triangles.clone();
    for _ in 0..iterations {
        cancelled(stop)?;
        split_long(&mut v, &mut f, hi, stop)?;
        collapse_pass(&mut v, &mut f, lo, hi, stop)?;
        flip_pass(&v, &mut f, stop)?;
        smooth_pass(&mut v, &f, &grid, stop)?;
    }
    compact(&v, &f)
}
fn compact(v: &[P], f: &[Tri]) -> Result<Mesh, String> {
    let mut map = vec![u32::MAX; v.len()];
    let mut vertices = Vec::new();
    let mut triangles = Vec::with_capacity(f.len());
    for t in f {
        let mut n = [0u32; 3];
        for (k, &i) in t.iter().enumerate() {
            let slot = map.get_mut(i as usize).ok_or("頂点番号が範囲外です。")?;
            if *slot == u32::MAX {
                *slot = vertices.len() as u32;
                vertices.push(at(v, i));
            }
            n[k] = *slot;
        }
        triangles.push(n);
    }
    Ok(Mesh {
        vertices,
        triangles,
    })
}
fn collapse_pass(
    v: &mut [P],
    f: &mut Vec<Tri>,
    lo: f64,
    hi: f64,
    stop: Stop,
) -> Result<(), String> {
    let edges = edge_map(f, stop)?;
    let nb = neighbours(v.len(), &edges);
    let boundary = boundary_flags(v.len(), &edges);
    let mut vf: Vec<Vec<usize>> = vec![Vec::new(); v.len()];
    for (i, t) in f.iter().enumerate() {
        for &x in t {
            if let Some(l) = vf.get_mut(x as usize) {
                l.push(i);
            }
        }
    }
    let mut cands: Vec<(f64, u32, u32)> = Vec::new();
    for (&(a, b), uses) in &edges {
        let (ai, bi) = (a as usize, b as usize);
        if uses.len() == 2 && !boundary[ai] && !boundary[bi] {
            let l = edge_len(v, a, b);
            if l < lo {
                cands.push((l, a, b));
            }
        }
    }
    cands.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut touched = vec![false; v.len()];
    let mut remap: Vec<u32> = (0..v.len() as u32).collect();
    let mut dead = vec![false; f.len()];
    for (k, &(_, a, b)) in cands.iter().enumerate() {
        tick(k, stop)?;
        let (ai, bi) = (a as usize, b as usize);
        if touched[ai] || touched[bi] {
            continue;
        }
        let Some(uses) = edges.get(&(a, b)) else {
            continue;
        };
        let [u0, u1] = uses.as_slice() else {
            continue;
        };
        let (c, d) = (u0.opposite, u1.opposite);
        let common: Vec<u32> = nb[ai]
            .iter()
            .copied()
            .filter(|x| nb[bi].contains(x))
            .collect();
        if common.len() != 2 || !common.contains(&c) || !common.contains(&d) {
            continue;
        }
        let p = mid(v[ai], v[bi]);
        let mut ok = true;
        for &face in vf[ai].iter().chain(vf[bi].iter()) {
            if face == u0.face || face == u1.face {
                continue;
            }
            let old = f[face];
            let new = old.map(|x| if x == a || x == b { u32::MAX } else { x });
            let pos = |x: u32| if x == u32::MAX { p } else { at(v, x) };
            let n_old = tri_normal(v, old);
            let n_new = cross(sub(pos(new[1]), pos(new[0])), sub(pos(new[2]), pos(new[0])));
            if dot(n_old, n_new) <= 0.25 * len(n_old) * len(n_new) || len(n_new) < 1e-12 {
                ok = false;
                break;
            }
            for j in 0..3 {
                let (x, y) = (new[j], new[(j + 1) % 3]);
                if (x == u32::MAX || y == u32::MAX) && len(sub(pos(x), pos(y))) > hi {
                    ok = false;
                }
            }
            if !ok {
                break;
            }
        }
        if !ok {
            continue;
        }
        v[bi] = p;
        remap[ai] = b;
        dead[u0.face] = true;
        dead[u1.face] = true;
        for x in [a, b] {
            touched[x as usize] = true;
            for &n in &nb[x as usize] {
                touched[n as usize] = true;
            }
        }
    }
    let mut out = Vec::with_capacity(f.len());
    for (i, t) in f.iter().enumerate() {
        if !dead[i] {
            out.push(t.map(|x| remap[x as usize]));
        }
    }
    *f = out;
    Ok(())
}
fn flip_pass(v: &[P], f: &mut [Tri], stop: Stop) -> Result<(), String> {
    let edges = edge_map(f, stop)?;
    let boundary = boundary_flags(v.len(), &edges);
    let mut nb: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); v.len()];
    for &(a, b) in edges.keys() {
        nb[a as usize].insert(b);
        nb[b as usize].insert(a);
    }
    let target = |i: u32| if boundary[i as usize] { 4i64 } else { 6i64 };
    let mut used = vec![false; f.len()];
    for (k, uses) in edges.values().enumerate() {
        tick(k, stop)?;
        let [u0, u1] = uses.as_slice() else {
            continue;
        };
        if u0.from != u1.to || u0.to != u1.from || used[u0.face] || used[u1.face] {
            continue;
        }
        let (x, y, c, d) = (u0.from, u0.to, u0.opposite, u1.opposite);
        if c == d || nb[c as usize].contains(&d) {
            continue;
        }
        let val = |i: u32| nb[i as usize].len() as i64;
        let cost = |i: u32, delta: i64| (val(i) + delta - target(i)).pow(2);
        let before = cost(x, 0) + cost(y, 0) + cost(c, 0) + cost(d, 0);
        let after = cost(x, -1) + cost(y, -1) + cost(c, 1) + cost(d, 1);
        if after >= before {
            continue;
        }
        let (t1, t2) = ([x, d, c], [d, y, c]);
        let (o1, o2) = (tri_normal(v, f[u0.face]), tri_normal(v, f[u1.face]));
        let (n1, n2) = (tri_normal(v, t1), tri_normal(v, t2));
        let good = |n: P| len(n) > 1e-12 && dot(n, o1) > 0. && dot(n, o2) > 0.;
        if !good(n1) || !good(n2) {
            continue;
        }
        f[u0.face] = t1;
        f[u1.face] = t2;
        used[u0.face] = true;
        used[u1.face] = true;
        nb[x as usize].remove(&y);
        nb[y as usize].remove(&x);
        nb[c as usize].insert(d);
        nb[d as usize].insert(c);
    }
    Ok(())
}
fn smooth_pass(v: &mut [P], f: &[Tri], grid: &TriGrid, stop: Stop) -> Result<(), String> {
    let edges = edge_map(f, stop)?;
    let boundary = boundary_flags(v.len(), &edges);
    let nb = neighbours(v.len(), &edges);
    let normals = vertex_normals(v, f);
    let mut next = v.to_vec();
    for (i, ns) in nb.iter().enumerate() {
        tick(i, stop)?;
        if ns.is_empty() || boundary[i] {
            continue;
        }
        let avg = mul(
            ns.iter().fold([0.; 3], |s, &j| add(s, at(v, j))),
            1. / ns.len() as f64,
        );
        let delta = sub(avg, v[i]);
        let n = normals[i];
        let tangent = sub(delta, mul(n, dot(delta, n)));
        let q = add(v[i], mul(tangent, 0.5));
        next[i] = grid.nearest(q).map_or(q, |h| h.point);
    }
    v.copy_from_slice(&next);
    Ok(())
}

// ---------------------------------------------------------------- hole filling

/// Fill boundary loops of up to `max_edges` edges by ear clipping (works for concave holes).
pub fn fill_holes(m: &Mesh, max_edges: usize, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if !(3..=300).contains(&max_edges) {
        return Err("穴の最大辺数は 3〜300 です。".into());
    }
    let mut directed: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for t in &m.triangles {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    let mut next: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&(a, b), &n) in &directed {
        if n == 1 && !directed.contains_key(&(b, a)) {
            next.entry(a).or_default().push(b);
        }
    }
    let starts: Vec<(u32, u32)> = next
        .iter()
        .flat_map(|(&a, bs)| bs.iter().map(move |&b| (a, b)))
        .collect();
    let mut visited: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut out = m.clone();
    for (k, &(a, b)) in starts.iter().enumerate() {
        tick(k, stop)?;
        if visited.contains(&(a, b)) {
            continue;
        }
        visited.insert((a, b));
        let mut ring = vec![a];
        let mut cur = b;
        let mut closed = true;
        while cur != a {
            ring.push(cur);
            let nxt = next
                .get(&cur)
                .and_then(|l| l.iter().copied().find(|&x| !visited.contains(&(cur, x))))
                .or_else(|| {
                    next.get(&cur)
                        .and_then(|l| l.iter().copied().find(|&x| x == a))
                });
            let Some(nxt) = nxt else {
                closed = false;
                break;
            };
            visited.insert((cur, nxt));
            cur = nxt;
            if ring.len() > max_edges {
                closed = false;
                break;
            }
        }
        if !closed || ring.len() < 3 || ring.len() > max_edges {
            continue;
        }
        ring.reverse();
        let tris = ear_clip(&ring, &m.vertices);
        limits(out.vertices.len(), out.triangles.len() + tris.len())?;
        out.triangles.extend(tris);
    }
    Ok(out)
}
fn ear_clip(ring: &[u32], v: &[P]) -> Vec<Tri> {
    let mut normal = [0.; 3];
    for k in 0..ring.len() {
        let (p, q) = (at(v, ring[k]), at(v, ring[(k + 1) % ring.len()]));
        normal = add(
            normal,
            [
                (p[1] - q[1]) * (p[2] + q[2]),
                (p[2] - q[2]) * (p[0] + q[0]),
                (p[0] - q[0]) * (p[1] + q[1]),
            ],
        );
    }
    let mut idx = ring.to_vec();
    let mut out = Vec::new();
    while idx.len() > 3 {
        let n = idx.len();
        let mut best: Option<(f64, usize)> = None;
        for i in 0..n {
            let (a, b, c) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (pa, pb, pc) = (at(v, a), at(v, b), at(v, c));
            let turn = dot(cross(sub(pb, pa), sub(pc, pb)), normal);
            if turn <= 1e-12 {
                continue;
            }
            let inside = idx.iter().any(|&o| {
                if o == a || o == b || o == c {
                    return false;
                }
                let p = at(v, o);
                dot(cross(sub(pb, pa), sub(p, pa)), normal) > 0.
                    && dot(cross(sub(pc, pb), sub(p, pb)), normal) > 0.
                    && dot(cross(sub(pa, pc), sub(p, pc)), normal) > 0.
            });
            if inside {
                continue;
            }
            let diag = len(sub(pc, pa));
            if best.is_none_or(|(d, _)| diag < d) {
                best = Some((diag, i));
            }
        }
        let Some((_, i)) = best else {
            for j in 1..n - 1 {
                out.push([idx[0], idx[j], idx[j + 1]]);
            }
            return out;
        };
        out.push([idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]]);
        idx.remove(i);
    }
    if let [a, b, c] = idx[..] {
        out.push([a, b, c]);
    }
    out
}

// ---------------------------------------------------------------- offset and thickening

/// Move every vertex along its normal by `distance` (topology unchanged).
pub fn offset(m: &Mesh, distance: f64, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if !distance.is_finite() {
        return Err("距離が無効です。".into());
    }
    let n = vertex_normals(&m.vertices, &m.triangles);
    let vertices: Vec<P> = m
        .vertices
        .iter()
        .zip(&n)
        .map(|(p, n)| add(*p, mul(*n, distance)))
        .collect();
    let result = Mesh {
        vertices,
        triangles: m.triangles.clone(),
    };
    check_offset_faces(m, &result.vertices, stop)?;
    check_contacts(&result, stop)?;
    Ok(result)
}
/// Turn a surface into a solid shell. The outer skin is `outer` along the normal and the
/// inner skin `inner` (outer > inner); open edges get side walls. Closed meshes become
/// hollow shells with outward normals on the outer skin.
pub fn thicken(m: &Mesh, outer: f64, inner: f64, stop: Stop) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    if !(outer.is_finite() && inner.is_finite() && outer > inner) {
        return Err("厚みの設定が無効です。".into());
    }
    if m.triangles.is_empty() {
        return Err("面のないメッシュです。".into());
    }
    let nv = m.vertices.len();
    let edges = edge_map(&m.triangles, stop)?;
    if edges.values().any(|u| {
        u.len() > 2 || (u.len() == 2 && u[0].from == u[1].from)
    }) {
        return Err("厚み付けには向きの整った多様体メッシュが必要です。".into());
    }
    let open: Vec<&EdgeUse> = edges
        .values()
        .filter_map(|u| match u.as_slice() {
            [one] => Some(one),
            _ => None,
        })
        .collect();
    limits(nv * 2, m.triangles.len() * 2 + open.len() * 2)?;
    let n = vertex_normals(&m.vertices, &m.triangles);
    let mut vertices = Vec::with_capacity(nv * 2);
    vertices.extend(
        m.vertices
            .iter()
            .zip(&n)
            .map(|(p, n)| add(*p, mul(*n, outer))),
    );
    vertices.extend(
        m.vertices
            .iter()
            .zip(&n)
            .map(|(p, n)| add(*p, mul(*n, inner))),
    );
    check_offset_faces(m, &vertices[..nv], stop)?;
    check_offset_faces(m, &vertices[nv..], stop)?;
    let off = nv as u32;
    let mut triangles = m.triangles.clone();
    triangles.extend(
        m.triangles
            .iter()
            .map(|t| [t[0] + off, t[2] + off, t[1] + off]),
    );
    for u in open {
        let (a, b) = (u.from, u.to);
        triangles.push([b, a, a + off]);
        triangles.push([b, a + off, b + off]);
    }
    let result = Mesh {
        vertices,
        triangles,
    };
    raw_check(&result, stop)?;
    let audit = crate::mesh_fabrication::audit(&result, stop)?;
    if !audit.closed_oriented() || audit.duplicate_vertices != 0 || audit.isolated_vertices != 0 {
        return Err("厚み付け結果は閉じた有効な多様体ではありません。厚みを見直してください。".into());
    }
    check_contacts(&result, stop)?;
    Ok(result)
}

// Conservative local fold guard along the straight vertex interpolation.
// A positive projected area throughout the path is necessary for this guard,
// but is not a certificate of global injectivity or exact offset distance.
fn check_offset_faces(m: &Mesh, target: &[P], stop: Stop) -> Result<(), String> {
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        let [a, b, c] = t.map(|v| m.vertices[v as usize]);
        let [x, y, z] = t.map(|v| target[v as usize]);
        let e = sub(b, a);
        let f = sub(c, a);
        let de = sub(sub(y, x), e);
        let df = sub(sub(z, x), f);
        let n = unit(cross(e, f)).ok_or("退化面があるためオフセットできません。")?;
        let q0 = dot(n, cross(e, f));
        let q1 = dot(n, add(cross(de, f), cross(e, df)));
        let q2 = dot(n, cross(de, df));
        let mut minimum = q0.min(q0 + q1 + q2);
        if q2 > 0. {
            let t = (-q1 / (2. * q2)).clamp(0., 1.);
            minimum = minimum.min(q0 + t * (q1 + t * q2));
        }
        if !minimum.is_finite() || minimum <= q0 * 1e-10 {
            return Err("オフセット途中に面の反転または退化の可能性があります。厚みを見直してください。".into());
        }
    }
    Ok(())
}

fn check_contacts(m: &Mesh, stop: Stop) -> Result<(), String> {
    raw_check(m, stop)?;
    let contacts = crate::mesh_geometry_checks::nonadjacent_contacts(m, stop)?;
    if contacts != 0 {
        return Err(format!("オフセット結果に非隣接面の接触候補が {contacts} 件あります。厚みを見直してください。"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn grid(n: usize, s: f64) -> Mesh {
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
            .map(|t| 0.5 * len(tri_normal(&m.vertices, *t)))
            .sum()
    }
    fn no_degenerate(m: &Mesh) -> bool {
        m.triangles
            .iter()
            .all(|t| len(tri_normal(&m.vertices, *t)) > 1e-12)
    }
    const GO: &dyn Fn() -> bool = &|| false;

    #[test]
    fn loop_quadruples_faces_and_keeps_flat_shape_flat() {
        let g = grid(4, 1.);
        let l = loop_subdivide(&g, 1, GO).unwrap();
        assert_eq!(l.triangles.len(), 128);
        assert_eq!(l.vertices.len(), 25 + 56);
        assert!(l.vertices.iter().all(|p| p[2].abs() < 1e-12));
        // Boundary corners are pulled in a little by the boundary mask.
        assert!(area(&l) < 16. && area(&l) > 14.);
        assert!(no_degenerate(&l));
        assert!(loop_subdivide(&g, 0, GO).is_err());
        assert!(loop_subdivide(&g, 5, GO).is_err());
    }
    #[test]
    fn loop_shrinks_a_closed_tetrahedron_smoothly() {
        let t = Mesh {
            vertices: vec![[1., 1., 1.], [-1., -1., 1.], [-1., 1., -1.], [1., -1., -1.]],
            triangles: vec![[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]],
        };
        let l = loop_subdivide(&t, 2, GO).unwrap();
        assert_eq!(l.triangles.len(), 64);
        assert!(no_degenerate(&l));
        assert!(l.vertices.iter().all(|p| len(*p) < 1.75));
    }
    #[test]
    fn splitting_keeps_the_area() {
        let g = grid(4, 1.);
        let c = split_centroid(&g, 1, GO).unwrap();
        assert_eq!(c.triangles.len(), 96);
        assert!((area(&c) - 16.).abs() < 1e-9);
        let s = split_long_edges(&g, 0.8, GO).unwrap();
        assert!((area(&s) - 16.).abs() < 1e-9);
        assert!(no_degenerate(&s));
        for t in &s.triangles {
            for k in 0..3 {
                assert!(edge_len(&s.vertices, t[k], t[(k + 1) % 3]) <= 0.8 + 1e-9);
            }
        }
        assert!(split_long_edges(&g, 0., GO).is_err());
    }
    #[test]
    fn taubin_keeps_a_flat_sheet_and_its_boundary() {
        let mut g = grid(6, 1.);
        g.vertices[24][2] = 0.5;
        let t = taubin(&g, 20, 0.5, true, GO).unwrap();
        assert!(t.vertices[24][2].abs() < 0.5);
        assert_eq!(t.vertices[0], g.vertices[0]);
        assert_eq!(t.vertices[6], g.vertices[6]);
        assert!(taubin(&g, 0, 0.5, true, GO).is_err());
    }
    // Frozen pre-optimization loop: compare exact floating-point output, not a
    // loose geometric tolerance that could hide a change in summation order.
    fn taubin_reference(m: &Mesh, iterations: usize, strength: f64, fix_boundary: bool) -> Mesh {
        let edges = edge_map(&m.triangles, GO).unwrap();
        let nb = neighbours(m.vertices.len(), &edges);
        let fixed = if fix_boundary {
            boundary_flags(m.vertices.len(), &edges)
        } else {
            vec![false; m.vertices.len()]
        };
        let mut v = m.vertices.clone();
        for _ in 0..iterations {
            for factor in [strength, 1. / (0.1 - 1. / strength)] {
                let mut next = v.clone();
                for (i, ns) in nb.iter().enumerate() {
                    if ns.is_empty() || fixed[i] {
                        continue;
                    }
                    let avg = mul(
                        ns.iter().fold([0.; 3], |s, &j| add(s, at(&v, j))),
                        1. / ns.len() as f64,
                    );
                    next[i] = add(v[i], mul(sub(avg, v[i]), factor));
                }
                v = next;
            }
        }
        Mesh {
            vertices: v,
            triangles: m.triangles.clone(),
        }
    }

    #[test]
    fn taubin_reused_buffers_match_reference_bits() {
        let mut g = grid(8, 1.);
        g.vertices[0][2] = -0.0;
        g.vertices[40][2] = 0.75;
        g.vertices.push([17., -0.0, 3.]);
        for fix_boundary in [false, true] {
            for iterations in [1, 2, 20, 200] {
                for strength in [0.01, 0.5, 0.95] {
                    let expected = taubin_reference(&g, iterations, strength, fix_boundary);
                    let actual = taubin(&g, iterations, strength, fix_boundary, GO).unwrap();
                    assert_eq!(actual.triangles, expected.triangles);
                    assert_eq!(actual.vertices.len(), expected.vertices.len());
                    for (a, b) in actual.vertices.iter().zip(&expected.vertices) {
                        assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits));
                    }
                }
            }
        }
    }

    #[test]
    fn taubin_cancels_during_smoothing() {
        use std::cell::Cell;
        let g = grid(8, 1.);
        let calls = Cell::new(0usize);
        // raw_check: 2 calls; edge_map: 1 call; each smoothing pass: 2.
        let result = taubin(&g, 20, 0.5, true, &|| {
            calls.set(calls.get() + 1);
            calls.get() >= 6
        });
        assert!(result.is_err());
        assert_eq!(calls.get(), 6);
    }

    #[test]
    fn fill_holes_restores_a_missing_triangle_and_ear_clips_a_ring() {
        let g = grid(4, 1.);
        let mut h = g.clone();
        h.triangles.remove(5);
        let f = fill_holes(&h, 10, GO).unwrap();
        assert_eq!(f.triangles.len(), g.triangles.len());
        assert!((area(&f) - 16.).abs() < 1e-9);
        // A bigger hole: remove every face touching the centre vertex (12).
        let mut big = g.clone();
        big.triangles.retain(|t| !t.contains(&12));
        let removed = g.triangles.len() - big.triangles.len();
        let filled = fill_holes(&big, 10, GO).unwrap();
        assert_eq!(filled.triangles.len(), big.triangles.len() + removed - 2);
        assert!(no_degenerate(&filled));
        // Holes above the size limit stay open.
        assert_eq!(
            fill_holes(&big, 3, GO).unwrap().triangles.len(),
            big.triangles.len()
        );
    }
    #[test]
    fn thicken_builds_walls_and_offset_moves_along_normals() {
        let g = grid(4, 1.);
        let t = thicken(&g, 0.5, -0.5, GO).unwrap();
        assert_eq!(t.vertices.len(), 50);
        assert_eq!(t.triangles.len(), 64 + 32);
        assert!(no_degenerate(&t));
        let o = offset(&g, 2., GO).unwrap();
        assert!(o.vertices.iter().all(|p| (p[2].abs() - 2.).abs() < 1e-9));
        assert!(thicken(&g, 0., 0., GO).is_err());
    }
    #[test]
    fn remesh_keeps_a_flat_sheet_flat_and_its_area() {
        let g = grid(8, 1.);
        let r = remesh(&g, 1.5, 3, GO).unwrap();
        assert!(no_degenerate(&r));
        assert!((area(&r) - 64.).abs() < 0.64);
        assert!(r.vertices.iter().all(|p| p[2].abs() < 1e-6));
        assert!(remesh(&g, -1., 3, GO).is_err());
    }
    #[test]
    fn cancel_and_hostile_input_return_errors() {
        let g = grid(4, 1.);
        assert!(loop_subdivide(&g, 1, &|| true).is_err());
        let bad = Mesh {
            vertices: vec![[0.; 3], [1., 0., 0.]],
            triangles: vec![[0, 1, 9]],
        };
        assert!(loop_subdivide(&bad, 1, GO).is_err());
        assert!(remesh(&bad, 1., 1, GO).is_err());
        assert!(fill_holes(&bad, 10, GO).is_err());
    }
    #[test]
    fn offset_rejects_collapsed_or_crossing_shells() {
        let m = Mesh {
            vertices: vec![
                [0., 0., 0.], [2., 0., 0.], [0., 2., 0.],
                [0., 0., 0.5], [2., 0., 0.5], [0., 2., 0.5],
            ],
            triangles: vec![[0, 1, 2], [3, 5, 4]],
        };
        assert!(thicken(&m, 0.1, -0.1, GO).is_ok());
        assert!(thicken(&m, 0.25, -0.25, GO).is_err());
        let mut reversed = grid(1, 1.);
        reversed.vertices.swap(0, 1);
        let original = grid(1, 1.);
        assert!(check_offset_faces(&original, &reversed.vertices, GO).is_err());
    }

}
