use nodes::engine::Evaluator;
use nodes::model::{Graph, NodeKind, Value};

#[test]
fn cage_edit_text_invalidates_cached_geometry() {
    let mut graph = Graph::default();
    let cage = graph.add(NodeKind::SubdBoxCage, [0., 0.]).unwrap();
    let edit = graph.add(NodeKind::SubdCageEdit, [0., 0.]).unwrap();
    graph.connect(cage, edit, "cage").unwrap();
    let mut evaluator = Evaluator::default();
    let before = evaluator.evaluate(&graph, &|| false);
    let original = before.outputs[&edit].as_ref().unwrap();
    graph.node_mut(edit).unwrap().script = nodes::subd::edits_text(&[nodes::subd::EditOp::Move {
        verts: vec![0],
        d: [3., 0., 0.],
        sym: None,
    }]);
    let after = evaluator.evaluate(&graph, &|| false);
    let cold = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(after.outputs, cold.outputs);
    let updated = after.outputs[&edit].as_ref().unwrap();
    let (Some(Value::Cage(a)), Some(Value::Cage(b))) = (
        original.branches.values().flatten().next(),
        updated.branches.values().flatten().next(),
    ) else {
        panic!("expected cages");
    };
    assert_eq!(b.vertices()[0][0], a.vertices()[0][0] + 3.);
    assert!(after.node_timings[&cage].cached);
    assert!(!after.node_timings[&edit].cached);
}

#[test]
fn gcode_start_and_end_text_edits_propagate() {
    let mut graph = Graph::default();
    let printer = graph.add(NodeKind::GcodePrinter, [0., 0.]).unwrap();
    let start = graph.add(NodeKind::GcodeStart, [0., 0.]).unwrap();
    let end = graph.add(NodeKind::GcodeEnd, [0., 0.]).unwrap();
    graph.connect(printer, start, "job").unwrap();
    graph.connect(start, end, "job").unwrap();
    for id in [start, end] {
        graph.node_mut(id).unwrap().params.insert("mode".into(), 2.);
    }
    let mut evaluator = Evaluator::default();
    let before = evaluator.evaluate(&graph, &|| false);
    assert!(before.outputs[&end].is_ok());
    for (id, text) in [(start, "; changed header"), (end, "; changed footer")] {
        graph.node_mut(id).unwrap().script = text.into();
        let warm = evaluator.evaluate(&graph, &|| false);
        let cold = Evaluator::default().evaluate(&graph, &|| false);
        assert_eq!(warm.outputs, cold.outputs);
        let tree = warm.outputs[&end].as_ref().unwrap();
        let Some(Value::Gcode(job)) = tree.branches.values().flatten().next() else {
            panic!("expected G-code job");
        };
        assert_eq!(job.start.text, "; changed header");
        if id == end {
            assert_eq!(job.end.text, "; changed footer");
        }
        assert!(warm.node_timings[&printer].cached);
        assert!(!warm.node_timings[&end].cached);
    }
}

#[test]
fn sketch_text_edits_invalidate_geometry_and_recover_from_errors() {
    let mut graph = Graph::default();
    let sketch = graph.add(NodeKind::ExactSketch, [0., 0.]).unwrap();
    let node = graph.node_mut(sketch).unwrap();
    node.params.insert("mode".into(), 1.);
    node.script = "line l 0 0 10 0".into();

    let mut evaluator = Evaluator::default();
    let before = evaluator.evaluate(&graph, &|| false);
    assert!(before.outputs[&sketch].is_ok());
    assert_eq!(evaluator.evaluate(&graph, &|| false).cached, 1);

    graph.node_mut(sketch).unwrap().script = "line l 0 0 20 0".into();
    let after = evaluator.evaluate(&graph, &|| false);
    let cold = Evaluator::default().evaluate(&graph, &|| false);
    assert!(after.outputs[&sketch].is_ok());
    assert_ne!(before.outputs[&sketch], after.outputs[&sketch]);
    assert_eq!(after.outputs, cold.outputs);
    assert_eq!((after.computed, after.cached), (1, 0));

    graph.node_mut(sketch).unwrap().script = "invalid command".into();
    let invalid = evaluator.evaluate(&graph, &|| false);
    assert!(invalid.outputs[&sketch].is_err());
    graph.node_mut(sketch).unwrap().script = "line l 0 0 20 0".into();
    let recovered = evaluator.evaluate(&graph, &|| false);
    assert_eq!(recovered.outputs, cold.outputs);
    assert!(recovered.outputs[&sketch].is_ok());
}
