//! Bridge between NODES values and the vendored SolidCraft geometry kernel
//! (`crates/sc-geom`, `crates/sc-kernel`, MIT OR Apache-2.0; see docs/SOLIDCRAFT_CHANGES.md).
//!
//! The kernel is used through this module only. It is *adapted* for NODES rather than exposed
//! as-is: every operation takes NODES curves and values, picks sensible defaults (all edges for a
//! fillet, the top face for a shell, a smooth loft with caps), checks size limits and the cancel
//! flag, and turns kernel errors into Japanese messages that say what failed, why, and how to fix
//! it. Kernel panics are caught by `solidcraft_kernel::guard`.
//!
//! The tolerance policy lives in [`tol`] and nowhere else.
use crate::curve_geometry::{CurveGeometry, NurbsCurve};
use crate::model::{Curve, Mesh};
use solidcraft_geom::bspline::basis_bezier;
use solidcraft_geom::{Loop2, MAX_BEZIER_CTRL, Plane, Region2, Seg2, Vec2, Vec3};
use solidcraft_kernel::{self as sk, Body, KernelError};
use std::collections::HashMap;
use std::sync::Arc;

pub type R<T> = Result<T, String>;
pub type Stop<'a> = &'a dyn Fn() -> bool;

/// The single tolerance policy for exact modelling in NODES (mm and radians).
pub mod tol {
    /// Two points closer than this are the same point.
    pub const ABS: f64 = 1e-6;
    /// Two directions closer than this angle are the same direction.
    pub const ANGLE: f64 = 1e-9;
    /// How far a profile curve may leave its plane and still count as planar.
    pub const PLANAR: f64 = 1e-4;
    /// The largest gap a closed profile may have between its end and start (it is closed exactly).
    pub const CLOSE: f64 = 1e-3;
    /// Sewing tolerance for faces that should share edges (caps, split sheets).
    pub const STITCH: f64 = 1e-4;
    /// Display/STL chord tolerance relative to the body size, clamped to [MIN, MAX] mm.
    pub const CHORD_FRACTION: f64 = 1e-3;
    pub const CHORD_MIN: f64 = 0.005;
    pub const CHORD_MAX: f64 = 0.25;
    /// Maximum volume difference accepted for a faceted Boolean preview.
    pub const BOOLEAN_PREVIEW_VOLUME_ERROR: f64 = 0.01;
    /// Chord tolerance for a body of the given size (bounding-box diagonal, mm).
    pub fn chord(size: f64) -> f64 {
        if size.is_finite() && size > 0. {
            (size * CHORD_FRACTION).clamp(CHORD_MIN, CHORD_MAX)
        } else {
            CHORD_MAX
        }
    }
    /// Weld distance for display meshes of a body of the given size.
    pub fn weld(size: f64) -> f64 {
        if size.is_finite() && size > 0. {
            (size * 1e-8).max(ABS)
        } else {
            ABS
        }
    }
}

/// Size limits that keep a single node from freezing the app.
pub mod limits {
    pub const MAX_SECTIONS: usize = 64;
    pub const MAX_PIECES: usize = 4096;
    pub const MAX_FACES: usize = 20_000;
    pub const MAX_TRIANGLES: usize = 600_000;
    pub const MAX_FACETED_BOOLEAN_TRIANGLES: usize = 8_192;
    pub const MAX_STEP_BYTES: usize = 64 * 1024 * 1024;
    pub const MAX_COORDINATE: f64 = 1e6;
    pub const MAX_SAMPLES: usize = 20_000;
}

pub const STOPPED: &str = "計算を停止しました。";

pub(crate) fn stopped(stop: Stop) -> R<()> {
    if stop() { Err(STOPPED.into()) } else { Ok(()) }
}

/// A kernel error as a Japanese message: what failed, why, and how to fix it.
pub fn kernel_error(what: &str, e: KernelError) -> String {
    match e {
        KernelError::Invalid(m) => format!(
            "{what}: 入力が正しくありません（{m}）。曲線が閉じて平面上にあるか、寸法が正の値かを確認してください。"
        ),
        KernelError::Failed(m) => format!(
            "{what}: 計算できませんでした（{m}）。形の重なり方や寸法を少し変えて、もう一度試してください。"
        ),
        KernelError::TooLarge(m) => format!(
            "{what}: 値が周りの形に対して大きすぎます（{m}）。半径や距離を小さくしてください。"
        ),
        KernelError::Internal(m) => format!(
            "{what}: 形状カーネルの内部エラーです（{m}）。入力の形は変わっていません。値を少し変えて試してください。"
        ),
    }
}

/// Run a kernel call with panic protection, cancel checks and a Japanese error.
pub(crate) fn kernel<T>(what: &str, stop: Stop, f: impl FnOnce() -> sk::Result<T>) -> R<T> {
    stopped(stop)?;
    let result = sk::guard(what, f).map_err(|e| kernel_error(what, e));
    stopped(stop)?;
    result
}

// ---------------------------------------------------------------------------------------------
// The B-rep value
// ---------------------------------------------------------------------------------------------

/// An exact B-rep body (solid or open sheet) with its welded display/STL mesh.
#[derive(Clone, Debug)]
pub struct BrepValue {
    body: Body,
    preview: Arc<Mesh>,
    closed: bool,
}

impl PartialEq for BrepValue {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.preview, &other.preview)
            || (self.closed == other.closed && self.preview == other.preview)
    }
}

impl BrepValue {
    pub fn new(body: Body, stop: Stop) -> R<Self> {
        let faces = body.face_count();
        if faces == 0 {
            return Err(
                "結果の形が空です。入力が重なっているか、寸法が正しいかを確認してください。".into(),
            );
        }
        if faces > limits::MAX_FACES {
            return Err(format!(
                "面が多すぎます（{faces} 面、上限 {}）。分割数や配列の数を減らしてください。",
                limits::MAX_FACES
            ));
        }
        let preview = Arc::new(display_mesh(&body, stop)?);
        let closed = is_watertight(&preview);
        Ok(Self {
            body,
            preview,
            closed,
        })
    }
    pub fn body(&self) -> &Body {
        &self.body
    }
    /// Welded triangle mesh for display and STL. A Boolean may use a separately
    /// faceted preview while retaining the original B-rep.
    pub fn preview(&self) -> &Arc<Mesh> {
        &self.preview
    }
    /// Whether the mesh is watertight (every edge shared by exactly two consistently oriented
    /// triangles): a closed solid.
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    /// Whether the display/STL mesh itself is watertight.
    pub fn mesh_is_watertight(&self) -> bool {
        is_watertight(&self.preview)
    }
    pub fn face_count(&self) -> usize {
        self.body.face_count()
    }
    pub fn edge_count(&self) -> usize {
        self.body.edge_count()
    }
    pub fn weight(&self) -> usize {
        256usize
            .saturating_add(self.preview.vertices.len())
            .saturating_add(self.preview.triangles.len().saturating_mul(3))
    }
    /// Enclosed volume (mm³) from the kernel, or the mesh when the kernel cannot measure.
    pub fn volume(&self) -> Option<f64> {
        if !self.closed {
            return None;
        }
        match sk::guard("体積", || sk::measure_mass(&self.body)) {
            Ok(m) if m.volume.is_finite() => Some(m.volume.abs()),
            _ => Some(mesh_volume(&self.preview).abs()),
        }
    }
}

/// Signed volume of a closed triangle mesh (mm³).
pub fn mesh_volume(m: &Mesh) -> f64 {
    let mut v = 0.;
    for t in &m.triangles {
        let (Some(a), Some(b), Some(c)) = (
            m.vertices.get(t[0] as usize),
            m.vertices.get(t[1] as usize),
            m.vertices.get(t[2] as usize),
        ) else {
            continue;
        };
        v += dot(*a, cross(*b, *c)) / 6.;
    }
    v
}

/// Whether every undirected edge is used by exactly two triangles in opposite directions.
pub fn is_watertight(m: &Mesh) -> bool {
    if m.triangles.is_empty() {
        return false;
    }
    let mut directed: HashMap<(u32, u32), u32> = HashMap::with_capacity(m.triangles.len() * 3);
    for t in &m.triangles {
        for k in 0..3 {
            let a = t[k];
            let b = t[(k + 1) % 3];
            if a == b {
                return false;
            }
            *directed.entry((a, b)).or_insert(0) += 1;
        }
    }
    directed
        .iter()
        .all(|(&(a, b), &n)| n == 1 && directed.get(&(b, a)) == Some(&1))
}

/// Edges used by one triangle only (the open boundary of a mesh), as directed pairs.
pub fn boundary_edges(m: &Mesh) -> Vec<(u32, u32)> {
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &m.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut out = Vec::new();
    for t in &m.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if count.get(&(a.min(b), a.max(b))) == Some(&1) {
                out.push((a, b));
            }
        }
    }
    out
}

