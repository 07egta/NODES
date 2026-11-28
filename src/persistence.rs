//! Versioned project files, bounded undo history, and validated mesh export.
//!
//! Writes are prepared next to the destination, flushed to disk, and renamed in one
//! operation. A failed write or rename leaves the old destination untouched.

use crate::model::{Graph, Mesh, Tree, Value};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PROJECT_BYTES: usize = 8 * 1024 * 1024;
pub const HISTORY_LIMIT: usize = 64;
const MAX_EXPORT_VERTICES: usize = 2_000_000;
const MAX_EXPORT_TRIANGLES: usize = 2_000_000;
const MAX_COORDINATE: f64 = 1.0e9;
const WELD_QUANTIZATION: f64 = 1.0e6;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub name: String,
    pub graph: Graph,
    #[serde(default)]
    pub pan: [f32; 2],
    #[serde(default = "default_zoom")]
    pub zoom: f32,
}

fn default_zoom() -> f32 {
    1.0
}

impl Project {
    pub fn new(name: impl Into<String>, graph: Graph) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: name.into(),
            graph,
            pan: [32.0, 100.0],
            zoom: 1.0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        check_schema_version(self.schema_version)?;
        if self.name.len() > 1024 || self.name.chars().any(|c| c == '\0') {
            return Err("作品名が長すぎるか、使えない文字が含まれています。".into());
        }
        if self.pan.iter().any(|v| !v.is_finite() || v.abs() > 1.0e7)
            || !self.zoom.is_finite()
            || !(0.1..=8.0).contains(&self.zoom)
        {
            return Err("キャンバスの位置または拡大率が正しくありません。".into());
        }
        self.graph.validate()
    }
}

fn check_schema_version(version: u32) -> Result<(), String> {
    if version != SCHEMA_VERSION {
        return Err(format!(
            "この作品は形式 v{} です。このアプリで読める形式は v{} です。",
            version, SCHEMA_VERSION
        ));
    }
    Ok(())
}

fn parse_error(e: serde_json::Error) -> String {
    let detail: String = e.to_string().chars().take(300).collect();
    format!(
        "作品の内容を読み取れませんでした（{} 行目）: {}",
        e.line(),
        detail
    )
}

/// Read into a separate value. The caller only replaces its open project on success.
pub fn load(path: &Path) -> Result<Project, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("作品を開けませんでした: {e}"))?;
    if !metadata.is_file() {
        return Err("作品には通常のファイルを指定してください。".into());
    }
    let file = File::open(path).map_err(|e| format!("作品を開けませんでした: {e}"))?;
    if !file
        .metadata()
        .map_err(|e| format!("作品を開けませんでした: {e}"))?
        .is_file()
    {
        return Err("作品には通常のファイルを指定してください。".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_PROJECT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("作品を読み込めませんでした: {e}"))?;
    decode_project(&bytes)
}

/// Decode a bounded v1 project without silently discarding unknown fields.
/// Future versions are reported before interpreting their graph; callers must
/// leave the original file untouched on error.
pub fn decode_project(bytes: &[u8]) -> Result<Project, String> {
    if bytes.len() > MAX_PROJECT_BYTES {
        return Err("作品ファイルが大きすぎます。上限は 8 MB です。".into());
    }
    #[derive(Deserialize)]
    struct Header {
        schema_version: u32,
    }
    // Read only the envelope first so future schemas get an explicit version error.
    let header: Header = serde_json::from_slice(bytes).map_err(parse_error)?;
    check_schema_version(header.schema_version)?;
    let mut project: Project = serde_json::from_slice(bytes).map_err(parse_error)?;
    crate::compatibility::normalize_legacy_inputs(&mut project.graph)?;
    project.validate()?;
    Ok(project)
}

/// Streaming limit: reject a large project while serializing into its temporary
/// file, rather than building an unbounded pretty-printed byte vector in RAM.
struct ProjectWriter<W> {
    inner: W,
    remaining: usize,
}

