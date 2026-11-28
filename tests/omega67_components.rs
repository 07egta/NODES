//! OMEGA 67: exercise the new registry nodes through the actual evaluator.
use nodes::engine::Evaluator;
use nodes::model::{Graph, NodeKind, Value};
use nodes::persistence::Project;

#[test]
fn scalar_nodes_evaluate_and_roundtrip() {
    let mut graph = Graph::default();
    let series = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    graph
        .node_mut(series)
        .unwrap()
        .params
        .insert("start".into(), 0.);
    graph
        .node_mut(series)
        .unwrap()
        .params
        .insert("step".into(), 1.);
    graph
        .node_mut(series)
        .unwrap()
        .params
        .insert("count".into(), 3.);
    let power = graph.add(NodeKind::MathPow2, [0., 0.]).unwrap();
    graph.connect(series, power, "x").unwrap();
    let project = Project::new("OMEGA 67", graph);
    let restored: Project =
        serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
    assert_eq!(project, restored);
    let result = Evaluator::default().evaluate(&restored.graph, &|| false);
    let tree = result.outputs[&power].as_ref().unwrap();
    let values: Vec<_> = tree.branches.values().flatten().cloned().collect();
    assert_eq!(
        values,
        vec![Value::Number(1.), Value::Number(2.), Value::Number(4.)]
    );
}
