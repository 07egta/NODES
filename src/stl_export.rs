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
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_SIZE_MM: f64 = 256.0;
const MAX_TRIANGLES: usize = 2_000_000;
/// Vertices closer than this are merged by the repair step.
const WELD_REPAIR_MM: f64 = 1.0e-4;
/// Same tolerance as `persistence::mesh_stats`, used when splitting bodies.
const WELD_SPLIT_MM: f64 = 1.0e-6;
const DEGENERATE_NORMAL_SQ: f64 = 1.0e-16;
const MAX_HOLE_EDGES: usize = 64;
const MIN_SIZE_MM: f64 = 1.0;
const JST_OFFSET_SECS: i64 = 9 * 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    StlBinary,
    StlAscii,
    ThreeMf,
    Obj,
}

impl OutputFormat {
    pub const ALL: [OutputFormat; 4] = [
        OutputFormat::StlBinary,
        OutputFormat::StlAscii,
        OutputFormat::ThreeMf,
        OutputFormat::Obj,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Self::StlBinary | Self::StlAscii => "stl",
            Self::ThreeMf => "3mf",
            Self::Obj => "obj",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::StlBinary => "STL バイナリ（おすすめ・小さい）",
            Self::StlAscii => "STL テキスト（ASCII）",
            Self::ThreeMf => "3MF（試験的）",
            Self::Obj => "OBJ（3D ソフト用）",
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
fn signed_volume(body: &Mesh) -> f64 {
    body.triangles
        .iter()
        .filter_map(|&t| tri_points(body, t))
        .map(|(a, b, c)| dot(a, cross(b, c)) / 6.0)
        .sum::<f64>()
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
            lines.push(format!("ほぼ同じ位置の頂点を {} 個まとめました", self.merged_vertices));
        }
        if self.removed_degenerate > 0 {
            lines.push(format!("つぶれた面を {} 枚取り除きました", self.removed_degenerate));
        }
        if self.removed_duplicate > 0 {
            lines.push(format!("重なった面を {} 枚取り除きました", self.removed_duplicate));
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
                "大きい・複雑な穴 {} か所は、そのまま残しました",
                self.kept_open_holes
            ));
        }
        if lines.is_empty() {
            lines.push("直す箇所は見つかりませんでした".into());
        }
        lines
    }
}

/// Repairs each mesh independently. The input is never modified.
pub fn repair_meshes(meshes: &[Mesh]) -> Result<(Vec<Mesh>, RepairReport), String> {
    let mut report = RepairReport::default();
    let mut out = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        out.push(repair_one(mesh, &mut report)?);
    }
    Ok((out, report))
}

