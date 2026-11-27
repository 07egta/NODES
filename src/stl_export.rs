//! Print-oriented export preparation: mesh repair, unit scale, bed placement,
//! body splitting, pre-export checks, and file writing (binary/ASCII STL, 3MF, OBJ).
//! Pure logic with no egui; the dialog lives in the app binary (`stl_export_ui.rs`).

use crate::model::{Mesh, Tree, Value};
use crate::persistence;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_SIZE_MM: f64 = 256.0;
const MAX_TRIANGLES: usize = 2_000_000;
/// Vertices closer than this are merged by the repair step.
const WELD_REPAIR_MM: f64 = 1.0e-4;
/// Same tolerance as `persistence::mesh_stats`, used when splitting bodies.
const WELD_SPLIT_MM: f64 = 1.0e-6;
const DEGENERATE_NORMAL_SQ: f64 = 1.0e-16;
const MAX_HOLE_EDGES: usize = 64;
const MAX_HOLE_LOOPS: usize = 256;
const MIN_SIZE_MM: f64 = 1.0;
const JST_OFFSET_SECS: i64 = 9 * 3600;
static EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    StlBinary,
    StlAscii,
    ThreeMf,
    Obj,
    Ply,
}

impl OutputFormat {
    pub const ALL: [OutputFormat; 5] = [
        OutputFormat::StlBinary,
        OutputFormat::StlAscii,
        OutputFormat::ThreeMf,
        OutputFormat::Obj,
        OutputFormat::Ply,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Self::StlBinary | Self::StlAscii => "stl",
            Self::ThreeMf => "3mf",
            Self::Obj => "obj",
            Self::Ply => "ply",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::StlBinary => "STL バイナリ（おすすめ・小さい）",
            Self::StlAscii => "STL テキスト（ASCII）",
            Self::ThreeMf => "3MF（試験的）",
            Self::Obj => "OBJ（3D ソフト用）",
            Self::Ply => "PLY（バイナリ・色つき可）",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportOptions {
    pub format: OutputFormat,
    pub scale: f64,
    pub place_on_bed: bool,
    pub center_xy: bool,
    pub split_bodies: bool,
    pub max_size_mm: f64,
    /// 3MF / PLY only: give each body its own colour (fixed palette).
    pub color_bodies: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::StlBinary,
            scale: 1.0,
            place_on_bed: true,
            center_xy: true,
            split_bodies: false,
            max_size_mm: DEFAULT_MAX_SIZE_MM,
            color_bodies: false,
        }
    }
}

impl ExportOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !self.scale.is_finite() || !(1.0e-6..=1.0e6).contains(&self.scale) {
            return Err("倍率は 0.000001 から 1000000 の間で指定してください。".into());
        }
        if !self.max_size_mm.is_finite() || self.max_size_mm <= 0.0 {
            return Err("印刷できる大きさは 0 より大きい値で指定してください。".into());
        }
        Ok(())
    }
}

/// Settings remembered between sessions (slicer path, last folder and file).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredSettings {
    pub slicer_exe: Option<PathBuf>,
    pub last_dir: Option<PathBuf>,
    /// The base file name chosen by the user. Split bodies get `_01`, `_02` suffixes.
    pub last_base_file: Option<PathBuf>,
}

/// Collects every mesh value. Returns the meshes and the number of non-mesh values skipped.
pub fn gather_meshes(trees: &[Tree]) -> Result<(Vec<Mesh>, usize), String> {
    let mut meshes = Vec::new();
    let mut skipped = 0usize;
    let mut triangles = 0usize;
    for value in trees.iter().flat_map(|t| t.branches.values()).flatten() {
        match value {
            Value::Mesh(mesh) => {
                triangles = triangles.saturating_add(mesh.triangles.len());
                if triangles > MAX_TRIANGLES {
                    return Err("形状が大きすぎます。分割数または複製数を減らしてください。".into());
                }
                meshes.push(mesh.as_ref().clone());
            }
            Value::Brep(b) => {
                let mesh = b.preview();
                triangles = triangles.saturating_add(mesh.triangles.len());
                if triangles > MAX_TRIANGLES {
                    return Err("形状が大きすぎます。分割数または複製数を減らしてください。".into());
                }
                meshes.push(mesh.as_ref().clone());
            }
            _ => skipped += 1,
        }
    }
    Ok((meshes, skipped))
}

