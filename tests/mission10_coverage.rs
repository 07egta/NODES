//! Mission 10: regression contracts. These are NODES expectations, not GH oracle results.
use morphlet::engine::Evaluator;
use morphlet::model::{DataType, Graph, NodeId, NodeKind, Tree, Value, MAX_NODES};
use morphlet::persistence::{Project, mesh_stats};
use std::collections::BTreeSet;

fn add(g: &mut Graph, k: NodeKind) -> NodeId {
    g.add(k, [0., 0.]).unwrap()
}
fn set(g: &mut Graph, id: NodeId, key: &str, value: f64) {
    g.node_mut(id).unwrap().params.insert(key.into(), value);
}
fn tree(g: &Graph, id: NodeId) -> Tree {
    Evaluator::default().evaluate(g, &|| false).outputs[&id]
        .as_ref().unwrap().clone()
}
fn series(g: &mut Graph, count: f64) -> NodeId {
    let id = add(g, NodeKind::Series);
    set(g, id, "start", 10.);
    set(g, id, "step", 2.);
    set(g, id, "count", count);
    id
}

#[test]
fn registry_ids_and_ports_are_unique_serializable_and_instantiable() {
    let mut ids = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for &kind in NodeKind::all() {
        let id = serde_json::to_string(&kind).unwrap();
        assert!(ids.insert(id.clone()), "duplicate persisted ID: {id}");
        assert!(kinds.insert(kind), "duplicate node kind: {kind:?}");
        assert_eq!(serde_json::from_str::<NodeKind>(&id).unwrap(), kind);
        let spec = kind.spec();
        assert!(!spec.label.is_empty() && !spec.category.is_empty());
        let mut keys = BTreeSet::new();
        for p in spec.inputs {
            assert!(!p.key.is_empty() && !p.label.is_empty(), "{id}");
            assert!(keys.insert(p.key), "{id}: duplicate port {}", p.key);
            if let Some(d) = p.default {
                assert_eq!(p.kind, DataType::Number);
                assert!(d.is_finite() && p.min.is_finite() && p.max.is_finite());
                assert!(p.min <= d && d <= p.max, "{id}: invalid default");
                assert!(p.step.is_finite() && p.step > 0., "{id}: invalid step");
                if p.integer { assert_eq!(d, d.round(), "{id}: noninteger default"); }
            }
        }
        let mut g = Graph::default();
        add(&mut g, kind);
        // Required inputs may be unconnected; the registry still must serialize.
        let encoded = serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<Graph>(&encoded).unwrap(), g);
    }
    assert!(!kinds.contains(&NodeKind::Cluster), "clusters require a definition");
}

#[test]
fn graft_mixed_depth_paths_never_erase_values_with_empty_branches() {
    let mut g = Graph::default();
    let s = series(&mut g, 2.);
    let shallow = add(&mut g, NodeKind::Graft);
    g.connect(s, shallow, "geometry").unwrap();
    let deep = add(&mut g, NodeKind::Graft);
    g.connect(shallow, deep, "geometry").unwrap();
    let empty = series(&mut g, 0.);
    g.connect(deep, empty, "start").unwrap();
    let join = add(&mut g, NodeKind::ConcatList);
    g.connect(shallow, join, "a").unwrap();
    g.connect(empty, join, "b").unwrap();
    let graft = add(&mut g, NodeKind::Graft);
    g.connect(join, graft, "geometry").unwrap();
    let flat = add(&mut g, NodeKind::Flatten);
    g.connect(graft, flat, "geometry").unwrap();

    let mut evaluator = Evaluator::default();
    let result = evaluator.evaluate(&g, &|| false);
    let before = result.outputs[&join].as_ref().unwrap();
    assert_eq!(before.branches.len(), 4);
    assert_eq!(before.branches[&vec![0, 0]], vec![Value::Number(10.)]);
    assert!(before.branches[&vec![0, 0, 0]].is_empty());
    let after = result.outputs[&graft].as_ref().unwrap();
    assert_eq!(after.item_count(), before.item_count());
    assert_eq!(after.branches.len(), 2);
    assert_eq!(after.branches[&vec![0, 0, 0]], vec![Value::Number(10.)]);
    assert_eq!(after.branches[&vec![0, 1, 0]], vec![Value::Number(12.)]);
    assert_eq!(result.outputs[&flat].as_ref().unwrap().branches[&vec![0]],
               vec![Value::Number(10.), Value::Number(12.)]);
    let cached = evaluator.evaluate(&g, &|| false);
    assert_eq!(cached.outputs, result.outputs);
}

