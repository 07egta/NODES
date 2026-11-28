//! SubD / T-spline control cages (independent NODES code).
//!
//! A cage is a polygon mesh of control points. Two kinds:
//! - **Grid cages** (made by 円柱ケージ): `cols × rows` points, closed around (u periodic) and
//!   open along the height. Their limit surface is defined *exactly* as the uniform bicubic
//!   B-spline in u (periodic) and the clamped uniform cubic B-spline in v, so the preview,
//!   refinement and the NURBS/B-rep conversion all describe the same surface. Refinement is
//!   exact knot insertion (the T-spline/NURBS refine of SolidCraft's `sc-geom` is used for v).
//! - **General cages** (箱ケージ, or any cage after 面の押し出し / 折り目): Catmull-Clark
//!   subdivision with sharp creases. They convert to a B-rep of bicubic NURBS patches, one per
//!   quad, by Loop & Schaefer's approximate Catmull-Clark (ACC, 2008): exact where the cage is
//!   regular, a close approximation of the limit surface around extraordinary points, and
//!   watertight everywhere (neighbouring patches share their boundary control points exactly).
//!
//! Cages are edited by a list of operations ([`EditOp`]) kept in the ケージ編集 node; the
//! viewport's edit mode appends to it (move, extrude, crease, subdivide, with symmetry).
use crate::kernel_bridge::{self as kb, BrepValue, R, Stop};
use crate::model::Mesh;
use serde::{Deserialize, Serialize};
use solidcraft_geom::Vec3;
use solidcraft_geom::bspline::{Dir, NurbsSurface};
use solidcraft_geom::tspline::TSpline;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

type P3 = [f64; 3];