/// Kernel tessellation at [`tol::chord`], welded so shared edges become shared vertices.
pub fn display_mesh(body: &Body, stop: Stop) -> R<Mesh> {
    let size = body.size();
    let raw = kernel("メッシュ化", stop, || {
        body.tessellate(tol::chord(size))
    })?;
    weld_tessellation(&raw, size, stop)
}

fn weld_tessellation(raw: &solidcraft_geom::Mesh, size: f64, stop: Stop) -> R<Mesh> {
    if raw.triangles.len() > limits::MAX_TRIANGLES {
        return Err(format!(
            "メッシュが大きすぎます（{} 三角形）。形を小さくするか、面の数を減らしてください。",
            raw.triangles.len()
        ));
    }
    let points: Vec<[f64; 3]> = raw.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
    if points.iter().any(|p| {
        p.iter()
            .any(|v| !v.is_finite() || v.abs() > limits::MAX_COORDINATE)
    }) {
        return Err("メッシュに無効な座標が含まれています。形の寸法を確認してください。".into());
    }
    let (vertices, map) = weld(&points, tol::weld(size));
    let mut triangles = Vec::with_capacity(raw.triangles.len());
    for t in &raw.triangles {
        let mut out = [0u32; 3];
        for k in 0..3 {
            let i = map
                .get(t[k] as usize)
                .copied()
                .ok_or("メッシュの頂点番号が範囲外です。")?;
            out[k] = i;
        }
        if out[0] != out[1] && out[1] != out[2] && out[0] != out[2] {
            triangles.push(out);
        }
    }
    stopped(stop)?;
    Ok(Mesh {
        vertices,
        triangles,
    })
}

