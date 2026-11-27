use morphlet::{diagnostics::diagnose, engine::Evaluator, model::{Graph, NodeKind}};

fn main() {
    let mut graph = Graph::default();
    let root = graph.add(NodeKind::Divide, [0.; 2]).unwrap();
    graph.node_mut(root).unwrap().params.insert("b".into(), 0.);
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let report = diagnose(&graph, &evaluation);
    let encoded = serde_json::to_value(&report).unwrap();
    let expects_field = std::env::args().any(|argument| argument == "--expect-truncation-field");
    let field = encoded["root_failures"][0].get("message_truncated");
    assert_eq!(field.is_some(), expects_field);
    if expects_field {
        assert_eq!(field, Some(&serde_json::Value::Bool(false)));
    }
    assert_eq!(encoded["summary"]["root_failure_count"], 1);
    println!("{}", serde_json::to_string_pretty(&encoded).unwrap());
}