pub const MAX_CAGE_VERTICES: usize = 50_000;
pub const MAX_PREVIEW_FACES: usize = 120_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cage {
    vertices: Vec<P3>,
    faces: Vec<Vec<u32>>,
    creases: BTreeSet<(u32, u32)>,
    grid: Option<Grid>,
    /// A T-spline patch cage: the control points are this T-spline's, in its vertex order, and
    /// the surface is the T-spline itself (exact, with T-junctions).
    ts: Option<Arc<TSpline>>,
    preview: Arc<Mesh>,
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: P3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn key(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

impl Cage {
    fn build(
        vertices: Vec<P3>,
        faces: Vec<Vec<u32>>,
        creases: BTreeSet<(u32, u32)>,
        grid: Option<Grid>,
        stop: Stop,
    ) -> R<Self> {
        if vertices.len() > MAX_CAGE_VERTICES {
            return Err(format!(
                "ケージの点が多すぎます（上限 {MAX_CAGE_VERTICES}）。分割回数を減らしてください。"
            ));
        }
        if vertices.iter().any(|p| {
            p.iter()
                .any(|v| !v.is_finite() || v.abs() > kb::limits::MAX_COORDINATE)
        }) {
            return Err("ケージに無効な座標があります。".into());
        }
        let n = vertices.len() as u32;
        if faces.is_empty()
            || faces
                .iter()
                .any(|f| f.len() < 3 || f.iter().any(|&i| i >= n))
        {
            return Err("ケージの面が正しくありません。".into());
        }
        let mut cage = Cage {
            vertices,
            faces,
            creases,
            grid,
            ts: None,
            preview: Arc::new(Mesh::default()),
        };
        let mesh = match grid {
            Some(g) => cage.grid_mesh(g, stop)?,
            None => cage.subdivided_mesh(stop)?,
        };
        cage.preview = Arc::new(mesh);
        Ok(cage)
    }

    /// A box cage centred on the XY origin standing on Z = 0 (8 points, 6 quads).
    pub fn box_cage(w: f64, d: f64, h: f64, stop: Stop) -> R<Self> {
        for (v, name) in [(w, "幅"), (d, "奥行"), (h, "高さ")] {
            if !(v.is_finite() && v > kb::tol::ABS && v <= kb::limits::MAX_COORDINATE) {
                return Err(format!("{name}は 0 より大きい値にしてください。"));
            }
        }
        let (x, y) = (w / 2., d / 2.);
        let v = vec![
            [-x, -y, 0.],
            [x, -y, 0.],
            [x, y, 0.],
            [-x, y, 0.],
            [-x, -y, h],
            [x, -y, h],
            [x, y, h],
            [-x, y, h],
        ];
        let f = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        Self::build(v, f, BTreeSet::new(), None, stop)
    }

    /// A cylinder grid cage: `segments` points around, `rings` rings from Z = 0 to `height`,
    /// the radius swelling by `bulge` (fraction of the radius) in the middle.
    pub fn cylinder_cage(
        radius: f64,
        height: f64,
        segments: usize,
        rings: usize,
        bulge: f64,
        stop: Stop,
    ) -> R<Self> {
        if !(radius.is_finite()
            && radius > kb::tol::ABS
            && height.is_finite()
            && height > kb::tol::ABS)
        {
            return Err("半径と高さは 0 より大きい値にしてください。".into());
        }
        if !(4..=256).contains(&segments) || !(4..=256).contains(&rings) {
            return Err("周方向の点数と段数は 4〜256 にしてください。".into());
        }
        if !bulge.is_finite() || bulge <= -0.95 || bulge > 10. {
            return Err("膨らみは -0.95 より大きく 10 以下にしてください。".into());
        }
        let mut v = Vec::with_capacity(segments * rings);
        for j in 0..rings {
            let t = j as f64 / (rings - 1) as f64;
            let r = radius * (1. + bulge * (std::f64::consts::PI * t).sin());
            for i in 0..segments {
                let a = std::f64::consts::TAU * i as f64 / segments as f64;
                v.push([r * a.cos(), r * a.sin(), height * t]);
            }
        }
        let grid = Grid {
            cols: segments,
            rows: rings,
        };
        let faces = grid_faces(grid);
        Self::build(v, faces, BTreeSet::new(), Some(grid), stop)
    }

    pub fn vertices(&self) -> &[P3] {
        &self.vertices
    }
    pub fn faces(&self) -> &[Vec<u32>] {
        &self.faces
    }
    pub fn creases(&self) -> &BTreeSet<(u32, u32)> {
        &self.creases
    }
    pub fn grid(&self) -> Option<Grid> {
        self.grid
    }
    /// The smooth limit surface as a triangle mesh (exact evaluation for grid cages,
    /// three Catmull-Clark levels otherwise).
    pub fn preview(&self) -> &Arc<Mesh> {
        &self.preview
    }
    pub fn weight(&self) -> usize {
        64usize
            .saturating_add(self.vertices.len())
            .saturating_add(self.preview.vertices.len())
            .saturating_add(self.preview.triangles.len().saturating_mul(3))
    }
    /// Unique edges in a stable order (numbered from 0 for 折り目).
    pub fn edges(&self) -> Vec<(u32, u32)> {
        let mut set = BTreeSet::new();
        for f in &self.faces {
            for k in 0..f.len() {
                if let (Some(&a), Some(&b)) = (f.get(k), f.get((k + 1) % f.len())) {
                    set.insert(key(a, b));
                }
            }
        }
        set.into_iter().collect()
    }

    // --- Grid cages: exact bicubic B-spline ------------------------------------------------

    /// The grid's surface: u periodic (columns wrapped), v clamped uniform cubic.
    pub fn grid_surface(&self) -> R<NurbsSurface> {
        let g = self.grid.ok_or("格子状のケージではありません。")?;
        grid_nurbs(&self.vertices, g)
    }

    fn grid_mesh(&self, g: Grid, stop: Stop) -> R<Mesh> {
        let s = grid_nurbs(&self.vertices, g)?;
        let [[u0, u1], [v0, v1]] = s.domain();
        let su = (g.cols * 6).clamp(24, 720);
        let sv = ((g.rows - 3) * 6).clamp(8, 720);
        let mut vertices = Vec::with_capacity(su * (sv + 1));
        for j in 0..=sv {
            kb_stop(stop)?;
            let v = v0 + (v1 - v0) * j as f64 / sv as f64;
            for i in 0..su {
                let u = u0 + (u1 - u0) * i as f64 / su as f64;
                let p = s
                    .eval(u, v)
                    .map_err(|e| format!("曲面の評価に失敗しました（{e}）。"))?;
                vertices.push([p.x, p.y, p.z]);
            }
        }
        let mut triangles = Vec::with_capacity(su * sv * 2);
        for j in 0..sv {
            for i in 0..su {
                let a = (j * su + i) as u32;
                let b = (j * su + (i + 1) % su) as u32;
                let c = ((j + 1) * su + (i + 1) % su) as u32;
                let d = ((j + 1) * su + i) as u32;
                triangles.push([a, b, c]);
                triangles.push([a, c, d]);
            }
        }
        Ok(Mesh {
            vertices,
            triangles,
        })
    }

    // --- General cages: Catmull-Clark with creases --------------------------------------------

    fn subdivided_mesh(&self, stop: Stop) -> R<Mesh> {
        let mut level = Cage {
            grid: None,
            ts: None,
            preview: Arc::new(Mesh::default()),
            ..self.clone()
        };
        for _ in 0..3 {
            if level.faces.len() * 4 > MAX_PREVIEW_FACES {
                break;
            }
            level = level.catmull_clark(stop)?;
        }
        Ok(triangulate(&level.vertices, &level.faces))
    }

    fn catmull_clark(&self, stop: Stop) -> R<Cage> {
        let nv = self.vertices.len();
        let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for k in 0..f.len() {
                if let (Some(&a), Some(&b)) = (f.get(k), f.get((k + 1) % f.len())) {
                    edge_faces.entry(key(a, b)).or_default().push(fi);
                }
            }
        }
        let mut edges: Vec<(u32, u32)> = edge_faces.keys().copied().collect();
        edges.sort_unstable();
        let edge_index: HashMap<(u32, u32), usize> =
            edges.iter().enumerate().map(|(i, e)| (*e, i)).collect();
        let point = |i: u32| self.vertices.get(i as usize).copied().unwrap_or([0.; 3]);
        let face_pts: Vec<P3> = self
            .faces
            .iter()
            .map(|f| {
                scale(
                    f.iter().fold([0.; 3], |s, &i| add(s, point(i))),
                    1. / f.len() as f64,
                )
            })
            .collect();
        let sharp = |e: &(u32, u32)| {
            self.creases.contains(e) || edge_faces.get(e).is_none_or(|f| f.len() != 2)
        };
        let mut edge_pts = Vec::with_capacity(edges.len());
        for e in &edges {
            let mid = scale(add(point(e.0), point(e.1)), 0.5);
            if sharp(e) {
                edge_pts.push(mid);
            } else {
                let fs = edge_faces.get(e).map(Vec::as_slice).unwrap_or(&[]);
                let fsum = fs.iter().fold([0.; 3], |s, &f| {
                    add(s, face_pts.get(f).copied().unwrap_or([0.; 3]))
                });
                edge_pts.push(scale(
                    add(add(point(e.0), point(e.1)), fsum),
                    1. / (2 + fs.len()) as f64,
                ));
            }
        }
        kb_stop(stop)?;
        // Vertex rules: smooth, crease (two sharp edges) or corner.
        let mut vert_edges: Vec<Vec<(u32, u32)>> = vec![Vec::new(); nv];
        let mut vert_faces: Vec<Vec<usize>> = vec![Vec::new(); nv];
        for e in &edges {
            for v in [e.0, e.1] {
                if let Some(list) = vert_edges.get_mut(v as usize) {
                    list.push(*e);
                }
            }
        }
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in f {
                if let Some(list) = vert_faces.get_mut(v as usize) {
                    list.push(fi);
                }
            }
        }
        let mut new_vertices = Vec::with_capacity(nv + edges.len() + self.faces.len());
        for v in 0..nv {
            let p = point(v as u32);
            let es = vert_edges.get(v).map(Vec::as_slice).unwrap_or(&[]);
            let sharp_edges: Vec<&(u32, u32)> = es.iter().filter(|e| sharp(e)).collect();
            let other = |e: &(u32, u32)| if e.0 == v as u32 { e.1 } else { e.0 };
            let q = match sharp_edges.len() {
                0 => {
                    let n = es.len().max(1) as f64;
                    let fs = vert_faces.get(v).map(Vec::as_slice).unwrap_or(&[]);
                    let favg = scale(
                        fs.iter().fold([0.; 3], |s, &f| {
                            add(s, face_pts.get(f).copied().unwrap_or([0.; 3]))
                        }),
                        1. / fs.len().max(1) as f64,
                    );
                    let ravg = scale(
                        es.iter().fold([0.; 3], |s, e| {
                            add(s, scale(add(point(e.0), point(e.1)), 0.5))
                        }),
                        1. / n,
                    );
                    scale(add(add(favg, scale(ravg, 2.)), scale(p, n - 3.)), 1. / n)
                }
                2 => {
                    let (Some(a), Some(b)) = (sharp_edges.first(), sharp_edges.get(1)) else {
                        continue;
                    };
                    scale(
                        add(add(scale(p, 6.), point(other(a))), point(other(b))),
                        1. / 8.,
                    )
                }
                _ => p,
            };
            new_vertices.push(q);
        }
        new_vertices.extend(edge_pts);
        new_vertices.extend(face_pts);
        let e_id = |a: u32, b: u32| {
            edge_index
                .get(&key(a, b))
                .map(|&i| (nv + i) as u32)
                .unwrap_or(0)
        };
        let f_base = nv + edges.len();
        let mut faces = Vec::with_capacity(self.faces.len() * 4);
        for (fi, f) in self.faces.iter().enumerate() {
            let m = f.len();
            for k in 0..m {
                let (Some(&prev), Some(&cur), Some(&next)) =
                    (f.get((k + m - 1) % m), f.get(k), f.get((k + 1) % m))
                else {
                    continue;
                };
                faces.push(vec![
                    cur,
                    e_id(cur, next),
                    (f_base + fi) as u32,
                    e_id(prev, cur),
                ]);
            }
        }
        let mut creases = BTreeSet::new();
        for e in &self.creases {
            let mid = e_id(e.0, e.1);
            creases.insert(key(e.0, mid));
            creases.insert(key(mid, e.1));
        }
        Ok(Cage {
            vertices: new_vertices,
            faces,
            creases,
            grid: None,
            ts: None,
            preview: Arc::new(Mesh::default()),
        })
    }

    /// One refinement step: exact knot insertion for grid cages (same surface, twice the
    /// control points each way), one Catmull-Clark step for general cages.
    pub fn subdivide(&self, stop: Stop) -> R<Cage> {
        if self.ts.is_some() {
            let all: Vec<usize> = (0..self.faces.len()).collect();
            return self.refine_faces(&all, stop);
        }
        match self.grid {
            Some(g) => {
                let (v, g2) = refine_grid(&self.vertices, g)?;
                Self::build(v, grid_faces(g2), BTreeSet::new(), Some(g2), stop)
            }
            None => {
                let c = self.catmull_clark(stop)?;
                Self::build(c.vertices, c.faces, c.creases, None, stop)
            }
        }
    }

    /// Mark edges (by number, see [`Cage::edges`]) as sharp creases. A grid cage becomes a
    /// general (Catmull-Clark) cage.
    pub fn crease(&self, numbers: &[usize], stop: Stop) -> R<Cage> {
        let edges = self.edges();
        let mut creases = self.creases.clone();
        for &n in numbers {
            let e = edges.get(n).ok_or_else(|| {
                format!(
                    "辺番号 {n} はありません（0〜{} を指定してください）。",
                    edges.len().saturating_sub(1)
                )
            })?;
            creases.insert(*e);
        }
        Self::build(
            self.vertices.clone(),
            self.faces.clone(),
            creases,
            None,
            stop,
        )
    }

    /// Push a face out by `distance` along its normal, adding side faces.
    pub fn extrude_face(&self, face: usize, distance: f64, stop: Stop) -> R<Cage> {
        if !distance.is_finite() || distance.abs() > kb::limits::MAX_COORDINATE {
            return Err("押し出し距離が無効です。".into());
        }
        let f = self
            .faces
            .get(face)
            .ok_or_else(|| {
                format!(
                    "面番号 {face} はありません（0〜{} を指定してください）。",
                    self.faces.len().saturating_sub(1)
                )
            })?
            .clone();
        let pts: Vec<P3> = f
            .iter()
            .filter_map(|&i| self.vertices.get(i as usize).copied())
            .collect();
        let mut n = [0.; 3];
        for k in 0..pts.len() {
            if let (Some(&a), Some(&b)) = (pts.get(k), pts.get((k + 1) % pts.len())) {
                n = add(n, cross(a, b));
            }
        }
        let l = norm(n);
        if l <= 1e-300 {
            return Err("面の向きを決められません（面がつぶれています）。".into());
        }
        let n = scale(n, distance / l);
        let mut vertices = self.vertices.clone();
        let base = vertices.len() as u32;
        let new_ids: Vec<u32> = (0..f.len() as u32).map(|k| base + k).collect();
        vertices.extend(pts.iter().map(|p| add(*p, n)));
        let mut faces = self.faces.clone();
        if let Some(slot) = faces.get_mut(face) {
            *slot = new_ids.clone();
        }
        for k in 0..f.len() {
            let k1 = (k + 1) % f.len();
            if let (Some(&a), Some(&b), Some(&a2), Some(&b2)) =
                (f.get(k), f.get(k1), new_ids.get(k), new_ids.get(k1))
            {
                faces.push(vec![a, b, b2, a2]);
            }
        }
        Self::build(vertices, faces, self.creases.clone(), None, stop)
    }

    /// Like [`Cage::to_brep`], but a general cage is first subdivided `levels` times (Catmull-Clark
    /// keeps the limit surface). The approximate patches around star points (valence 3, 5, ...)
    /// shrink with every step, so the gaps in tangent plane there shrink too and the B-rep follows
    /// the limit surface more closely, at 4x the faces per step. Grid and T-spline cages ignore it.
    pub fn to_brep_refined(&self, cap: bool, levels: usize, stop: Stop) -> R<BrepValue> {
        if self.grid.is_some() || self.ts.is_some() || levels == 0 {
            return self.to_brep(cap, stop);
        }
        let mut cage = self.clone();
        for _ in 0..levels.min(3) {
            kb_stop(stop)?;
            cage = cage.subdivide(stop)?;
        }
        cage.to_brep(cap, stop)
    }

    /// The exact B-rep of the cage's surface. Grid cages: the bicubic surface as T-spline sheets
    /// (SolidCraft `tspline_sheet`), split in two halves around, sewn, and capped top and bottom
    /// into a closed solid when `cap` is set.
    pub fn to_brep(&self, cap: bool, stop: Stop) -> R<BrepValue> {
        if let Some(ts) = &self.ts {
            // An open patch: exact NURBS faces (Bézier elements around T-junctions).
            let sheet = kb::tspline_body(ts, stop)?;
            return kb::brep(sheet, stop);
        }
        let Some(g) = self.grid else {
            return self.acc_brep(stop);
        };
        let s = grid_nurbs(&self.vertices, g)?;
        let halves = split_u(&s, (g.cols / 2) as f64)?;
        let mut sheets = Vec::with_capacity(2);
        for h in &halves {
            let ts = TSpline::from_nurbs(h)
                .map_err(|e| format!("T-spline への変換に失敗しました（{e}）。"))?;
            sheets.push(kb::tspline_body(&ts, stop)?);
        }
        let tube = kb::stitch_bodies(&sheets, stop)?;
        if cap {
            kb::cap_holes(&tube, stop)
        } else {
            Ok(tube)
        }
    }
}

