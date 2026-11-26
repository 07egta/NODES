//! Export coordinates must round-trip independently of the application's importer.
use morphlet::model::{Mesh, Tree, Value};
use morphlet::persistence;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("nodes-export-numeric-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn roundtrip(vertices: [[f64; 3]; 3]) -> Vec<[f64; 3]> {
    let temp = TempDir::new();
    let path = temp.0.join("precision.obj");
    let mesh = Mesh {
        vertices: vertices.to_vec(),
        triangles: vec![[0, 1, 2]],
    };
    let report =
        persistence::export_obj(&path, &[Tree::single(Value::Mesh(Arc::new(mesh)))]).unwrap();
    assert_eq!(report.degenerate_triangles, 0);
    let text = fs::read_to_string(path).unwrap();
    // Read standard OBJ vertex records directly, so an importer tolerance cannot
    // hide precision lost by the writer. The face indices must also survive.
    assert!(text.lines().any(|line| line == "f 1 2 3"));
    let decoded: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("v "))
        .map(|line| {
            let numbers: Vec<f64> = line
                .split_whitespace()
                .map(|word| word.parse().unwrap())
                .collect();
            <[f64; 3]>::try_from(numbers).unwrap()
        })
        .collect();
    assert_eq!(decoded.len(), vertices.len());
    for (actual, expected) in decoded.iter().zip(vertices) {
        assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
    }
    decoded
}

#[test]
fn obj_preserves_a_valid_thin_triangle_instead_of_rounding_it_flat() {
    // The cross-product magnitude is 4e-9, above the exporter's 1e-12 limit.
    // Fixed nine-decimal output turns all Y coordinates into zero.
    let decoded = roundtrip([[0., 0., 0.], [10., 0., 0.], [5., 4e-10, 0.]]);
    assert!(decoded[1][0] * decoded[2][1] > 0.);
}

#[test]
fn obj_preserves_large_coordinates_and_small_offsets_together() {
    let translated = f64::from_bits(100_000_000.0_f64.to_bits() + 1);
    roundtrip([
        [translated, -100_000_000., 3e-10],
        [100_000_100., -100_000_000., 7e-10],
        [100_000_050., -99_999_999.75, -4e-10],
    ]);
}