/// Merge points closer than `eps` (grid hashing with neighbour lookup). Returns the merged
/// points and the new index of every input point.
pub fn weld(points: &[[f64; 3]], eps: f64) -> (Vec<[f64; 3]>, Vec<u32>) {
    let cell = eps.max(f64::MIN_POSITIVE) * 4.;
    let key = |p: [f64; 3]| p.map(|v| (v / cell).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(points.len());
    let mut out: Vec<[f64; 3]> = Vec::with_capacity(points.len());
    let mut map = Vec::with_capacity(points.len());
    for &p in points {
        let k = key(p);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        for &i in list {
                            if out.get(i as usize).is_some_and(|q| dist(*q, p) <= eps) {
                                found = Some(i);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        let i = match found {
            Some(i) => i,
            None => {
                let i = out.len() as u32;
                out.push(p);
                grid.entry(k).or_default().push(i);
                i
            }
        };
        map.push(i);
    }
    (out, map)
}

// ---------------------------------------------------------------------------------------------
// Small vector helpers on NODES points
// ---------------------------------------------------------------------------------------------

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
fn dist(a: P3, b: P3) -> f64 {
    norm(sub(a, b))
}
pub fn v3(p: P3) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}
pub fn p3(v: Vec3) -> P3 {
    [v.x, v.y, v.z]
}
fn finite(p: P3) -> bool {
    p.iter()
        .all(|v| v.is_finite() && v.abs() <= limits::MAX_COORDINATE)
}

// ---------------------------------------------------------------------------------------------
// NODES curves as Bézier pieces
// ---------------------------------------------------------------------------------------------

/// One rational Bézier piece, homogeneous control points `(x·w, y·w, z·w, w)`.
#[derive(Clone, Debug)]
pub struct Piece {
    pub h: Vec<[f64; 4]>,
}

impl Piece {
    fn degree(&self) -> usize {
        self.h.len().saturating_sub(1)
    }
    fn point_at(&self, t: f64) -> P3 {
        let mut work = self.h.clone();
        let n = work.len();
        for r in 1..n {
            for i in 0..n - r {
                let (Some(&a), Some(&b)) = (work.get(i), work.get(i + 1)) else {
                    continue;
                };
                if let Some(slot) = work.get_mut(i) {
                    *slot = std::array::from_fn(|k| a[k] * (1. - t) + b[k] * t);
                }
            }
        }
        let h = work.first().copied().unwrap_or([0., 0., 0., 1.]);
        let w = if h[3].abs() > 1e-300 { h[3] } else { 1. };
        [h[0] / w, h[1] / w, h[2] / w]
    }
    fn start(&self) -> P3 {
        self.point_at(0.)
    }
    fn end(&self) -> P3 {
        self.point_at(1.)
    }
    fn points(&self) -> Vec<(P3, f64)> {
        self.h
            .iter()
            .map(|h| {
                let w = if h[3].abs() > 1e-300 { h[3] } else { 1. };
                ([h[0] / w, h[1] / w, h[2] / w], h[3])
            })
            .collect()
    }
    fn line(a: P3, b: P3) -> Piece {
        Piece {
            h: vec![[a[0], a[1], a[2], 1.], [b[0], b[1], b[2], 1.]],
        }
    }
}

/// A curve as Bézier pieces in order, and whether it is closed.
pub fn curve_pieces(c: &Curve, stop: Stop) -> R<(Vec<Piece>, bool)> {
    let pieces = match c.geometry.as_ref() {
        Some(g) => match g.as_nurbs() {
            Some(n) if n.degree() <= solidcraft_geom::bspline::MAX_SPLINE_DEGREE => {
                nurbs_pieces(n, g.domain(), stop)?
            }
            _ => polyline_pieces(&g.preview(512, stop)?, g.closed()?),
        },
        None => polyline_pieces(&c.points, c.closed),
    };
    if pieces.is_empty() {
        return Err("曲線に長さがありません。点が重なっていないか確認してください。".into());
    }
    if pieces.len() > limits::MAX_PIECES {
        return Err(format!(
            "曲線が細かすぎます（{} 区間、上限 {}）。「曲線を再構築」で制御点を減らしてください。",
            pieces.len(),
            limits::MAX_PIECES
        ));
    }
    let (first, last) = match (pieces.first(), pieces.last()) {
        (Some(f), Some(l)) => (f.start(), l.end()),
        _ => return Err("曲線が空です。".into()),
    };
    let size = pieces
        .iter()
        .flat_map(|p| p.points())
        .fold(0f64, |m, (p, _)| m.max(norm(p)));
    let closed = c.closed || dist(first, last) <= tol::CLOSE.max(size * 1e-9);
    if pieces
        .iter()
        .flat_map(|p| p.points())
        .any(|(p, w)| !finite(p) || w.is_nan() || w <= 0.)
    {
        return Err("曲線に無効な座標または重みがあります。".into());
    }
    Ok((pieces, closed))
}

fn polyline_pieces(points: &[P3], closed: bool) -> Vec<Piece> {
    let mut out = Vec::new();
    let n = points.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    for i in 0..count {
        let (Some(&a), Some(&b)) = (points.get(i), points.get((i + 1) % n.max(1))) else {
            continue;
        };
        if dist(a, b) > tol::ABS {
            out.push(Piece::line(a, b));
        }
    }
    out
}

/// Exact Bézier decomposition of a NURBS curve over `interval` (reversed when it runs backwards),
/// using each basis function's Bernstein coefficients on every knot span.
fn nurbs_pieces(n: &NurbsCurve, interval: [f64; 2], stop: Stop) -> R<Vec<Piece>> {
    let p = n.degree();
    let knots = n.knots();
    let h: Vec<[f64; 4]> = n
        .control_points()
        .into_iter()
        .zip(n.weights())
        .map(|(q, w)| [q[0] * w, q[1] * w, q[2] * w, w])
        .collect();
    let (lo, hi, reversed) = if interval[0] <= interval[1] {
        (interval[0], interval[1], false)
    } else {
        (interval[1], interval[0], true)
    };
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return Err("曲線の定義域が空です。".into());
    }
    let mut breaks = vec![lo];
    for &k in knots {
        if k > lo && k < hi && breaks.last().is_some_and(|&b| k > b) {
            breaks.push(k);
        }
    }
    breaks.push(hi);
    if breaks.len() > limits::MAX_PIECES + 1 {
        return Err(
            "曲線のノットが多すぎます。「曲線を再構築」で制御点を減らしてください。".into(),
        );
    }
    let mut pieces = Vec::with_capacity(breaks.len());
    for w in breaks.windows(2) {
        stopped(stop)?;
        let &[a, b] = w else { continue };
        if b - a <= 0. {
            continue;
        }
        let mut bez = vec![[0f64; 4]; p + 1];
        for (j, hj) in h.iter().enumerate() {
            let Some(local) = knots.get(j..=j + p + 1) else {
                continue;
            };
            let (Some(&k0), Some(&k1)) = (local.first(), local.last()) else {
                continue;
            };
            if k1 <= a || k0 >= b {
                continue;
            }
            let c = basis_bezier(local, a, b)
                .map_err(|e| format!("曲線の分解に失敗しました（{e}）。"))?;
            for (slot, ck) in bez.iter_mut().zip(c) {
                for d in 0..4 {
                    slot[d] += ck * hj[d];
                }
            }
        }
        pieces.push(Piece { h: bez });
    }
    if reversed {
        pieces.reverse();
        for piece in &mut pieces {
            piece.h.reverse();
        }
    }
    Ok(pieces)
}

/// Points along a curve (every piece sampled by its degree), for paths, rails and fitting.
pub fn sample_curve(c: &Curve, per_piece: usize, stop: Stop) -> R<Vec<P3>> {
    let (pieces, closed) = curve_pieces(c, stop)?;
    let mut out: Vec<P3> = Vec::new();
    for piece in &pieces {
        let k = if piece.degree() <= 1 {
            1
        } else {
            per_piece.max(2) * piece.degree()
        };
        for i in 0..k {
            out.push(piece.point_at(i as f64 / k as f64));
        }
        if out.len() > limits::MAX_SAMPLES {
            return Err("曲線の点が多すぎます。「曲線を再構築」で単純にしてください。".into());
        }
    }
    if let Some(last) = pieces.last() {
        if closed {
            if let Some(&first) = out.first() {
                out.push(first);
            }
        } else {
            out.push(last.end());
        }
    }
    out.dedup_by(|a, b| dist(*a, *b) <= tol::ABS);
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Planar profiles
// ---------------------------------------------------------------------------------------------

/// The plane of a set of points (Newell normal of the polygon, centroid origin), oriented along
/// `hint` when given. Fails when the points are not planar within [`tol::PLANAR`].
pub fn fit_plane(points: &[P3], hint: Option<P3>) -> R<Plane> {
    if points.len() < 3 {
        return Err("平面を決めるには 3 点以上が必要です。".into());
    }
    let c = scale(
        points.iter().fold([0.; 3], |s, p| add(s, *p)),
        1. / points.len() as f64,
    );
    let mut nrm = [0.; 3];
    for (i, a) in points.iter().enumerate() {
        let b = points.get((i + 1) % points.len()).copied().unwrap_or(*a);
        nrm = add(nrm, cross(sub(*a, c), sub(b, c)));
    }
    let size = points.iter().fold(0f64, |m, p| m.max(dist(*p, c)));
    if norm(nrm) <= (size * size) * 1e-12 {
        match hint {
            Some(h) if norm(h) > 0. => {
                // A straight profile: any plane through it that contains the hint direction.
                let along = points.iter().fold([0.; 3], |best, p| {
                    let d = sub(*p, c);
                    if norm(d) > norm(best) { d } else { best }
                });
                nrm = cross(along, cross(h, along));
                if norm(nrm) <= 1e-12 {
                    nrm = h;
                }
            }
            _ => return Err("曲線が一直線上にあり、平面を決められません。".into()),
        }
    }
    let mut n = scale(nrm, 1. / norm(nrm));
    if let Some(h) = hint
        && dot(n, h) < 0.
    {
        n = scale(n, -1.);
    }
    let worst = points
        .iter()
        .fold(0f64, |m, p| m.max(dot(sub(*p, c), n).abs()));
    let allowed = tol::PLANAR.max(size * 1e-7);
    if worst > allowed {
        return Err(format!(
            "断面曲線が平面上にありません（平面から最大 {worst:.4} mm 離れています）。平らな曲線を使ってください。"
        ));
    }
    Plane::from_normal(v3(c), v3(n)).ok_or_else(|| "断面の平面を作れませんでした。".to_string())
}

fn to2(plane: &Plane, p: P3) -> Vec2 {
    plane.to_local(v3(p))
}

/// Sketch segments of pieces on a plane, each starting exactly where the one before it ends.
fn pieces_to_segs(plane: &Plane, pieces: &[Piece], close: bool) -> R<Vec<Seg2>> {
    let mut raw: Vec<(Vec<Vec2>, Vec<f64>)> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let pts = piece.points();
        let p2: Vec<Vec2> = pts.iter().map(|(p, _)| to2(plane, *p)).collect();
        let w: Vec<f64> = pts.iter().map(|(_, w)| *w).collect();
        if p2.windows(2).all(|q| {
            q.first()
                .zip(q.get(1))
                .is_some_and(|(a, b)| a.dist(*b) <= tol::ABS)
        }) {
            continue;
        }
        raw.push((p2, w));
    }
    // Chain exactly: every start is the previous end, and a closed chain ends at its start.
    for i in 1..raw.len() {
        let prev_end = raw.get(i - 1).and_then(|(p, _)| p.last().copied());
        if let (Some(end), Some((p, _))) = (prev_end, raw.get_mut(i))
            && let Some(first) = p.first_mut()
        {
            *first = end;
        }
    }
    if close {
        let start = raw.first().and_then(|(p, _)| p.first().copied());
        let gap = match (start, raw.last().and_then(|(p, _)| p.last().copied())) {
            (Some(s), Some(e)) => s.dist(e),
            _ => 0.,
        };
        if gap > tol::CLOSE {
            return Err(format!(
                "断面曲線が閉じていません（端点の間が {gap:.4} mm 空いています）。閉じた曲線を使ってください。"
            ));
        }
        if let (Some(s), Some((p, _))) = (start, raw.last_mut())
            && let Some(last) = p.last_mut()
        {
            *last = s;
        }
    }
    let mut segs = Vec::with_capacity(raw.len());
    for (p, w) in raw {
        segs.extend(seg_of(&p, &w));
    }
    if segs.is_empty() {
        return Err("断面曲線に長さがありません。".into());
    }
    Ok(segs)
}

fn seg_of(p: &[Vec2], w: &[f64]) -> Vec<Seg2> {
    let deg = p.len().saturating_sub(1);
    let w0 = w.first().copied().unwrap_or(1.);
    let rational = w.iter().any(|x| (x / w0 - 1.).abs() > 1e-12);
    let at = |i: usize| p.get(i).copied().unwrap_or_default();
    match (deg, rational) {
        (1, _) => vec![Seg2::Line { a: at(0), b: at(1) }],
        (2, _) => {
            let (wa, wm, wb) = (
                w0,
                w.get(1).copied().unwrap_or(1.),
                w.get(2).copied().unwrap_or(1.),
            );
            let k = (wa * wb).sqrt();
            if k > 0. && wm > 0. {
                vec![Seg2::Conic {
                    a: at(0),
                    apex: at(1),
                    b: at(2),
                    w: wm / k,
                }]
            } else {
                vec![Seg2::Line { a: at(0), b: at(2) }]
            }
        }
        (3, false) => vec![Seg2::Cubic {
            p0: at(0),
            p1: at(1),
            p2: at(2),
            p3: at(3),
        }],
        (4..=9, false) if deg < MAX_BEZIER_CTRL => {
            let mut ctrl = [Vec2::default(); MAX_BEZIER_CTRL];
            for (slot, q) in ctrl.iter_mut().zip(p) {
                *slot = *q;
            }
            vec![Seg2::Bezier {
                n: deg as u8,
                p: ctrl,
            }]
        }
        _ => {
            // A rational piece above degree 2: followed by short lines (approximation).
            let piece = Piece {
                h: p.iter()
                    .zip(w)
                    .map(|(q, wi)| [q.x * wi, q.y * wi, 0., *wi])
                    .collect(),
            };
            let k = 8 * deg.max(1);
            let pts: Vec<Vec2> = (0..=k)
                .map(|i| {
                    let q = piece.point_at(i as f64 / k as f64);
                    Vec2::new(q[0], q[1])
                })
                .collect();
            let mut out = Vec::with_capacity(k);
            for i in 0..k {
                let a = if i == 0 {
                    at(0)
                } else {
                    pts.get(i).copied().unwrap_or_default()
                };
                let b = if i + 1 == k {
                    at(deg)
                } else {
                    pts.get(i + 1).copied().unwrap_or_default()
                };
                out.push(Seg2::Line { a, b });
            }
            out
        }
    }
}

/// A closed planar curve as a plane and a counter-clockwise loop on it.
pub fn profile(c: &Curve, hint: Option<P3>, stop: Stop) -> R<(Plane, Loop2)> {
    let (pieces, closed) = curve_pieces(c, stop)?;
    if !closed {
        return Err(
            "断面曲線が閉じていません。円・多角形など閉じた曲線を接続してください。".into(),
        );
    }
    let samples = sample_curve(c, 4, stop)?;
    let plane = fit_plane(&samples, hint)?;
    let segs = pieces_to_segs(&plane, &pieces, true)?;
    let lp = Loop2 { segs };
    Ok((
        plane,
        if lp.signed_area() < 0. {
            lp.reversed()
        } else {
            lp
        },
    ))
}

/// A planar curve projected onto a given plane (it must lie on it).
fn loop_on(plane: &Plane, c: &Curve, stop: Stop) -> R<Loop2> {
    let (pieces, closed) = curve_pieces(c, stop)?;
    if !closed {
        return Err("穴の曲線が閉じていません。".into());
    }
    for p in sample_curve(c, 2, stop)? {
        if plane.height(v3(p)).abs() > tol::PLANAR.max(1e-7 * norm(p)) {
            return Err("穴の曲線が外形と同じ平面にありません。".into());
        }
    }
    let lp = Loop2 {
        segs: pieces_to_segs(plane, &pieces, true)?,
    };
    Ok(if lp.signed_area() < 0. {
        lp.reversed()
    } else {
        lp
    })
}

// ---------------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------------

pub(crate) fn brep(body: Body, stop: Stop) -> R<BrepValue> {
    BrepValue::new(body, stop)
}

pub(crate) fn first_body(what: &str, bodies: Vec<Body>, stop: Stop) -> R<BrepValue> {
    let mut iter = bodies.into_iter();
    let Some(first) = iter.next() else {
        return Err(format!(
            "{what}: 結果が空です。断面の大きさを確認してください。"
        ));
    };
    let mut acc = first;
    for b in iter {
        stopped(stop)?;
        if let Some(u) = kernel(what, stop, || sk::boolean(&acc, &b, sk::BoolOp::Union))? {
            acc = u;
        }
    }
    brep(acc, stop)
}

pub(crate) fn positive(what: &str, v: f64) -> R<f64> {
    if v.is_finite() && v > tol::ABS && v <= limits::MAX_COORDINATE {
        Ok(v)
    } else {
        Err(format!(
            "{what}は 0 より大きく {} 以下の値にしてください。",
            limits::MAX_COORDINATE
        ))
    }
}

/// Exact axis-aligned box centred on the XY origin, standing on Z = 0.
pub fn box_solid(w: f64, d: f64, h: f64, stop: Stop) -> R<BrepValue> {
    let (w, d, h) = (
        positive("幅", w)?,
        positive("奥行", d)?,
        positive("高さ", h)?,
    );
    let body = kernel("直方体", stop, || {
        sk::box_solid(
            Vec3::new(-w / 2., -d / 2., 0.),
            Vec3::new(w / 2., d / 2., h),
        )
    })?;
    brep(body, stop)
}

/// Exact cylinder with an arbitrary base point and axis.
pub fn cylinder(base: P3, axis: P3, radius: f64, height: f64, stop: Stop) -> R<BrepValue> {
    let (r, h) = (positive("半径", radius)?, positive("高さ", height)?);
    if !finite(base) || norm(axis) <= tol::ABS {
        return Err("円柱の基点または軸の向きが無効です。".into());
    }
    let body = kernel("円柱", stop, || sk::cylinder(v3(base), v3(axis), r, h))?;
    brep(body, stop)
}

/// Exact sphere.
pub fn sphere(center: P3, radius: f64, stop: Stop) -> R<BrepValue> {
    let r = positive("半径", radius)?;
    if !finite(center) {
        return Err("球の中心が無効です。".into());
    }
    let body = kernel("球", stop, || sk::sphere(v3(center), r))?;
    brep(body, stop)
}

/// Extrude a closed planar curve (with optional holes) by `height` along its plane normal
/// (the normal faces +Z where it can).
pub fn extrude(outer: &Curve, holes: &[&Curve], height: f64, stop: Stop) -> R<BrepValue> {
    if !height.is_finite() || height.abs() <= tol::ABS || height.abs() > limits::MAX_COORDINATE {
        return Err("押し出しの高さは 0 以外の有限な値にしてください。".into());
    }
    let (plane, lp) = profile(outer, Some([0., 0., 1.]), stop)?;
    let holes = holes
        .iter()
        .map(|h| loop_on(&plane, h, stop).map(|l| l.reversed()))
        .collect::<R<Vec<_>>>()?;
    let region = Region2 { outer: lp, holes };
    let (start, end) = if height > 0. {
        (0., height)
    } else {
        (height, 0.)
    };
    let bodies = kernel("押し出し", stop, || {
        sk::extrude(&plane, std::slice::from_ref(&region), start, end)
    })?;
    first_body("押し出し", bodies, stop)
}

/// Revolve a planar profile about an axis lying in its plane. An open profile is closed with
/// a straight line (a vase profile whose ends sit on the axis closes along the axis).
pub fn revolve(
    profile_curve: &Curve,
    axis_point: P3,
    axis_dir: P3,
    angle_deg: f64,
    stop: Stop,
) -> R<BrepValue> {
    if !finite(axis_point) || norm(axis_dir) <= tol::ABS {
        return Err("回転軸の点または向きが無効です。".into());
    }
    if !angle_deg.is_finite() || angle_deg.abs() <= 1e-6 || angle_deg.abs() > 360. {
        return Err("回転角度は 0 より大きく 360 以下にしてください。".into());
    }
    let (pieces, closed) = curve_pieces(profile_curve, stop)?;
    let mut samples = sample_curve(profile_curve, 4, stop)?;
    let dir = scale(axis_dir, 1. / norm(axis_dir));
    let length = samples
        .iter()
        .fold(1f64, |m, p| m.max(dist(*p, axis_point)));
    samples.push(axis_point);
    samples.push(add(axis_point, scale(dir, length)));
    let plane = fit_plane(&samples, None).map_err(|_| {
        "回転軸が断面曲線と同じ平面にありません。断面を軸を含む平面（例: XZ 平面）に描いてください。".to_string()
    })?;
    let mut pieces = pieces;
    if !closed {
        let (Some(first), Some(last)) = (
            pieces.first().map(Piece::start),
            pieces.last().map(Piece::end),
        ) else {
            return Err("断面曲線が空です。".into());
        };
        pieces.push(Piece::line(last, first));
    }
    let segs = pieces_to_segs(&plane, &pieces, true)?;
    let lp = Loop2 { segs };
    let lp = if lp.signed_area() < 0. {
        lp.reversed()
    } else {
        lp
    };
    let o2 = to2(&plane, axis_point);
    let d2 = to2(&plane, add(axis_point, dir)) - o2;
    let region = Region2 {
        outer: lp,
        holes: Vec::new(),
    };
    let bodies = kernel("回転体", stop, || {
        sk::revolve(
            &plane,
            std::slice::from_ref(&region),
            o2,
            d2,
            angle_deg.to_radians(),
        )
    })?;
    first_body("回転体", bodies, stop)
}

fn sections_of(curves: &[&Curve], stop: Stop) -> R<Vec<(Plane, Loop2)>> {
    if curves.len() < 2 {
        return Err("ロフトには 2 本以上の断面曲線が必要です。".into());
    }
    if curves.len() > limits::MAX_SECTIONS {
        return Err(format!(
            "断面が多すぎます（上限 {}）。",
            limits::MAX_SECTIONS
        ));
    }
    let centroid = |c: &Curve| -> R<P3> {
        let s = sample_curve(c, 2, stop)?;
        Ok(scale(
            s.iter().fold([0.; 3], |a, p| add(a, *p)),
            1. / s.len().max(1) as f64,
        ))
    };
    let (Some(first), Some(last)) = (curves.first(), curves.last()) else {
        return Err("断面がありません。".into());
    };
    let along = sub(centroid(last)?, centroid(first)?);
    let hint = if norm(along) > tol::ABS {
        along
    } else {
        [0., 0., 1.]
    };
    curves
        .iter()
        .map(|c| profile(c, Some(hint), stop))
        .collect()
}

/// Loft closed planar sections into a solid. Smooth (one B-spline wall through all sections,
/// capped with planar faces and sewn) when there are three or more sections and `smooth` is set;
/// otherwise the kernel's ruled solid loft.
pub fn loft(curves: &[&Curve], smooth: bool, stop: Stop) -> R<BrepValue> {
    let sections = sections_of(curves, stop)?;
    if smooth && sections.len() >= 3 {
        match smooth_loft(&sections, stop) {
            Ok(b) if b.is_closed() => return Ok(b),
            Ok(_) | Err(_) => {}
        }
    }
    let body = kernel("ロフト", stop, || sk::loft(&sections))?;
    brep(body, stop)
}

fn smooth_loft(sections: &[(Plane, Loop2)], stop: Stop) -> R<BrepValue> {
    let chains: Vec<(Plane, Vec<Seg2>)> =
        sections.iter().map(|(p, l)| (*p, l.segs.clone())).collect();
    let wall = kernel("ロフト", stop, || sk::loft_sheet(&chains))?;
    let (Some(first), Some(last)) = (sections.first(), sections.last()) else {
        return Err("断面がありません。".into());
    };
    let cap = |s: &(Plane, Loop2)| {
        let region = Region2 {
            outer: s.1.clone(),
            holes: Vec::new(),
        };
        kernel("ロフトの端面", stop, || {
            sk::patch_region(&s.0, &region)
        })
    };
    let (a, b) = (cap(first)?, cap(last)?);
    let solid = kernel("ロフトの縫い合わせ", stop, || {
        sk::stitch(&[&wall, &a, &b], tol::STITCH)
    })?;
    brep(solid, stop)
}

/// A smooth open surface lofted through sections (closed or open, planar).
pub fn loft_sheet(curves: &[&Curve], stop: Stop) -> R<BrepValue> {
    let chains = curves
        .iter()
        .map(|c| {
            let (pieces, closed) = curve_pieces(c, stop)?;
            let plane = fit_plane(&sample_curve(c, 4, stop)?, Some([0., 0., 1.]))?;
            Ok((plane, pieces_to_segs(&plane, &pieces, closed)?))
        })
        .collect::<R<Vec<_>>>()?;
    let body = kernel("ロフト（面）", stop, || sk::loft_sheet(&chains))?;
    brep(body, stop)
}

/// Sweep a closed planar profile along a rail (Sweep1).
pub fn sweep1(profile_curve: &Curve, rail: &Curve, stop: Stop) -> R<BrepValue> {
    let path = sample_curve(rail, 6, stop)?;
    let tangent = match path.as_slice() {
        [a, b, ..] => sub(*b, *a),
        _ => return Err("レール曲線が短すぎます。".into()),
    };
    let (plane, lp) = profile(profile_curve, Some(tangent), stop)?;
    // Parallel transport along a planar rail keeps a parallel profile in its
    // original plane, so there is no volume to sweep.
    let start = path[0];
    let normal = plane.normal();
    if path
        .iter()
        .skip(1)
        .all(|point| normal.dot(v3(sub(*point, start))).abs() <= tol::ABS)
    {
        return Err("断面とレールが平行な平面にあり、立体を作れません。レールが断面を抜ける向きに変えてください。".into());
    }
    let region = Region2 {
        outer: lp,
        holes: Vec::new(),
    };
    let path3: Vec<Vec3> = path.iter().map(|p| v3(*p)).collect();
    let body = kernel("スイープ", stop, || {
        sk::sweep_path(&plane, &region, &path3)
    })?;
    brep(body, stop)
}

/// Sweep a closed planar profile along a rail, shaped by a second rail (Sweep2).
pub fn sweep2(profile_curve: &Curve, rail: &Curve, rail2: &Curve, stop: Stop) -> R<BrepValue> {
    let path = sample_curve(rail, 6, stop)?;
    let second = sample_curve(rail2, 6, stop)?;
    let tangent = match path.as_slice() {
        [a, b, ..] => sub(*b, *a),
        _ => return Err("レール曲線が短すぎます。".into()),
    };
    let (plane, lp) = profile(profile_curve, Some(tangent), stop)?;
    let region = Region2 {
        outer: lp,
        holes: Vec::new(),
    };
    let path3: Vec<Vec3> = path.iter().map(|p| v3(*p)).collect();
    let ctl = sk::SweepControl {
        rails: vec![second.iter().map(|p| v3(*p)).collect()],
        ..Default::default()
    };
    let body = kernel("2 レールスイープ", stop, || {
        sk::sweep_path_with(&plane, &region, &path3, &ctl)
    })?;
    brep(body, stop)
}

/// A B-spline surface through a network of curves (Gordon / Coons / loft).
pub fn network_surface(u: &[&Curve], v: &[&Curve], stop: Stop) -> R<BrepValue> {
    if u.len() < 2 && v.len() < 2 {
        return Err("曲線ネットワークには、どちらかの向きに 2 本以上の曲線が必要です。".into());
    }
    let make = |list: &[&Curve], name: &str| -> R<Vec<sk::BoundaryCurve<'static>>> {
        list.iter()
            .enumerate()
            .map(|(i, c)| {
                Ok(sk::BoundaryCurve {
                    label: format!("{name}{}", i + 1),
                    source: sk::CurveSource::Points(
                        sample_curve(c, 6, stop)?.into_iter().map(v3).collect(),
                    ),
                    continuity: sk::Continuity::G0,
                })
            })
            .collect()
    };
    let (uc, vc) = (make(u, "U")?, make(v, "V")?);
    let body = kernel("ネットワーク曲面", stop, || {
        sk::boundary_surface(&uc, &vc)
    })?;
    brep(body, stop)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bool {
    Union,
    Difference,
    Intersection,
}

/// Exact solid boolean.
pub fn boolean(a: &BrepValue, b: &BrepValue, op: Bool, stop: Stop) -> R<BrepValue> {
    let (what, kop) = match op {
        Bool::Union => ("和（ブーリアン）", sk::BoolOp::Union),
        Bool::Difference => ("差（ブーリアン）", sk::BoolOp::Cut),
        Bool::Intersection => ("積（ブーリアン）", sk::BoolOp::Intersect),
    };
    if !a.is_closed() || !b.is_closed() {
        return Err(format!(
            "{what}: 閉じた立体どうしでのみ計算できます。開いた面は「穴をふさぐ」で閉じてください。"
        ));
    }
    match kernel(what, stop, || sk::boolean(a.body(), b.body(), kop))? {
        Some(body) => {
            let mut result = brep(body, stop)?;
            if !result.is_closed() {
                // The kernel may produce a valid B-rep whose trimmed faces leave gaps in
                // the mesh, for example at the tangencies of two equal crossing cylinders.
                // Keep the original body for STEP, measurement and subsequent operations.
                let preview = faceted_boolean_preview(a.body(), b.body(), result.body(), kop, stop);
                stopped(stop)?;
                if let Ok(preview) = preview {
                    result.preview = Arc::new(preview);
                    result.closed = true;
                }
            }
            Ok(result)
        }
        None => Err(format!(
            "{what}: 結果が空です。2 つの立体が重なっているか確認してください。"
        )),
    }
}

fn faceted_boolean_preview(
    a: &Body,
    b: &Body,
    result: &Body,
    op: sk::BoolOp,
    stop: Stop,
) -> R<Mesh> {
    let chord = tol::chord(result.size());
    let a = a.keeping_meshes();
    let (target, tool) = kernel("ブーリアンの表示メッシュ", stop, || {
        Ok((a.tessellate(chord)?, b.tessellate(chord)?))
    })?;
    if target.triangles.len().saturating_add(tool.triangles.len())
        > limits::MAX_FACETED_BOOLEAN_TRIANGLES
    {
        return Err("ブーリアンの表示メッシュが大きすぎます。".into());
    }
    for (body, mesh) in [(&a, &target), (b, &tool)] {
        if !is_watertight(&weld_tessellation(mesh, body.size(), stop)?) {
            return Err("入力の表示メッシュが閉じていません。".into());
        }
    }
    let faceted = kernel("ブーリアンの表示メッシュ", stop, || {
        sk::faceted_boolean(&a, &tool, op, chord)
    })?
    .ok_or("ブーリアンの表示メッシュが空です。")?;
    let raw = kernel("メッシュ化", stop, || faceted.tessellate(chord))?;
    let bounds = raw.bounds();
    let mesh = weld_tessellation(&raw, result.size(), stop)?;
    let volume = crate::mesh_fabrication::signed_volume(&mesh, stop)?;
    let exact = kernel("体積", stop, || sk::measure_mass(result))?;
    if !exact.volume.is_finite()
        || volume <= 0.0
        || (volume - exact.volume).abs() > exact.volume * tol::BOOLEAN_PREVIEW_VOLUME_ERROR
        || bounds
            .min
            .dist(exact.bbox.min)
            .max(bounds.max.dist(exact.bbox.max))
            > chord * 4.0
    {
        return Err("表示メッシュがブーリアンの形状と一致しません。".into());
    }
    Ok(mesh)
}

/// Edge pick points: every edge, or the listed edge numbers.
pub(crate) fn edge_picks(b: &BrepValue, only: &[usize], stop: Stop) -> R<Vec<Vec3>> {
    let edges = kernel("稜線の取得", stop, || {
        b.body().edges(tol::chord(b.body().size()))
    })?;
    if only.is_empty() {
        return Ok(edges.iter().map(|e| e.mid).collect());
    }
    only.iter()
        .map(|&i| {
            edges.get(i).map(|e| e.mid).ok_or_else(|| {
                format!(
                    "稜線番号 {i} はありません（0〜{} を指定してください）。",
                    edges.len().saturating_sub(1)
                )
            })
        })
        .collect()
}

/// Constant-radius fillet of all edges (or the listed ones).
pub fn fillet(b: &BrepValue, radius: f64, only: &[usize], stop: Stop) -> R<BrepValue> {
    let r = positive("フィレット半径", radius)?;
    let picks = edge_picks(b, only, stop)?;
    let body = kernel("フィレット", stop, || sk::fillet(b.body(), &picks, r))?;
    brep(body, stop)
}

/// Equal-distance chamfer of all edges (or the listed ones).
pub fn chamfer(b: &BrepValue, distance: f64, only: &[usize], stop: Stop) -> R<BrepValue> {
    let d = positive("面取り距離", distance)?;
    let picks = edge_picks(b, only, stop)?;
    let body = kernel("面取り", stop, || sk::chamfer(b.body(), &picks, d))?;
    brep(body, stop)
}

/// Hollow a solid with walls of `thickness`, removing the face with the given number, or by
/// default the highest face (an open box or cup). `open = None` keeps every face (a sealed void).
pub fn shell(b: &BrepValue, thickness: f64, open: Option<usize>, stop: Stop) -> R<BrepValue> {
    let t = positive("厚み", thickness)?;
    let faces = kernel("面の取得", stop, || {
        b.body().faces(tol::chord(b.body().size()))
    })?;
    let open_at: Vec<Vec3> = match open {
        None => Vec::new(),
        Some(usize::MAX) => faces
            .iter()
            .max_by(|x, y| x.centroid.z.total_cmp(&y.centroid.z))
            .map(|f| vec![f.centroid])
            .unwrap_or_default(),
        Some(i) => vec![faces.get(i).map(|f| f.centroid).ok_or_else(|| {
            format!(
                "面番号 {i} はありません（0〜{} を指定してください）。",
                faces.len().saturating_sub(1)
            )
        })?],
    };
    match kernel("シェル", stop, || sk::shell(b.body(), &open_at, t)) {
        Ok(body) => brep(body, stop),
        Err(e) if e == STOPPED => Err(e),
        Err(planar) => {
            // Curved solids: take the open face away and thicken the rest inward.
            let face = match open {
                Some(usize::MAX) => crate::kernel_ops::top_face(b, stop)?,
                Some(i) => i,
                None => {
                    return Err(format!(
                        "{planar}（閉じたままの中空化は平面の凸形状だけに対応しています。開ける面を選んでください。）"
                    ));
                }
            };
            crate::kernel_ops::shell_curved(b, t, face, stop)
                .map_err(|curved| format!("{curved}（平面の方法でも失敗しました: {planar}）"))
        }
    }
}

/// Close the open boundary loops of a surface body with fill faces and sew it into a solid.
pub fn cap_holes(b: &BrepValue, stop: Stop) -> R<BrepValue> {
    if b.is_closed() {
        return Ok(b.clone());
    }
    let mesh = b.preview();
    let open = boundary_edges(mesh);
    if open.is_empty() {
        return Err("ふさぐ穴が見つかりません。".into());
    }
    // Group the open mesh edges into loops (connected components).
    let mut parent: HashMap<u32, u32> = HashMap::new();
    fn find(parent: &mut HashMap<u32, u32>, x: u32) -> u32 {
        let mut r = x;
        let mut guard = 0;
        while let Some(&p) = parent.get(&r) {
            if p == r || guard > 1_000_000 {
                break;
            }
            r = p;
            guard += 1;
        }
        parent.insert(x, r);
        r
    }
    for &(a, c) in &open {
        parent.entry(a).or_insert(a);
        parent.entry(c).or_insert(c);
        let (ra, rc) = (find(&mut parent, a), find(&mut parent, c));
        if ra != rc {
            parent.insert(ra, rc);
        }
    }
    let mut loops: HashMap<u32, Vec<(P3, P3)>> = HashMap::new();
    for &(a, c) in &open {
        let root = find(&mut parent, a);
        if let (Some(&pa), Some(&pc)) =
            (mesh.vertices.get(a as usize), mesh.vertices.get(c as usize))
        {
            loops.entry(root).or_default().push((pa, pc));
        }
    }
    let size = b.body().size();
    let edges = kernel("稜線の取得", stop, || b.body().edges(tol::chord(size)))?;
    let near = tol::chord(size) * 4. + tol::ABS;
    let mut caps: Vec<Body> = Vec::new();
    let mut keys: Vec<_> = loops.keys().copied().collect();
    keys.sort_unstable();
    for key in keys {
        stopped(stop)?;
        let Some(segments) = loops.get(&key) else {
            continue;
        };
        let picks: Vec<Vec3> = edges
            .iter()
            .filter(|e| {
                let m = p3(e.mid);
                segments
                    .iter()
                    .any(|(a, c)| point_segment_distance(m, *a, *c) <= near)
            })
            .map(|e| e.mid)
            .collect();
        if picks.is_empty() {
            return Err("穴のふちの稜線が見つかりませんでした。面の形を確認してください。".into());
        }
        caps.push(kernel("穴をふさぐ", stop, || {
            sk::patch_edges(b.body(), &picks)
        })?);
    }
    let mut all: Vec<&Body> = vec![b.body()];
    all.extend(caps.iter());
    let solid = kernel("縫い合わせ", stop, || sk::stitch(&all, tol::STITCH))?;
    brep(solid, stop)
}

fn point_segment_distance(p: P3, a: P3, b: P3) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0. {
        (dot(sub(p, a), ab) / l2).clamp(0., 1.)
    } else {
        0.
    };
    dist(p, add(a, scale(ab, t)))
}