fn kb_stop(stop: Stop) -> R<()> {
    if stop() {
        Err(kb::STOPPED.into())
    } else {
        Ok(())
    }
}

fn grid_faces(g: Grid) -> Vec<Vec<u32>> {
    let mut faces = Vec::with_capacity(g.cols * (g.rows - 1));
    for j in 0..g.rows.saturating_sub(1) {
        for i in 0..g.cols {
            let a = (j * g.cols + i) as u32;
            let b = (j * g.cols + (i + 1) % g.cols) as u32;
            let c = ((j + 1) * g.cols + (i + 1) % g.cols) as u32;
            let d = ((j + 1) * g.cols + i) as u32;
            faces.push(vec![a, b, c, d]);
        }
    }
    faces
}

fn clamped_uniform(n: usize) -> Vec<f64> {
    // n control points, cubic: [0,0,0,0,1,…,n-3,n-3,n-3,n-3]
    let spans = n - 3;
    let mut k = vec![0.; 4];
    k.extend((1..spans).map(|i| i as f64));
    k.extend([spans as f64; 4]);
    k
}

fn grid_nurbs(vertices: &[P3], g: Grid) -> R<NurbsSurface> {
    if g.cols < 4 || g.rows < 4 || vertices.len() != g.cols * g.rows {
        return Err("格子ケージの大きさが正しくありません（周 4 点以上・4 段以上）。".into());
    }
    let nu = g.cols + 3;
    let ku: Vec<f64> = (0..nu + 4).map(|k| k as f64 - 3.).collect();
    let kv = clamped_uniform(g.rows);
    let mut pts = Vec::with_capacity(nu * g.rows);
    for j in 0..g.rows {
        for i in 0..nu {
            let p = vertices
                .get(j * g.cols + i % g.cols)
                .copied()
                .unwrap_or([0.; 3]);
            pts.push(Vec3::new(p[0], p[1], p[2]));
        }
    }
    let w = vec![1.; pts.len()];
    NurbsSurface::new([3, 3], [ku, kv], &pts, &w)
        .map_err(|e| format!("NURBS 曲面を作れませんでした（{e}）。"))
}

