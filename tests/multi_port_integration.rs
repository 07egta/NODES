//! Native port identity, evaluation, migration and editing contracts.
use morphlet::cluster::{self, ClusterDef, ClusterPort};
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, NodeId, NodeKind};
use morphlet::multi_port::PortRef;
use morphlet::persistence::{self, History, Project};

fn add(g: &mut Graph, k: NodeKind) -> NodeId {
    g.add(k, [0., 0.]).unwrap()
}
fn set(g: &mut Graph, n: NodeId, key: &str, v: f64) {
    g.node_mut(n).unwrap().params.insert(key.into(), v);
}
fn number(g: &mut Graph, v: f64) -> NodeId {
    let n = add(g, NodeKind::Number);
    set(g, n, "value", v);
    n
}
fn p(node: NodeId, port: u32) -> PortRef {
    PortRef { node, port }
}
fn nums(e: &Evaluation, source: PortRef) -> Vec<f64> {
    e.output(source)
        .expect("missing port")
        .as_ref()
        .expect("port failed")
        .branches
        .values()
        .flatten()
        .map(|v| v.number().unwrap())
        .collect()
}
fn xyz(g: &mut Graph) -> (NodeId, NodeId) {
    let pt = add(g, NodeKind::Point);
    set(g, pt, "x", 2.);
    set(g, pt, "y", 3.);
    set(g, pt, "z", 5.);
    let split = add(g, NodeKind::DeconstructPoint);
    g.connect(pt, split, "point").unwrap();
    (pt, split)
}
#[test]
fn independent_outputs_survive_visual_reorder_rename_and_cache() {
    let mut g = Graph::default();
    let (pt, split) = xyz(&mut g);
    let sum = add(&mut g, NodeKind::Add);
    g.connect_port(p(split, 1), sum, "a", false).unwrap();
    g.connect_port(p(split, 2), sum, "b", false).unwrap();
    let mut ev = Evaluator::default();
    let first = ev.evaluate(&g, &|| false);
    assert_eq!(nums(&first, p(split, 0)), vec![2.]);
    assert_eq!(nums(&first, p(split, 1)), vec![3.]);
    assert_eq!(nums(&first, p(split, 2)), vec![5.]);
    assert_eq!(nums(&first, p(sum, 0)), vec![8.]);
    assert_eq!(ev.evaluate(&g, &|| false).cached, 3);
    g.reorder_outputs(split, vec![2, 0, 1]).unwrap();
    g.rename_output(split, 1, "height".into()).unwrap();
    g.reorder_inputs(sum, vec!["b".into(), "a".into()]).unwrap();
    g.rename_input(sum, "a", "first".into()).unwrap();
    assert_eq!(g.node(sum).unwrap().sources("a"), vec![p(split, 1)]);
    assert_eq!(nums(&ev.evaluate(&g, &|| false), p(sum, 0)), vec![8.]);
    set(&mut g, pt, "y", 11.);
    assert_eq!(nums(&ev.evaluate(&g, &|| false), p(sum, 0)), vec![16.]);
    g.connect_port(p(split, 0), sum, "a", false).unwrap();
    assert_eq!(nums(&ev.evaluate(&g, &|| false), p(sum, 0)), vec![7.]);
}

#[test]
fn ordered_multiple_wires_and_deletion() {
    let mut g = Graph::default();
    let a = number(&mut g, 9.);
    let (_, s) = xyz(&mut g);
    let sink = add(&mut g, NodeKind::Number);
    g.connect(a, sink, "value").unwrap();
    g.connect_port(p(s, 2), sink, "value", true).unwrap();
    g.connect_port(p(s, 1), sink, "value", true).unwrap();
    // Repeated append is idempotent, and different ports of the same node are distinct.
    g.connect_port(p(s, 2), sink, "value", true).unwrap();
    assert_eq!(
        g.node(sink).unwrap().sources("value"),
        vec![p(a, 0), p(s, 2), p(s, 1)]
    );
    assert_eq!(
        nums(&Evaluator::default().evaluate(&g, &|| false), p(sink, 0)),
        vec![9., 5., 3.]
    );
    g.disconnect_port(p(s, 2), sink, "value").unwrap();
    assert_eq!(
        nums(&Evaluator::default().evaluate(&g, &|| false), p(sink, 0)),
        vec![9., 3.]
    );
    g.remove(s);
    assert_eq!(g.node(sink).unwrap().sources("value"), vec![p(a, 0)]);
    g.validate().unwrap();
}