impl<W: Write> Write for ProjectWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other(
                "作品ファイルが大きすぎます。上限は 8 MB です。",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn save(path: &Path, project: &Project) -> Result<(), String> {
    project.validate()?;
    atomic_write(path, |file| {
        let mut bounded = ProjectWriter {
            inner: file,
            remaining: MAX_PROJECT_BYTES,
        };
        serde_json::to_writer_pretty(&mut bounded, project).map_err(std::io::Error::other)?;
        bounded.write_all(b"\n")
    })
}

pub(crate) fn atomic_write(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> std::io::Result<()>,
) -> Result<(), String> {
    path.file_name()
        .ok_or_else(|| "保存先のファイル名を指定してください。".to_string())?;
    if path.is_dir() {
        return Err("フォルダーではなく、ファイル名を指定してください。".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err("保存先のフォルダーが見つかりません。".into());
    }
    let mut temporary: Option<(PathBuf, File)> = None;
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // Do not append to the destination basename: it may already be at the
        // filesystem component-length limit. Keep the staging name short.
        let candidate = parent.join(format!(".morphlet-{}-{sequence}.tmp", std::process::id()));
        if candidate == path {
            continue;
        }
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("保存用ファイルを作成できませんでした: {e}")),
        }
    }
    let Some((temporary_path, file)) = temporary else {
        return Err("保存用ファイルの名前を確保できませんでした。".into());
    };
    let mut file = BufWriter::with_capacity(128 * 1024, file);
    let written = write(&mut file)
        .and_then(|_| file.flush())
        .and_then(|_| file.get_ref().sync_all());
    drop(file);
    if let Err(e) = written {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "書き込みに失敗しました。元のファイルは保持されています: {e}"
        ));
    }
    // std::fs::rename replaces a file on both Windows and Unix. Do not remove
    // the old destination first: an occupied/locked Windows file must stay intact.
    if let Err(e) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "保存先を更新できませんでした。別のアプリで開いていないか確認してください。元のファイルは保持されています: {e}"
        ));
    }
    // Persist the directory entry on platforms that permit opening directories.
    // Windows does not; the already-flushed file remains a successful save.
    #[cfg(unix)]
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

/// Compare parameter representations as well as their numeric values.
///
/// Graph/Project retain ordinary structural PartialEq. Editor and history checks
/// supplement it with this comparison because +0 and -0 are equal numerically
/// but produce different branch-cut results and evaluator cache identities.
/// This also checks parameters inside saved cluster definitions, without
/// allocating or serializing the graph. Use it alongside the structural checks.
pub fn same_parameter_bits(a: &Graph, b: &Graph) -> bool {
    a.nodes.len() == b.nodes.len()
        && a.clusters.len() == b.clusters.len()
        && a.nodes.iter().zip(&b.nodes).all(|(a, b)| {
            a.params.len() == b.params.len()
                && a.params
                    .iter()
                    .zip(&b.params)
                    .all(|((ak, av), (bk, bv))| ak == bk && av.to_bits() == bv.to_bits())
        })
        && a.clusters
            .iter()
            .zip(&b.clusters)
            .all(|(a, b)| same_parameter_bits(&a.graph, &b.graph))
}

