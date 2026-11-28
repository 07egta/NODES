//! Bounded 3MF Core mesh import. Geometry only, not a slicer project reader.
use crate::engine::{cancelled, validate_mesh};
use crate::model::{MAX_TRIANGLES, MAX_VERTICES, Mesh};
use crate::stl_export::crc32;
use std::collections::{HashMap, HashSet};
use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};

const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const MODEL_REL: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const MODEL_PATH: &str = "3D/3dmodel.model";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_RELS: usize = 256 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_OBJECTS: usize = 4096;
const BAD_ZIP: &str = "3MF の ZIP 構造が不正か、未対応の形式です。";
const BAD_XML: &str = "3MF の XML 構造または数値が不正です。";
const LIMIT: &str = "3MF の読み込み上限を超えています。";
const UNSUPPORTED: &str = "未対応の 3MF 構造です。単純なメッシュの 3MF または STL を使用してください。";

type Res<T> = Result<T, String>;

fn bytes_at(b: &[u8], at: usize, len: usize) -> Res<&[u8]> {
    b.get(at..at.checked_add(len).ok_or(BAD_ZIP)?)
        .ok_or_else(|| BAD_ZIP.into())
}
fn u16_at(b: &[u8], at: usize) -> Res<usize> {
    let raw = bytes_at(b, at, 2)?.try_into().map_err(|_| BAD_ZIP)?;
    Ok(usize::from(u16::from_le_bytes(raw)))
}
fn u32_at(b: &[u8], at: usize) -> Res<usize> {
    let raw = bytes_at(b, at, 4)?.try_into().map_err(|_| BAD_ZIP)?;
    usize::try_from(u32::from_le_bytes(raw)).map_err(|_| BAD_ZIP.into())
}

/// Read a single named part using the central directory (including archives with
/// data descriptors). Never extract files or resolve external relationships.
/// ZIP64, multidisk, encryption, and non-store/non-deflate methods are unsupported.
fn zip_part(b: &[u8], wanted: &str, limit: usize, stop: &dyn Fn() -> bool) -> Res<Vec<u8>> {
    let last = b.len().checked_sub(22).ok_or(BAD_ZIP)?;
    let first = last.saturating_sub(65_535);
    let end = (first..=last)
        .rev()
        .find(|&i| {
            b.get(i..i + 4) == Some(b"PK\x05\x06".as_slice())
                && u16_at(b, i + 20).is_ok_and(|n| i + 22 + n == b.len())
        })
        .ok_or(BAD_ZIP)?;
    let count = u16_at(b, end + 10)?;
    let cd_len = u32_at(b, end + 12)?;
    let cd_start = u32_at(b, end + 16)?;
    if u16_at(b, end + 4)? != 0
        || u16_at(b, end + 6)? != 0
        || u16_at(b, end + 8)? != count
        || count > MAX_ENTRIES
        || cd_start.checked_add(cd_len) != Some(end)
    {
        return Err(BAD_ZIP.into());
    }
    let mut at = cd_start;
    let mut found = None;
    for _ in 0..count {
        cancelled(stop)?;
        if bytes_at(b, at, 4)? != b"PK\x01\x02" || at + 46 > end {
            return Err(BAD_ZIP.into());
        }
        let flags = u16_at(b, at + 8)?;
        let method = u16_at(b, at + 10)?;
        let crc = u32_at(b, at + 16)? as u32;
        let packed = u32_at(b, at + 20)?;
        let size = u32_at(b, at + 24)?;
        let name_len = u16_at(b, at + 28)?;
        let extra_len = u16_at(b, at + 30)?;
        let comment_len = u16_at(b, at + 32)?;
        let disk = u16_at(b, at + 34)?;
        let offset = u32_at(b, at + 42)?;
        let name = bytes_at(b, at + 46, name_len)?;
        at += 46 + name_len + extra_len + comment_len;
        if at > end || disk != 0 {
            return Err(BAD_ZIP.into());
        }
        if name != wanted.as_bytes() {
            continue;
        }
        if found.is_some() || flags & !0x080e != 0 || !matches!(method, 0 | 8) {
            return Err(BAD_ZIP.into());
        }
        if size > limit || packed > MAX_BYTES {
            return Err(LIMIT.into());
        }
        if offset >= cd_start || bytes_at(b, offset, 4)? != b"PK\x03\x04" {
            return Err(BAD_ZIP.into());
        }
        let local_flags = u16_at(b, offset + 6)?;
        let local_method = u16_at(b, offset + 8)?;
        let local_name_len = u16_at(b, offset + 26)?;
        let local_extra_len = u16_at(b, offset + 28)?;
        if local_flags != flags
            || local_method != method
            || bytes_at(b, offset + 30, local_name_len)? != name
        {
            return Err(BAD_ZIP.into());
        }
        if flags & 8 == 0
            && (u32_at(b, offset + 14)? as u32 != crc
                || u32_at(b, offset + 18)? != packed
                || u32_at(b, offset + 22)? != size)
        {
            return Err(BAD_ZIP.into());
        }
        let data_at = offset + 30 + local_name_len + local_extra_len;
        if data_at.checked_add(packed).is_none_or(|e| e > cd_start) {
            return Err(BAD_ZIP.into());
        }
        let input = bytes_at(b, data_at, packed)?;
        cancelled(stop)?;
        let data = if method == 0 {
            if packed != size {
                return Err(BAD_ZIP.into());
            }
            input.to_vec()
        } else {
            miniz_oxide::inflate::decompress_to_vec_with_limit(input, size)
                .map_err(|_| BAD_ZIP)?
        };
        cancelled(stop)?;
        if data.len() != size || crc32(&data) != crc {
            return Err(BAD_ZIP.into());
        }
        found = Some(data);
    }
    if at != end {
        return Err(BAD_ZIP.into());
    }
    found.ok_or_else(|| format!("3MF に {wanted} がありません。"))
}