/// Move a body.
pub fn translate(b: &BrepValue, d: P3, stop: Stop) -> R<BrepValue> {
    if !finite(d) {
        return Err("移動量が無効です。".into());
    }
    let body = kernel("移動", stop, || {
        sk::transform(b.body(), v3(d), Vec3::ZERO, Vec3::Z, 0.)
    })?;
    brep(body, stop)
}

// ---------------------------------------------------------------------------------------------
// STEP
// ---------------------------------------------------------------------------------------------

/// STEP AP242 text for the bodies (one part).
pub fn step_export(bodies: &[&BrepValue], stop: Stop) -> R<String> {
    if bodies.is_empty() {
        return Err("書き出す B-rep がありません。".into());
    }
    let refs: Vec<&Body> = bodies.iter().map(|b| b.body()).collect();
    let text = kernel("STEP 書き出し", stop, || {
        sk::step_export(&refs, "NODES")
    })?;
    if text.len() > limits::MAX_STEP_BYTES {
        return Err("STEP が大きすぎます。".into());
    }
    Ok(text)
}

/// IGES 5.3 text for the bodies (through SolidCraft's STEP → IGES writer).
pub fn iges_export(bodies: &[&BrepValue], stop: Stop) -> R<String> {
    if bodies.is_empty() {
        return Err("書き出す B-rep がありません。".into());
    }
    let list: Vec<sk::ExportBody> = bodies
        .iter()
        .enumerate()
        .map(|(i, b)| sk::ExportBody {
            name: format!("Body{}", i + 1),
            body: b.body(),
            color: None,
        })
        .collect();
    let header = sk::StepHeader {
        file_name: "NODES".into(),
        time_stamp: String::new(),
        author: String::new(),
        organization: String::new(),
        description: "NODES model".into(),
    };
    let text = kernel("IGES 書き出し", stop, || {
        sk::iges_export_bodies(&list, &header)
    })?;
    if text.len() > limits::MAX_STEP_BYTES {
        return Err("IGES が大きすぎます。".into());
    }
    Ok(text)
}

