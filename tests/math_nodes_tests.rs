//! Graph-level tests for Mission 04; compiled by cargo test --all-targets.
use NodeKind::*;
use nodes::{
    engine::Evaluator,
    math_nodes,
    model::{Graph, NodeId, NodeKind, Value},
    persistence::Project,
};

fn node(g: &mut Graph, kind: NodeKind, settings: &[(&str, f64)]) -> NodeId {
    let id = g.add(kind, [0.0, 0.0]).unwrap();
    for &(key, value) in settings {
        g.node_mut(id).unwrap().params.insert(key.into(), value);
    }
    id
}
fn link(g: &mut Graph, from: NodeId, to: NodeId, key: &str) {
    g.connect(from, to, key).unwrap();
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn registered_specs_have_stable_unique_serde_names() {
    let mut names = std::collections::BTreeSet::new();
    let mut count = 0;
    for &kind in NodeKind::all() {
        let name = serde_json::to_string(&kind).unwrap();
        assert!(names.insert(name.clone()), "duplicate identity {name}");
        assert_eq!(serde_json::from_str::<NodeKind>(&name).unwrap(), kind);
        if let Some(spec) = math_nodes::spec(kind) {
            count += 1;
            assert!(!spec.label.is_empty());
            assert_eq!(kind.spec().label, spec.label);
        }
    }
    assert_eq!(count, 90);
    assert_eq!(serde_json::to_string(&Range).unwrap(), "\"range\"");
    assert_eq!(serde_json::to_string(&Sine).unwrap(), "\"sine\"");
}

#[test]
fn gh_range_uses_step_count_and_preserves_old_range() {
    let mut g = Graph::default();
    let a = node(
        &mut g,
        GhRange,
        &[("start", 0.0), ("end", 1.0), ("steps", 4.0)],
    );
    let b = node(
        &mut g,
        Range,
        &[("start", 0.0), ("end", 1.0), ("count", 4.0)],
    );
    let ev = Evaluator::default().evaluate(&g, &|| false);
    let a = &ev.outputs[&a].as_ref().unwrap().branches[&vec![0]];
    assert_eq!(a, &[0.0, 0.25, 0.5, 0.75, 1.0].map(Value::Number));
    assert_eq!(ev.outputs[&b].as_ref().unwrap().item_count(), 4);
}

#[test]
fn branch_paths_and_broadcast_are_preserved() {
    let mut g = Graph::default();
    let a = node(
        &mut g,
        GhRange,
        &[("start", 0.0), ("end", 1.0), ("steps", 4.0)],
    );
    let graft = node(&mut g, Graft, &[]);
    link(&mut g, a, graft, "geometry");
    let sum = node(&mut g, Summation, &[]);
    link(&mut g, graft, sum, "values");
    let add = node(&mut g, Add, &[("b", 10.0)]);
    link(&mut g, sum, add, "a");
    g.validate().unwrap();
    let ev = Evaluator::default().evaluate(&g, &|| false);
    let tree = ev.outputs[&add].as_ref().unwrap();
    assert_eq!(tree.branches.len(), 5);
    for j in 0..5 {
        let values = &tree.branches[&vec![0, j]];
        assert_eq!(values.len(), 1);
        close(values[0].number().unwrap(), 10.0 + j as f64 * 0.25);
    }
}

#[test]
fn numeric_domain_and_vector_are_composable() {
    let mut g = Graph::default();
    let interval = node(&mut g, ConstructDomain, &[("start", 2.0), ("end", 10.0)]);
    let param = node(&mut g, GhRange, &[("steps", 2.0)]);
    let eval = node(&mut g, EvaluateDomain, &[]);
    link(&mut g, interval, eval, "domain");
    link(&mut g, param, eval, "t");
    let x = node(&mut g, UnitX, &[]);
    let y = node(&mut g, UnitY, &[]);
    let cross = node(&mut g, CrossProduct, &[]);
    link(&mut g, x, cross, "a");
    link(&mut g, y, cross, "b");
    let ev = Evaluator::default().evaluate(&g, &|| false);
    let v = &ev.outputs[&eval].as_ref().unwrap().branches[&vec![0]];
    assert_eq!(v, &[2.0, 6.0, 10.0].map(Value::Number));
    assert_eq!(
        ev.outputs[&cross].as_ref().unwrap().branches[&vec![0]],
        vec![Value::Point([0.0, 0.0, 1.0])]
    );
}

#[test]
fn mapper_recomputes_downstream_and_round_trips_project() {
    let mut g = Graph::default();
    let m = node(&mut g, GraphMapper, &[("x", 0.5), ("mode", 0.0)]);
    let p = node(&mut g, Point, &[]);
    link(&mut g, m, p, "x");
    g.validate().unwrap();
    let mut engine = Evaluator::default();
    let first = engine.evaluate(&g, &|| false);
    close(
        first.outputs[&m].as_ref().unwrap().branches[&vec![0]][0]
            .number()
            .unwrap(),
        0.5,
    );
    g.node_mut(m).unwrap().params.insert("mode".into(), 2.0);
    let second = engine.evaluate(&g, &|| false);
    close(
        second.outputs[&m].as_ref().unwrap().branches[&vec![0]][0]
            .number()
            .unwrap(),
        1.0,
    );
    assert_ne!(first.outputs[&p], second.outputs[&p]);
    let project = Project::new("数学", g);
    let bytes = serde_json::to_vec(&project).unwrap();
    let loaded: Project = serde_json::from_slice(&bytes).unwrap();
    loaded.validate().unwrap();
    assert_eq!(project, loaded);
    let third = Evaluator::default().evaluate(&loaded.graph, &|| false);
    assert_eq!(second.outputs[&p], third.outputs[&p]);
}

#[test]
fn graph_error_propagation_and_seed_repeatability() {
    let mut g = Graph::default();
    let a = node(&mut g, Root, &[("x", -1.0), ("degree", 2.0)]);
    let b = node(&mut g, Negate, &[]);
    link(&mut g, a, b, "x");
    let r = node(&mut g, RandomNormal, &[("count", 16.0), ("seed", 42.0)]);
    let s = node(&mut g, RandomNormal, &[("count", 16.0), ("seed", 42.0)]);
    let ev = Evaluator::default().evaluate(&g, &|| false);
    assert!(ev.outputs[&a].is_err());
    assert!(ev.outputs[&b].is_err());
    assert_eq!(ev.outputs[&r], ev.outputs[&s]);
    g.node_mut(s).unwrap().params.insert("seed".into(), 43.0);
    let ev = Evaluator::default().evaluate(&g, &|| false);
    assert_ne!(ev.outputs[&r], ev.outputs[&s]);
}