fn xml_guard(bytes: &[u8]) -> Res<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| BAD_XML)?;
    if text.contains('\0') || text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return Err(BAD_XML.into());
    }
    Ok(())
}
fn attr<'a>(attrs: &'a [OwnedAttribute], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.name.namespace.is_none() && a.name.local_name == name)
        .map(|a| a.value.as_str())
}
fn required<'a>(attrs: &'a [OwnedAttribute], name: &str) -> Res<&'a str> {
    attr(attrs, name).ok_or_else(|| BAD_XML.into())
}
fn number(text: &str) -> Res<f64> {
    let n = text.parse::<f64>().map_err(|_| BAD_XML)?;
    if !n.is_finite() {
        return Err(BAD_XML.into());
    }
    Ok(n)
}
fn id(attrs: &[OwnedAttribute], key: &str) -> Res<u32> {
    required(attrs, key)?
        .parse::<u32>()
        .map_err(|_| BAD_XML.into())
}
fn check_start_part(bytes: &[u8], stop: &dyn Fn() -> bool) -> Res<()> {
    xml_guard(bytes)?;
    let mut depth = 0usize;
    let mut found = false;
    for event in EventReader::new(bytes) {
        cancelled(stop)?;
        match event.map_err(|_| BAD_XML)? {
            XmlEvent::StartDocument { encoding, .. } => {
                if !encoding.eq_ignore_ascii_case("utf-8") {
                    return Err(UNSUPPORTED.into());
                }
            }
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.namespace.as_deref() != Some(REL_NS) || attributes.len() > 32 {
                    return Err(BAD_XML.into());
                }
                match (depth, name.local_name.as_str()) {
                    (0, "Relationships") => {}
                    (1, "Relationship") => {
                        if required(&attributes, "Type")? == MODEL_REL {
                            let target = required(&attributes, "Target")?;
                            if found
                                || attr(&attributes, "TargetMode").unwrap_or("Internal") != "Internal"
                                || target.strip_prefix('/').unwrap_or(target) != MODEL_PATH
                            {
                                return Err(UNSUPPORTED.into());
                            }
                            found = true;
                        }
                    }
                    _ => return Err(BAD_XML.into()),
                }
                depth += 1;
            }
            XmlEvent::EndElement { .. } => depth = depth.checked_sub(1).ok_or(BAD_XML)?,
            XmlEvent::Characters(s) | XmlEvent::Whitespace(s) | XmlEvent::CData(s) => {
                if !s.trim().is_empty() {
                    return Err(BAD_XML.into());
                }
            }
            XmlEvent::Comment(_) | XmlEvent::EndDocument => {}
            #[allow(unreachable_patterns)]
            _ => return Err(BAD_XML.into()),
        }
    }
    if !found || depth != 0 {
        return Err(BAD_XML.into());
    }
    Ok(())
}

