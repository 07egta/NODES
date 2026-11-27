//! Independent adapter contracts at the graph boundary, including empty lists.
//! These regressions require the root engine/API fixes; they do not claim GH parity.
use nodes::{
    engine::Evaluator,
    graph_api::{self, API_VERSION, Edit, Request},
    model::{Graph, NodeId, NodeKind as K, Tree, Value},
};

fn connect(graph: &mut Graph, source: NodeId, target: NodeId, input: usize) {
    let key = graph.node(target).unwrap().kind.spec().inputs[input].key;
    graph.connect(source, target, key).unwrap();
}

fn numeric_lists() -> (Graph, NodeId, NodeId) {
    let mut graph = Graph::default();
    let source = graph.add(K::Series, [0.; 2]).unwrap();
    graph.node_mut(source).unwrap().params.extend([
        ("start".into(), 10.),
        ("step".into(), 10.),
        ("count".into(), 2.),
    ]);
    let empty = graph.add(K::SubList, [200., 0.]).unwrap();
    connect(&mut graph, source, empty, 0);
    graph
        .node_mut(empty)
        .unwrap()
        .params
        .insert("count".into(), 0.);
    (graph, source, empty)
}

fn output(graph: &Graph, id: NodeId) -> Result<Tree, String> {
    graph.validate().unwrap();
    let mut evaluation = Evaluator::default().evaluate(graph, &|| false);
    assert!(!evaluation.cancelled);
    evaluation.outputs.remove(&id).expect("node was evaluated")
}

#[test]
fn no_cull_replace_or_insert_indices_preserve_the_original_list() {
    for kind in [K::CullIndex, K::ReplaceItems, K::InsertItems] {
        let (mut graph, source, empty) = numeric_lists();
        let target = graph.add(kind, [400., 0.]).unwrap();
        connect(&mut graph, source, target, 0);
        connect(&mut graph, empty, target, 1);
        if kind != K::CullIndex {
            connect(&mut graph, source, target, 2);
        }
        let result = output(&graph, target).unwrap();
        assert_eq!(
            result.branches,
            [(vec![0], vec![Value::Number(10.), Value::Number(20.)])].into(),
            "an empty index list must perform zero edits: {kind:?}"
        );
    }
}

#[test]
fn no_curve_shatter_parameters_preserve_one_complete_exact_curve() {
    let (mut graph, _, empty) = numeric_lists();
    let circle = graph.add(K::Circle, [0., 200.]).unwrap();
    graph
        .node_mut(circle)
        .unwrap()
        .params
        .insert("radius".into(), 2.);
    let shatter = graph.add(K::CurveShatter, [400., 200.]).unwrap();
    connect(&mut graph, circle, shatter, 0);
    connect(&mut graph, empty, shatter, 1);
    let result = output(&graph, shatter).unwrap();
    let branch = result.branches.get(&vec![0]).unwrap();
    assert_eq!(
        branch.len(),
        1,
        "zero cuts produce one curve, not zero curves"
    );
    let Value::Curve(curve) = &branch[0] else {
        panic!("expected a curve")
    };
    assert!(curve.geometry.is_some());
    let length = graph.add(K::CurveLength, [600., 200.]).unwrap();
    connect(&mut graph, shatter, length, 0);
    let length = output(&graph, length).unwrap().branches[&vec![0]][0]
        .number()
        .unwrap();
    assert!((length - 2. * std::f64::consts::TAU).abs() < 1e-8);
}

#[test]
fn an_empty_first_item_setting_still_propagates_an_empty_branch() {
    let (mut graph, source, empty) = numeric_lists();
    let shifted = graph.add(K::ShiftList, [400., 0.]).unwrap();
    connect(&mut graph, source, shifted, 0);
    connect(&mut graph, empty, shifted, 1);
    assert_eq!(
        output(&graph, shifted).unwrap().branches,
        [(vec![0], vec![])].into()
    );
}

#[test]
fn single_item_shape_access_rejects_empty_and_multiple_shapes() {
    let (mut graph, _, empty) = numeric_lists();
    let point = graph.add(K::Point, [0., 200.]).unwrap();
    let circle = graph.add(K::Circle, [0., 400.]).unwrap();
    let two = graph.add(K::LinearArray, [200., 400.]).unwrap();
    graph
        .node_mut(two)
        .unwrap()
        .params
        .insert("count".into(), 2.);
    connect(&mut graph, circle, two, 0);
    for shapes in [empty, two] {
        let array = graph.add(K::PointArray, [400., 200.]).unwrap();
        connect(&mut graph, point, array, 0);
        connect(&mut graph, shapes, array, 1);
        assert!(
            output(&graph, array).is_err(),
            "SingleItem requires exactly one shape"
        );
    }
}

#[test]
fn api_rejects_duplicate_connections_through_legacy_aliases_atomically() {
    for (alias, canonical) in [("points", "candidates"), ("targets", "queries")] {
        let mut graph = Graph::default();
        let first = graph.add(K::Point, [0.; 2]).unwrap();
        let second = graph.add(K::Point, [0., 200.]).unwrap();
        let closest = graph.add(K::ClosestPoint, [200., 0.]).unwrap();
        let before = graph.clone();
        let request = Request {
            api_version: API_VERSION,
            edits: vec![
                Edit::Connect {
                    from: first.to_string(),
                    to: closest.to_string(),
                    input: alias.into(),
                },
                Edit::Connect {
                    from: second.to_string(),
                    to: closest.to_string(),
                    input: canonical.into(),
                },
            ],
        };
        assert!(
            graph_api::apply(&mut graph, &request).is_err(),
            "both spellings name one input"
        );
        assert_eq!(graph, before, "rejection must preserve the caller's graph");
    }
}