/// Wraps each mesh in its own tree so the persistence writers and checks can be reused.
pub fn to_trees(bodies: &[Mesh]) -> Vec<Tree> {
    bodies
        .iter()
        .map(|m| Tree::single(Value::Mesh(Arc::new(m.clone()))))
        .collect()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = dot(v, v).sqrt();
    if length > 0.0 {
        v.map(|x| x / length)
    } else {
        [0.0; 3]
    }
}
fn edges_of(t: [usize; 3]) -> [(usize, usize); 3] {
    [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
}
fn undirected(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}
fn tri_points(mesh: &Mesh, tri: [u32; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    Some((
        *mesh.vertices.get(tri[0] as usize)?,
        *mesh.vertices.get(tri[1] as usize)?,
        *mesh.vertices.get(tri[2] as usize)?,
    ))
}
fn compensated_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut sum = 0.0;
    let mut correction = 0.0;
    for value in values {
        let corrected = value - correction;
        let next = sum + corrected;
        correction = (next - sum) - corrected;
        sum = next;
    }
    sum
}

fn signed_volume(body: &Mesh) -> f64 {
    // A local origin avoids catastrophic cancellation for small parts far from
    // the world origin. Unused vertices must not select the integration origin.
    let origin = body
        .triangles
        .iter()
        .find_map(|&t| tri_points(body, t))
        .map_or([0.0; 3], |(a, _, _)| a);
    compensated_sum(
        body.triangles
            .iter()
            .filter_map(|&t| tri_points(body, t))
            .map(|(a, b, c)| dot(sub(a, origin), cross(sub(b, origin), sub(c, origin))) / 6.0),
    )
}
fn bounds(bodies: &[Mesh]) -> Option<([f64; 3], [f64; 3])> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for body in bodies {
        for p in &body.vertices {
            for (axis, value) in p.iter().enumerate() {
                lo[axis] = lo[axis].min(*value);
                hi[axis] = hi[axis].max(*value);
            }
        }
    }
    if lo.iter().all(|v| v.is_finite()) {
        Some((lo, hi))
    } else {
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub merged_vertices: usize,
    pub removed_degenerate: usize,
    pub removed_duplicate: usize,
    pub flipped_faces: usize,
    pub flipped_bodies: usize,
    pub filled_holes: usize,
    pub added_triangles: usize,
    pub kept_open_holes: usize,
    /// Shells whose global direction was retained because nesting or closure
    /// could not be established. Local winding repair may still have occurred.
    pub unresolved_shells: usize,
}

impl RepairReport {
    pub fn changed(&self) -> bool {
        self.merged_vertices
            + self.removed_degenerate
            + self.removed_duplicate
            + self.flipped_faces
            + self.filled_holes
            > 0
    }

    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.merged_vertices > 0 {
            lines.push(format!(
                "ほぼ同じ位置の頂点を {} 個まとめました",
                self.merged_vertices
            ));
        }
        if self.removed_degenerate > 0 {
            lines.push(format!(
                "つぶれた面を {} 枚取り除きました",
                self.removed_degenerate
            ));
        }
        if self.removed_duplicate > 0 {
            lines.push(format!(
                "重なった面を {} 枚取り除きました",
                self.removed_duplicate
            ));
        }
        if self.flipped_faces > 0 {
            lines.push(format!(
                "向きがそろっていない面を {} 枚直しました（裏返しの部品 {} 個）",
                self.flipped_faces, self.flipped_bodies
            ));
        }
        if self.filled_holes > 0 {
            lines.push(format!(
                "小さな穴を {} か所ふさぎました（面 {} 枚を追加）",
                self.filled_holes, self.added_triangles
            ));
        }
        if self.kept_open_holes > 0 {
            lines.push(format!(
                "大きい・非平面・重なる境界など、安全に埋められない穴 {} か所は残しました",
                self.kept_open_holes
            ));
        }
        if self.unresolved_shells > 0 {
            lines.push(format!(
                "殻 {} 個の内外は未判定です。空洞を埋めないよう全体の向きを保持しました",
                self.unresolved_shells
            ));
        }
        if lines.is_empty() {
            lines.push("直す箇所は見つかりませんでした".into());
        }
        lines
    }
}

/// Repairs local topology without mutating the input. Global winding is only
/// changed for a single closed shell in a single input mesh. Multiple shells may
/// bound a cavity; their existing global directions are retained because shell
/// nesting is not classified here.
pub fn repair_meshes(meshes: &[Mesh]) -> Result<(Vec<Mesh>, RepairReport), String> {
    let mut report = RepairReport::default();
    let mut out = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        out.push(repair_one(mesh, &mut report, meshes.len() == 1)?);
    }
    Ok((out, report))
}

fn repair_one(
    mesh: &Mesh,
    report: &mut RepairReport,
    allow_global_flip: bool,
) -> Result<Mesh, String> {
    if mesh
        .vertices
        .iter()
        .any(|p| p.iter().any(|v| !v.is_finite()))
    {
        return Err("無限大または非数の座標があるため、自動修復できません。".into());
    }
    // 1. Weld near-identical vertices onto one representative each.
    let mut by_key: HashMap<[i64; 3], usize> = HashMap::new();
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut canonical: Vec<usize> = Vec::with_capacity(mesh.vertices.len());
    for &p in &mesh.vertices {
        let key = p.map(|v| (v / WELD_REPAIR_MM).round() as i64);
        let id = match by_key.get(&key) {
            Some(&id) => {
                report.merged_vertices += 1;
                id
            }
            None => {
                let id = points.len();
                points.push(p);
                by_key.insert(key, id);
                id
            }
        };
        canonical.push(id);
    }
    // 2. Drop invalid, degenerate and duplicate triangles.
    let mut seen: HashSet<[usize; 3]> = HashSet::new();
    let mut tris: Vec<[usize; 3]> = Vec::with_capacity(mesh.triangles.len());
    for &tri in &mesh.triangles {
        let [Some(a), Some(b), Some(c)] = tri.map(|i| canonical.get(i as usize).copied()) else {
            report.removed_degenerate += 1;
            continue;
        };
        if a == b || b == c || c == a {
            report.removed_degenerate += 1;
            continue;
        }
        let (Some(&pa), Some(&pb), Some(&pc)) = (points.get(a), points.get(b), points.get(c))
        else {
            report.removed_degenerate += 1;
            continue;
        };
        let n = cross(sub(pb, pa), sub(pc, pa));
        if dot(n, n) <= DEGENERATE_NORMAL_SQ {
            report.removed_degenerate += 1;
            continue;
        }
        let mut key = [a, b, c];
        key.sort_unstable();
        if !seen.insert(key) {
            report.removed_duplicate += 1;
            continue;
        }
        tris.push([a, b, c]);
    }
    // 3. Make neighbouring faces agree. A disconnected negative shell can be
    // the inside boundary of a hollow solid, so do not flip it independently.
    orient_faces(&mut tris, &points, report, allow_global_flip);
    // 4. Triangulate only simple planar holes, preserving existing vertices.
    fill_small_holes(&points, &mut tris, report);
    let triangles: Vec<[u32; 3]> = tris.iter().map(|&t| t.map(|i| i as u32)).collect();
    Ok(Mesh {
        vertices: points,
        triangles,
    })
}