fn unit_mm(attrs: &[OwnedAttribute]) -> Res<f64> {
    match attr(attrs, "unit").unwrap_or("millimeter") {
        "micron" => Ok(0.001),
        "millimeter" => Ok(1.0),
        "centimeter" => Ok(10.0),
        "inch" => Ok(25.4),
        "foot" => Ok(304.8),
        "meter" => Ok(1000.0),
        _ => Err(BAD_XML.into()),
    }
}

fn empty_mesh() -> Mesh {
    Mesh {
        vertices: Vec::new(),
        triangles: Vec::new(),
    }
}

/// Apply the row-vector affine transform specified by 3MF, then convert to mm.
/// Mirrored instances reverse their triangle winding to preserve orientation.
fn append_instance(
    out: &mut Mesh,
    mesh: &Mesh,
    attrs: &[OwnedAttribute],
    unit: f64,
    stop: &dyn Fn() -> bool,
) -> Res<()> {
    if out.vertices.len() + mesh.vertices.len() > MAX_VERTICES
        || out.triangles.len() + mesh.triangles.len() > MAX_TRIANGLES
    {
        return Err(LIMIT.into());
    }
    let mut m = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    if let Some(text) = attr(attrs, "transform") {
        let mut words = text.split_whitespace();
        for slot in &mut m {
            *slot = number(words.next().ok_or(BAD_XML)?)?;
        }
        if words.next().is_some() {
            return Err(BAD_XML.into());
        }
    }
    let det = m[0] * (m[4] * m[8] - m[5] * m[7])
        - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if !det.is_finite() || det == 0.0 {
        return Err(BAD_XML.into());
    }
    let offset = out.vertices.len() as u32;
    for (i, p) in mesh.vertices.iter().enumerate() {
        if i % 4096 == 0 {
            cancelled(stop)?;
        }
        let q = std::array::from_fn(|a| {
            (p[0] * m[a] + p[1] * m[3 + a] + p[2] * m[6 + a] + m[9 + a]) * unit
        });
        if q.iter().any(|v| !v.is_finite() || v.abs() > 1.0e9) {
            return Err(BAD_XML.into());
        }
        out.vertices.push(q);
    }
    for (i, t) in mesh.triangles.iter().enumerate() {
        if i % 4096 == 0 {
            cancelled(stop)?;
        }
        let mut face = t.map(|v| v + offset);
        if det < 0.0 {
            face.swap(1, 2);
        }
        out.triangles.push(face);
    }
    Ok(())
}