/// Bodies read from IGES text.
pub fn iges_import(text: &str, stop: Stop) -> R<Vec<BrepValue>> {
    if text.len() > limits::MAX_STEP_BYTES {
        return Err(format!(
            "IGES ファイルが大きすぎます（上限 {} MB）。",
            limits::MAX_STEP_BYTES >> 20
        ));
    }
    let import = kernel("IGES 読み込み", stop, || sk::iges_import(text))?;
    if import.bodies.is_empty() {
        return Err(
            "IGES に読み込める立体がありませんでした（面だけのファイルは未対応のことがあります）。"
                .into(),
        );
    }
    import
        .bodies
        .into_iter()
        .map(|b| brep(b.body, stop))
        .collect()
}

/// Bodies read from STEP text.
pub fn step_import(text: &str, stop: Stop) -> R<Vec<BrepValue>> {
    if text.len() > limits::MAX_STEP_BYTES {
        return Err(format!(
            "STEP ファイルが大きすぎます（上限 {} MB）。",
            limits::MAX_STEP_BYTES >> 20
        ));
    }
    let import = kernel("STEP 読み込み", stop, || sk::step_import(text))?;
    if import.bodies.is_empty() {
        return Err("STEP に読み込める立体がありませんでした。".into());
    }
    import
        .bodies
        .into_iter()
        .map(|b| brep(b.body, stop))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Curves: rebuild and intersections (independent numerical code)
// ---------------------------------------------------------------------------------------------

/// A NODES curve value from exact geometry.
pub fn curve_value(g: CurveGeometry, stop: Stop) -> R<Curve> {
    let closed = g.closed()?;
    let points = g.preview(128, stop)?;
    Ok(Curve {
        points,
        closed,
        geometry: Some(g),
    })
}

/// Rebuild a curve with `count` control points of `degree`, through points at equal arc length.
pub fn rebuild_curve(c: &Curve, count: usize, degree: usize, stop: Stop) -> R<Curve> {
    if !(2..=128).contains(&count) || degree == 0 || degree >= count {
        return Err("制御点数は 2〜128、次数は制御点数より小さくしてください。".into());
    }
    let (_, closed) = curve_pieces(c, stop)?;
    let pts = sample_curve(c, 16, stop)?;
    // Cumulative length over the dense samples.
    let mut acc = vec![0.];
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else {
            continue;
        };
        let last = acc.last().copied().unwrap_or(0.);
        acc.push(last + dist(*a, *b));
    }
    let total = acc.last().copied().unwrap_or(0.);
    if total <= tol::ABS {
        return Err("曲線に長さがありません。".into());
    }
    let n = if closed { count } else { count - 1 };
    let mut out = Vec::with_capacity(count);
    let mut j = 0usize;
    for i in 0..count {
        let s = total * i as f64 / n.max(1) as f64;
        while j + 2 < acc.len() && acc.get(j + 1).is_some_and(|&x| x < s) {
            j += 1;
        }
        let (a, b) = (
            acc.get(j).copied().unwrap_or(0.),
            acc.get(j + 1).copied().unwrap_or(total),
        );
        let t = if b > a {
            ((s - a) / (b - a)).clamp(0., 1.)
        } else {
            0.
        };
        let (Some(&pa), Some(&pb)) = (pts.get(j), pts.get(j + 1)) else {
            continue;
        };
        out.push(add(pa, scale(sub(pb, pa), t)));
    }
    let nurbs = if closed {
        NurbsCurve::uniform(out, degree, true)?
    } else {
        NurbsCurve::interpolate(&out, degree, stop)?
    };
    curve_value(CurveGeometry::nurbs(nurbs), stop)
}