fn orient_faces(
    tris: &mut [[usize; 3]],
    points: &[[f64; 3]],
    report: &mut RepairReport,
    allow_global_flip: bool,
) {
    let mut faces_of_edge: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (index, &tri) in tris.iter().enumerate() {
        for (a, b) in edges_of(tri) {
            faces_of_edge
                .entry(undirected(a, b))
                .or_default()
                .push(index);
        }
    }
    let mut visited = vec![false; tris.len()];
    let mut components = Vec::new();
    for start in 0..visited.len() {
        if visited.get(start) != Some(&false) {
            continue;
        }
        if let Some(flag) = visited.get_mut(start) {
            *flag = true;
        }
        let mut component = vec![start];
        let mut stack = vec![start];
        while let Some(face) = stack.pop() {
            let Some(&current) = tris.get(face) else {
                continue;
            };
            for (a, b) in edges_of(current) {
                let Some(neighbours) = faces_of_edge.get(&undirected(a, b)) else {
                    continue;
                };
                for &other in neighbours {
                    if other == face || visited.get(other) != Some(&false) {
                        continue;
                    }
                    if let Some(flag) = visited.get_mut(other) {
                        *flag = true;
                    }
                    if let Some(tri) = tris.get_mut(other)
                        && edges_of(*tri).contains(&(a, b))
                    {
                        // Same direction as the neighbour's shared edge: flip it.
                        tri.swap(1, 2);
                        report.flipped_faces += 1;
                    }
                    component.push(other);
                    stack.push(other);
                }
            }
        }
        components.push(component);
    }
    // Only a single closed shell has an unambiguous global sign under the
    // non-self-intersection assumption. An open patch has no enclosed volume.
    let closed = faces_of_edge.values().all(|faces| faces.len() == 2);
    if !allow_global_flip || components.len() != 1 || !closed {
        report.unresolved_shells += components.len();
        return;
    }
    let component = &components[0];
    let origin = points[tris[component[0]][0]];
    let volume = compensated_sum(component.iter().map(|&i| {
        let [a, b, c] = tris[i].map(|v| sub(points[v], origin));
        dot(a, cross(b, c)) / 6.0
    }));
    if volume < 0.0 {
        for &i in component {
            tris[i].swap(1, 2);
        }
        report.flipped_bodies += 1;
        report.flipped_faces += component.len();
    } else if volume == 0.0 || !volume.is_finite() {
        report.unresolved_shells += 1;
    }
}

fn fill_small_holes(points: &[[f64; 3]], tris: &mut Vec<[usize; 3]>, report: &mut RepairReport) {
    let mut count_of_edge: HashMap<(usize, usize), usize> = HashMap::new();
    for &tri in tris.iter() {
        for (a, b) in edges_of(tri) {
            *count_of_edge.entry(undirected(a, b)).or_insert(0) += 1;
        }
    }
    // A boundary edge a->b is left by the hole in the direction b->a.
    let mut hole_out: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut hole_in: HashMap<usize, usize> = HashMap::new();
    for &tri in tris.iter() {
        for (a, b) in edges_of(tri) {
            if count_of_edge.get(&undirected(a, b)) == Some(&1) {
                hole_out.entry(b).or_default().push(a);
                *hole_in.entry(a).or_insert(0) += 1;
            }
        }
    }
    let mut starts: Vec<usize> = hole_out.keys().copied().collect();
    starts.sort_unstable();
    let mut done: HashSet<usize> = HashSet::new();
    let mut rings = Vec::new();
    let mut overflow = 0;
    let mut incomplete_boundary = false;
    for start in starts {
        if done.contains(&start) {
            continue;
        }
        let mut ring = vec![start];
        let mut current = start;
        let mut closed = false;
        loop {
            if hole_in.get(&current) != Some(&1) {
                break;
            }
            let to = match hole_out.get(&current).map(Vec::as_slice) {
                Some([only]) => *only,
                _ => break,
            };
            if to == start {
                closed = ring.len() >= 3;
                break;
            }
            if done.contains(&to) || ring.contains(&to) || ring.len() >= MAX_HOLE_EDGES {
                break;
            }
            ring.push(to);
            current = to;
        }
        done.extend(ring.iter().copied());
        if !closed {
            report.kept_open_holes += 1;
            incomplete_boundary = true;
        } else if rings.len() < MAX_HOLE_LOOPS {
            rings.push(ring);
        } else {
            overflow += 1;
        }
    }
    if overflow > 0 || incomplete_boundary {
        // An unclassified loop might lie inside another opening. Do not fill
        // the enclosing ring while silently ignoring a large/branched hole.
        // Also bound pair work for hundreds of independent openings.
        report.kept_open_holes += rings.len() + overflow;
        return;
    }
    let boxes: Vec<([f64; 3], [f64; 3])> = rings
        .iter()
        .map(|ring| {
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for &v in ring {
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(points[v][axis]);
                    hi[axis] = hi[axis].max(points[v][axis]);
                }
            }
            (lo, hi)
        })
        .collect();
    let mut ambiguous = vec![false; rings.len()];
    for i in 0..boxes.len() {
        for j in i + 1..boxes.len() {
            let (a, b) = boxes[i];
            let (c, d) = boxes[j];
            let scale = (0..3)
                .map(|axis| (b[axis] - a[axis]).max(d[axis] - c[axis]))
                .fold(0., f64::max);
            let tolerance = 1.0e-9 * scale + 1.0e-12;
            if (0..3).all(|axis| a[axis] <= d[axis] + tolerance && c[axis] <= b[axis] + tolerance) {
                // Conservative: nested coplanar loops can describe a face with
                // a hole. Filling each independently would erase that hole.
                // Disjoint polygons with overlapping boxes are also left open.
                ambiguous[i] = true;
                ambiguous[j] = true;
            }
        }
    }
    for (index, ring) in rings.iter().enumerate() {
        if ambiguous[index] {
            report.kept_open_holes += 1;
            continue;
        }
        match crate::polygon_triangulation::triangulate_indexed(points, ring, &|| false) {
            Ok(caps) if tris.len().saturating_add(caps.len()) <= MAX_TRIANGLES => {
                report.filled_holes += 1;
                report.added_triangles += caps.len();
                tris.extend(caps);
            }
            _ => report.kept_open_holes += 1,
        }
    }
}