fn parse_model(bytes: &[u8], stop: &dyn Fn() -> bool) -> Res<Mesh> {
    xml_guard(bytes)?;
    let mut stack: Vec<String> = Vec::new();
    let mut ids = HashSet::new();
    let mut objects = HashMap::new();
    let mut current: Option<(u32, Mesh)> = None;
    let mut out = empty_mesh();
    let mut unit = 1.0;
    let mut resources = false;
    let mut build = false;
    let mut mesh_seen = false;
    let mut vertices_seen = false;
    let mut triangles_seen = false;
    let mut raw_vertices = 0usize;
    let mut raw_triangles = 0usize;
    for (i, event) in EventReader::new(bytes).into_iter().enumerate() {
        if i % 4096 == 0 {
            cancelled(stop)?;
        }
        if i > 2_000_000 {
            return Err(LIMIT.into());
        }
        match event.map_err(|_| BAD_XML)? {
            XmlEvent::StartDocument { encoding, .. } => {
                if !encoding.eq_ignore_ascii_case("utf-8") {
                    return Err(UNSUPPORTED.into());
                }
            }
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.namespace.as_deref() != Some(CORE)
                    || stack.len() >= 8
                    || attributes.len() > 32
                    || attributes.iter().any(|a| {
                        a.name.namespace.is_some()
                            && !(a.name.namespace.as_deref()
                                == Some("http://www.w3.org/XML/1998/namespace")
                                && a.name.local_name == "lang")
                    })
                {
                    return Err(UNSUPPORTED.into());
                }
                let parent = stack.last().map(String::as_str).unwrap_or("");
                match (parent, name.local_name.as_str()) {
                    ("", "model") => {
                        if !attr(&attributes, "requiredextensions").unwrap_or("").trim().is_empty() {
                            return Err(UNSUPPORTED.into());
                        }
                        unit = unit_mm(&attributes)?;
                    }
                    ("model", "resources") if !resources && !build => resources = true,
                    ("model", "build") if resources && !build => build = true,
                    ("resources", "object") | ("resources", "basematerials") => {
                        let resource = id(&attributes, "id")?;
                        if resource == 0 || ids.len() >= MAX_OBJECTS || !ids.insert(resource) {
                            return Err(BAD_XML.into());
                        }
                        if name.local_name == "object" {
                            if attr(&attributes, "type").unwrap_or("model") != "model" {
                                return Err(UNSUPPORTED.into());
                            }
                            current = Some((resource, empty_mesh()));
                            mesh_seen = false;
                            vertices_seen = false;
                            triangles_seen = false;
                        }
                    }
                    ("basematerials", "base") | ("model" | "object", "metadata") => {}
                    ("object", "mesh") if !mesh_seen => mesh_seen = true,
                    ("mesh", "vertices") if !vertices_seen && !triangles_seen => vertices_seen = true,
                    ("mesh", "triangles") if vertices_seen && !triangles_seen => triangles_seen = true,
                    ("vertices", "vertex") => {
                        raw_vertices += 1;
                        if raw_vertices > MAX_VERTICES {
                            return Err(LIMIT.into());
                        }
                        let (_, mesh) = current.as_mut().ok_or(BAD_XML)?;
                        mesh.vertices.push([
                            number(required(&attributes, "x")?)?,
                            number(required(&attributes, "y")?)?,
                            number(required(&attributes, "z")?)?,
                        ]);
                    }
                    ("triangles", "triangle") => {
                        raw_triangles += 1;
                        if raw_triangles > MAX_TRIANGLES {
                            return Err(LIMIT.into());
                        }
                        let (_, mesh) = current.as_mut().ok_or(BAD_XML)?;
                        let face = [
                            id(&attributes, "v1")?,
                            id(&attributes, "v2")?,
                            id(&attributes, "v3")?,
                        ];
                        if face.iter().any(|&v| v as usize >= mesh.vertices.len())
                            || face[0] == face[1]
                            || face[1] == face[2]
                            || face[2] == face[0]
                        {
                            return Err(BAD_XML.into());
                        }
                        mesh.triangles.push(face);
                    }
                    ("build", "item") => {
                        let object = id(&attributes, "objectid")?;
                        let mesh = objects.get(&object).ok_or(BAD_XML)?;
                        append_instance(&mut out, mesh, &attributes, unit, stop)?;
                    }
                    _ => return Err(UNSUPPORTED.into()),
                }
                stack.push(name.local_name);
            }
            XmlEvent::EndElement { .. } => {
                if stack.pop().ok_or(BAD_XML)? == "object" {
                    let (id, mesh) = current.take().ok_or(BAD_XML)?;
                    if !mesh_seen || mesh.vertices.is_empty() || mesh.triangles.is_empty() {
                        return Err(BAD_XML.into());
                    }
                    objects.insert(id, mesh);
                }
            }
            XmlEvent::Characters(s) | XmlEvent::Whitespace(s) | XmlEvent::CData(s) => {
                if !s.trim().is_empty() && stack.last().map(String::as_str) != Some("metadata") {
                    return Err(BAD_XML.into());
                }
            }
            XmlEvent::Comment(_) | XmlEvent::EndDocument => {}
            #[allow(unreachable_patterns)]
            _ => return Err(BAD_XML.into()),
        }
    }
    if !resources || !build || !stack.is_empty() || out.triangles.is_empty() {
        return Err(BAD_XML.into());
    }
    validate_mesh(&out, stop)?;
    Ok(out)
}

/// Import a simple core 3MF archive into one indexed mesh in millimetres.
/// Build instances remain separate index ranges. Names, colours, materials,
/// print settings and other non-geometry metadata are not retained.
pub fn parse_3mf(bytes: &[u8], stop: &dyn Fn() -> bool) -> Res<Mesh> {
    if bytes.len() > MAX_BYTES {
        return Err(LIMIT.into());
    }
    cancelled(stop)?;
    let rels = zip_part(bytes, "_rels/.rels", MAX_RELS, stop)?;
    check_start_part(&rels, stop)?;
    let model = zip_part(bytes, MODEL_PATH, MAX_BYTES, stop)?;
    parse_model(&model, stop)
}
