//! Independent geometric expectations for print preparation. These tests do not
//! treat an edge count or positive volume as proof of a valid printable solid.
use morphlet::mesh_fabrication::{audit, signed_volume};
use morphlet::model::Mesh;
use morphlet::stl_export::{Level, check_bodies, repair_meshes};

fn cube(side: f64, origin: [f64; 3]) -> Mesh {
    let mut mesh = Mesh {
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
    };
    for point in &mut mesh.vertices {
        for axis in 0..3 {
            point[axis] = origin[axis] + side * point[axis];
        }
    }
    mesh
}

fn reverse(mesh: &mut Mesh) {
    for face in &mut mesh.triangles {
        face.swap(1, 2);
    }
}

fn append(mut a: Mesh, b: Mesh) -> Mesh {
    let offset = a.vertices.len() as u32;
    a.vertices.extend(b.vertices);
    a.triangles
        .extend(b.triangles.into_iter().map(|t| t.map(|i| i + offset)));
    a
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-10,
        "{actual} != {expected}"
    );
}

#[test]
fn repair_preserves_the_inner_shell_of_a_hollow_cube() {
    let mut inner = cube(1., [1.; 3]);
    reverse(&mut inner);
    let hollow = append(cube(3., [0.; 3]), inner);
    // Material occupies the outer cube minus the inner cube: 3³ - 1³ = 26.
    near(signed_volume(&hollow, &|| false).unwrap(), 26.);
    let (repaired, report) = repair_meshes(&[hollow.clone()]).unwrap();
    near(signed_volume(&repaired[0], &|| false).unwrap(), 26.);
    assert_eq!(repaired[0], hollow);
    assert_eq!(report.flipped_bodies, 0);
    assert_eq!(report.unresolved_shells, 2);
    assert!(
        report
            .lines()
            .iter()
            .any(|line| line.contains("内外は未判定"))
    );
}

#[test]
fn repair_does_not_fill_a_cavity_stored_as_separate_meshes() {
    let mut inner = cube(1., [1.; 3]);
    reverse(&mut inner);
    let input = [cube(3., [0.; 3]), inner];
    let (repaired, report) = repair_meshes(&input).unwrap();
    assert_eq!(repaired, input);
    near(signed_volume(&repaired[0], &|| false).unwrap(), 27.);
    near(signed_volume(&repaired[1], &|| false).unwrap(), -1.);
    assert_eq!(report.unresolved_shells, 2);
}

#[test]
fn repair_and_orientation_check_are_translation_invariant() {
    // The former world-origin integral evaluates to -66,666,666.666... for
    // this outward unit cube; its analytic volume is still exactly +1.
    let translated = cube(1., [-100_000_000.; 3]);
    let (repaired, report) = repair_meshes(&[translated.clone()]).unwrap();
    assert_eq!(repaired[0], translated);
    assert_eq!(report.flipped_bodies, 0);
    near(signed_volume(&repaired[0], &|| false).unwrap(), 1.);
    let (checks, _) = check_bodies(&repaired, 256.).unwrap();
    let winding = checks
        .iter()
        .find(|c| c.title == "面の向きがそろっている")
        .unwrap();
    assert_eq!(winding.level, Level::Ok);
    assert!(!winding.detail.contains("すべて外側"));
}

#[test]
fn repair_can_still_reverse_one_closed_inward_shell() {
    let mut inward = cube(1., [-100_000_000.; 3]);
    reverse(&mut inward);
    let (repaired, report) = repair_meshes(&[inward]).unwrap();
    near(signed_volume(&repaired[0], &|| false).unwrap(), 1.);
    assert_eq!(report.flipped_bodies, 1);
    assert_eq!(report.unresolved_shells, 0);
}

#[test]
fn repair_does_not_infer_global_orientation_from_an_open_surface() {
    let mut open = cube(1., [-100_000_000.; 3]);
    open.triangles.remove(0);
    let (repaired, report) = repair_meshes(&[open]).unwrap();
    assert_eq!(report.filled_holes, 1);
    assert_eq!(report.flipped_bodies, 0);
    assert_eq!(report.unresolved_shells, 1);
    assert!(audit(&repaired[0], &|| false).unwrap().closed_oriented());
    near(signed_volume(&repaired[0], &|| false).unwrap(), 1.);
}

#[test]
fn nonmanifold_edges_do_not_receive_a_watertight_success() {
    let mut duplicate = cube(1., [0.; 3]);
    duplicate.triangles.extend(duplicate.triangles.clone());
    let report = audit(&duplicate, &|| false).unwrap();
    assert_eq!(report.boundary_edges, 0);
    assert_eq!(report.non_manifold_edges, 18);
    let (checks, _) = check_bodies(&[duplicate], 256.).unwrap();
    let watertight = checks
        .iter()
        .find(|c| c.title == "閉じている（水密）")
        .unwrap();
    assert_eq!(watertight.level, Level::Error);
}