/// Dense polyline of a curve with the piece parameter of every sample, for intersections.
/// A dense sample: piece index, piece parameter and point.
type Sample = (usize, f64, P3);

fn dense(c: &Curve, stop: Stop) -> R<(Vec<Piece>, Vec<Sample>)> {
    let (pieces, _) = curve_pieces(c, stop)?;
    let mut out = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let k = if piece.degree() <= 1 {
            1
        } else {
            12 * piece.degree()
        };
        for s in 0..=k {
            let t = s as f64 / k as f64;
            out.push((i, t, piece.point_at(t)));
        }
        if out.len() > limits::MAX_SAMPLES {
            return Err(
                "曲線が細かすぎて交点を計算できません。「曲線を再構築」で単純にしてください。"
                    .into(),
            );
        }
    }
    Ok((pieces, out))
}

fn segment_closest(a0: P3, a1: P3, b0: P3, b1: P3) -> (f64, f64, f64) {
    let (d1, d2, r) = (sub(a1, a0), sub(b1, b0), sub(a0, b0));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (mut s, mut t);
    if a <= 1e-300 && e <= 1e-300 {
        return (0., 0., dist(a0, b0));
    }
    if a <= 1e-300 {
        s = 0.;
        t = (f / e).clamp(0., 1.);
    } else {
        let c = dot(d1, r);
        if e <= 1e-300 {
            t = 0.;
            s = (-c / a).clamp(0., 1.);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            s = if denom > 1e-300 {
                ((b * f - c * e) / denom).clamp(0., 1.)
            } else {
                0.
            };
            t = (b * s + f) / e;
            if t < 0. {
                t = 0.;
                s = (-c / a).clamp(0., 1.);
            } else if t > 1. {
                t = 1.;
                s = ((b - c) / a).clamp(0., 1.);
            }
        }
    }
    let pa = add(a0, scale(d1, s));
    let pb = add(b0, scale(d2, t));
    (s, t, dist(pa, pb))
}