#[test]
fn cycle_type_and_unknown_port_checks_are_transactional() {
    let mut g = Graph::default();
    let (pt, s) = xyz(&mut g);
    let sink = add(&mut g, NodeKind::Number);
    g.connect_port(p(s, 1), sink, "value", false).unwrap();
    let before = g.clone();
    assert!(g.connect_port(p(sink, 0), pt, "x", false).is_err());
    assert!(g.connect_port(p(pt, 0), sink, "value", true).is_err());
    assert!(g.connect_port(p(s, 999), sink, "value", false).is_err());
    assert_eq!(g, before);
    g.node_mut(sink)
        .unwrap()
        .port_links
        .insert("value".into(), vec![p(s, 999)]);
    assert!(g.validate().is_err());
}

#[test]
fn variable_ports_protect_wires_and_never_reuse_ids() {
    let mut g = Graph::default();
    let n = number(&mut g, 42.);
    let gate = add(&mut g, NodeKind::StreamGate);
    let sink = add(&mut g, NodeKind::Number);
    let id = g.add_variable_output(gate).unwrap();
    g.connect(n, gate, "stream").unwrap();
    set(&mut g, gate, "gate", f64::from(id));
    g.connect_port(p(gate, id), sink, "value", false).unwrap();
    assert!(g.remove_variable_output(gate, id).is_err());
    g.reorder_outputs(gate, vec![id, 1, 0]).unwrap();
    let ev = Evaluator::default().evaluate(&g, &|| false);
    assert_eq!(nums(&ev, p(sink, 0)), vec![42.]);
    assert!(nums(&ev, p(gate, 0)).is_empty());
    g.disconnect_port(p(gate, id), sink, "value").unwrap();
    g.remove_variable_output(gate, id).unwrap();
    assert!(g.add_variable_output(gate).unwrap() > id);
    let merge = add(&mut g, NodeKind::StreamMerge);
    let input = g.add_variable_input(merge).unwrap();
    g.connect(n, merge, &input).unwrap();
    assert!(g.remove_variable_input(merge, &input).is_err());
    g.disconnect(merge, &input);
    g.remove_variable_input(merge, &input).unwrap();
    assert_ne!(g.add_variable_input(merge).unwrap(), input);
}

#[test]
fn dispatch_and_stable_paired_sort_preserve_correspondence() {
    let mut g = Graph::default();
    let a = number(&mut g, 3.);
    let b = number(&mut g, 1.);
    let c = number(&mut g, 3.);
    let x = number(&mut g, 30.);
    let y = number(&mut g, 10.);
    let z = number(&mut g, 31.);
    let sort = add(&mut g, NodeKind::SortListPaired);
    for (key, sources) in [("keys", [a, b, c]), ("values", [x, y, z])] {
        for source in sources {
            g.connect_port(p(source, 0), sort, key, true).unwrap();
        }
    }
    let dispatch = add(&mut g, NodeKind::Dispatch);
    let one = number(&mut g, 1.);
    let zero = number(&mut g, 0.);
    g.connect_port(p(sort, 1), dispatch, "list", false).unwrap();
    g.connect_port(p(one, 0), dispatch, "pattern", false)
        .unwrap();
    g.connect_port(p(zero, 0), dispatch, "pattern", true)
        .unwrap();
    let ev = Evaluator::default().evaluate(&g, &|| false);
    assert_eq!(nums(&ev, p(sort, 0)), vec![1., 3., 3.]);
    assert_eq!(nums(&ev, p(sort, 1)), vec![10., 30., 31.]);
    assert_eq!(nums(&ev, p(dispatch, 0)), vec![10., 31.]);
    assert_eq!(nums(&ev, p(dispatch, 1)), vec![30.]);
}

