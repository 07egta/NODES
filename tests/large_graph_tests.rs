//! Headless capacity/cache contracts. UI responsiveness is not measured here.
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, MAX_NODES, NodeId, NodeKind};

fn number(evaluation: &Evaluation, id: NodeId) -> f64 {
    evaluation.outputs[&id].as_ref().unwrap().branches[&vec![0]][0]
        .number()
        .unwrap()
}
fn scalar_chain(count: usize) -> Graph {
    assert!(count >= 2);
    let mut graph = Graph::default();
    graph.add(NodeKind::Number, [0., 0.]).unwrap();
    graph.add(NodeKind::Add, [200., 0.]).unwrap();
    let number = graph.nodes[0].clone();
    let mut add = graph.nodes[1].clone();
    add.params.insert("b".into(), 1.);
    // Construct once and validate once. Per-wire validation is not evaluation.
    graph.nodes = (0..count)
        .map(|index| {
            let mut node = if index == 0 {
                number.clone()
            } else {
                add.clone()
            };
            node.id = index as NodeId;
            if index > 0 {
                node.inputs.insert("a".into(), (index - 1) as NodeId);
            }
            node
        })
        .collect();
    graph.next_id = count as NodeId;
    graph.validate().unwrap();
    graph
}

#[test]
fn ten_thousand_scalar_nodes_evaluate_cache_and_recompute_only_the_edited_suffix() {
    const COUNT: usize = 10_000;
    const MIDDLE: usize = COUNT / 2;
    assert!(MAX_NODES >= COUNT);
    let mut graph = scalar_chain(COUNT);
    let last = (COUNT - 1) as NodeId;
    let mut evaluator = Evaluator::default();
    let cold = evaluator.evaluate(&graph, &|| false);
    assert!(!cold.cancelled);
    assert_eq!((cold.computed, cold.cached), (COUNT, 0));
    assert_eq!(cold.node_timings.len(), COUNT);
    assert!(cold.outputs.values().all(Result::is_ok));
    assert_eq!(number(&cold, last), COUNT as f64);
    let cold_ms = cold.elapsed_ms;
    drop(cold);
    let warm = evaluator.evaluate(&graph, &|| false);
    assert_eq!((warm.computed, warm.cached), (0, COUNT));
    assert!(warm.node_timings.values().all(|timing| timing.cached));
    assert_eq!(number(&warm, last), COUNT as f64);
    let warm_ms = warm.elapsed_ms;
    drop(warm);
    graph
        .node_mut(MIDDLE as NodeId)
        .unwrap()
        .params
        .insert("b".into(), 2.);
    let edited = evaluator.evaluate(&graph, &|| false);
    assert_eq!((edited.computed, edited.cached), (COUNT - MIDDLE, MIDDLE));
    assert_eq!(number(&edited, last), (COUNT + 1) as f64);
    for id in 0..COUNT {
        assert_eq!(edited.node_timings[&(id as NodeId)].cached, id < MIDDLE);
    }
    let edited_ms = edited.elapsed_ms;
    drop(edited);
    let bytes = serde_json::to_vec(&graph).unwrap();
    let loaded: Graph = serde_json::from_slice(&bytes).unwrap();
    loaded.validate().unwrap();
    assert_eq!(loaded, graph);
    let reopened = evaluator.evaluate(&loaded, &|| false);
    assert_eq!((reopened.computed, reopened.cached), (0, COUNT));
    assert_eq!(number(&reopened, last), (COUNT + 1) as f64);
    println!(
        "nodes={COUNT} cold_ms={cold_ms:.3} warm_ms={warm_ms:.3} middle_edit_ms={edited_ms:.3} serialized_bytes={}",
        bytes.len()
    );
}
