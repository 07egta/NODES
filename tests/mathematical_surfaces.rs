use nodes::{
    engine::Evaluator,
    model::{Graph, Mesh, NodeKind, Value},
    persistence::{self, Project},
};
use std::{collections::BTreeMap, path::Path};

const CASES: &[(NodeKind, &str, usize, usize, usize, i64)] = &[
    (NodeKind::Catenoid, "catenoid", 2112, 4096, 128, 0),
    (NodeKind::Helicoid, "helicoid", 2425, 4608, 240, 1),
    (NodeKind::EnneperSurface, "enneper_surface", 4225, 8192, 256, 1),
    (NodeKind::MobiusStrip, "mobius_strip", 1632, 3072, 192, 0),
    (NodeKind::KleinBottle, "klein_bottle", 4608, 9216, 0, 0),
    (NodeKind::DiniSurface, "dini_surface", 3201, 6144, 256, 1),
];

fn mesh(value: &Value) -> &Mesh {
    let Value::Mesh(mesh) = value else {
        panic!("expected mesh");
    };
    mesh
}

fn topology(m: &Mesh) -> (usize, i64, usize) {
    let mut edges = BTreeMap::<(u32, u32), (usize, i32)>::new();
    for t in &m.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            assert_ne!(a, b);
            let edge = edges.entry((a.min(b), a.max(b))).or_default();
            edge.0 += 1;
            edge.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(edges.values().all(|e| e.0 <= 2));
    let boundary = edges.values().filter(|e| e.0 == 1).count();
    let orientation_conflicts = edges.values().filter(|e| e.0 == 2 && e.1 != 0).count();
    let euler = m.vertices.len() as i64 - edges.len() as i64 + m.triangles.len() as i64;
    (boundary, euler, orientation_conflicts)
}

#[test]
fn mathematical_examples_roundtrip_and_have_expected_topology() {
    for &(kind, id, vertices, faces, boundary, euler) in CASES {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/mathematical")
            .join(format!("{id}.morph"));
        let project = persistence::load(&path).unwrap();
        let json = serde_json::to_string(&project).unwrap();
        let decoded: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, project);
        assert_eq!(decoded.graph.nodes[0].kind, kind);
        assert!(NodeKind::all().contains(&kind));
        assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{id}\""));
        let evaluation = Evaluator::default().evaluate(&decoded.graph, &|| false);
        let tree = evaluation.outputs[&1].as_ref().unwrap();
        assert_eq!(tree.item_count(), 1);
        let m = mesh(tree.branches.values().flatten().next().unwrap());
        assert_eq!((m.vertices.len(), m.triangles.len()), (vertices, faces), "{id}");
        let (actual_boundary, actual_euler, conflicts) = topology(m);
        assert_eq!((actual_boundary, actual_euler), (boundary, euler), "{id}");
        assert_eq!(
            conflicts > 0,
            matches!(kind, NodeKind::MobiusStrip | NodeKind::KleinBottle),
            "{id}"
        );
    }
}

#[test]
fn catenoid_vertices_satisfy_analytic_radius_relation() {
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Catenoid, [0., 0.]).unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let value = evaluation.outputs[&id]
        .as_ref()
        .unwrap()
        .branches
        .values()
        .flatten()
        .next()
        .unwrap();
    for &[x, y, z] in &mesh(value).vertices {
        assert!((x.hypot(y) - 20. * (z / 20.).cosh()).abs() < 1e-10);
    }
}

#[test]
fn incompatible_parameter_combinations_are_rejected() {
    for (kind, key, value) in [
        (NodeKind::MobiusStrip, "half_width", 29.),
        (NodeKind::DiniSurface, "v_min", 1.4),
        (NodeKind::Helicoid, "u_segments", 8.),
        (NodeKind::KleinBottle, "v_segments", 2.),
    ] {
        let mut graph = Graph::default();
        let id = graph.add(kind, [0., 0.]).unwrap();
        graph.node_mut(id).unwrap().params.insert(key.into(), value);
        let evaluation = Evaluator::default().evaluate(&graph, &|| false);
        assert!(evaluation.outputs.get(&id).is_none_or(Result::is_err), "{kind:?}");
    }
}