/// Exact refinement: periodic cubic midpoint masks around, knot insertion along the height.
fn refine_grid(vertices: &[P3], g: Grid) -> R<(Vec<P3>, Grid)> {
    let cols2 = g.cols * 2;
    if cols2 * (2 * g.rows) > MAX_CAGE_VERTICES {
        return Err("これ以上細かくできません（点が多すぎます）。".into());
    }
    let at = |i: usize, j: usize| {
        vertices
            .get(j * g.cols + i % g.cols)
            .copied()
            .unwrap_or([0.; 3])
    };
    // Around: (P[i-1] + 6P[i] + P[i+1]) / 8 and (P[i] + P[i+1]) / 2.
    let mut rows_u: Vec<Vec<P3>> = Vec::with_capacity(g.rows);
    for j in 0..g.rows {
        let mut row = Vec::with_capacity(cols2);
        for i in 0..g.cols {
            let (pm, p, pp) = (at(i + g.cols - 1, j), at(i, j), at(i + 1, j));
            row.push(scale(add(add(pm, scale(p, 6.)), pp), 1. / 8.));
            row.push(scale(add(p, pp), 0.5));
        }
        rows_u.push(row);
    }
    // Along: insert the midpoint of every span of the clamped knot vector.
    let mut flat = Vec::with_capacity(cols2 * g.rows);
    for row in &rows_u {
        for p in row {
            flat.push(Vec3::new(p[0], p[1], p[2]));
        }
    }
    let ku: Vec<f64> = (0..cols2 + 4).map(|k| k as f64).collect();
    let kv = clamped_uniform(g.rows);
    let w = vec![1.; flat.len()];
    let mut s = NurbsSurface::new([3, 3], [ku, kv.clone()], &flat, &w)
        .map_err(|e| format!("細分に失敗しました（{e}）。"))?;
    let spans = g.rows - 3;
    for k in 0..spans {
        s = s
            .insert_knot(Dir::T, k as f64 + 0.5)
            .map_err(|e| format!("ノット挿入に失敗しました（{e}）。"))?;
    }
    let [nu, nv] = s.count();
    let mut out = Vec::with_capacity(nu * nv);
    for j in 0..nv {
        for i in 0..nu {
            let h = s.point(i, j);
            let p = h.project().ok_or("細分した制御点が無効です。")?;
            out.push([p.x, p.y, p.z]);
        }
    }
    // The refined clamped vector has unit-half spacing; it is the clamped uniform vector of
    // 2·rows − 3 points scaled by 1/2, so the grid convention (and the surface) is kept.
    Ok((
        out,
        Grid {
            cols: cols2,
            rows: nv,
        },
    ))
}

