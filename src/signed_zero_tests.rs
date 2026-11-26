use super::*;
use morphlet::model::Value;

fn angle_graph() -> (Graph, NodeId, NodeId) {
    let mut graph = Graph::default();
    let input = graph.add(NodeKind::Number, [0., 0.]).unwrap();
    graph
        .node_mut(input)
        .unwrap()
        .params
        .insert("value".into(), 0.);
    let angle = graph.add(NodeKind::Atan2, [280., 0.]).unwrap();
    graph
        .node_mut(angle)
        .unwrap()
        .params
        .insert("x".into(), -1.);
    graph.connect(input, angle, "y").unwrap();
    (graph, input, angle)
}

fn scalar(evaluation: &Evaluation, node: NodeId) -> f64 {
    let values = &evaluation.outputs[&node].as_ref().unwrap().branches[&vec![0]];
    let [Value::Number(value)] = values.as_slice() else {
        panic!("expected one numeric value");
    };
    *value
}

#[test]
fn signed_zero_edit_marks_dirty_recalculates_and_roundtrips_undo() {
    let (graph, input, angle) = angle_graph();
    let mut project = Project::new("signed zero", graph);
    let mut tracker = EditTracker::new(&project);
    let mut history = History::default();
    let mut evaluator = Evaluator::default();
    assert_eq!(
        scalar(&evaluator.evaluate(&project.graph, &|| false), angle),
        std::f64::consts::PI
    );

    tracker.begin(&project, false);
    set_value(&mut project, input, -0.0);
    // The independent expected branch-cut values are +pi for (+0, -1)
    // and -pi for (-0, -1), even though ordinary float equality says 0 == -0.
    assert_eq!(
        scalar(&evaluator.evaluate(&project.graph, &|| false), angle),
        -std::f64::consts::PI
    );
    assert_eq!(
        tracker.finish(&project, false, false, &mut history),
        (true, true)
    );
    assert!(history.undo(&mut project));
    assert_eq!(
        project.graph.node(input).unwrap().params["value"].to_bits(),
        0.0_f64.to_bits()
    );
    tracker.finish(&project, false, true, &mut history);
    assert!(history.redo(&mut project));
    assert_eq!(
        project.graph.node(input).unwrap().params["value"].to_bits(),
        (-0.0_f64).to_bits()
    );
    tracker.finish(&project, false, true, &mut history);
    tracker.begin(&project, false);
    assert_eq!(
        tracker.finish(&project, false, false, &mut history),
        (false, false)
    );

    let json = serde_json::to_string(&project).unwrap();
    let loaded: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(
        loaded.graph.node(input).unwrap().params["value"].to_bits(),
        (-0.0_f64).to_bits()
    );
}

#[test]
fn signed_zero_checkpoints_remain_distinct_but_exact_duplicates_coalesce() {
    let (graph, input, _) = angle_graph();
    let positive = Project::new("history", graph);
    let mut negative = positive.clone();
    set_value(&mut negative, input, -0.0);
    let mut history = History::default();
    history.checkpoint(&positive);
    history.checkpoint_owned(negative.clone());
    history.checkpoint(&negative);
    let mut project = negative;
    set_value(&mut project, input, 2.0);
    assert!(history.undo(&mut project));
    assert_eq!(
        project.graph.node(input).unwrap().params["value"].to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(history.undo(&mut project));
    assert_eq!(
        project.graph.node(input).unwrap().params["value"].to_bits(),
        0.0_f64.to_bits()
    );
    assert!(!history.can_undo());
    assert!(history.redo(&mut project));
    assert_eq!(
        project.graph.node(input).unwrap().params["value"].to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(history.redo(&mut project));
    assert_eq!(project.graph.node(input).unwrap().params["value"], 2.0);
}

#[test]
fn signed_zero_inside_cluster_is_detected_by_editor_and_evaluator() {
    let (inner, input, angle) = angle_graph();
    let mut graph = Graph::default();
    graph.clusters.push(cluster::ClusterDef {
        id: 7,
        name: "branch cut".into(),
        graph: inner,
        inputs: vec![],
        output: angle,
    });
    let instance = graph.add(NodeKind::Cluster, [0., 0.]).unwrap();
    graph.node_mut(instance).unwrap().cluster = Some(7);
    let mut project = Project::new("cluster signed zero", graph);
    let mut tracker = EditTracker::new(&project);
    let mut history = History::default();
    let mut evaluator = Evaluator::default();
    assert_eq!(
        scalar(&evaluator.evaluate(&project.graph, &|| false), instance),
        std::f64::consts::PI
    );
    tracker.begin(&project, false);
    project.graph.clusters[0]
        .graph
        .node_mut(input)
        .unwrap()
        .params
        .insert("value".into(), -0.0);
    assert_eq!(
        scalar(&evaluator.evaluate(&project.graph, &|| false), instance),
        -std::f64::consts::PI
    );
    assert_eq!(
        tracker.finish(&project, false, false, &mut history),
        (true, true)
    );
    assert!(history.undo(&mut project));
    assert_eq!(
        project.graph.clusters[0].graph.node(input).unwrap().params["value"].to_bits(),
        0.0_f64.to_bits()
    );
}

#[test]
fn signed_zero_drag_that_returns_to_original_value_needs_no_undo_entry() {
    let (graph, input, _) = angle_graph();
    let mut project = Project::new("gesture", graph);
    let mut tracker = EditTracker::new(&project);
    let mut history = History::default();
    tracker.begin(&project, true);
    set_value(&mut project, input, -0.0);
    assert_eq!(
        tracker.finish(&project, true, false, &mut history),
        (true, true)
    );
    tracker.begin(&project, false);
    set_value(&mut project, input, 0.0);
    assert_eq!(
        tracker.finish(&project, false, false, &mut history),
        (true, true)
    );
    assert!(!history.can_undo());
}

fn set_value(project: &mut Project, id: NodeId, value: f64) {
    project
        .graph
        .node_mut(id)
        .unwrap()
        .params
        .insert("value".into(), value);
}
