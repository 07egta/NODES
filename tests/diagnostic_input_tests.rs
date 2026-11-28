use nodes::data_tree::{PORT_PREFIX, PortTransform};
use nodes::diagnostics::{
    InputCheck, InputIssue, InputState, InputTransform, MAX_INPUT_PREVIEW_WORK, SourceResult,
    inspect_failure_inputs,
};
use nodes::engine::{Evaluation, Evaluator};
use nodes::model::{DataType, Graph, NodeId, NodeKind, Tree, Value};
use std::collections::BTreeMap;

fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add(kind, [0.; 2]).unwrap()
}

#[test]
fn missing_required_input_and_local_values_are_distinct() {
    let mut graph = Graph::default();
    let id = add(&mut graph, NodeKind::Extrude);
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let inputs = inspect_failure_inputs(&graph, &evaluation, id).unwrap();
    assert_eq!(inputs[0].key, "curve");
    assert!(matches!(inputs[0].state, InputState::MissingRequired));
    assert_eq!(inputs[0].expected, DataType::Curve);
    assert!(matches!(inputs[1].state, InputState::LocalNumber(40.)));
}

#[test]
fn upstream_failure_has_source_id_and_does_not_hide_other_inputs() {
    let mut graph = Graph::default();
    let bad = add(&mut graph, NodeKind::Divide);
    graph.node_mut(bad).unwrap().params.insert("b".into(), 0.);
    let downstream = add(&mut graph, NodeKind::Add);
    graph.connect(bad, downstream, "a").unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let inputs = inspect_failure_inputs(&graph, &evaluation, downstream).unwrap();
    assert!(matches!(
        inputs[0].state,
        InputState::Connected {
            source,
            result: SourceResult::Failed,
        } if source == bad
    ));
    assert!(matches!(inputs[1].state, InputState::LocalNumber(_)));
    assert_eq!(inputs[0].check, InputCheck::NotChecked);
}

#[test]
fn dynamic_type_mismatch_reports_actual_source_path_and_item() {
    let mut graph = Graph::default();
    let number = add(&mut graph, NodeKind::Number);
    let relay = add(&mut graph, NodeKind::Reroute);
    let extrude = add(&mut graph, NodeKind::Extrude);
    graph.connect(number, relay, "value").unwrap();
    graph.connect(relay, extrude, "curve").unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let inputs = inspect_failure_inputs(&graph, &evaluation, extrude).unwrap();
    let InputState::Connected {
        source,
        result: SourceResult::Available(tree),
    } = inputs[0].state
    else {
        panic!("expected borrowed upstream tree");
    };
    assert_eq!(source, relay);
    assert!(std::ptr::eq(
        tree,
        evaluation.outputs[&relay].as_ref().unwrap()
    ));
    assert_eq!(
        inputs[0].check,
        InputCheck::Issue {
            path: Some(&[0]),
            index: Some(0),
            issue: InputIssue::TypeMismatch(DataType::Number),
        }
    );
    assert!(inspect_failure_inputs(&graph, &evaluation, relay).is_none());
}

#[test]
fn connected_numeric_range_and_integer_findings_are_explicit() {
    let mut graph = Graph::default();
    let zero = add(&mut graph, NodeKind::Number);
    graph.node_mut(zero).unwrap().params.insert("value".into(), 0.);
    let fractional = add(&mut graph, NodeKind::Number);
    graph
        .node_mut(fractional)
        .unwrap()
        .params
        .insert("value".into(), 8.5);
    let circle = add(&mut graph, NodeKind::Circle);
    graph.connect(zero, circle, "radius").unwrap();
    graph.connect(fractional, circle, "segments").unwrap();
    let evaluation = Evaluator::default().evaluate(&graph, &|| false);
    let inputs = inspect_failure_inputs(&graph, &evaluation, circle).unwrap();
    assert_eq!(
        inputs[0].check,
        InputCheck::Issue {
            path: Some(&[0]),
            index: Some(0),
            issue: InputIssue::OutOfRange {
                value: 0.,
                min: 0.1,
                max: 1000.,
            },
        }
    );
    assert!(matches!(
        inputs[1].check,
        InputCheck::Issue {
            issue: InputIssue::NonInteger(8.5),
            ..
        }
    ));
}

#[test]
fn bounded_preview_and_transform_do_not_claim_input_validity() {
    let mut graph = Graph::default();
    let source = add(&mut graph, NodeKind::Number);
    let circle = add(&mut graph, NodeKind::Circle);
    graph.connect(source, circle, "radius").unwrap();
    // Synthetic snapshot exercises the inspection bound independently of kernels.
    let mut values = vec![Value::Number(1.); MAX_INPUT_PREVIEW_WORK];
    values.push(Value::Number(0.));
    let evaluation = Evaluation {
        outputs: BTreeMap::from([
            (
                source,
                Ok(Tree {
                    branches: BTreeMap::from([(vec![0], values)]),
                }),
            ),
            (circle, Err("fixture".into())),
        ]),
        ..Default::default()
    };
    let inputs = inspect_failure_inputs(&graph, &evaluation, circle).unwrap();
    assert_eq!(
        inputs[0].check,
        InputCheck::NoIssueInPreview { limited: true }
    );
    graph
        .node_mut(circle)
        .unwrap()
        .params
        .insert(format!("{PORT_PREFIX}radius"), 2.);
    let inputs = inspect_failure_inputs(&graph, &evaluation, circle).unwrap();
    assert_eq!(
        inputs[0].transform,
        InputTransform::Configured(PortTransform::Graft)
    );
    assert_eq!(inputs[0].check, InputCheck::NotChecked);
}

#[test]
fn empty_source_and_unvisited_source_are_not_mislabeled_as_failures() {
    let mut graph = Graph::default();
    let source = add(&mut graph, NodeKind::Number);
    let divide = add(&mut graph, NodeKind::Divide);
    graph.connect(source, divide, "a").unwrap();
    let mut evaluation = Evaluation {
        outputs: BTreeMap::from([
            (source, Ok(Tree::default())),
            (divide, Err("fixture".into())),
        ]),
        ..Default::default()
    };
    let inputs = inspect_failure_inputs(&graph, &evaluation, divide).unwrap();
    assert!(matches!(
        inputs[0].state,
        InputState::Connected {
            result: SourceResult::Available(tree),
            ..
        } if tree.branches.is_empty()
    ));
    evaluation.outputs.remove(&source);
    let inputs = inspect_failure_inputs(&graph, &evaluation, divide).unwrap();
    assert!(matches!(
        inputs[0].state,
        InputState::Connected {
            result: SourceResult::NotEvaluated,
            ..
        }
    ));
    graph.node_mut(divide).unwrap().off = true;
    assert!(inspect_failure_inputs(&graph, &evaluation, divide).is_none());
}