/// Splits a mesh into its disconnected parts (triangles connected through shared vertices).
pub fn split_bodies(mesh: &Mesh) -> Vec<Mesh> {
    let n = mesh.vertices.len();
    let mut by_key: HashMap<[i64; 3], usize> = HashMap::new();
    let mut canonical: Vec<usize> = Vec::with_capacity(n);
    for (index, &p) in mesh.vertices.iter().enumerate() {
        let key = p.map(|v| (v / WELD_SPLIT_MM).round() as i64);
        canonical.push(*by_key.entry(key).or_insert(index));
    }
    let valid: Vec<[usize; 3]> = mesh
        .triangles
        .iter()
        .filter_map(|&t| {
            let ids = t.map(|i| canonical.get(i as usize).copied());
            match ids {
                [Some(a), Some(b), Some(c)] => Some([a, b, c]),
                _ => None,
            }
        })
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    for tri in &valid {
        let root = find(&mut parent, tri[0]);
        for &v in &tri[1..] {
            let other = find(&mut parent, v);
            if other != root
                && let Some(slot) = parent.get_mut(other)
            {
                *slot = root;
            }
        }
    }
    let mut group_of_root: HashMap<usize, usize> = HashMap::new();
    let mut groups: Vec<Vec<[usize; 3]>> = Vec::new();
    for tri in valid {
        let root = find(&mut parent, tri[0]);
        let index = *group_of_root.entry(root).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        if let Some(group) = groups.get_mut(index) {
            group.push(tri);
        }
    }
    groups
        .iter()
        .map(|tris| compact(&mesh.vertices, tris))
        .collect()
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    loop {
        let Some(&p) = parent.get(x) else {
            return x;
        };
        if p == x {
            return x;
        }
        let grand = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(x) {
            *slot = grand;
        }
        x = grand;
    }
}

/// Builds a mesh from the vertices referenced by `tris` (indices into `vertices`).
fn compact(vertices: &[[f64; 3]], tris: &[[usize; 3]]) -> Mesh {
    let mut remap: HashMap<usize, u32> = HashMap::new();
    let mut out_vertices: Vec<[f64; 3]> = Vec::new();
    let mut out_tris: Vec<[u32; 3]> = Vec::with_capacity(tris.len());
    for tri in tris {
        let mut out = [0u32; 3];
        for (slot, &v) in out.iter_mut().zip(tri.iter()) {
            let id = match remap.get(&v) {
                Some(&id) => id,
                None => {
                    let id = out_vertices.len() as u32;
                    out_vertices.push(vertices.get(v).copied().unwrap_or([0.0; 3]));
                    remap.insert(v, id);
                    id
                }
            };
            *slot = id;
        }
        out_tris.push(out);
    }
    Mesh {
        vertices: out_vertices,
        triangles: out_tris,
    }
}