/// Points where two curves meet (within [`tol::ABS`] after refinement on the exact pieces).
pub fn curve_curve(a: &Curve, b: &Curve, stop: Stop) -> R<Vec<P3>> {
    let (pa, sa) = dense(a, stop)?;
    let (pb, sb) = dense(b, stop)?;
    let size = sa
        .iter()
        .chain(&sb)
        .fold(1f64, |m, (_, _, p)| m.max(norm(*p)));
    let mut found: Vec<P3> = Vec::new();
    for wa in sa.windows(2) {
        stopped(stop)?;
        let (Some(x0), Some(x1)) = (wa.first(), wa.get(1)) else {
            continue;
        };
        if x0.0 != x1.0 {
            continue;
        }
        let la = dist(x0.2, x1.2);
        for wb in sb.windows(2) {
            let (Some(y0), Some(y1)) = (wb.first(), wb.get(1)) else {
                continue;
            };
            if y0.0 != y1.0 {
                continue;
            }
            let (s, t, d) = segment_closest(x0.2, x1.2, y0.2, y1.2);
            if d > (la + dist(y0.2, y1.2)) * 0.5 + tol::ABS {
                continue;
            }
            // Refine on the exact pieces (Gauss-Newton with numerical derivatives).
            let (Some(qa), Some(qb)) = (pa.get(x0.0), pb.get(y0.0)) else {
                continue;
            };
            let mut u = x0.1 + (x1.1 - x0.1) * s;
            let mut v = y0.1 + (y1.1 - y0.1) * t;
            for _ in 0..30 {
                let h = 1e-7;
                let r = sub(qa.point_at(u), qb.point_at(v));
                let du = scale(
                    sub(qa.point_at((u + h).min(1.)), qa.point_at((u - h).max(0.))),
                    0.5 / h,
                );
                let dv = scale(
                    sub(qb.point_at((v + h).min(1.)), qb.point_at((v - h).max(0.))),
                    -0.5 / h,
                );
                let (a11, a12, a22) = (dot(du, du), dot(du, dv), dot(dv, dv));
                let (b1, b2) = (-dot(du, r), -dot(dv, r));
                let det = a11 * a22 - a12 * a12;
                if det.abs() <= 1e-300 {
                    break;
                }
                let (step_u, step_v) = ((b1 * a22 - b2 * a12) / det, (a11 * b2 - a12 * b1) / det);
                u = (u + step_u).clamp(0., 1.);
                v = (v + step_v).clamp(0., 1.);
                if step_u.abs() + step_v.abs() < 1e-15 {
                    break;
                }
            }
            let (p, q) = (qa.point_at(u), qb.point_at(v));
            if dist(p, q) <= tol::ABS.max(size * 1e-10) * 10. {
                let m = scale(add(p, q), 0.5);
                if !found.iter().any(|f| dist(*f, m) <= tol::ABS * 100.) {
                    found.push(m);
                }
            }
        }
    }
    Ok(found)
}

fn triangle(m: &Mesh, t: &[u32; 3]) -> Option<[P3; 3]> {
    Some([
        *m.vertices.get(t[0] as usize)?,
        *m.vertices.get(t[1] as usize)?,
        *m.vertices.get(t[2] as usize)?,
    ])
}

fn segment_triangle(p0: P3, p1: P3, tri: [P3; 3]) -> Option<f64> {
    let [a, b, c] = tri;
    let (e1, e2, d) = (sub(b, a), sub(c, a), sub(p1, p0));
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() <= 1e-300 {
        return None;
    }
    let f = 1. / det;
    let s = sub(p0, a);
    let u = f * dot(s, h);
    if !(-1e-12..=1. + 1e-12).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = f * dot(d, q);
    if v < -1e-12 || u + v > 1. + 1e-12 {
        return None;
    }
    let t = f * dot(e2, q);
    (-1e-12..=1. + 1e-12)
        .contains(&t)
        .then_some(t.clamp(0., 1.))
}

