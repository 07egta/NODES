//! Mesh file import (STL and OBJ) for the ファイル読み込み node.
//!
//! The files come from outside the app, so every input is treated as hostile: sizes are
//! checked before anything is allocated, coordinates must be finite, slices are read with
//! `get`, and every failure is a Japanese message. Nothing here panics.
//!
//! Vertices of both formats are welded by position (quantized to 0.000001 mm), so a closed
//! source becomes a closed indexed mesh. Degenerate triangles are dropped.
use crate::engine::{cancelled, cross, length, sub};
use crate::model::{MAX_TRIANGLES, MAX_VERTICES, Mesh};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Largest file that is read at all (64 MiB).
pub const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_COORDINATE: f64 = 1.0e9;
const WELD_SCALE: f64 = 1.0e6;
/// Same threshold as `engine::validate_mesh` uses for zero-area faces.
const MIN_CROSS: f64 = 1.0e-12;
const STL_HEADER_BYTES: usize = 84;
const STL_RECORD_BYTES: usize = 50;
const CHECK_INTERVAL: usize = 4_096;

const NO_FILE: &str =
    "ファイルが選ばれていません。「ファイルを選ぶ」で STL または OBJ を選んでください。";
const BAD_FORMAT: &str = "STL または OBJ として読めませんでした。ファイルが途中で切れていないか、形式を確認してください。";
const BAD_NUMBER: &str =
    "座標の数値が正しくありません（NaN・無限大・極端に大きな値は使えません）。";
const TOO_MANY_VERTICES: &str = "頂点数が上限（250,000）を超えています。";
const TOO_MANY_TRIANGLES: &str = "三角形の数が上限（500,000）を超えています。";
const INTERNAL: &str = "読み込み中に内部の整合性エラーが起きました。";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Stl,
    Obj,
}

/// File format from the extension (case-insensitive). None for other files.
pub fn format_of(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "stl" => Some(Format::Stl),
        "obj" => Some(Format::Obj),
        _ => None,
    }
}

/// Cache-key text for a source file: path, modified time and size. Any change to them makes
/// the node compute again. A missing file is keyed as missing.
pub fn source_key(path: &str) -> String {
    match std::fs::metadata(path) {
        Ok(meta) => format!("{path}\n{:?}\n{}", meta.modified().ok(), meta.len()),
        Err(_) => format!("{path}\nmissing"),
    }
}

/// Reads and parses one STL or OBJ file. The size is checked before and after reading.
pub fn read_file(path: &str, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    if path.is_empty() {
        return Err(NO_FILE.into());
    }
    let format = format_of(Path::new(path))
        .ok_or("STL または OBJ 以外のファイルは読み込めません。拡張子を確認してください。")?;
    let meta = std::fs::metadata(path).map_err(|_| {
        "ファイルを開けませんでした。移動・削除されていないか確認してください。".to_string()
    })?;
    if !meta.is_file() {
        return Err("フォルダーではなくファイルを選んでください。".into());
    }
    if meta.len() > MAX_IMPORT_BYTES {
        return Err("ファイルが大きすぎます（上限 64 MiB）。".into());
    }
    let file = File::open(path).map_err(|_| "ファイルを開けませんでした。".to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "ファイルを読み込めませんでした。".to_string())?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("ファイルが大きすぎます（上限 64 MiB）。".into());
    }
    cancelled(cancel)?;
    match format {
        Format::Stl => parse_stl(&bytes, cancel),
        Format::Obj => parse_obj(&bytes, cancel),
    }
}

/// Parses binary or ASCII STL. A file is binary when its size equals 84 + 50 × count
/// (whatever its header says); otherwise a file that starts with "solid" is read as ASCII.
pub fn parse_stl(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    if let Some(count) = binary_stl_count(bytes) {
        return parse_binary_stl(bytes, count, cancel);
    }
    if bytes.starts_with(b"solid") {
        return parse_ascii_stl(bytes, cancel);
    }
    Err(BAD_FORMAT.into())
}

/// Triangle count from the binary header, if the file size agrees with it.
fn binary_stl_count(bytes: &[u8]) -> Option<u32> {
    let raw: [u8; 4] = <[u8; 4]>::try_from(bytes.get(80..84)?).ok()?;
    let count = u32::from_le_bytes(raw);
    let expected = STL_HEADER_BYTES as u64 + STL_RECORD_BYTES as u64 * u64::from(count);
    (expected == bytes.len() as u64).then_some(count)
}