/// Split a u-periodic surface (wrapped, see [`grid_nurbs`]) into two clamped halves at `m`.
fn split_u(s: &NurbsSurface, m: f64) -> R<[NurbsSurface; 2]> {
    let err =
        |e: solidcraft_geom::bspline::SplineError| format!("曲面の分割に失敗しました（{e}）。");
    let mut c = s.clamped().map_err(err)?;
    let have = c.knots()[0].iter().filter(|&&k| k == m).count();
    for _ in have..3 {
        c = c.insert_knot(Dir::S, m).map_err(err)?;
    }
    let ku = c.knots()[0].clone();
    let kv = c.knots()[1].clone();
    let r = ku
        .iter()
        .position(|&k| k == m)
        .ok_or("分割位置が見つかりません。")?;
    let [nu, nv] = c.count();
    let take = |cols: std::ops::Range<usize>| -> Vec<solidcraft_geom::bspline::HPoint> {
        let mut out = Vec::with_capacity(cols.len() * nv);
        for j in 0..nv {
            for i in cols.clone() {
                out.push(c.point(i, j));
            }
        }
        out
    };
    let mut left_k: Vec<f64> = ku.get(..r).map(<[f64]>::to_vec).unwrap_or_default();
    left_k.extend([m; 4]);
    let mut right_k = vec![m; 4];
    right_k.extend(ku.get(r + 3..).map(<[f64]>::to_vec).unwrap_or_default());
    let left =
        NurbsSurface::from_homogeneous([3, 3], [left_k, kv.clone()], take(0..r)).map_err(err)?;
    let right =
        NurbsSurface::from_homogeneous([3, 3], [right_k, kv], take(r.saturating_sub(1)..nu))
            .map_err(err)?;
    Ok([left, right])
}

/// Fan-triangulate polygons.
fn triangulate(vertices: &[P3], faces: &[Vec<u32>]) -> Mesh {
    let mut triangles = Vec::with_capacity(faces.len() * 2);
    for f in faces {
        for k in 1..f.len().saturating_sub(1) {
            if let (Some(&a), Some(&b), Some(&c)) = (f.first(), f.get(k), f.get(k + 1)) {
                triangles.push([a, b, c]);
            }
        }
    }
    Mesh {
        vertices: vertices.to_vec(),
        triangles,
    }
}

// -------------------------------------------------------------------------------------------
// General cages as B-reps: approximate Catmull-Clark (Loop & Schaefer 2008)
// -------------------------------------------------------------------------------------------

/// Most patches one general cage may turn into (each is one NURBS face).
pub const MAX_ACC_PATCHES: usize = 4096;

impl Cage {
    /// Rebuild the cage after its points moved, keeping its kind (grid or general) and creases.
    fn with_vertices(&self, vertices: Vec<P3>, stop: Stop) -> R<Cage> {
        if let Some(ts) = &self.ts {
            let pts: Vec<Vec3> = vertices
                .iter()
                .map(|p| Vec3::new(p[0], p[1], p[2]))
                .collect();
            let moved = TSpline::new(ts.mesh().clone(), ts.degree(), &pts, &ts.weights())
                .map_err(|e| format!("T-spline を作り直せませんでした（{e}）。"))?;
            return Self::from_tspline(moved, stop);
        }
        Self::build(
            vertices,
            self.faces.clone(),
            self.creases.clone(),
            self.grid,
            stop,
        )
    }