/// Points where a curve crosses a B-rep's surface. Accuracy follows the display mesh
/// ([`tol::chord`]): an approximation, refined on the curve only.
pub fn curve_brep(c: &Curve, b: &BrepValue, stop: Stop) -> R<Vec<P3>> {
    let (_, samples) = dense(c, stop)?;
    let mesh = b.preview();
    let mut out: Vec<P3> = Vec::new();
    for w in samples.windows(2) {
        stopped(stop)?;
        let (Some(x0), Some(x1)) = (w.first(), w.get(1)) else {
            continue;
        };
        if x0.0 != x1.0 {
            continue;
        }
        let (lo, hi) = (x0.2.map2(x1.2, f64::min), x0.2.map2(x1.2, f64::max));
        for t in &mesh.triangles {
            let Some(tri) = triangle(mesh, t) else {
                continue;
            };
            let tlo = tri[0].map2(tri[1], f64::min).map2(tri[2], f64::min);
            let thi = tri[0].map2(tri[1], f64::max).map2(tri[2], f64::max);
            if (0..3).any(|k| thi[k] < lo[k] - tol::ABS || tlo[k] > hi[k] + tol::ABS) {
                continue;
            }
            if let Some(s) = segment_triangle(x0.2, x1.2, tri) {
                let p = add(x0.2, scale(sub(x1.2, x0.2), s));
                if !out
                    .iter()
                    .any(|q| dist(*q, p) <= tol::chord(b.body().size()))
                {
                    out.push(p);
                }
            }
        }
        if out.len() > 10_000 {
            return Err("交点が多すぎます。".into());
        }
    }
    Ok(out)
}

trait Map2 {
    fn map2(self, o: Self, f: fn(f64, f64) -> f64) -> Self;
}
impl Map2 for P3 {
    fn map2(self, o: Self, f: fn(f64, f64) -> f64) -> Self {
        [f(self[0], o[0]), f(self[1], o[1]), f(self[2], o[2])]
    }
}

/// The segment where two triangles cross, if they do.
fn triangle_triangle(t1: [P3; 3], t2: [P3; 3]) -> Option<(P3, P3)> {
    let plane = |t: [P3; 3]| {
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        (n, dot(n, t[0]))
    };
    let crossing = |t: [P3; 3], n: P3, d: f64| -> Option<(P3, P3)> {
        let s = t.map(|p| dot(n, p) - d);
        if s.iter().all(|&x| x > 0.) || s.iter().all(|&x| x < 0.) {
            return None;
        }
        let mut pts = Vec::with_capacity(2);
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            let (sa, sb) = (s.get(a).copied()?, s.get(b).copied()?);
            let (pa, pb) = (t.get(a).copied()?, t.get(b).copied()?);
            if sa == 0. {
                pts.push(pa);
            }
            if (sa < 0. && sb > 0.) || (sa > 0. && sb < 0.) {
                pts.push(add(pa, scale(sub(pb, pa), sa / (sa - sb))));
            }
        }
        match pts.as_slice() {
            [a, b, ..] => Some((*a, *b)),
            _ => None,
        }
    };
    let (n1, d1) = plane(t1);
    let (n2, d2) = plane(t2);
    let dir = cross(n1, n2);
    if norm(dir) <= 1e-300 {
        return None;
    }
    let (a0, a1) = crossing(t1, n2, d2)?;
    let (b0, b1) = crossing(t2, n1, d1)?;
    let (ta0, ta1) = (dot(a0, dir), dot(a1, dir));
    let (tb0, tb1) = (dot(b0, dir), dot(b1, dir));
    let (alo, ahi, pa_lo, pa_hi) = if ta0 <= ta1 {
        (ta0, ta1, a0, a1)
    } else {
        (ta1, ta0, a1, a0)
    };
    let (blo, bhi, pb_lo, pb_hi) = if tb0 <= tb1 {
        (tb0, tb1, b0, b1)
    } else {
        (tb1, tb0, b1, b0)
    };
    let lo = if alo >= blo { pa_lo } else { pb_lo };
    let hi = if ahi <= bhi { pa_hi } else { pb_hi };
    (alo.max(blo) < ahi.min(bhi)).then_some((lo, hi))
}

/// Curves where two B-reps' surfaces cross. Closed solids use the kernel's exact intersection
/// curves (`kernel_ops::exact_intersection`); open sheets fall back to the display meshes.
pub fn brep_brep(a: &BrepValue, b: &BrepValue, stop: Stop) -> R<Vec<Curve>> {
    if a.is_closed() && b.is_closed() {
        match crate::kernel_ops::exact_intersection(a, b, stop) {
            Ok(curves) => return Ok(curves),
            Err(e) if e == STOPPED => return Err(e),
            Err(_) => {}
        }
    }
    brep_brep_mesh(a, b, stop)
}

/// Curves where two B-reps' surfaces cross, as polylines chained from their display meshes
/// (approximate, within [`tol::chord`]).
pub fn brep_brep_mesh(a: &BrepValue, b: &BrepValue, stop: Stop) -> R<Vec<Curve>> {
    let (ma, mb) = (a.preview(), b.preview());
    let size = a.body().size().max(b.body().size());
    // Bin B's triangles in a grid to avoid testing every pair.
    let cell = (size / 32.).max(tol::ABS * 1000.);
    let key = |p: P3| p.map(|v| (v / cell).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, t) in mb.triangles.iter().enumerate() {
        let Some(tri) = triangle(mb, t) else { continue };
        let lo = key(tri[0].map2(tri[1], f64::min).map2(tri[2], f64::min));
        let hi = key(tri[0].map2(tri[1], f64::max).map2(tri[2], f64::max));
        if (0..3)
            .map(|k| hi[k].saturating_sub(lo[k]).saturating_add(1))
            .product::<i64>()
            > 4096
        {
            return Err("交線の計算に必要な分割が大きすぎます。".into());
        }
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    grid.entry([x, y, z]).or_default().push(i);
                }
            }
        }
    }
    let mut segments: Vec<(P3, P3)> = Vec::new();
    let mut tested = std::collections::HashSet::new();
    for t in &ma.triangles {
        stopped(stop)?;
        let Some(tri) = triangle(ma, t) else { continue };
        let lo = key(tri[0].map2(tri[1], f64::min).map2(tri[2], f64::min));
        let hi = key(tri[0].map2(tri[1], f64::max).map2(tri[2], f64::max));
        tested.clear();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    for &j in grid.get(&[x, y, z]).map(Vec::as_slice).unwrap_or(&[]) {
                        if !tested.insert(j) {
                            continue;
                        }
                        let Some(other) = mb.triangles.get(j).and_then(|u| triangle(mb, u)) else {
                            continue;
                        };
                        if let Some(s) = triangle_triangle(tri, other) {
                            segments.push(s);
                        }
                    }
                }
            }
        }
        if segments.len() > 200_000 {
            return Err("交線が細かすぎます。".into());
        }
    }
    chain_segments(&segments, tol::weld(size) * 100.)
}

fn chain_segments(segments: &[(P3, P3)], eps: f64) -> R<Vec<Curve>> {
    let mut points = Vec::with_capacity(segments.len() * 2);
    for (a, b) in segments {
        points.push(*a);
        points.push(*b);
    }
    let (verts, map) = weld(&points, eps);
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
    for pair in map.chunks(2) {
        let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if a != b {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
    }
    let mut used: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut curves = Vec::new();
    let mut starts: Vec<u32> = adj.keys().copied().collect();
    // Open chains start at their ends; then closed loops anywhere.
    starts.sort_by_key(|k| (adj.get(k).map_or(0, Vec::len) == 2, *k));
    for s in starts {
        let mut path = vec![s];
        let mut cur = s;
        loop {
            let next = adj.get(&cur).and_then(|n| {
                n.iter()
                    .find(|&&m| !used.contains(&(cur.min(m), cur.max(m))))
                    .copied()
            });
            let Some(m) = next else { break };
            used.insert((cur.min(m), cur.max(m)));
            path.push(m);
            cur = m;
            if path.len() > 1_000_000 {
                break;
            }
        }
        if path.len() < 2 {
            continue;
        }
        let closed = path.len() > 3 && path.first() == path.last();
        if closed {
            path.pop();
        }
        let pts: Vec<P3> = path
            .iter()
            .filter_map(|&i| verts.get(i as usize).copied())
            .collect();
        curves.push(Curve {
            points: pts,
            closed,
            geometry: None,
        });
    }
    Ok(curves)
}

// ---------------------------------------------------------------------------------------------
// T-spline sheets and sewing (used by `subd`)
// ---------------------------------------------------------------------------------------------

/// A T-spline surface as a surface body of NURBS faces (SolidCraft `tspline_sheet`).
pub fn tspline_body(ts: &solidcraft_geom::tspline::TSpline, stop: Stop) -> R<Body> {
    kernel("T-spline → B-rep", stop, || sk::tspline_sheet(ts))
}

/// Sew surface bodies along coincident edges ([`tol::STITCH`]).
pub fn stitch_bodies(bodies: &[Body], stop: Stop) -> R<BrepValue> {
    let refs: Vec<&Body> = bodies.iter().collect();
    let body = kernel("縫い合わせ", stop, || sk::stitch(&refs, tol::STITCH))?;
    brep(body, stop)
}