/// Splits (optional), scales to mm, and places on the bed. Relative placement is kept across bodies.
pub fn prepare_bodies(meshes: &[Mesh], options: &ExportOptions) -> Result<Vec<Mesh>, String> {
    options.validate()?;
    let mut bodies: Vec<Mesh> = if options.split_bodies {
        meshes.iter().flat_map(split_bodies).collect()
    } else {
        meshes.to_vec()
    };
    for body in &mut bodies {
        for p in &mut body.vertices {
            for v in p.iter_mut() {
                *v *= options.scale;
            }
        }
    }
    if options.place_on_bed
        && let Some((lo, hi)) = bounds(&bodies)
    {
        let shift = [
            if options.center_xy {
                -(lo[0] + hi[0]) / 2.0
            } else {
                0.0
            },
            if options.center_xy {
                -(lo[1] + hi[1]) / 2.0
            } else {
                0.0
            },
            -lo[2],
        ];
        for body in &mut bodies {
            for p in &mut body.vertices {
                for (axis, v) in p.iter_mut().enumerate() {
                    *v += shift[axis];
                }
            }
        }
    }
    Ok(bodies)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub level: Level,
    pub title: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyStats {
    pub bodies: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub size_mm: [f64; 3],
}

fn check(level: Level, title: &'static str, detail: impl Into<String>) -> Check {
    Check {
        level,
        title,
        detail: detail.into(),
    }
}

/// Plain-language checks for the traffic-light list. Topology comes from `persistence::mesh_stats`.
/// These are not a proof of printability: self-intersection and wall thickness are not measured.
pub fn check_bodies(bodies: &[Mesh], max_size_mm: f64) -> Result<(Vec<Check>, BodyStats), String> {
    let triangles: usize = bodies.iter().map(|b| b.triangles.len()).sum();
    let vertices: usize = bodies.iter().map(|b| b.vertices.len()).sum();
    let mut stats = BodyStats {
        bodies: bodies.len(),
        vertices,
        triangles,
        size_mm: [0.0; 3],
    };
    if triangles == 0 {
        let none = check(
            Level::Error,
            "書き出せる面",
            "面がありません。「出力」ノードに立体をつないでください。",
        );
        return Ok((vec![none], stats));
    }
    let report = persistence::mesh_stats(&to_trees(bodies))?;
    let mut checks = Vec::with_capacity(9);
    checks.push(if report.is_closed() {
        check(Level::Ok, "閉じている（水密）", "座標の結合後、辺・頂点が閉じた多様体を構成し、面方向は一貫しています。自己交差・殻の内外・肉厚は未検査です。")
    } else {
        check(
            Level::Error,
            "閉じている（水密）",
            format!("境界辺 {} 本、非多様体の辺 {} 本・頂点 {} 個、向きの不整合辺 {} 本、重複面 {} 枚、退化面 {} 枚。閉じた多様体の条件を満たしません。", report.open_edges, report.non_manifold_edges, report.non_manifold_vertices, report.inconsistent_edges, report.duplicate_faces, report.degenerate_triangles),
        )
    });
    let inside_out = bodies.iter().filter(|b| signed_volume(b) < 0.0).count();
    checks.push(if report.inconsistent_edges > 0 {
        check(
            Level::Error,
            "面の向きがそろっている",
            format!("向きが食い違う辺が {} 本あります。「自動修復」で直せます。", report.inconsistent_edges),
        )
    } else if !report.is_closed() {
        check(Level::Warn, "面の向きがそろっている", "閉じた有効な面構成ではないため、体積の符号による内外の判定は行いません。")
    } else if inside_out > 0 {
        check(
            Level::Error,
            "面の向きがそろっている",
            format!("負の符号付き体積を持つ入力メッシュが {inside_out} 個あります。内側の空洞の殻でないか確認してください。複数の殻の内外は自動判定しません。"),
        )
    } else {
        check(Level::Ok, "面の向きがそろっている", "隣接面の向きは一貫しています。殻の内外・入れ子・自己交差は未検査です。")
    });
    checks.push(if report.non_manifold_edges == 0 {
        check(
            Level::Ok,
            "3つ以上の面が共有する辺がない",
            "辺ごとの面の数は正常です。",
        )
    } else {
        check(
            Level::Error,
            "3つ以上の面が共有する辺がない",
            format!(
                "3枚以上の面が共有する辺が {} 本あります。",
                report.non_manifold_edges
            ),
        )
    });
    checks.push(if report.non_manifold_vertices == 0 {
        check(
            Level::Ok,
            "頂点で面が枝分かれしていない",
            "頂点まわりの面は一続きです。自己交差の検査とは異なります。",
        )
    } else {
        check(
            Level::Error,
            "頂点で面が枝分かれしていない",
            format!("非多様体の頂点が {} 個あります。点だけで接触する形状や、頂点まわりの面の分岐を確認してください。", report.non_manifold_vertices),
        )
    });
    checks.push(if report.duplicate_faces == 0 {
        check(
            Level::Ok,
            "重複する面がない",
            "同じ頂点を使う重複面はありません。",
        )
    } else {
        check(
            Level::Error,
            "重複する面がない",
            format!(
                "頂点の結合後に重複する面が {} 枚あります。表裏を逆にした重複も含みます。",
                report.duplicate_faces
            ),
        )
    });
    checks.push(if report.degenerate_triangles == 0 {
        check(
            Level::Ok,
            "つぶれた三角形がない",
            "面積 0 の面はありません。",
        )
    } else {
        check(
            Level::Error,
            "つぶれた三角形がない",
            format!(
                "面積 0 の三角形が {} 枚あります。「自動修復」で取り除けます。",
                report.degenerate_triangles
            ),
        )
    });
    if let Some([lo, hi]) = report.bounds {
        stats.size_mm = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    }
    let [sx, sy, sz] = stats.size_mm;
    let longest = sx.max(sy).max(sz);
    checks.push(if longest < MIN_SIZE_MM {
        check(
            Level::Warn,
            "小さすぎない",
            format!("全体の大きさが {longest:.3} mm しかありません。cm や inch の取り違えがないか、倍率を確認してください。"),
        )
    } else {
        check(Level::Ok, "小さすぎない", "1 mm 以上の大きさがあります。")
    });
    let oversize = stats.size_mm.iter().any(|s| *s > max_size_mm);
    checks.push(if oversize {
        check(
            Level::Error,
            "印刷機の大きさに収まる",
            format!(
                "{sx:.1} × {sy:.1} × {sz:.1} mm。{max_size_mm:.0} mm を超えています。倍率を下げるか、大きさを確認してください。"
            ),
        )
    } else {
        check(
            Level::Ok,
            "印刷機の大きさに収まる",
            format!("{sx:.1} × {sy:.1} × {sz:.1} mm（上限 {max_size_mm:.0} mm）"),
        )
    });
    Ok((checks, stats))
}

/// Refuses meshes with degenerate faces for the formats written here. Returns the triangle count.
fn ensure_writable(bodies: &[Mesh]) -> Result<usize, String> {
    let report = persistence::mesh_stats(&to_trees(bodies))?;
    if report.degenerate_triangles > 0 {
        return Err(format!(
            "面積が 0 の面が {} 枚あるため書き出せません。「自動修復」を試してください。",
            report.degenerate_triangles
        ));
    }
    Ok(report.triangles)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WriteSummary {
    pub files: Vec<PathBuf>,
    pub triangles: usize,
}

/// A batch can fail during replacement after some complete files were saved.
/// Preparation errors have an empty `completed`; every destination is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteFailure {
    pub completed: WriteSummary,
    pub failed_path: Option<PathBuf>,
    pub message: String,
}

impl From<String> for WriteFailure {
    fn from(message: String) -> Self {
        Self {
            completed: WriteSummary::default(),
            failed_path: None,
            message,
        }
    }
}

impl WriteFailure {
    fn at(path: &Path, message: impl Into<String>) -> Self {
        Self {
            failed_path: Some(path.to_path_buf()),
            ..Self::from(message.into())
        }
    }
}

impl std::fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.completed.files.is_empty() {
            writeln!(
                f,
                "書き出しを完了できませんでした。保存先は変更されていません。"
            )?;
        } else {
            writeln!(
                f,
                "書き出しが途中で停止しました。{} 個のファイル（{} 面）は保存済みです。",
                self.completed.files.len(),
                self.completed.triangles
            )?;
            for path in &self.completed.files {
                writeln!(f, "保存済み: {}", path.display())?;
            }
        }
        if let Some(path) = &self.failed_path {
            writeln!(f, "失敗した保存先: {}", path.display())?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for WriteFailure {}

/// Compatibility wrapper. Detailed callers can inspect which files were saved.
pub fn write_bodies(
    format: OutputFormat,
    path: &Path,
    bodies: &[Mesh],
    separate: bool,
) -> Result<WriteSummary, String> {
    write_bodies_detailed(format, path, bodies, separate).map_err(|error| error.to_string())
}

/// Prepare all split bodies before replacing any destination. Each replacement
/// is atomic; multiple replacements are not one atomic filesystem transaction.
pub fn write_bodies_detailed(
    format: OutputFormat,
    path: &Path,
    bodies: &[Mesh],
    separate: bool,
) -> Result<WriteSummary, WriteFailure> {
    write_bodies_colored(format, path, bodies, separate, false)
}

/// Same as `write_bodies_detailed`; with `color_bodies` the 3MF and PLY writers give
/// every body its own colour from `BODY_PALETTE`. STL and OBJ cannot hold colour and
/// ignore the flag.
pub fn write_bodies_colored(
    format: OutputFormat,
    path: &Path,
    bodies: &[Mesh],
    separate: bool,
    color_bodies: bool,
) -> Result<WriteSummary, WriteFailure> {
    if bodies.is_empty() {
        return Err(WriteFailure::from(
            "書き出せる立体がありません。".to_owned(),
        ));
    }
    if separate && bodies.len() > 1 {
        commit_prepared(prepare_split(format, path, bodies, color_bodies)?)
    } else {
        // The existing single-file writers already validate, flush and replace
        // atomically; an additional staging pass would provide no extra safety.
        let triangles = write_file(format, path, bodies, color_bodies)
            .map_err(|message| WriteFailure::at(path, message))?;
        Ok(WriteSummary {
            files: vec![path.to_path_buf()],
            triangles,
        })
    }
}

fn export_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

struct PreparedOutput {
    destination: PathBuf,
    stage: PathBuf,
    triangles: usize,
    committed: bool,
}

impl PreparedOutput {
    fn reserve(destination: PathBuf) -> Result<Self, String> {
        let parent = export_parent(&destination);
        for _ in 0..32 {
            let sequence = EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let stage = parent.join(format!(
                ".morphlet-export-{}-{sequence}.stage",
                std::process::id()
            ));
            // A user may choose any name, including our temporary-name pattern.
            if stage.file_name() == destination.file_name() {
                continue;
            }
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stage)
            {
                Ok(file) => {
                    drop(file);
                    return Ok(Self {
                        destination,
                        stage,
                        triangles: 0,
                        committed: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "書き出し準備用ファイルを作成できませんでした: {error}"
                    ));
                }
            }
        }
        Err("書き出し準備用ファイルの名前を確保できませんでした。".into())
    }
}

impl Drop for PreparedOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.stage);
        }
    }
}