#[test]
fn slider_cache_and_serialized_graph_preserve_numeric_result() {
    let mut g = Graph::default();
    let n = add(&mut g, NodeKind::Number);
    let s = series(&mut g, 3.);
    let a = add(&mut g, NodeKind::Add);
    set(&mut g, n, "value", 3.);
    g.connect(s, a, "a").unwrap();
    g.connect(n, a, "b").unwrap();
    let mut engine = Evaluator::default();
    let first = engine.evaluate(&g, &|| false);
    assert_eq!(first.outputs[&a].as_ref().unwrap().branches[&vec![0]],
               vec![Value::Number(13.), Value::Number(15.), Value::Number(17.)]);
    let unchanged = engine.evaluate(&g, &|| false);
    assert_eq!((unchanged.computed, unchanged.cached), (0, 3));
    set(&mut g, n, "value", -2.);
    let updated = engine.evaluate(&g, &|| false);
    assert_eq!((updated.computed, updated.cached), (2, 1));
    assert_eq!(updated.outputs[&a].as_ref().unwrap().branches[&vec![0]],
               vec![Value::Number(8.), Value::Number(10.), Value::Number(12.)]);
    let project = Project::new("mission10", g);
    let bytes = serde_json::to_vec(&project).unwrap();
    let decoded: Project = serde_json::from_slice(&bytes).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded, project);
    assert_eq!(tree(&decoded.graph, a), updated.outputs[&a].as_ref().unwrap().clone());
}

#[test]
fn sampled_circle_has_expected_radius_and_plane_not_nurbs() {
    let mut g = Graph::default();
    let c = add(&mut g, NodeKind::Circle);
    set(&mut g, c, "radius", 7.);
    set(&mut g, c, "segments", 32.);
    let t = tree(&g, c);
    assert_eq!(t.item_count(), 1);
    match &t.branches[&vec![0]][0] {
        Value::Curve(curve) => {
            assert!(curve.closed);
            assert_eq!(curve.points.len(), 32);
            for &[x, y, z] in &curve.points {
                assert!((x.hypot(y) - 7.).abs() < 1e-10);
                assert_eq!(z, 0.);
            }
        }
        other => panic!("expected sampled curve, got {other:?}"),
    }
}

#[test]
fn circle_extrusion_is_closed_and_matches_polygonal_bounds() {
    let mut g = Graph::default();
    let c = add(&mut g, NodeKind::Circle);
    set(&mut g, c, "radius", 10.);
    set(&mut g, c, "segments", 32.);
    let e = add(&mut g, NodeKind::Extrude);
    set(&mut g, e, "height", 20.);
    set(&mut g, e, "layers", 1.);
    g.connect(c, e, "curve").unwrap();
    let report = mesh_stats(&[tree(&g, e)]).unwrap();
    assert!(report.is_closed());
    let [[xmin, ymin, zmin], [xmax, ymax, zmax]] = report.bounds.unwrap();
    for (got, expected) in [(xmin,-10.),(ymin,-10.),(zmin,0.),(xmax,10.),(ymax,10.),(zmax,20.)] {
        assert!((got - expected).abs() < 1e-8, "{got} != {expected}");
    }
}

#[test]
fn node_limit_rejects_extra_add_without_mutating_graph() {
    let mut g = Graph::default();
    for _ in 0..MAX_NODES { add(&mut g, NodeKind::Number); }
    let before = g.clone();
    assert!(g.add(NodeKind::Number, [0., 0.]).is_err());
    assert_eq!(g, before);
}
