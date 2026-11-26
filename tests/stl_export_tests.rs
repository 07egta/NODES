use morphlet::model::{Mesh, Tree, Value};
use morphlet::persistence;
use morphlet::stl_export::{
    ExportOptions, OutputFormat, check_bodies, crc32, encode_3mf, gather_meshes, numbered_path,
    prepare_bodies, repair_meshes, slicer_command, split_bodies, timestamp_jst, write_bodies,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let number = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("morphlet-stl-test-{}-{number}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Unit cube with outward-facing counter-clockwise triangles (closed, volume 1).
fn cube() -> Mesh {
    Mesh {
        vertices: vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [1., 0., 1.],
            [1., 1., 1.],
            [0., 1., 1.],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
            [1, 2, 6],
            [1, 6, 5],
        ],
    }
}

fn tree(mesh: Mesh) -> Tree {
    Tree::single(Value::Mesh(Arc::new(mesh)))
}

fn stats_of(mesh: &Mesh) -> persistence::ExportReport {
    persistence::mesh_stats(&[tree(mesh.clone())]).unwrap()
}

fn bounds_of(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &mesh.vertices {
        for (axis, value) in p.iter().enumerate() {
            lo[axis] = lo[axis].min(*value);
            hi[axis] = hi[axis].max(*value);
        }
    }
    (lo, hi)
}

#[test]
fn binary_stl_has_correct_header_count_and_size() {
    let dir = TempDir::new();
    let path = dir.file("cube.stl");
    let summary = write_bodies(OutputFormat::StlBinary, &path, &[cube()], false).unwrap();
    assert_eq!(summary.triangles, 12);
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 84 + 50 * 12);
    assert_eq!(
        u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]),
        12
    );
    assert!(bytes[..80].starts_with(b"Morphlet binary STL"));
}

#[test]
fn ascii_stl_parses_back_to_the_same_triangles() {
    let dir = TempDir::new();
    let path = dir.file("cube_ascii.stl");
    write_bodies(OutputFormat::StlAscii, &path, &[cube()], false).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("solid "));
    assert!(text.trim_end().ends_with("endsolid Morphlet"));
    assert_eq!(text.matches("facet normal").count(), 12);
    assert_eq!(text.matches("vertex ").count(), 36);
}

#[test]
fn scale_and_bed_placement_are_applied_in_order() {
    let options = ExportOptions {
        scale: 10.0,
        place_on_bed: true,
        center_xy: true,
        ..ExportOptions::default()
    };
    let bodies = prepare_bodies(&[cube()], &options).unwrap();
    let (lo, hi) = bounds_of(&bodies[0]);
    assert!((lo[2]).abs() < 1e-9, "lowest point must sit on the bed");
    assert!((hi[2] - 10.0).abs() < 1e-9);
    assert!((lo[0] + 5.0).abs() < 1e-9 && (hi[0] - 5.0).abs() < 1e-9);
    assert!((lo[1] + 5.0).abs() < 1e-9 && (hi[1] - 5.0).abs() < 1e-9);

    let as_modelled = ExportOptions {
        scale: 2.0,
        place_on_bed: false,
        ..ExportOptions::default()
    };
    let kept = prepare_bodies(&[cube()], &as_modelled).unwrap();
    let (lo, hi) = bounds_of(&kept[0]);
    assert!(lo.iter().all(|v| v.abs() < 1e-9));
    assert!((hi[0] - 2.0).abs() < 1e-9);
}

#[test]
fn repair_fixes_a_flipped_face() {
    let mut mesh = cube();
    mesh.triangles[4].swap(1, 2);
    assert!(stats_of(&mesh).inconsistent_edges > 0);
    let (repaired, report) = repair_meshes(&[mesh]).unwrap();
    assert!(report.changed());
    let after = stats_of(&repaired[0]);
    assert_eq!(after.inconsistent_edges, 0);
    assert_eq!(after.open_edges, 0);
    assert!(
        after.signed_volume_mm3 > 0.9,
        "outward orientation expected"
    );
}

#[test]
fn repair_fills_a_missing_triangle_hole() {
    let mut mesh = cube();
    mesh.triangles.remove(0);
    assert!(stats_of(&mesh).open_edges > 0);
    let (repaired, report) = repair_meshes(&[mesh]).unwrap();
    assert_eq!(report.filled_holes, 1);
    assert_eq!(repaired[0].triangles.len(), 12);
    assert_eq!(
        repaired[0].vertices.len(),
        8,
        "repair must preserve the original vertices"
    );
    assert_eq!(report.added_triangles, 1);
    let after = stats_of(&repaired[0]);
    assert_eq!(after.open_edges, 0);
    assert_eq!(after.inconsistent_edges, 0);
}

