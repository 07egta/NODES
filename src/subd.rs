//! SubD / T-spline control cages (independent NODES code).
//!
//! A cage is a polygon mesh of control points. Two kinds:
//! - **Grid cages** (made by 円柱ケージ): `cols × rows` points, closed around (u periodic) and
//!   open along the height. Their limit surface is defined *exactly* as the uniform bicubic
//!   B-spline in u (periodic) and the clamped uniform cubic B-spline in v, so the preview,
//!   refinement and the NURBS/B-rep conversion all describe the same surface. Refinement is
//!   exact knot insertion (the T-spline/NURBS refine of SolidCraft's `sc-geom` is used for v).
//! - **General cages** (箱ケージ, or any cage after 面の押し出し / 折り目): Catmull-Clark
//!   subdivision with sharp creases. Converting a general cage to an exact B-rep is the next
//!   milestone (see docs/NURBS_ROADMAP.md); they can be meshed for preview and STL.
use crate::kernel_bridge::{self as kb, BrepValue, R, Stop};
use crate::model::Mesh;
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
    preview: Arc<Mesh>,
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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
            preview: Arc::new(Mesh::default()),
        })
    }

    /// One refinement step: exact knot insertion for grid cages (same surface, twice the
    /// control points each way), one Catmull-Clark step for general cages.
    pub fn subdivide(&self, stop: Stop) -> R<Cage> {
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

    /// The exact B-rep of the cage's surface. Grid cages: the bicubic surface as T-spline sheets
    /// (SolidCraft `tspline_sheet`), split in two halves around, sewn, and capped top and bottom
    /// into a closed solid when `cap` is set.
    pub fn to_brep(&self, cap: bool, stop: Stop) -> R<BrepValue> {
        let Some(g) = self.grid else {
            return Err("箱ケージなど格子でないケージの厳密 B-rep 変換は次の段階で対応します。今は「SubD → メッシュ」でメッシュにしてください。".into());
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
