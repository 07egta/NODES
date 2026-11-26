//! Spiral vase G-code: layer count, monotonic Z and E, bed bounds, and rejection cases.
use morphlet::gcode::{self, GcodeSettings};
use morphlet::model::{Mesh, Tree, Value};
use std::sync::Arc;

/// Closed cylinder, outward-facing triangles, bottom at z = 0.
fn cylinder(radius: f64, height: f64, segments: usize) -> Mesh {
    let n = segments as u32;
    let mut vertices = Vec::new();
    for z in [0.0, height] {
        for i in 0..segments {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            vertices.push([radius * a.cos(), radius * a.sin(), z]);
        }
    }
    let bottom_center = vertices.len() as u32;
    vertices.push([0.0, 0.0, 0.0]);
    let top_center = vertices.len() as u32;
    vertices.push([0.0, 0.0, height]);
    let mut triangles = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b, c, d) = (i, j, n + i, n + j);
        triangles.push([a, b, d]);
        triangles.push([a, d, c]);
        triangles.push([bottom_center, j, i]);
        triangles.push([top_center, c, d]);
    }
    Mesh {
        vertices,
        triangles,
    }
}

fn tree(mesh: Mesh) -> Vec<Tree> {
    vec![Tree::single(Value::Mesh(Arc::new(mesh)))]
}

/// Body lines only: between ;BODY-START and ;BODY-END.
fn body_lines(text: &str) -> Vec<&str> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with(";BODY-START") {
            inside = true;
            continue;
        }
        if line.starts_with(";BODY-END") {
            break;
        }
        if inside {
            out.push(line);
        }
    }
    out
}

/// Value of an axis word (X, Y, Z, E) before any comment.
fn word(line: &str, axis: char) -> Option<f64> {
    let code = line.split(';').next().unwrap_or("");
    code.split_whitespace().find_map(|w| {
        let mut chars = w.chars();
        if chars.next() == Some(axis) {
            chars.as_str().parse::<f64>().ok()
        } else {
            None
        }
    })
}

fn is_move(line: &str) -> bool {
    line.starts_with("G0 ") || line.starts_with("G1 ")
}

#[test]
fn cylinder_slices_into_expected_layers() {
    let (_, report) = gcode::generate(&tree(cylinder(10.0, 20.0, 48)), &GcodeSettings::default())
        .unwrap_or_else(|e| panic!("{e}"));
    // 20 mm tall at 0.2 mm layers: planes at 0.1, 0.3, ... 19.9 mm.
    assert_eq!(report.layers, 100);
    assert_eq!(report.flat_layers, 3);
    assert_eq!(report.spiral_layers, 97);
    assert!(report.filament_mm > 0.0);
    assert!(report.print_seconds > 0.0);
}

#[test]
fn z_is_monotonic_in_body() {
    let (text, _) = gcode::generate(&tree(cylinder(10.0, 20.0, 48)), &GcodeSettings::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let mut last = f64::NEG_INFINITY;
    for line in body_lines(&text).into_iter().filter(|l| is_move(l)) {
        if let Some(z) = word(line, 'Z') {
            assert!(z >= last - 1e-9, "Z went down: {last} -> {z} in `{line}`");
            last = z;
        }
    }
    assert!(last > 0.0);
}

#[test]
fn all_coordinates_stay_inside_bed() {
    let settings = GcodeSettings::default();
    let bed = settings.bed_mm;
    let (text, _) = gcode::generate(&tree(cylinder(10.0, 20.0, 48)), &settings)
        .unwrap_or_else(|e| panic!("{e}"));
    for line in text.lines().filter(|l| is_move(l)) {
        if let Some(x) = word(line, 'X') {
            assert!((0.0..=bed[0]).contains(&x), "X out of bed: `{line}`");
        }
        if let Some(y) = word(line, 'Y') {
            assert!((0.0..=bed[1]).contains(&y), "Y out of bed: `{line}`");
        }
        if let Some(z) = word(line, 'Z') {
            assert!((0.0..=bed[2]).contains(&z), "Z out of bed: `{line}`");
        }
    }
}

#[test]
fn extrusion_is_monotonic_in_body() {
    let (text, _) = gcode::generate(&tree(cylinder(10.0, 20.0, 48)), &GcodeSettings::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let mut last = f64::NEG_INFINITY;
    let mut count = 0usize;
    for line in body_lines(&text)
        .into_iter()
        .filter(|l| l.starts_with("G1 "))
    {
        if let Some(e) = word(line, 'E') {
            assert!(e >= last, "E went down: {last} -> {e} in `{line}`");
            last = e;
            count += 1;
        }
    }
    assert!(count > 1000, "too few extrusion moves: {count}");
}

#[test]
fn model_larger_than_bed_is_rejected() {
    // 200 mm across does not fit a 180 mm A1 mini bed.
    let result = gcode::generate(&tree(cylinder(100.0, 20.0, 48)), &GcodeSettings::default());
    assert!(result.is_err());
}

#[test]
fn empty_mesh_is_rejected() {
    let empty = Mesh {
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    assert!(gcode::generate(&tree(empty), &GcodeSettings::default()).is_err());
}

#[test]
fn open_mesh_is_rejected() {
    let mut open = cylinder(10.0, 20.0, 48);
    open.triangles.pop();
    assert!(gcode::generate(&tree(open), &GcodeSettings::default()).is_err());
}