/// Call checkpoint immediately before an actual document edit.
#[derive(Default)]
pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl History {
    pub fn checkpoint(&mut self, project: &Project) {
        self.store_checkpoint(Cow::Borrowed(project));
    }

    /// Accept a pre-edit snapshot that the editor no longer needs to retain.
    pub fn checkpoint_owned(&mut self, project: Project) {
        self.store_checkpoint(Cow::Owned(project));
    }

    fn store_checkpoint(&mut self, project: Cow<'_, Project>) {
        if self.undo.last().is_none_or(|previous| {
            previous != project.as_ref() || !same_parameter_bits(&previous.graph, &project.graph)
        }) {
            self.undo.push(project.into_owned());
            if self.undo.len() > HISTORY_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(project, previous));
        true
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(project, next));
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportReport {
    pub vertices: usize,
    pub triangles: usize,
    /// Edges incident to one triangle, after coordinate welding at 0.000001 mm.
    pub open_edges: usize,
    pub non_manifold_edges: usize,
    /// Welded vertices whose incident faces do not form one manifold fan.
    pub non_manifold_vertices: usize,
    pub inconsistent_edges: usize,
    /// Repeated non-degenerate faces after welding, including opposite winding.
    pub duplicate_faces: usize,
    pub degenerate_triangles: usize,
    pub mesh_count: usize,
    pub signed_volume_mm3: f64,
    pub bounds: Option<[[f64; 3]; 2]>,
    pub skipped_values: usize,
}

impl ExportReport {
    /// Closed, consistently oriented manifold topology after coordinate welding.
    /// Does not establish absence of self-intersections, correct shell nesting,
    /// material volume, wall thickness, or printability.
    pub fn is_closed(&self) -> bool {
        self.triangles > 0
            && self.open_edges == 0
            && self.non_manifold_edges == 0
            && self.non_manifold_vertices == 0
            && self.inconsistent_edges == 0
            && self.duplicate_faces == 0
            && self.degenerate_triangles == 0
    }
}

fn meshes(trees: &[Tree]) -> Result<(Vec<&Mesh>, usize), String> {
    let mut result = Vec::new();
    let mut skipped = 0usize;
    let mut vertices = 0usize;
    let mut triangles = 0usize;
    for value in trees.iter().flat_map(|t| t.branches.values()).flatten() {
        // Exact B-reps export their welded display mesh (`kernel_bridge::display_mesh`).
        let mesh = match value {
            Value::Mesh(mesh) => Some(mesh),
            Value::Brep(b) => Some(b.preview()),
            _ => None,
        };
        if let Some(mesh) = mesh {
            vertices = vertices.saturating_add(mesh.vertices.len());
            triangles = triangles.saturating_add(mesh.triangles.len());
            if vertices > MAX_EXPORT_VERTICES || triangles > MAX_EXPORT_TRIANGLES {
                return Err("形状が大きすぎます。分割数または複製数を減らしてください。".into());
            }
            result.push(mesh.as_ref());
        } else {
            skipped += 1;
        }
    }
    if result.is_empty() || triangles == 0 {
        return Err(
            "書き出せる立体がありません。曲線には「押し出し」などをつないでください。".into(),
        );
    }
    Ok((result, skipped))
}

/// Counts are topological checks, not proof of printable wall thickness or absence
/// of self-intersections. Coincident positions are quantized to 0.000001 mm.
pub fn mesh_stats(trees: &[Tree]) -> Result<ExportReport, String> {
    let (all_meshes, skipped) = meshes(trees)?;
    let mut report = ExportReport {
        skipped_values: skipped,
        ..ExportReport::default()
    };
    let mut welded: HashMap<[i64; 3], usize> = HashMap::new();
    let mut faces = Vec::with_capacity(all_meshes.iter().map(|mesh| mesh.triangles.len()).sum());
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for mesh in all_meshes {
        report.mesh_count += 1;
        report.vertices += mesh.vertices.len();
        report.triangles += mesh.triangles.len();
        let mut ids = Vec::with_capacity(mesh.vertices.len());
        let volume_origin = mesh.vertices.first().copied().unwrap_or([0.0; 3]);
        for &point in &mesh.vertices {
            if point
                .iter()
                .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
            {
                return Err("形状に無限大、非数、または大きすぎる座標が含まれています。".into());
            }
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(point[axis]);
                maximum[axis] = maximum[axis].max(point[axis]);
            }
            let key = point.map(|v| (v * WELD_QUANTIZATION).round() as i64);
            let next = welded.len();
            ids.push(*welded.entry(key).or_insert(next));
        }
        for &triangle in &mesh.triangles {
            let [ia, ib, ic] = triangle.map(|i| i as usize);
            let (Some(&a), Some(&b), Some(&c)) = (
                mesh.vertices.get(ia),
                mesh.vertices.get(ib),
                mesh.vertices.get(ic),
            ) else {
                return Err(
                    "面が存在しない頂点を参照しています。形状を作り直してください。".into(),
                );
            };
            let normal = cross(sub(b, a), sub(c, a));
            let area_squared = dot(normal, normal);
            let (Some(&wa), Some(&wb), Some(&wc)) = (ids.get(ia), ids.get(ib), ids.get(ic)) else {
                return Err("頂点の対応を確認できませんでした。".into());
            };
            if area_squared <= 1.0e-24 || wa == wb || wb == wc || wc == wa {
                report.degenerate_triangles += 1;
                continue;
            }
            report.signed_volume_mm3 += dot(
                sub(a, volume_origin),
                cross(sub(b, volume_origin), sub(c, volume_origin)),
            ) / 6.0;
            // The aggregate export budget is 2m vertices, well within u32.
            // Analyse all meshes together: their welded vertices can touch even
            // when every separately indexed source is individually manifold.
            faces.push([wa as u32, wb as u32, wc as u32]);
        }
    }
    let topology = crate::mesh_fabrication::audit_topology(welded.len(), &faces, &|| false)?;
    report.open_edges = topology.boundary_edges;
    report.non_manifold_edges = topology.non_manifold_edges;
    report.non_manifold_vertices = topology.non_manifold_vertices;
    report.inconsistent_edges = topology.orientation_conflicts;
    report.duplicate_faces = topology.duplicate_faces;
    if minimum.iter().all(|v| v.is_finite()) {
        report.bounds = Some([minimum, maximum]);
    }
    Ok(report)
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

fn export_check(trees: &[Tree]) -> Result<ExportReport, String> {
    let report = mesh_stats(trees)?;
    if report.degenerate_triangles > 0 {
        return Err(format!(
            "面積が 0 の面が {} 枚あります。形状の設定を見直してください。",
            report.degenerate_triangles
        ));
    }
    Ok(report)
}

pub fn export_stl(path: &Path, trees: &[Tree]) -> Result<ExportReport, String> {
    let report = export_check(trees)?;
    let (all_meshes, _) = meshes(trees)?;
    // STL stores f32 coordinates. Check after rounding too: far-away small faces
    // can collapse even when their f64 source is valid.
    for mesh in &all_meshes {
        for triangle in &mesh.triangles {
            let mut points = [[0.0f64; 3]; 3];
            for (corner, &index) in triangle.iter().enumerate() {
                let Some(point) = mesh.vertices.get(index as usize) else {
                    return Err("面の頂点を読み取れませんでした。".into());
                };
                points[corner] = point.map(|v| (v as f32) as f64);
            }
            let normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
            if dot(normal, normal) <= 1.0e-24 {
                return Err("STL の座標精度では小さすぎる面があります。原点へ移動するか OBJ を選んでください。".into());
            }
        }
    }
    atomic_write(path, |file| {
        let mut header = [0u8; 80];
        let label = b"NODES binary STL | units: millimeters | mesh geometry";
        header[..label.len()].copy_from_slice(label);
        file.write_all(&header)?;
        file.write_all(&(report.triangles as u32).to_le_bytes())?;
        for mesh in &all_meshes {
            for triangle in &mesh.triangles {
                let a = mesh.vertices[triangle[0] as usize];
                let b = mesh.vertices[triangle[1] as usize];
                let c = mesh.vertices[triangle[2] as usize];
                let n = cross(sub(b, a), sub(c, a));
                let length = dot(n, n).sqrt();
                for value in n.map(|v| (v / length) as f32) {
                    file.write_all(&value.to_le_bytes())?;
                }
                for point in [a, b, c] {
                    for value in point {
                        file.write_all(&(value as f32).to_le_bytes())?;
                    }
                }
                file.write_all(&0u16.to_le_bytes())?;
            }
        }
        Ok(())
    })?;
    Ok(report)
}

pub fn export_obj(path: &Path, trees: &[Tree]) -> Result<ExportReport, String> {
    let report = export_check(trees)?;
    let (all_meshes, _) = meshes(trees)?;
    atomic_write(path, |file| {
        writeln!(
            file,
            "# NODES OBJ\n# Units: millimeters\n# Triangulated mesh geometry"
        )?;
        let mut offset = 1usize;
        for (object, mesh) in all_meshes.iter().enumerate() {
            writeln!(file, "o NODES_{}", object + 1)?;
            for point in &mesh.vertices {
                // Preserve the validated binary64 coordinates on text round-trip.
                // Fixed decimals can flatten valid thin triangles after checking.
                writeln!(file, "v {} {} {}", point[0], point[1], point[2])?;
            }
            for t in &mesh.triangles {
                writeln!(
                    file,
                    "f {} {} {}",
                    t[0] as usize + offset,
                    t[1] as usize + offset,
                    t[2] as usize + offset
                )?;
            }
            offset += mesh.vertices.len();
        }
        Ok(())
    })?;
    Ok(report)
}