    /// One bicubic Bézier patch per quad. A cage with other polygons is subdivided once first
    /// (Catmull-Clark makes every face a quad).
    pub fn acc_patches(&self, stop: Stop) -> R<Vec<[[P3; 4]; 4]>> {
        if self.faces.iter().any(|f| f.len() != 4) {
            let quads = self.catmull_clark(stop)?;
            return quads.acc_patches(stop);
        }
        if self.faces.len() > MAX_ACC_PATCHES {
            return Err(format!(
                "面が多すぎます（{} 面、上限 {MAX_ACC_PATCHES}）。細分の回数を減らしてください。",
                self.faces.len()
            ));
        }
        let mut edge_faces: HashMap<(u32, u32), Vec<(usize, usize)>> = HashMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for k in 0..4 {
                if let (Some(&a), Some(&b)) = (f.get(k), f.get((k + 1) % 4)) {
                    edge_faces.entry(key(a, b)).or_default().push((fi, k));
                }
            }
        }
        if let Some(e) = edge_faces.values().find(|v| v.len() != 2) {
            return Err(format!(
                "ケージに開いた辺または 3 枚以上の面が集まる辺があります（{} 枚）。閉じたケージにしてください。",
                e.len()
            ));
        }
        let point = |i: u32| self.vertices.get(i as usize).copied().unwrap_or([0.; 3]);
        let mut valence = vec![0usize; self.vertices.len()];
        for e in edge_faces.keys() {
            for v in [e.0, e.1] {
                if let Some(n) = valence.get_mut(v as usize) {
                    *n += 1;
                }
            }
        }
        // Interior point of face f at corner k: (n·v + 2(e_prev + e_next) + d) / (n + 5).
        let interior = |f: &[u32], k: usize| -> P3 {
            let v = f.get(k).copied().unwrap_or(0);
            let n = valence.get(v as usize).copied().unwrap_or(4) as f64;
            let next = point(f.get((k + 1) % 4).copied().unwrap_or(0));
            let prev = point(f.get((k + 3) % 4).copied().unwrap_or(0));
            let diag = point(f.get((k + 2) % 4).copied().unwrap_or(0));
            scale(
                add(add(scale(point(v), n), scale(add(prev, next), 2.)), diag),
                1. / (n + 5.),
            )
        };
        let mut inner: Vec<[P3; 4]> = Vec::with_capacity(self.faces.len());
        for f in &self.faces {
            kb_stop(stop)?;
            inner.push(std::array::from_fn(|k| interior(f, k)));
        }
        let at = |fi: usize, k: usize| {
            inner
                .get(fi)
                .and_then(|c| c.get(k))
                .copied()
                .unwrap_or([0.; 3])
        };
        // Corner points: the average of the interior points around the vertex.
        let mut corner_sum: Vec<(P3, usize)> = vec![([0.; 3], 0); self.vertices.len()];
        for (fi, f) in self.faces.iter().enumerate() {
            for (k, &v) in f.iter().enumerate() {
                if let Some(slot) = corner_sum.get_mut(v as usize) {
                    slot.0 = add(slot.0, at(fi, k));
                    slot.1 += 1;
                }
            }
        }
        let corner = |v: u32| {
            corner_sum
                .get(v as usize)
                .map_or([0.; 3], |(p, n)| scale(*p, 1. / (*n).max(1) as f64))
        };
        // Edge point next to corner `v` on edge (v, w): the average of the interior points at `v`
        // of the two faces sharing the edge, in a fixed order so both patches get the same bits.
        let edge_point = |v: u32, w: u32| -> P3 {
            let Some(pair) = edge_faces.get(&key(v, w)) else {
                return point(v);
            };
            let mut corners: Vec<(usize, usize)> = pair
                .iter()
                .filter_map(|&(fi, _)| {
                    let f = self.faces.get(fi)?;
                    let k = f.iter().position(|&x| x == v)?;
                    Some((fi, k))
                })
                .collect();
            corners.sort_unstable();
            match corners.as_slice() {
                [(f1, k1), (f2, k2)] => scale(add(at(*f1, *k1), at(*f2, *k2)), 0.5),
                _ => point(v),
            }
        };
        let mut patches = Vec::with_capacity(self.faces.len());
        for (fi, f) in self.faces.iter().enumerate() {
            let (Some(&v0), Some(&v1), Some(&v2), Some(&v3)) =
                (f.first(), f.get(1), f.get(2), f.get(3))
            else {
                continue;
            };
            // b[i][j]: i along v0 → v1, j along v0 → v3.
            let mut b = [[[0.; 3]; 4]; 4];
            b[0][0] = corner(v0);
            b[3][0] = corner(v1);
            b[3][3] = corner(v2);
            b[0][3] = corner(v3);
            b[1][0] = edge_point(v0, v1);
            b[2][0] = edge_point(v1, v0);
            b[3][1] = edge_point(v1, v2);
            b[3][2] = edge_point(v2, v1);
            b[2][3] = edge_point(v2, v3);
            b[1][3] = edge_point(v3, v2);
            b[0][2] = edge_point(v3, v0);
            b[0][1] = edge_point(v0, v3);
            b[1][1] = at(fi, 0);
            b[2][1] = at(fi, 1);
            b[2][2] = at(fi, 2);
            b[1][2] = at(fi, 3);
            patches.push(b);
        }
        Ok(patches)
    }

    /// The cage's surface as a closed B-rep of bicubic NURBS faces (see [`Cage::acc_patches`]).
    pub fn acc_brep(&self, stop: Stop) -> R<BrepValue> {
        let patches = self.acc_patches(stop)?;
        let knots = vec![0., 0., 0., 0., 1., 1., 1., 1.];
        let mut sheets = Vec::with_capacity(patches.len());
        for b in &patches {
            kb_stop(stop)?;
            let mut pts = Vec::with_capacity(16);
            for j in 0..4 {
                for row in b.iter() {
                    let p = row.get(j).copied().unwrap_or([0.; 3]);
                    pts.push(Vec3::new(p[0], p[1], p[2]));
                }
            }
            let s = NurbsSurface::new([3, 3], [knots.clone(), knots.clone()], &pts, &[1.; 16])
                .map_err(|e| format!("パッチを作れませんでした（{e}）。"))?;
            let ts = TSpline::from_nurbs(&s)
                .map_err(|e| format!("T-spline への変換に失敗しました（{e}）。"))?;
            sheets.push(kb::tspline_body(&ts, stop)?);
        }
        kb::stitch_bodies(&sheets, stop)
    }

    /// The vertex mirroring `v` across the plane `axis = 0` (itself when it lies on the plane).
    pub fn mirror_of(&self, v: usize, axis: usize) -> Option<usize> {
        let p = *self.vertices.get(v)?;
        let mut q = p;
        *q.get_mut(axis)? = -*p.get(axis)?;
        let size = self.vertices.iter().fold(1f64, |m, x| m.max(norm(*x)));
        let eps = size * 1e-6;
        self.vertices
            .iter()
            .enumerate()
            .filter(|(_, x)| norm([x[0] - q[0], x[1] - q[1], x[2] - q[2]]) <= eps)
            .map(|(i, _)| i)
            .next()
    }

    /// Apply one edit.
    pub fn edit(&self, op: &EditOp, stop: Stop) -> R<Cage> {
        match op {
            EditOp::Move { verts, d, sym } => {
                if d.iter()
                    .any(|x| !x.is_finite() || x.abs() > kb::limits::MAX_COORDINATE)
                {
                    return Err("移動量が無効です。".into());
                }
                let mut v = self.vertices.clone();
                let mut moved = BTreeSet::new();
                for &i in verts {
                    let mut targets = vec![(i, *d)];
                    if let Some(axis) = sym.map(usize::from).filter(|a| *a < 3) {
                        match self.mirror_of(i, axis) {
                            Some(m) if m == i => {
                                if let Some(t) = targets.first_mut() {
                                    t.1[axis] = 0.;
                                }
                            }
                            Some(m) => {
                                let mut md = *d;
                                md[axis] = -md[axis];
                                targets.push((m, md));
                            }
                            None => {}
                        }
                    }
                    for (t, dt) in targets {
                        if !moved.insert(t) {
                            continue;
                        }
                        let p = v.get_mut(t).ok_or_else(|| {
                            format!(
                                "点番号 {t} はありません（0〜{}）。",
                                self.vertices.len().saturating_sub(1)
                            )
                        })?;
                        *p = add(*p, dt);
                    }
                }
                self.with_vertices(v, stop)
            }
            EditOp::Scale { verts, s } => {
                if !(s.is_finite() && *s > 1e-6 && *s < 1e6) {
                    return Err("拡大率は 0 より大きい値にしてください。".into());
                }
                let pts: Vec<P3> = verts
                    .iter()
                    .filter_map(|&i| self.vertices.get(i).copied())
                    .collect();
                if pts.is_empty() {
                    return Err("拡大する点を選んでください。".into());
                }
                let c = scale(
                    pts.iter().fold([0.; 3], |a, p| add(a, *p)),
                    1. / pts.len() as f64,
                );
                let mut v = self.vertices.clone();
                for &i in verts {
                    if let Some(p) = v.get_mut(i) {
                        *p = add(c, scale([p[0] - c[0], p[1] - c[1], p[2] - c[2]], *s));
                    }
                }
                self.with_vertices(v, stop)
            }
            EditOp::Refine { faces } => self.refine_faces(faces, stop),
            EditOp::Extrude { .. } | EditOp::Crease { .. } if self.ts.is_some() => Err(
                "T-spline パッチケージでは面の押し出し・折り目は使えません（局所細分と移動が使えます）。".into(),
            ),
            EditOp::Extrude { faces, d } => {
                let mut cage = self.clone();
                let mut seen = BTreeSet::new();
                for &f in faces {
                    if seen.insert(f) {
                        cage = cage.extrude_face(f, *d, stop)?;
                    }
                }
                Ok(cage)
            }
            EditOp::Crease { edges } => self.crease(edges, stop),
            EditOp::Uncrease => Self::build(
                self.vertices.clone(),
                self.faces.clone(),
                BTreeSet::new(),
                self.grid,
                stop,
            ),
            EditOp::Subdivide => self.subdivide(stop),
        }
    }

    /// Apply the edit list kept as JSON text (an empty text keeps the cage).
    pub fn apply_edits(&self, text: &str, stop: Stop) -> R<Cage> {
        let ops = parse_edits(text)?;
        let mut cage = self.clone();
        for (i, op) in ops.iter().enumerate() {
            kb_stop(stop)?;
            cage = cage
                .edit(op, stop)
                .map_err(|e| format!("編集 {}（{}）: {e}", i + 1, op.label()))?;
        }
        Ok(cage)
    }
}