fn prepare_split(
    format: OutputFormat,
    path: &Path,
    bodies: &[Mesh],
    color_bodies: bool,
) -> Result<Vec<PreparedOutput>, WriteFailure> {
    let targets: Vec<_> = (0..bodies.len())
        .map(|index| numbered_path(path, index + 1, bodies.len()))
        .collect();
    // Reject predictable destination failures before preparing or replacing any file.
    for target in &targets {
        if target.file_name().is_none() || target.is_dir() {
            return Err(WriteFailure::at(
                target,
                "保存先にはフォルダーではなくファイル名を指定してください。",
            ));
        }
        if !export_parent(target).is_dir() {
            return Err(WriteFailure::at(
                target,
                "保存先のフォルダーが見つかりません。",
            ));
        }
    }
    let mut prepared = Vec::with_capacity(bodies.len());
    let mut triangles = 0usize;
    for (target, body) in targets.into_iter().zip(bodies) {
        let mut output = PreparedOutput::reserve(target.clone())
            .map_err(|message| WriteFailure::at(&target, message))?;
        // Reuse the exact writer/validator for each format. It finishes its
        // atomic write to our owned stage, leaving all destinations untouched.
        output.triangles = write_file(
            format,
            &output.stage,
            std::slice::from_ref(body),
            color_bodies,
        )
        .map_err(|message| WriteFailure::at(&target, message))?;
        triangles = triangles
            .checked_add(output.triangles)
            .ok_or_else(|| WriteFailure::at(&target, "合計の面数が大きすぎます。"))?;
        prepared.push(output);
    }
    Ok(prepared)
}

fn commit_prepared(prepared: Vec<PreparedOutput>) -> Result<WriteSummary, WriteFailure> {
    let mut completed = WriteSummary::default();
    for mut output in prepared {
        // Same-directory std::fs::rename replaces existing regular files on
        // Windows and Unix, as in persistence::atomic_write. Never delete the
        // destination first; locked files must remain intact on failure.
        if let Err(error) = fs::rename(&output.stage, &output.destination) {
            return Err(WriteFailure {
                completed,
                failed_path: Some(output.destination.clone()),
                message: format!(
                    "保存先を更新できませんでした。別のアプリで開いていないか確認してください: {error}"
                ),
            });
        }
        output.committed = true;
        #[cfg(unix)]
        if let Ok(directory) = fs::File::open(export_parent(&output.destination)) {
            let _ = directory.sync_all();
        }
        completed.triangles += output.triangles;
        completed.files.push(output.destination.clone());
    }
    Ok(completed)
}

fn write_file(
    format: OutputFormat,
    path: &Path,
    bodies: &[Mesh],
    color_bodies: bool,
) -> Result<usize, String> {
    match format {
        OutputFormat::StlBinary => {
            persistence::export_stl(path, &to_trees(bodies)).map(|r| r.triangles)
        }
        OutputFormat::Obj => persistence::export_obj(path, &to_trees(bodies)).map(|r| r.triangles),
        OutputFormat::StlAscii => {
            let triangles = ensure_writable(bodies)?;
            write_ascii(path, bodies)?;
            Ok(triangles)
        }
        OutputFormat::ThreeMf => {
            let triangles = ensure_writable(bodies)?;
            let bytes = if color_bodies {
                encode_3mf_colored(bodies)?
            } else {
                encode_3mf(bodies)?
            };
            persistence::atomic_write(path, |file| file.write_all(&bytes))?;
            Ok(triangles)
        }
        OutputFormat::Ply => {
            let triangles = ensure_writable(bodies)?;
            let bytes = encode_ply(bodies, color_bodies)?;
            persistence::atomic_write(path, |file| file.write_all(&bytes))?;
            Ok(triangles)
        }
    }
}