#[test]
fn old_v1_migration_new_roundtrip_and_history() {
    let mut g = Graph::default();
    let a = number(&mut g, 12.);
    let b = add(&mut g, NodeKind::Number);
    g.connect(a, b, "value").unwrap();
    let mut old = serde_json::to_value(Project::new("old", g)).unwrap();
    old["schema_version"] = 1.into();
    let mut project = persistence::decode_project(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(project.schema_version, 2);
    assert_eq!(
        project.graph.node(b).unwrap().sources("value"),
        vec![p(a, 0)]
    );
    let (_, split) = xyz(&mut project.graph);
    let mut history = History::default();
    history.checkpoint(&project);
    project
        .graph
        .connect_port(p(split, 1), b, "value", true)
        .unwrap();
    let changed = project.clone();
    assert!(history.undo(&mut project));
    assert_eq!(
        project.graph.node(b).unwrap().sources("value"),
        vec![p(a, 0)]
    );
    assert!(history.redo(&mut project));
    assert_eq!(project, changed);
    let path = std::env::temp_dir().join(format!("nodes-multi-port-{}.morph", std::process::id()));
    persistence::save(&path, &project).unwrap();
    let loaded = persistence::load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(loaded, project);
    assert_eq!(
        nums(
            &Evaluator::default().evaluate(&loaded.graph, &|| false),
            p(b, 0)
        ),
        vec![12., 3.]
    );
    let mut malformed = serde_json::to_value(&loaded).unwrap();
    let nodes = malformed["graph"]["nodes"].as_array_mut().unwrap();
    let target = nodes
        .iter_mut()
        .find(|n| n["id"].as_u64() == Some(b))
        .unwrap();
    target["port_links"]["value"][1]["port"] = 999.into();
    assert!(persistence::decode_project(&serde_json::to_vec(&malformed).unwrap()).is_err());
}

#[test]
fn cluster_internal_and_external_secondary_links() {
    let mut inner = Graph::default();
    let (pt, s) = xyz(&mut inner);
    let sum = add(&mut inner, NodeKind::Add);
    inner.connect_port(p(s, 1), sum, "a", false).unwrap();
    inner.connect_port(p(s, 2), sum, "b", false).unwrap();
    let mut g = Graph::default();
    g.clusters.push(ClusterDef {
        id: 1,
        name: "multi".into(),
        graph: inner,
        inputs: vec![ClusterPort {
            node: pt,
            key: "y".into(),
            label: "Y".into(),
        }],
        output: sum,
    });
    let instance = cluster::instantiate(&mut g, 1, [0., 0.]).unwrap();
    let (_, s) = xyz(&mut g);
    let key = g.node(instance).unwrap().input_ports()[0].key.to_owned();
    g.connect_port(p(s, 2), instance, &key, false).unwrap();
    g.validate().unwrap();
    assert_eq!(
        nums(
            &Evaluator::default().evaluate(&g, &|| false),
            p(instance, 0)
        ),
        vec![10.]
    );
}

#[test]
fn graph_api_v2_connects_secondary_and_v1_rejects_new_operation() {
    use morphlet::graph_api;
    let mut request = serde_json::json!({"api_version":2,"edits":[
        {"op":"add","alias":"point","kind":"point","position":[0,0]},
        {"op":"set","node":"point","input":"y","value":17},
        {"op":"add","alias":"split","kind":"deconstruct_point","position":[0,0]},
        {"op":"connect","from":"point","to":"split","input":"point"},
        {"op":"add","alias":"sink","kind":"number","position":[0,0]},
        {"op":"connect_port","from":"split","port":1,"to":"sink","input":"value"}
    ]});
    let parsed = graph_api::parse_request(&serde_json::to_vec(&request).unwrap()).unwrap();
    let project = graph_api::build(&parsed).unwrap();
    let sink = project.graph.nodes.last().unwrap().id;
    assert_eq!(
        nums(
            &graph_api::evaluate(&project.graph, &|| false).unwrap(),
            p(sink, 0)
        ),
        vec![17.]
    );
    request["api_version"] = 1.into();
    let parsed = graph_api::parse_request(&serde_json::to_vec(&request).unwrap()).unwrap();
    assert!(graph_api::build(&parsed).is_err());
}