/// Most edits one ケージ編集 node keeps.
pub const MAX_EDITS: usize = 2000;

/// One cage edit. Indices refer to the cage as it is when the edit is applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    /// Move points by `d` (mm). With `sym` (0 = X, 1 = Y, 2 = Z) the mirror points move mirrored.
    Move {
        verts: Vec<usize>,
        d: [f64; 3],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sym: Option<u8>,
    },
    /// Refine faces locally (T-spline patch cages): each face is cut through its middle both
    /// ways, inserting T-junctions; the surface does not change.
    Refine { faces: Vec<usize> },
    /// Scale points about their centre.
    Scale { verts: Vec<usize>, s: f64 },
    /// Push faces out along their normals.
    Extrude { faces: Vec<usize>, d: f64 },
    /// Make edges sharp.
    Crease { edges: Vec<usize> },
    /// Remove every crease.
    Uncrease,
    /// One subdivision step.
    Subdivide,
}

impl EditOp {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Move { .. } => "移動",
            Self::Scale { .. } => "拡大縮小",
            Self::Refine { .. } => "局所細分",
            Self::Extrude { .. } => "面の押し出し",
            Self::Crease { .. } => "折り目",
            Self::Uncrease => "折り目を消す",
            Self::Subdivide => "細分",
        }
    }
}

pub fn parse_edits(text: &str) -> R<Vec<EditOp>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let ops: Vec<EditOp> = serde_json::from_str(text).map_err(|e| {
        format!("ケージ編集の記録を読めません（{e}）。「編集をリセット」でやり直してください。")
    })?;
    if ops.len() > MAX_EDITS {
        return Err(format!("編集が多すぎます（上限 {MAX_EDITS}）。"));
    }
    Ok(ops)
}

pub fn edits_text(ops: &[EditOp]) -> String {
    serde_json::to_string(ops).unwrap_or_default()
}

// -------------------------------------------------------------------------------------------
// T-spline patch cages (T-junctions, local refinement)
// -------------------------------------------------------------------------------------------

/// Most evaluation points per direction for a T-spline preview.
const TS_PREVIEW: usize = 64;