fn write_ascii(path: &Path, bodies: &[Mesh]) -> Result<(), String> {
    persistence::atomic_write(path, |file| {
        writeln!(file, "solid NODES")?;
        for body in bodies {
            for &t in &body.triangles {
                let Some((a, b, c)) = tri_points(body, t) else {
                    continue;
                };
                let n = unit(cross(sub(b, a), sub(c, a)));
                writeln!(file, "  facet normal {:e} {:e} {:e}", n[0], n[1], n[2])?;
                writeln!(file, "    outer loop")?;
                for p in [a, b, c] {
                    writeln!(file, "      vertex {:e} {:e} {:e}", p[0], p[1], p[2])?;
                }
                writeln!(file, "    endloop")?;
                writeln!(file, "  endfacet")?;
            }
        }
        writeln!(file, "endsolid NODES")
    })
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n <Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/>\n</Types>\n";
const RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n <Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/>\n</Relationships>\n";

/// Builds an experimental 3MF package with one model object per input mesh.
/// A mesh containing nested shells remains a single object. This does not detect
/// shell nesting, self-intersection or geometric overlap between model objects.
pub fn encode_3mf(bodies: &[Mesh]) -> Result<Vec<u8>, String> {
    encode_3mf_inner(bodies, false)
}

/// Fixed colours (R, G, B) given to bodies in order, repeating after the last one.
pub const BODY_PALETTE: [[u8; 3]; 8] = [
    [0xE6, 0x4B, 0x35],
    [0x4D, 0xBB, 0xD5],
    [0x00, 0xA0, 0x87],
    [0xF3, 0x9B, 0x7F],
    [0x3C, 0x54, 0x88],
    [0x84, 0x91, 0xB4],
    [0xB0, 0x9C, 0x85],
    [0x7E, 0x61, 0x48],
];

/// Like `encode_3mf`, but every object is named (`NODES_n`) and refers to a
/// `basematerials` colour from `BODY_PALETTE`. This is the plain 3MF core material
/// extension; it is not Bambu Studio's own project format (no filament slots).
/// Whether a slicer maps these colours to filaments has not been checked on a real
/// printer or slicer.
pub fn encode_3mf_colored(bodies: &[Mesh]) -> Result<Vec<u8>, String> {
    encode_3mf_inner(bodies, true)
}

fn encode_3mf_inner(bodies: &[Mesh], colored: bool) -> Result<Vec<u8>, String> {
    ensure_writable(bodies)?;
    if bodies.is_empty() {
        return Err("3MFに書き出せるモデルがありません。".into());
    }
    let mut model = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"ja-JP\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n <resources>\n",
    );
    // Resource ids: objects use 1..=n, the colour table uses n+1.
    let material_id = bodies.len() + 1;
    if colored {
        model.push_str(&format!("  <basematerials id=\"{material_id}\">\n"));
        for index in 0..bodies.len() {
            let [r, g, b] = BODY_PALETTE[index % BODY_PALETTE.len()];
            model.push_str(&format!(
                "   <base name=\"NODES_{}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}\"/>\n",
                index + 1
            ));
        }
        model.push_str("  </basematerials>\n");
    }
    for (index, body) in bodies.iter().enumerate() {
        let id = index + 1;
        // The model object requires indexed, consistently oriented manifold
        // triangles. An independent cavity shell must not become a model object.
        // No assertion of exact solid validity is made by this check.
        let volume = crate::mesh_fabrication::signed_volume(body, &|| false)
            .map_err(|e| format!("3MF オブジェクト {id}: {e}"))?;
        if volume <= 0. {
            return Err(format!(
                "3MF オブジェクト {id} の体積が正ではありません。内側の殻を別オブジェクトにせず、面方向を確認してください。"
            ));
        }
        if colored {
            model.push_str(&format!(
                "  <object id=\"{id}\" name=\"NODES_{id}\" type=\"model\" pid=\"{material_id}\" pindex=\"{index}\">\n   <mesh>\n    <vertices>\n"
            ));
        } else {
            model.push_str(&format!(
                "  <object id=\"{id}\" type=\"model\">\n   <mesh>\n    <vertices>\n"
            ));
        }
        // Rust's shortest round-trip representation avoids 6-decimal rounding
        // that could collapse close vertices. Coordinates are checked beforehand.
        for p in &body.vertices {
            model.push_str(&format!(
                "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>\n",
                p[0], p[1], p[2]
            ));
        }
        model.push_str("    </vertices>\n    <triangles>\n");
        for t in &body.triangles {
            model.push_str(&format!(
                "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>\n",
                t[0], t[1], t[2]
            ));
        }
        model.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    model.push_str(" </resources>\n <build>\n");
    for id in 1..=bodies.len() {
        model.push_str(&format!("  <item objectid=\"{id}\"/>\n"));
    }
    model.push_str(" </build>\n</model>\n");
    zip_stored(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("3D/3dmodel.model", model.as_bytes()),
    ])
}

/// Binary little-endian PLY, millimetres, `double` coordinates. With `color_bodies`
/// each vertex carries the body's `BODY_PALETTE` colour as `red green blue` (uchar).
/// Bodies are written one after another; vertices are not merged between bodies.
pub fn encode_ply(bodies: &[Mesh], color_bodies: bool) -> Result<Vec<u8>, String> {
    ensure_writable(bodies)?;
    let mut vertex_total = 0usize;
    let mut face_total = 0usize;
    for body in bodies {
        vertex_total = vertex_total
            .checked_add(body.vertices.len())
            .ok_or("頂点が多すぎます。")?;
        face_total = face_total
            .checked_add(body.triangles.len())
            .ok_or("面が多すぎます。")?;
    }
    if u32::try_from(vertex_total).is_err() || u32::try_from(face_total).is_err() {
        return Err("PLY に書ける頂点・面の数を超えました。".into());
    }
    let mut out = Vec::with_capacity(256 + vertex_total * 27 + face_total * 13);
    out.extend_from_slice(
        b"ply\nformat binary_little_endian 1.0\ncomment NODES; units: millimeters\n",
    );
    out.extend_from_slice(format!("element vertex {vertex_total}\n").as_bytes());
    out.extend_from_slice(b"property double x\nproperty double y\nproperty double z\n");
    if color_bodies {
        out.extend_from_slice(b"property uchar red\nproperty uchar green\nproperty uchar blue\n");
    }
    out.extend_from_slice(format!("element face {face_total}\n").as_bytes());
    out.extend_from_slice(b"property list uchar uint vertex_indices\nend_header\n");
    for (index, body) in bodies.iter().enumerate() {
        let color = BODY_PALETTE[index % BODY_PALETTE.len()];
        for p in &body.vertices {
            for c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
            if color_bodies {
                out.extend_from_slice(&color);
            }
        }
    }
    let mut offset = 0u32;
    for body in bodies {
        for t in &body.triangles {
            out.push(3);
            for &v in t {
                let global = v.checked_add(offset).ok_or("頂点番号が大きすぎます。")?;
                out.extend_from_slice(&global.to_le_bytes());
            }
        }
        let count = u32::try_from(body.vertices.len()).map_err(|_| "頂点が多すぎます。")?;
        offset = offset.checked_add(count).ok_or("頂点が多すぎます。")?;
    }
    Ok(out)
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// A ZIP archive with stored (uncompressed) entries, written by hand.
pub fn zip_stored(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = 0x0021; // 1980-01-01
    let too_big = || "ファイルが大きすぎます。".to_string();
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let count = u16::try_from(entries.len()).map_err(|_| too_big())?;
    for &(name, data) in entries {
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let crc = crc32(data);
        // Local file header.
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 20);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, DOS_TIME);
        put16(&mut out, DOS_DATE);
        put32(&mut out, crc);
        put32(&mut out, size);
        put32(&mut out, size);
        put16(&mut out, name_len);
        put16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // Central directory entry.
        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20);
        put16(&mut central, 20);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, DOS_TIME);
        put16(&mut central, DOS_DATE);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, name_len);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_big())?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    // End of central directory record.
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, cd_size);
    put32(&mut out, cd_offset);
    put16(&mut out, 0);
    Ok(out)
}