fn parse_binary_stl(bytes: &[u8], count: u32, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    if count == 0 {
        return Err("STL に三角形がありません。".into());
    }
    // Checked before the loop: nothing is allocated for an oversized header.
    if count > MAX_TRIANGLES {
        return Err(TOO_MANY_TRIANGLES.into());
    }
    let mut builder = Builder::default();
    for index in 0..count {
        if index % CHECK_INTERVAL == 0 {
            cancelled(cancel)?;
        }
        // Each record is 12 bytes of normal (not used; normals are recomputed), then 3 corners.
        let base = STL_HEADER_BYTES + index * STL_RECORD_BYTES;
        let corners = [
            read_point(bytes, base + 12)?,
            read_point(bytes, base + 24)?,
            read_point(bytes, base + 36)?,
        ];
        builder.triangle(corners)?;
    }
    builder.finish()
}

fn read_point(bytes: &[u8], offset: usize) -> Result<[f64; 3], String> {
    let mut point = [0.0f64; 3];
    for (axis, slot) in point.iter_mut().enumerate() {
        let start = offset + axis * 4;
        let raw: [u8; 4] = bytes
            .get(start..start + 4)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
            .ok_or(BAD_FORMAT)?;
        *slot = checked_coordinate(f64::from(f32::from_le_bytes(raw)))?;
    }
    Ok(point)
}

fn parse_ascii_stl(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut builder = Builder::default();
    let mut corners = [[0.0f64; 3]; 3];
    let mut filled = 0usize;
    for (line_no, line) in text.lines().enumerate() {
        if line_no % CHECK_INTERVAL == 0 {
            cancelled(cancel)?;
        }
        let mut words = line.split_whitespace();
        if words.next() != Some("vertex") {
            continue;
        }
        let point = parse_xyz(words)?;
        if let Some(slot) = corners.get_mut(filled) {
            *slot = point;
        }
        filled += 1;
        if filled == 3 {
            builder.triangle(corners)?;
            filled = 0;
        }
    }
    if filled != 0 {
        return Err(
            "STL の頂点の数が三角形になりません。ファイルが途中で切れていないか確認してください。"
                .into(),
        );
    }
    builder.finish()
}