impl Cage {
    /// A T-spline patch: a bicubic plane `width × depth` on `nu × nv` control points, the middle
    /// raised by `bulge` (mm). The control net can then be refined locally (T-junctions).
    pub fn tspline_patch(
        width: f64,
        depth: f64,
        nu: usize,
        nv: usize,
        bulge: f64,
        stop: Stop,
    ) -> R<Cage> {
        for (v, name) in [(width, "幅"), (depth, "奥行")] {
            if !(v.is_finite() && v > kb::tol::ABS && v <= kb::limits::MAX_COORDINATE) {
                return Err(format!("{name}は 0 より大きい値にしてください。"));
            }
        }
        if !(4..=64).contains(&nu)
            || !(4..=64).contains(&nv)
            || !bulge.is_finite()
            || bulge.abs() > 1e5
        {
            return Err("点の数は 4〜64、膨らみは有限な値にしてください。".into());
        }
        let mut pts = Vec::with_capacity(nu * nv);
        for j in 0..nv {
            for i in 0..nu {
                let (u, v) = (i as f64 / (nu - 1) as f64, j as f64 / (nv - 1) as f64);
                let z = bulge * (std::f64::consts::PI * u).sin() * (std::f64::consts::PI * v).sin();
                pts.push(Vec3::new((u - 0.5) * width, (v - 0.5) * depth, z));
            }
        }
        let s = NurbsSurface::new(
            [3, 3],
            [clamped_uniform(nu), clamped_uniform(nv)],
            &pts,
            &vec![1.; pts.len()],
        )
        .map_err(|e| format!("NURBS 曲面を作れませんでした（{e}）。"))?;
        let ts = TSpline::from_nurbs(&s)
            .map_err(|e| format!("T-spline への変換に失敗しました（{e}）。"))?;
        Self::from_tspline(ts, stop)
    }

    /// A cage showing a T-spline's control net (faces of its T-mesh, T-junction points on their
    /// sides), with the exact surface as preview.
    pub fn from_tspline(ts: TSpline, stop: Stop) -> R<Cage> {
        let mesh = ts.mesh();
        let vertices: Vec<P3> = ts.points().iter().map(|p| [p.x, p.y, p.z]).collect();
        if vertices.len() > MAX_CAGE_VERTICES {
            return Err(format!(
                "ケージの点が多すぎます（上限 {MAX_CAGE_VERTICES}）。"
            ));
        }
        let mut faces = Vec::new();
        for r in mesh.faces() {
            let mut poly: Vec<u32> = Vec::new();
            let mut push = |i: usize, j: usize| {
                if let Some(v) = mesh.vertex_at(i, j)
                    && poly.last() != Some(&(v as u32))
                {
                    poly.push(v as u32);
                }
            };
            for i in r.i0..r.i1 {
                push(i, r.j0);
            }
            for j in r.j0..r.j1 {
                push(r.i1, j);
            }
            for i in (r.i0 + 1..=r.i1).rev() {
                push(i, r.j1);
            }
            for j in (r.j0 + 1..=r.j1).rev() {
                push(r.i0, j);
            }
            if poly.first() == poly.last() && poly.len() > 1 {
                poly.pop();
            }
            if poly.len() >= 3 {
                faces.push(poly);
            }
        }
        // The exact surface, sampled on its domain.
        let [[s0, s1], [t0, t1]] = ts.domain();
        let n = TS_PREVIEW;
        let mut pv = Vec::with_capacity((n + 1) * (n + 1));
        for j in 0..=n {
            kb_stop(stop)?;
            for i in 0..=n {
                let (s, t) = (
                    s0 + (s1 - s0) * i as f64 / n as f64,
                    t0 + (t1 - t0) * j as f64 / n as f64,
                );
                let p = ts
                    .eval(s, t)
                    .map_err(|e| format!("T-spline の評価に失敗しました（{e}）。"))?;
                pv.push([p.x, p.y, p.z]);
            }
        }
        let mut triangles = Vec::with_capacity(n * n * 2);
        for j in 0..n {
            for i in 0..n {
                let a = (j * (n + 1) + i) as u32;
                let (b, c, d) = (a + 1, a + 1 + (n + 1) as u32, a + (n + 1) as u32);
                triangles.push([a, b, c]);
                triangles.push([a, c, d]);
            }
        }
        Ok(Cage {
            vertices,
            faces,
            creases: BTreeSet::new(),
            grid: None,
            ts: Some(Arc::new(ts)),
            preview: Arc::new(Mesh {
                vertices: pv,
                triangles,
            }),
        })
    }

    pub fn tspline(&self) -> Option<&TSpline> {
        self.ts.as_deref()
    }

    /// Local refinement of the listed faces (T-spline patch cages only).
    pub fn refine_faces(&self, faces: &[usize], stop: Stop) -> R<Cage> {
        let ts = self
            .ts
            .as_ref()
            .ok_or("局所細分は T-spline パッチケージで使えます。")?;
        let mesh = ts.mesh();
        let rects = mesh.faces();
        let [[s0, s1], [t0, t1]] = ts.domain();
        let (sk, tk) = (mesh.s_knots(), mesh.t_knots());
        let mut cuts = Vec::new();
        for &f in faces {
            // Faces are numbered as `faces()` lists them (frame faces included).
            let r = rects.get(f).ok_or_else(|| {
                format!(
                    "面番号 {f} はありません（0〜{}）。",
                    rects.len().saturating_sub(1)
                )
            })?;
            let (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
                (sk.get(r.i0), sk.get(r.i1), tk.get(r.j0), tk.get(r.j1))
            else {
                continue;
            };
            let (a0, a1, b0, b1) = (a0.max(s0), a1.min(s1), b0.max(t0), b1.min(t1));
            if !(a1 - a0 > 1e-9 && b1 - b0 > 1e-9) {
                continue; // a frame face outside the domain: nothing to cut
            }
            cuts.push(solidcraft_geom::tspline::Cut::S {
                s: (a0 + a1) / 2.,
                t0: b0,
                t1: b1,
            });
            cuts.push(solidcraft_geom::tspline::Cut::T {
                t: (b0 + b1) / 2.,
                s0: a0,
                s1: a1,
            });
        }
        if cuts.is_empty() {
            return Err("細分できる面がありません（外周の枠の面は細分できません）。内側の面を選んでください。".into());
        }
        let refined = ts
            .refine(&cuts)
            .map_err(|e| format!("局所細分に失敗しました（{e}）。"))?;
        Self::from_tspline(refined, stop)
    }
}
