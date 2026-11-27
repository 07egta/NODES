//! Preparing a later body must never overwrite an earlier destination first.
use morphlet::model::Mesh;
use morphlet::stl_export::{OutputFormat, numbered_path, write_bodies, write_bodies_detailed};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);
const FIRST: &[u8] = b"FIRST EXISTING FILE";
const SECOND: &[u8] = b"SECOND EXISTING FILE";

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("nodes-export-batch-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tetrahedron() -> Mesh {
    Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    }
}

fn existing_files(base: &Path) -> [PathBuf; 2] {
    let paths = [numbered_path(base, 1, 2), numbered_path(base, 2, 2)];
    fs::write(&paths[0], FIRST).unwrap();
    fs::write(&paths[1], SECOND).unwrap();
    paths
}

fn assert_preserved(paths: &[PathBuf; 2]) {
    assert_eq!(fs::read(&paths[0]).unwrap(), FIRST);
    assert_eq!(fs::read(&paths[1]).unwrap(), SECOND);
    assert_eq!(fs::read_dir(paths[0].parent().unwrap()).unwrap().count(), 2);
}

#[test]
fn later_binary_stl_precision_failure_preserves_both_existing_files() {
    let temp = TempDir::new();
    let base = temp.0.join("parts.stl");
    let paths = existing_files(&base);
    let mut far = tetrahedron();
    for point in &mut far.vertices {
        *point = [
            100_000_000. + point[0] * 0.01,
            point[1] * 0.01,
            point[2] * 0.01,
        ];
    }
    let error =
        write_bodies(OutputFormat::StlBinary, &base, &[tetrahedron(), far], true).unwrap_err();
    assert!(error.contains("座標精度"), "{error}");
    assert_preserved(&paths);
}

#[test]
fn later_3mf_topology_failure_preserves_both_existing_files() {
    let temp = TempDir::new();
    let base = temp.0.join("parts.3mf");
    let paths = existing_files(&base);
    let mut open = tetrahedron();
    open.triangles.pop();
    assert!(write_bodies(OutputFormat::ThreeMf, &base, &[tetrahedron(), open], true).is_err());
    assert_preserved(&paths);
}

#[test]
fn all_formats_validate_every_body_before_replacing_any_destination() {
    for format in OutputFormat::ALL {
        let temp = TempDir::new();
        let base = temp.0.join(format!("parts.{}", format.extension()));
        let paths = existing_files(&base);
        let mut invalid = tetrahedron();
        invalid.vertices[0][0] = f64::NAN;
        assert!(write_bodies(format, &base, &[tetrahedron(), invalid], true).is_err());
        assert_preserved(&paths);
    }
}

#[test]
fn a_later_directory_destination_is_rejected_before_the_first_replacement() {
    let temp = TempDir::new();
    let base = temp.0.join("parts.stl");
    let first = numbered_path(&base, 1, 2);
    let second = numbered_path(&base, 2, 2);
    fs::write(&first, FIRST).unwrap();
    fs::create_dir(&second).unwrap();
    assert!(
        write_bodies(
            OutputFormat::StlBinary,
            &base,
            &[tetrahedron(), tetrahedron()],
            true
        )
        .is_err()
    );
    assert_eq!(fs::read(&first).unwrap(), FIRST);
    assert!(second.is_dir());
    assert_eq!(fs::read_dir(temp.0.clone()).unwrap().count(), 2);
}

#[test]
fn successful_split_exports_replace_each_file_and_report_only_finished_outputs() {
    for format in OutputFormat::ALL {
        let temp = TempDir::new();
        let base = temp.0.join(format!("parts.{}", format.extension()));
        let paths = existing_files(&base);
        let summary = write_bodies(format, &base, &[tetrahedron(), tetrahedron()], true).unwrap();
        assert_eq!(summary.files, paths);
        assert_eq!(summary.triangles, 8);
        assert_ne!(fs::read(&paths[0]).unwrap(), FIRST);
        assert_ne!(fs::read(&paths[1]).unwrap(), SECOND);
        assert_eq!(fs::read_dir(temp.0.clone()).unwrap().count(), 2);
    }
}

#[test]
fn detailed_preparation_failure_names_the_target_and_reports_no_saved_files() {
    let temp = TempDir::new();
    let base = temp.0.join("parts.obj");
    let paths = existing_files(&base);
    let mut invalid = tetrahedron();
    invalid.triangles[0][0] = 100;
    let failure = write_bodies_detailed(OutputFormat::Obj, &base, &[tetrahedron(), invalid], true)
        .unwrap_err();
    assert!(failure.completed.files.is_empty());
    assert_eq!(failure.completed.triangles, 0);
    assert_eq!(failure.failed_path.as_ref(), Some(&paths[1]));
    assert!(failure.to_string().contains("保存先は変更されていません"));
    assert_preserved(&paths);
}