/// Parses OBJ `v` and `f` lines only. Polygons are fan-triangulated; negative indices count
/// back from the last vertex defined so far. Other lines (vt, vn, g, usemtl...) are ignored.
pub fn parse_obj(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut raw: Vec<[f64; 3]> = Vec::new();
    let mut builder = Builder::default();
    for (line_no, line) in text.lines().enumerate() {
        if line_no % CHECK_INTERVAL == 0 {
            cancelled(cancel)?;
        }
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => {
                if raw.len() >= MAX_VERTICES {
                    return Err(TOO_MANY_VERTICES.into());
                }
                raw.push(parse_xyz(words)?);
            }
            Some("f") => {
                let mut corners: Vec<[f64; 3]> = Vec::new();
                for word in words {
                    let head = word.split('/').next().unwrap_or("");
                    let index: i64 = head
                        .parse()
                        .map_err(|_| "面の頂点番号が正しくありません。".to_string())?;
                    let len = i64::try_from(raw.len()).unwrap_or(i64::MAX);
                    let position = match index {
                        0 => {
                            return Err(
                                "OBJ の頂点番号は 1 から始まります（0 は使えません）。".into()
                            );
                        }
                        n if n > 0 => n - 1,
                        n => len + n,
                    };
                    let point = usize::try_from(position)
                        .ok()
                        .and_then(|p| raw.get(p))
                        .copied()
                        .ok_or("面が存在しない頂点を参照しています。")?;
                    corners.push(point);
                }
                if corners.len() < 3 {
                    return Err("面の頂点は 3 つ以上必要です。".into());
                }
                if let Some((first, rest)) = corners.split_first() {
                    for pair in rest.windows(2) {
                        if let [second, third] = pair {
                            builder.triangle([*first, *second, *third])?;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    builder.finish()
}

/// Three finite numbers from the words after a `vertex` or `v` keyword.
fn parse_xyz<'a>(mut words: impl Iterator<Item = &'a str>) -> Result<[f64; 3], String> {
    let mut point = [0.0f64; 3];
    for slot in &mut point {
        let value = words
            .next()
            .and_then(|w| w.parse::<f64>().ok())
            .ok_or(BAD_NUMBER)?;
        *slot = checked_coordinate(value)?;
    }
    Ok(point)
}

fn checked_coordinate(value: f64) -> Result<f64, String> {
    if value.is_finite() && value.abs() <= MAX_COORDINATE {
        Ok(value)
    } else {
        Err(BAD_NUMBER.into())
    }
}

/// Collects welded vertices and triangles, with the limits enforced as they grow.
#[derive(Default)]
struct Builder {
    index: HashMap<[i64; 3], u32>,
    vertices: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
}

impl Builder {
    /// Index of the welded vertex at `point`, and the stored position of that vertex.
    fn corner(&mut self, point: [f64; 3]) -> Result<(u32, [f64; 3]), String> {
        let key = point.map(|v| (v * WELD_SCALE).round() as i64);
        if let Some(&index) = self.index.get(&key) {
            let stored = self.vertices.get(index as usize).copied().ok_or(INTERNAL)?;
            return Ok((index, stored));
        }
        if self.vertices.len() >= MAX_VERTICES {
            return Err(TOO_MANY_VERTICES.into());
        }
        let index = u32::try_from(self.vertices.len()).map_err(|_| TOO_MANY_VERTICES)?;
        self.vertices.push(point);
        self.index.insert(key, index);
        Ok((index, point))
    }

    fn triangle(&mut self, points: [[f64; 3]; 3]) -> Result<(), String> {
        if self.triangles.len() >= MAX_TRIANGLES {
            return Err(TOO_MANY_TRIANGLES.into());
        }
        let [a, b, c] = points;
        let (ia, pa) = self.corner(a)?;
        let (ib, pb) = self.corner(b)?;
        let (ic, pc) = self.corner(c)?;
        // Corners that weld together, or a zero-area face, are dropped.
        if ia == ib || ib == ic || ia == ic || length(cross(sub(pb, pa), sub(pc, pa))) < MIN_CROSS {
            return Ok(());
        }
        self.triangles.push([ia, ib, ic]);
        Ok(())
    }

    /// Keeps only the welded vertices that some triangle uses, numbered in order of first use.
    fn finish(self) -> Result<Mesh, String> {
        if self.triangles.is_empty() {
            return Err(
                "読み込める三角形がありませんでした。面積がゼロの面だけの可能性があります。".into(),
            );
        }
        let mut remap = vec![u32::MAX; self.vertices.len()];
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        let mut triangles = Vec::with_capacity(self.triangles.len());
        for tri in &self.triangles {
            let mut out = [0u32; 3];
            for (slot, &old) in out.iter_mut().zip(tri.iter()) {
                let mapped = remap.get_mut(old as usize).ok_or(INTERNAL)?;
                if *mapped == u32::MAX {
                    *mapped = u32::try_from(vertices.len()).map_err(|_| INTERNAL)?;
                    vertices.push(self.vertices.get(old as usize).copied().ok_or(INTERNAL)?);
                }
                *slot = *mapped;
            }
            triangles.push(out);
        }
        Ok(Mesh {
            vertices,
            triangles,
        })
    }
}

/// Scale, then optionally centre X and Y on the origin and/or put the lowest point on Z=0.
/// The node's inputs are plain numbers, so the result is checked for finite coordinates here.
pub fn place(mesh: &mut Mesh, scale: f64, center: bool, bed: bool) -> Result<(), String> {
    if !(scale.is_finite() && scale > 0.) {
        return Err("倍率は 0 より大きい数にしてください。".into());
    }
    if mesh.vertices.is_empty() {
        return Err("読み込んだ形状に頂点がありません。".into());
    }
    for point in &mut mesh.vertices {
        *point = point.map(|v| v * scale);
    }
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for point in &mesh.vertices {
        for (axis, value) in point.iter().enumerate() {
            low[axis] = low[axis].min(*value);
            high[axis] = high[axis].max(*value);
        }
    }
    let mut offset = [0.0f64; 3];
    if center {
        offset[0] = -(low[0] + high[0]) / 2.;
        offset[1] = -(low[1] + high[1]) / 2.;
    }
    if bed {
        offset[2] = -low[2];
    }
    for point in &mut mesh.vertices {
        for (value, shift) in point.iter_mut().zip(offset) {
            *value += shift;
        }
    }
    let finite = mesh
        .vertices
        .iter()
        .all(|p| p.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE));
    if !finite {
        return Err("倍率や位置が大きすぎて座標の範囲を超えます。倍率を見直してください。".into());
    }
    Ok(())
}
