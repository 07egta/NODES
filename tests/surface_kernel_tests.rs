use morphlet::{
    engine::Evaluator,
    model::{Graph, NodeId, NodeKind as K, Value},
    persistence,
};
use std::path::PathBuf;

fn node(g: &mut Graph, k: K, p: &[(&str, f64)]) -> NodeId {
    let id = g.add(k, [0., 0.]).unwrap();
    for &(key, value) in p {
        g.node_mut(id).unwrap().params.insert(key.into(), value);
    }
    id
}
fn link(g: &mut Graph, from: NodeId, to: NodeId, key: &str) {
    g.connect(from, to, key).unwrap();
}
fn first(e: &morphlet::engine::Evaluation, id: NodeId) -> &Value {
    e.outputs[&id]
        .as_ref()
        .unwrap_or_else(|x| panic!("node {id}: {x}"))
        .branches
        .values()
        .flatten()
        .next()
        .unwrap()
}
fn near(x: f64, y: f64) {
    assert!((x - y).abs() < 1e-8 * (1. + y.abs()), "{x} != {y}");
}
fn xyz(x: [f64; 3], y: [f64; 3]) {
    for i in 0..3 {
        near(x[i], y[i]);
    }
}
fn number(e: &morphlet::engine::Evaluation, id: NodeId) -> f64 {
    first(e, id).number().unwrap()
}
#[test]
fn precise_geometry_graph_recomputes_and_keeps_typed_values() {
    let mut g = Graph::default();
    let w = node(&mut g, K::Number, &[("value", 4.)]);
    let p = node(&mut g, K::PlaneSurface, &[("depth", 6.)]);
    let a = node(&mut g, K::SurfaceArea, &[]);
    let q = node(&mut g, K::EvaluateSurface, &[("u", 1.), ("v", 1.)]);
    let out = node(&mut g, K::Output, &[]);
    let mesh = node(&mut g, K::SurfaceMesh, &[]);
    link(&mut g, w, p, "width");
    link(&mut g, p, a, "surface");
    link(&mut g, p, q, "surface");
    link(&mut g, p, out, "geometry");
    link(&mut g, p, mesh, "surface");
    let legacy = node(&mut g, K::Twist, &[]);
    assert!(g.connect(p, legacy, "geometry").is_err());
    g.remove(legacy);
    let mut eval = Evaluator::default();
    let e = eval.evaluate(&g, &|| false);
    near(number(&e, a), 24.);
    let Value::Point(point) = first(&e, q) else {
        panic!("expected point")
    };
    xyz(*point, [2., 3., 0.]);
    assert!(matches!(first(&e, out), Value::Surface(_)));
    assert!(matches!(first(&e, mesh), Value::Mesh(_)));
    g.node_mut(w).unwrap().params.insert("value".into(), 8.);
    let e = eval.evaluate(&g, &|| false);
    near(number(&e, a), 48.);
    let Value::Point(point) = first(&e, q) else {
        panic!("expected point")
    };
    xyz(*point, [4., 3., 0.]);
    let cached = eval.evaluate(&g, &|| false);
    assert_eq!(cached.computed, 0);
    assert_eq!(cached.cached, g.nodes.len());
}
#[test]
fn box_graph_extracts_topology_and_exports_only_explicit_mesh() {
    let mut g = Graph::default();
    let a = node(
        &mut g,
        K::BrepBox,
        &[("width", 4.), ("depth", 6.), ("height", 8.)],
    );
    let b = node(&mut g, K::BrepMove, &[("x", 2.)]);
    let intersection = node(&mut g, K::BrepIntersection, &[]);
    let volume = node(&mut g, K::BrepVolume, &[]);
    let faces = node(&mut g, K::BrepFaces, &[]);
    let edges = node(&mut g, K::BrepEdges, &[]);
    let verts = node(&mut g, K::BrepVertices, &[]);
    let mesh = node(&mut g, K::BrepMesh, &[]);
    link(&mut g, a, b, "solid");
    link(&mut g, a, intersection, "solid");
    link(&mut g, b, intersection, "other");
    for id in [volume, faces, edges, verts, mesh] {
        link(&mut g, intersection, id, "solid");
    }
    let e = Evaluator::default().evaluate(&g, &|| false);
    near(number(&e, volume), 96.);
    for (id, count) in [(faces, 6), (edges, 12), (verts, 8)] {
        assert_eq!(e.outputs[&id].as_ref().unwrap().item_count(), count);
    }
    assert!(matches!(first(&e, intersection), Value::Solid(_)));
    let report = persistence::mesh_stats(&[e.outputs[&mesh].as_ref().unwrap().clone()]).unwrap();
    assert_eq!(report.open_edges, 0);
    assert_eq!(report.inconsistent_edges, 0);
    assert_eq!(report.non_manifold_edges, 0);
    near(report.signed_volume_mm3, 96.);
    assert!(
        persistence::mesh_stats(&[e.outputs[&intersection].as_ref().unwrap().clone()]).is_err()
    );
}
#[test]
fn saved_examples_roundtrip_and_evaluate_after_edits() {
    for (name, key, value) in [
        ("surface-vase.morph", "radius", 32.),
        ("surface-panel.morph", "rise", 45.),
        ("surface-box-intersection.morph", "width", 45.),
        ("surface-ruled.morph", "height", 36.),
    ] {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut project = persistence::load(&root.join("examples").join(name)).unwrap();
        let mut evaluator = Evaluator::default();
        let before = evaluator.evaluate(&project.graph, &|| false);
        assert!(
            before.outputs.values().all(Result::is_ok),
            "{name}: {:?}",
            before.outputs
        );
        let id = project.graph.nodes[0].id;
        project
            .graph
            .node_mut(id)
            .unwrap()
            .params
            .insert(key.into(), value);
        let after = evaluator.evaluate(&project.graph, &|| false);
        assert!(
            after.outputs.values().all(Result::is_ok),
            "{name}: {:?}",
            after.outputs
        );
        assert_ne!(first(&before, id), first(&after, id));
        let path =
            std::env::temp_dir().join(format!("nodes-surface-{}-{name}", std::process::id()));
        persistence::save(&path, &project).unwrap();
        let loaded = persistence::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded.graph, project.graph);
        let e = Evaluator::default().evaluate(&loaded.graph, &|| false);
        assert_eq!(after.outputs, e.outputs);
    }
}
#[test]
fn all_precise_nodes_have_stable_specs_and_evaluate_on_examples() {
    use K::*;
    let mut g = Graph::default();
    let plane = node(&mut g, PlaneSurface, &[]);
    let solid = node(&mut g, BrepBox, &[]);
    let shifted = node(&mut g, BrepMove, &[]);
    link(&mut g, solid, shifted, "solid");
    let points = [
        node(&mut g, Point, &[("x", 0.), ("y", 0.), ("z", 0.)]),
        node(&mut g, Point, &[("x", 0.), ("y", 10.), ("z", 0.)]),
        node(&mut g, Point, &[("x", 10.), ("y", 0.), ("z", 0.)]),
        node(&mut g, Point, &[("x", 10.), ("y", 10.), ("z", 5.)]),
    ];
    let curve = node(&mut g, Rectangle, &[]);
    let raised = node(&mut g, Move, &[("z", 20.)]);
    link(&mut g, curve, raised, "geometry");
    let mut ids = Vec::new();
    for &kind in morphlet::surface_nodes::KINDS {
        let id = node(&mut g, kind, &[]);
        assert!(K::all().contains(&kind));
        let encoded = serde_json::to_string(&kind).unwrap();
        assert_eq!(serde_json::from_str::<K>(&encoded).unwrap(), kind);
        for s in kind.spec().inputs.iter().filter(|s| s.default.is_none()) {
            let from = match s.key {
                "surface" => plane,
                "solid" => solid,
                "other" => shifted,
                "point" | "p00" => points[0],
                "p01" => points[1],
                "p10" => points[2],
                "p11" => points[3],
                "a" if kind == BrepBoxTwoPoints => points[0],
                "b" if kind == BrepBoxTwoPoints => points[3],
                "a" | "curve" => curve,
                "b" => raised,
                _ => panic!("unexpected input {}", s.key),
            };
            link(&mut g, from, id, s.key);
        }
        ids.push((id, kind));
    }
    let e = Evaluator::default().evaluate(&g, &|| false);
    for (id, kind) in ids {
        assert!(
            e.outputs[&id].as_ref().is_ok_and(|t| t.item_count() > 0),
            "{kind:?}: {:?}",
            e.outputs[&id]
        );
    }
}
#[test]
fn tree_matching_and_safe_unsupported_operations() {
    let mut g = Graph::default();
    let range = node(
        &mut g,
        K::Range,
        &[("start", 10.), ("end", 20.), ("count", 3.)],
    );
    let graft = node(&mut g, K::Graft, &[]);
    link(&mut g, range, graft, "geometry");
    let plane = node(&mut g, K::PlaneSurface, &[("depth", 2.)]);
    link(&mut g, graft, plane, "width");
    let area = node(&mut g, K::SurfaceArea, &[]);
    link(&mut g, plane, area, "surface");
    let e = Evaluator::default().evaluate(&g, &|| false);
    let t = e.outputs[&area].as_ref().unwrap();
    assert_eq!(t.branches.len(), 3);
    let values: Vec<_> = t
        .branches
        .values()
        .flatten()
        .map(|v| v.number().unwrap())
        .collect();
    assert_eq!(values, vec![20., 30., 40.]);
    let nurbs = node(&mut g, K::NurbsPanel, &[]);
    let p = node(&mut g, K::Point, &[]);
    let closest = node(&mut g, K::SurfaceClosestPoint, &[]);
    link(&mut g, nurbs, closest, "surface");
    link(&mut g, p, closest, "point");
    assert!(Evaluator::default().evaluate(&g, &|| false).outputs[&closest].is_err());
}
