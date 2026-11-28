//! SolidCraft ring torus: analytical volume, invalid inputs, and graph persistence.
use nodes::engine::Evaluator;
use nodes::kernel_bridge as kb;
use nodes::model::{Graph, NodeKind, Value};
use nodes::persistence::{Project, decode_project};
use std::f64::consts::PI;

#[test]
fn ring_torus_has_analytic_faces_and_expected_volume() {
    let (major, minor) = (30., 8.);
    let b = kb::torus(major, minor, &|| false).unwrap();
    assert!(b.is_closed());
    assert!(kb::is_watertight(b.preview()));
    assert!((0..b.face_count()).any(|i| matches!(
        solidcraft_kernel::face_surface(b.body(), i),
        Some(solidcraft_kernel::FaceSurface::Torus { .. })
    )));
    let expected = 2. * PI * PI * major * minor * minor;
    let actual = b.volume().unwrap();
    assert!((actual - expected).abs() / expected < 1e-3);
}

#[test]
fn ring_torus_rejects_invalid_inputs_and_cancellation() {
    for (major, minor) in [
        (10., 10.),
        (5., 10.),
        (10., 0.),
        (10., -1.),
        (f64::NAN, 1.),
        (10., f64::INFINITY),
        (900_000., 200_000.),
    ] {
        assert!(kb::torus(major, minor, &|| false).is_err());
    }
    assert_eq!(
        kb::torus(30., 8., &|| true).unwrap_err(),
        kb::STOPPED
    );
}

#[test]
fn ring_torus_graph_roundtrip_and_volume_node() {
    let mut graph = Graph::default();
    let torus = graph.add(NodeKind::ExactTorus, [0., 0.]).unwrap();
    let volume = graph.add(NodeKind::ExactVolume, [200., 0.]).unwrap();
    graph.connect(torus, volume, "brep").unwrap();
    let project = Project::new("ring torus", graph);
    let bytes = serde_json::to_vec(&project).unwrap();
    let loaded = decode_project(&bytes).unwrap();
    assert_eq!(loaded, project);
    let evaluated = Evaluator::default().evaluate(&loaded.graph, &|| false);
    let output = evaluated.outputs[&volume].as_ref().unwrap();
    let values: Vec<_> = output.branches.values().flatten().collect();
    assert_eq!(values.len(), 1);
    let Value::Number(actual) = values[0] else {
        panic!("expected volume");
    };
    let expected = 2. * PI * PI * 30. * 8. * 8.;
    assert!((*actual - expected).abs() / expected < 1e-3);
}