fn repair_one(mesh: &Mesh, report: &mut RepairReport) -> Result<Mesh, String> {
    if mesh.vertices.iter().any(|p| p.iter().any(|v| !v.is_finite())) {
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
        let (Some(&pa), Some(&pb), Some(&pc)) = (points.get(a), points.get(b), points.get(c)) else {
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
    // 3. Make neighbouring faces agree, then point each closed part outward.
    orient_faces(&mut tris, &points, report);
    // 4. Fill small simple holes with a fan from the loop centroid.
    fill_small_holes(&mut points, &mut tris, report);
    let triangles: Vec<[u32; 3]> = tris
        .iter()
        .map(|&t| t.map(|i| i as u32))
        .collect();
    Ok(Mesh {
        vertices: points,
        triangles,
    })
}

fn orient_faces(tris: &mut [[usize; 3]], points: &[[f64; 3]], report: &mut RepairReport) {
    let mut faces_of_edge: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (index, &tri) in tris.iter().enumerate() {
        for (a, b) in edges_of(tri) {
            faces_of_edge.entry(undirected(a, b)).or_default().push(index);
        }
    }
    let mut visited = vec![false; tris.len()];
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
        let volume: f64 = component
            .iter()
            .filter_map(|&i| tris.get(i))
            .map(|t| signed_tri(*t, points))
            .sum();
        if volume < 0.0 {
            for &i in &component {
                if let Some(tri) = tris.get_mut(i) {
                    tri.swap(1, 2);
                }
            }
            report.flipped_bodies += 1;
            report.flipped_faces += component.len();
        }
    }
}

fn signed_tri(t: [usize; 3], points: &[[f64; 3]]) -> f64 {
    match (points.get(t[0]), points.get(t[1]), points.get(t[2])) {
        (Some(&a), Some(&b), Some(&c)) => dot(a, cross(b, c)) / 6.0,
        _ => 0.0,
    }
}

fn fill_small_holes(
    points: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[usize; 3]>,
    report: &mut RepairReport,
) {
    let mut count_of_edge: HashMap<(usize, usize), usize> = HashMap::new();
    for &tri in tris.iter() {
        for (a, b) in edges_of(tri) {
            *count_of_edge.entry(undirected(a, b)).or_insert(0) += 1;
        }
    }
    // A boundary edge a->b is left by the hole in the direction b->a.
    let mut hole_out: HashMap<usize, Vec<usize>> = HashMap::new();
    for &tri in tris.iter() {
        for (a, b) in edges_of(tri) {
            if count_of_edge.get(&undirected(a, b)) == Some(&1) {
                hole_out.entry(b).or_default().push(a);
            }
        }
    }
    let mut starts: Vec<usize> = hole_out.keys().copied().collect();
    starts.sort_unstable();
    let mut done: HashSet<usize> = HashSet::new();
    for start in starts {
        if done.contains(&start) {
            continue;
        }
        let mut ring = vec![start];
        let mut current = start;
        let mut closed = false;
        loop {
            let to = match hole_out.get(&current).map(Vec::as_slice) {
                Some([only]) => *only,
                _ => break,
            };
            if to == start {
                closed = ring.len() >= 3;
                break;
            }
            if ring.contains(&to) || ring.len() >= MAX_HOLE_EDGES {
                break;
            }
            ring.push(to);
            current = to;
        }
        done.extend(ring.iter().copied());
        if !closed {
            report.kept_open_holes += 1;
            continue;
        }
        let count = ring.len();
        let mut sum = [0.0f64; 3];
        for &v in &ring {
            if let Some(p) = points.get(v) {
                sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
            }
        }
        let apex = sum.map(|s| s / count as f64);
        let apex_index = points.len();
        points.push(apex);
        for (index, &from) in ring.iter().enumerate() {
            let to = ring.get((index + 1) % count).copied().unwrap_or(from);
            tris.push([from, to, apex_index]);
        }
        report.filled_holes += 1;
        report.added_triangles += count;
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
            if options.center_xy { -(lo[0] + hi[0]) / 2.0 } else { 0.0 },
            if options.center_xy { -(lo[1] + hi[1]) / 2.0 } else { 0.0 },
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
        let none = check(Level::Error, "書き出せる面", "面がありません。「出力」ノードに立体をつないでください。");
        return Ok((vec![none], stats));
    }
    let report = persistence::mesh_stats(&to_trees(bodies))?;
    let mut checks = Vec::with_capacity(7);
    checks.push(if report.open_edges == 0 {
        check(Level::Ok, "閉じている（水密）", "すべての辺が 2 枚の面で囲まれています。")
    } else {
        check(
            Level::Error,
            "閉じている（水密）",
            format!("面がつながっていない辺が {} 本あります。「自動修復」で小さな穴はふさげます。", report.open_edges),
        )
    });
    let inside_out = bodies.iter().filter(|b| signed_volume(b) < 0.0).count();
    checks.push(if report.inconsistent_edges > 0 {
        check(
            Level::Error,
            "面の向きがそろっている",
            format!("向きが食い違う辺が {} 本あります。「自動修復」で直せます。", report.inconsistent_edges),
        )
    } else if inside_out > 0 {
        check(
            Level::Error,
            "面の向きがそろっている",
            format!("内側を向いた部品が {inside_out} 個あります。「自動修復」で直せます。"),
        )
    } else {
        check(Level::Ok, "面の向きがそろっている", "表面はすべて外側を向いています。")
    });
    checks.push(if report.non_manifold_edges == 0 {
        check(Level::Ok, "3つ以上の面が共有する辺がない", "辺ごとの面の数は正常です。")
    } else {
        check(
            Level::Error,
            "3つ以上の面が共有する辺がない",
            format!("3枚以上の面が共有する辺が {} 本あります。", report.non_manifold_edges),
        )
    });
    checks.push(if report.degenerate_triangles == 0 {
        check(Level::Ok, "つぶれた三角形がない", "面積 0 の面はありません。")
    } else {
        check(
            Level::Error,
            "つぶれた三角形がない",
            format!("面積 0 の三角形が {} 枚あります。「自動修復」で取り除けます。", report.degenerate_triangles),
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

/// Writes one file, or one file per body when `separate` is set and there is more than one body.
pub fn write_bodies(
    format: OutputFormat,
    path: &Path,
    bodies: &[Mesh],
    separate: bool,
) -> Result<WriteSummary, String> {
    if bodies.is_empty() {
        return Err("書き出せる立体がありません。".into());
    }
    let mut summary = WriteSummary::default();
    if separate && bodies.len() > 1 {
        let total = bodies.len();
        for (index, body) in bodies.iter().enumerate() {
            let target = numbered_path(path, index + 1, total);
            summary.triangles += write_file(format, &target, std::slice::from_ref(body))?;
            summary.files.push(target);
        }
    } else {
        summary.triangles = write_file(format, path, bodies)?;
        summary.files.push(path.to_path_buf());
    }
    Ok(summary)
}

fn write_file(format: OutputFormat, path: &Path, bodies: &[Mesh]) -> Result<usize, String> {
    match format {
        OutputFormat::StlBinary => persistence::export_stl(path, &to_trees(bodies)).map(|r| r.triangles),
        OutputFormat::Obj => persistence::export_obj(path, &to_trees(bodies)).map(|r| r.triangles),
        OutputFormat::StlAscii => {
            let triangles = ensure_writable(bodies)?;
            write_ascii(path, bodies)?;
            Ok(triangles)
        }
        OutputFormat::ThreeMf => {
            let triangles = ensure_writable(bodies)?;
            let bytes = encode_3mf(bodies)?;
            persistence::atomic_write(path, |file| file.write_all(&bytes))?;
            Ok(triangles)
        }
    }
}

fn write_ascii(path: &Path, bodies: &[Mesh]) -> Result<(), String> {
    persistence::atomic_write(path, |file| {
        writeln!(file, "solid Morphlet")?;
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
        writeln!(file, "endsolid Morphlet")
    })
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n <Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/>\n</Types>\n";
const RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n <Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/>\n</Relationships>\n";

/// Builds a 3MF package (experimental). One object holding all bodies; units are millimetres.
pub fn encode_3mf(bodies: &[Mesh]) -> Result<Vec<u8>, String> {
    ensure_writable(bodies)?;
    let total_vertices: usize = bodies.iter().map(|b| b.vertices.len()).sum();
    if u32::try_from(total_vertices).is_err() {
        return Err("形状が大きすぎて 3MF に書き出せません。".into());
    }
    let mut vertices = String::new();
    let mut triangles = String::new();
    let mut offset: u32 = 0;
    for body in bodies {
        for p in &body.vertices {
            vertices.push_str(&format!(
                "     <vertex x=\"{:.6}\" y=\"{:.6}\" z=\"{:.6}\"/>\n",
                p[0], p[1], p[2]
            ));
        }
        for t in &body.triangles {
            triangles.push_str(&format!(
                "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>\n",
                t[0] + offset,
                t[1] + offset,
                t[2] + offset
            ));
        }
        offset += body.vertices.len() as u32;
    }
    let model = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"ja-JP\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n <resources>\n  <object id=\"1\" type=\"model\">\n   <mesh>\n    <vertices>\n{vertices}    </vertices>\n    <triangles>\n{triangles}    </triangles>\n   </mesh>\n  </object>\n </resources>\n <build>\n  <item objectid=\"1\"/>\n </build>\n</model>\n"
    );
    zip_stored(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("3D/3dmodel.model", model.as_bytes()),
    ])
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
    base.map(|dir| dir.join("Morphlet").join("stl_export.json"))
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
