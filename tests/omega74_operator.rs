//! OMEGA 74: fail-closed solid operations and independent geometric oracles.
use nodes::kernel_bridge::{self as kb, Bool};
use nodes::kernel_ops;
use nodes::model::Mesh;

fn go() -> bool {
    false
}

#[test]
fn boxes_boolean_obeys_independent_volume_oracle() {
    let a = kb::box_solid(10., 10., 10., &go).unwrap();
    let b = kb::translate(&a, [5., 0., 0.], &go).unwrap();
    for (op, expected) in [
        (Bool::Union, 1500.),
        (Bool::Difference, 500.),
        (Bool::Intersection, 500.),
    ] {
        let result = kb::boolean(&a, &b, op, &go).unwrap();
        assert!(!result.body().is_mesh());
        assert!(result.body().has_closed_brep_topology());
        assert!(kb::is_watertight(result.preview()));
        assert!((result.volume().unwrap() - expected).abs() < 1e-5);
        assert!(kb::mesh_volume(result.preview()) > 0.);
    }
    let separate = kb::translate(&a, [30., 0., 0.], &go).unwrap();
    assert!(kb::boolean(&a, &separate, Bool::Intersection, &go).is_err());
    assert!((a.volume().unwrap() - 1000.).abs() < 1e-5);
}

#[test]
fn watertight_rejects_missing_and_nonfinite_vertices() {
    // Oriented tetrahedron: every edge has an opposite edge.
    let mut mesh = Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
    };
    assert!(kb::is_watertight(&mesh));
    mesh.vertices[0][0] = f64::NAN;
    assert!(!kb::is_watertight(&mesh));
    mesh.vertices[0][0] = 0.;
    mesh.vertices.pop();
    assert!(!kb::is_watertight(&mesh));
}

#[test]
fn planar_trim_keeps_the_requested_volume() {
    let b = kb::box_solid(10., 10., 10., &go).unwrap();
    let cut = kernel_ops::trim_by_plane(&b, 3., true, &go).unwrap();
    assert!(cut.body().has_closed_brep_topology());
    assert!(cut.is_closed());
    assert!((cut.volume().unwrap() - 700.).abs() < 1e-5);
    assert!(kernel_ops::trim_by_plane(&b, 3., false, &|| true).is_err());
}

#[test]
fn solid_operator_cancel_is_an_error() {
    let b = kb::box_solid(10., 10., 10., &go).unwrap();
    let stop = || true;
    assert_eq!(
        kb::boolean(&b, &b, Bool::Union, &stop).unwrap_err(),
        kb::STOPPED
    );
    assert_eq!(kb::fillet(&b, 1., &[], &stop).unwrap_err(), kb::STOPPED);
    assert_eq!(kb::shell(&b, 1., None, &stop).unwrap_err(), kb::STOPPED);
}