#[test]
fn split_bodies_counts_disconnected_parts() {
    let mut two = cube();
    let offset = two.vertices.len() as u32;
    let mut far = cube();
    for p in &mut far.vertices {
        p[0] += 10.0;
    }
    two.vertices.extend(far.vertices.iter().copied());
    two.triangles.extend(
        far.triangles
            .iter()
            .map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]),
    );
    let parts = split_bodies(&two);
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().all(|p| p.triangles.len() == 12));
    let (_, stats) = check_bodies(&parts, 256.0).unwrap();
    assert_eq!(stats.bodies, 2);
}

#[test]
fn check_flags_oversize_and_tiny_models() {
    let (checks, _) = check_bodies(&[cube()], 0.5).unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c.level == morphlet::stl_export::Level::Error)
    );
    let mut tiny = cube();
    for p in &mut tiny.vertices {
        for v in p.iter_mut() {
            *v *= 0.01;
        }
    }
    let (checks, _) = check_bodies(&[tiny], 256.0).unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c.level == morphlet::stl_export::Level::Warn)
    );
}

#[test]
fn gather_counts_non_mesh_values_as_skipped() {
    let trees = vec![tree(cube()), Tree::single(Value::Number(3.0))];
    let (meshes, skipped) = gather_meshes(&trees).unwrap();
    assert_eq!(meshes.len(), 1);
    assert_eq!(skipped, 1);
}

#[test]
fn file_names_and_split_numbering() {
    assert_eq!(timestamp_jst(UNIX_EPOCH), "19700101-0900");
    assert_eq!(
        numbered_path(Path::new("out/vase.stl"), 2, 3),
        PathBuf::from("out/vase_02.stl")
    );
    assert_eq!(
        numbered_path(Path::new("out/vase.stl"), 1, 1),
        PathBuf::from("out/vase.stl")
    );
}

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

#[test]
fn threemf_zip_has_valid_local_headers_and_mm_units() {
    let bytes = encode_3mf(&[cube()]).unwrap();
    assert_eq!(&bytes[..4], &[0x50, 0x4b, 0x03, 0x04]);

    // Walk the local headers: name, stored size and CRC must agree with the data.
    let mut at = 0usize;
    let mut names = Vec::new();
    let mut model_xml = Vec::new();
    while at + 4 <= bytes.len() && read_u32(&bytes, at) == 0x0403_4b50 {
        let crc = read_u32(&bytes, at + 14);
        let size = read_u32(&bytes, at + 22) as usize;
        let name_len = read_u16(&bytes, at + 26) as usize;
        let start = at + 30;
        let name = String::from_utf8(bytes[start..start + name_len].to_vec()).unwrap();
        let data_start = start + name_len;
        let data = &bytes[data_start..data_start + size];
        assert_eq!(crc32(data), crc, "CRC mismatch in {name}");
        if name == "3D/3dmodel.model" {
            model_xml = data.to_vec();
        }
        names.push(name);
        at = data_start + size;
    }
    assert_eq!(
        names,
        vec!["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]
    );
    let xml = String::from_utf8(model_xml).unwrap();
    assert!(xml.contains("unit=\"millimeter\""));
    assert_eq!(xml.matches("<object ").count(), 1, "single object expected");
    assert!(
        read_u32(&bytes, at) == 0x0201_4b50,
        "central directory follows"
    );
}

#[test]
fn slicer_is_started_directly_without_a_shell() {
    let exe = Path::new(r"C:\Program Files\Bambu Studio\bambu-studio.exe");
    let file = Path::new(r"C:\out\vase_20261126-2130.stl");
    let command = slicer_command(exe, file);
    assert_eq!(command.get_program(), exe.as_os_str());
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, vec![file.as_os_str()]);
    let program = command.get_program().to_string_lossy().to_lowercase();
    assert!(!program.ends_with("cmd.exe") && !program.ends_with("sh"));
}

#[test]
fn options_reject_bad_scale() {
    let bad = ExportOptions {
        scale: f64::NAN,
        ..ExportOptions::default()
    };
    assert!(prepare_bodies(&[cube()], &bad).is_err());
}
