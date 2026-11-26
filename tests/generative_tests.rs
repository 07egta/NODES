use morphlet::{
    engine::Evaluator,
    model::{Graph, NodeKind, Value},
    persistence,
};
use std::path::PathBuf;

#[test]
fn generative_examples_load_evaluate_and_react_to_changes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/generative");
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("morph") {
            continue;
        }
        let project = persistence::load(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let mut evaluator = Evaluator::default();
        let result = evaluator.evaluate(&project.graph, &|| false);
        for (id, output) in &result.outputs {
            assert!(output.is_ok(), "{path:?} {id}: {output:?}");
        }
        let id = project
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Output)
            .unwrap()
            .id;
        let tree = result.outputs[&id].as_ref().unwrap();
        assert!(tree.item_count() > 0, "{path:?} empty output");
        assert!(
            tree.branches
                .values()
                .flatten()
                .all(|v| !matches!(v, Value::Number(_)))
        );
        count += 1;
    }
    assert_eq!(count, 10);

    let mut graph = Graph::default();
    let grid = graph.add(NodeKind::RectGrid, [0., 0.]).unwrap();
    let mut eval = Evaluator::default();
    let before = eval.evaluate(&graph, &|| false).outputs[&grid]
        .clone()
        .unwrap();
    graph.node_mut(grid).unwrap().params.insert("nx".into(), 3.);
    let after = eval.evaluate(&graph, &|| false).outputs[&grid]
        .clone()
        .unwrap();
    assert_ne!(before, after);
    assert_eq!(after.item_count(), 18);
}

#[test]
fn generative_list_nodes_keep_grafted_branch_paths() {
    let mut graph = Graph::default();
    let grid = graph.add(NodeKind::RectGrid, [0., 0.]).unwrap();
    let node = graph.node_mut(grid).unwrap();
    node.params.insert("nx".into(), 2.);
    node.params.insert("ny".into(), 2.);
    let graft = graph.add(NodeKind::Graft, [200., 0.]).unwrap();
    graph.connect(grid, graft, "geometry").unwrap();
    let origin = graph.add(NodeKind::Point, [0., 300.]).unwrap();
    let attr = graph.add(NodeKind::MultiAttractor, [400., 0.]).unwrap();
    graph.connect(graft, attr, "points").unwrap();
    graph.connect(origin, attr, "targets").unwrap();
    let result = Evaluator::default().evaluate(&graph, &|| false);
    let out = result.outputs[&attr].as_ref().unwrap();
    assert_eq!(out.branches.len(), 4);
    for k in 0..4 {
        assert_eq!(out.branches[&vec![0, k]].len(), 1);
    }
    assert_eq!(out.branches[&vec![0, 0]], vec![Value::Number(0.)]);
}

#[test]
fn new_nodes_have_registered_search_and_unique_input_keys() {
    for kind in NodeKind::all() {
        let spec = kind.spec();
        assert!(!spec.label.is_empty());
        let keys = spec
            .inputs
            .iter()
            .map(|i| i.key)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(keys.len(), spec.inputs.len(), "{kind:?}");
    }
}

#[test]
fn generative_kinds_have_stable_serialized_names_and_registry_entries() {
    let pairs = [
        (NodeKind::RectGrid, "rect_grid"),
        (NodeKind::TriangularGrid, "triangular_grid"),
        (NodeKind::HexGrid, "hex_grid"),
        (NodeKind::RadialGrid, "radial_grid"),
        (NodeKind::HexPanels, "hex_panels"),
        (NodeKind::TriPanels, "tri_panels"),
        (NodeKind::Voronoi2d, "voronoi2d"),
        (NodeKind::ConvexHull2d, "convex_hull2d"),
        (NodeKind::ClosestPoint, "closest_point"),
        (NodeKind::MultiAttractor, "multi_attractor"),
        (NodeKind::CurveAttractor, "curve_attractor"),
        (NodeKind::GridArray, "grid_array"),
        (NodeKind::PointArray, "point_array"),
        (NodeKind::Rotate3d, "rotate3d"),
        (NodeKind::Shear, "shear"),
        (NodeKind::Taper, "taper"),
        (NodeKind::FractalTree, "fractal_tree"),
        (NodeKind::CylinderWrap, "cylinder_wrap"),
        (NodeKind::Delaunay2d, "delaunay2d"),
        (NodeKind::PopulateMesh, "populate_mesh"),
        (NodeKind::CurveArray, "curve_array"),
        (NodeKind::NoiseField, "noise_field"),
        (NodeKind::LifeGrid, "life_grid"),
    ];
    for (kind, name) in pairs {
        assert!(NodeKind::all().contains(&kind));
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, format!("\"{name}\""));
        assert_eq!(serde_json::from_str::<NodeKind>(&json).unwrap(), kind);
    }
}