/// Name for one of several split files: `base_01.stl`. Unchanged when there is only one file.
pub fn numbered_path(base: &Path, index: usize, total: usize) -> PathBuf {
    if total <= 1 {
        return base.to_path_buf();
    }
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".into());
    let name = match base.extension() {
        Some(ext) => format!("{stem}_{index:02}.{}", ext.to_string_lossy()),
        None => format!("{stem}_{index:02}"),
    };
    base.with_file_name(name)
}

/// Japan Standard Time, `YYYYMMDD-HHMM`. The time zone is fixed to JST on purpose.
pub fn timestamp_jst(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + JST_OFFSET_SECS;
    let days = seconds.div_euclid(86_400);
    let minutes = seconds.rem_euclid(86_400) / 60;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}",
        minutes / 60,
        minutes % 60
    )
}

/// Days since 1970-01-01 to (year, month, day). Proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `<base>_<YYYYMMDD-HHMM>.<ext>`, e.g. `花器_20261126-2130.stl`.
pub fn default_file_name(base: &str, format: OutputFormat, now: SystemTime) -> String {
    format!("{base}_{}.{}", timestamp_jst(now), format.extension())
}

/// Install locations checked on Windows, in order. Only existing files are used.
pub fn slicer_candidates() -> Vec<PathBuf> {
    let program_files = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    vec![
        program_files.join("Bambu Studio").join("bambu-studio.exe"),
        program_files.join("OrcaSlicer").join("orca-slicer.exe"),
        program_files
            .join("Prusa3D")
            .join("PrusaSlicer")
            .join("prusa-slicer.exe"),
    ]
}

pub fn detect_slicer() -> Option<PathBuf> {
    slicer_candidates().into_iter().find(|p| p.is_file())
}

/// The program is run directly with the file as its only argument. No shell is involved.
pub fn slicer_command(exe: &Path, file: &Path) -> Command {
    let mut command = Command::new(exe);
    command.arg(file);
    command
}

pub fn launch_slicer(exe: &Path, file: &Path) -> Result<(), String> {
    if !exe.is_file() {
        return Err("スライサーの exe が見つかりません。選び直してください。".into());
    }
    slicer_command(exe, file)
        .spawn()
        .map(|_child| ())
        .map_err(|e| format!("スライサーを起動できませんでした: {e}"))
}

pub fn config_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    base.map(|dir| dir.join("NODES").join("stl_export.json"))
}

pub fn load_stored(path: &Path) -> StoredSettings {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_stored(path: &Path, settings: &StoredSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("設定フォルダーを作成できませんでした: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|e| format!("設定を保存形式に変換できませんでした: {e}"))?;
    persistence::atomic_write(path, |file| file.write_all(&bytes))
}

#[cfg(test)]
mod batch_commit_tests {
    use super::*;

    #[test]
    fn failed_replacement_reports_completed_files_and_cleans_uncommitted_stages() {
        struct TemporaryDirectory(PathBuf);
        impl Drop for TemporaryDirectory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let sequence = EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = TemporaryDirectory(std::env::temp_dir().join(format!(
            "nodes-export-commit-{}-{sequence}",
            std::process::id()
        )));
        fs::create_dir_all(&temp.0).unwrap();
        let base = temp.0.join("parts.stl");
        let targets: Vec<_> = (1..=3)
            .map(|index| numbered_path(&base, index, 3))
            .collect();
        for target in &targets {
            fs::write(target, b"EXISTING").unwrap();
        }
        let mesh = Mesh {
            vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        };
        let prepared = prepare_split(
            OutputFormat::StlBinary,
            &base,
            &[mesh.clone(), mesh.clone(), mesh],
            false,
        )
        .unwrap();
        for target in &targets {
            assert_eq!(fs::read(target).unwrap(), b"EXISTING");
        }
        // A real filesystem change between preparation and replacement must
        // produce a partial result, without a timing race or injected mock.
        fs::remove_file(&targets[1]).unwrap();
        fs::create_dir(&targets[1]).unwrap();
        let failure = commit_prepared(prepared).unwrap_err();
        assert_eq!(failure.completed.files, vec![targets[0].clone()]);
        assert_eq!(failure.completed.triangles, 4);
        assert_eq!(failure.failed_path.as_ref(), Some(&targets[1]));
        assert!(
            failure
                .to_string()
                .contains("1 個のファイル（4 面）は保存済み")
        );
        assert_ne!(fs::read(&targets[0]).unwrap(), b"EXISTING");
        assert!(targets[1].is_dir());
        assert_eq!(fs::read(&targets[2]).unwrap(), b"EXISTING");
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 3);
    }
}
