use morphlet::engine::{Evaluation, Evaluator};
use morphlet::graph_api;
use morphlet::model::{Graph, NodeId, NodeKind, NodeKind::*, Value};
use morphlet::persistence::{self, Project};
use std::f64::consts::PI;

fn node(graph: &mut Graph, kind: NodeKind, params: &[(&str, f64)]) -> NodeId {
    let id = graph.add(kind, [0.0, 0.0]).unwrap();
    graph.node_mut(id).unwrap().params.extend(
        params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), *value)),
    );
    id
}

fn point(graph: &mut Graph, [x, y, z]: [f64; 3]) -> NodeId {
    node(graph, Point, &[("x", x), ("y", y), ("z", z)])
}

fn connect_frame(graph: &mut Graph, destination: NodeId, frame: [NodeId; 3]) {
    for (id, port) in frame.into_iter().zip(["origin", "axis_x", "axis_y"]) {
        graph.connect(id, destination, port).unwrap();
    }
}

fn root(result: &Evaluation, id: NodeId) -> &[Value] {
    &result.outputs[&id].as_ref().unwrap().branches[&[0][..]]
}

fn close(actual: [f64; 3], expected: [f64; 3]) {
    for (a, b) in actual.into_iter().zip(expected) {
        assert!((a - b).abs() < 1e-12, "{actual:?} != {expected:?}");
    }
}

#[test]
fn cylindrical_and_polar_coordinates_follow_right_handed_orthonormal_frame() {
    let mut graph = Graph::default();
    let frame = [
        point(&mut graph, [10.0, -3.0, 2.0]),
        point(&mut graph, [0.0, 2.0, 0.0]),
        point(&mut graph, [0.0, 1.0, 4.0]),
    ];
    let polar = node(
        &mut graph,
        PointPolar,
        &[
            ("azimuth", -PI / 4.0),
            ("elevation_angle", PI / 6.0),
            ("radius", 4.0),
        ],
    );
    let cylindrical = node(
        &mut graph,
        PointCylindrical,
        &[("azimuth", PI / 2.0), ("radius", 2.0), ("elevation", -3.0)],
    );
    let to_polar = node(&mut graph, PointToPolar, &[]);
    for id in [polar, cylindrical, to_polar] {
        connect_frame(&mut graph, id, frame);
    }
    graph.connect(polar, to_polar, "point").unwrap();

    let pole = point(&mut graph, [13.0, -3.0, 2.0]);
    let pole_to_polar = node(&mut graph, PointToPolar, &[]);
    connect_frame(&mut graph, pole_to_polar, frame);
    graph.connect(pole, pole_to_polar, "point").unwrap();

    let origin_to_polar = node(&mut graph, PointToPolar, &[]);
    connect_frame(&mut graph, origin_to_polar, frame);
    graph.connect(frame[0], origin_to_polar, "point").unwrap();

    let result = Evaluator::default().evaluate(&graph, &|| false);
    let [Value::Point(p)] = root(&result, polar) else {
        panic!("expected one polar point");
    };
    close(*p, [12.0, -3.0 + 6.0_f64.sqrt(), 2.0 - 6.0_f64.sqrt()]);
    let [Value::Point(p)] = root(&result, cylindrical) else {
        panic!("expected one cylindrical point");
    };
    close(*p, [7.0, -3.0, 4.0]);
    let [
        Value::Number(azimuth),
        Value::Number(elevation),
        Value::Number(radius),
    ] = root(&result, to_polar)
    else {
        panic!("expected azimuth, elevation and radius");
    };
    close(
        [*azimuth, *elevation, *radius],
        [7.0 * PI / 4.0, PI / 6.0, 4.0],
    );
    assert_eq!(
        root(&result, pole_to_polar),
        &[0.0, PI / 2.0, 3.0].map(Value::Number)
    );
    assert_eq!(root(&result, origin_to_polar), &[0.0; 3].map(Value::Number));

    graph.connect(frame[1], polar, "axis_y").unwrap();
    let invalid = Evaluator::default().evaluate(&graph, &|| false);
    assert!(
        invalid.outputs[&polar]
            .as_ref()
            .unwrap_err()
            .contains("平行")
    );
}

#[test]
fn barycentric_weights_are_homogeneous_and_zero_sum_is_rejected() {
    let mut graph = Graph::default();
    let ids = [
        point(&mut graph, [0.0, 0.0, 1.0]),
        point(&mut graph, [8.0, 0.0, 1.0]),
        point(&mut graph, [0.0, 4.0, 2.0]),
    ];
    let bary = node(
        &mut graph,
        PointBarycentric,
        &[("u", 1e-310), ("v", 1e-310), ("w", 2e-310)],
    );
    for (id, port) in ids.into_iter().zip(["a", "b", "c"]) {
        graph.connect(id, bary, port).unwrap();
    }
    let mut evaluator = Evaluator::default();
    let result = evaluator.evaluate(&graph, &|| false);
    assert_eq!(root(&result, bary), &[Value::Point([2.0, 2.0, 1.5])]);
    graph
        .node_mut(bary)
        .unwrap()
        .params
        .insert("w".into(), -2e-310);
    let invalid = evaluator.evaluate(&graph, &|| false);
    assert!(
        invalid.outputs[&bary]
            .as_ref()
            .unwrap_err()
            .contains("重みの和")
    );
}

#[test]
fn point_list_nodes_compose_and_preserve_native_save_reload() {
    let mut graph = Graph::default();
    let x = node(
        &mut graph,
        Series,
        &[("start", 2.25), ("step", -0.75), ("count", 4.0)],
    );
    let points = point(&mut graph, [0.0, 30.0, 10.0]);
    graph.connect(x, points, "x").unwrap();
    let sorted = node(&mut graph, SortPoints, &[]);
    graph.connect(points, sorted, "points").unwrap();
    let culled = node(&mut graph, CullDuplicatePoints, &[("tolerance", 1.0)]);
    graph.connect(sorted, culled, "points").unwrap();
    let numbers = node(&mut graph, PointsToNumbers, &[("mask", 31.0)]);
    graph.connect(culled, numbers, "points").unwrap();
    let composed = node(&mut graph, NumbersToPoints, &[("mask", 13.0)]);
    graph.connect(numbers, composed, "values").unwrap();
    let incomplete = node(&mut graph, NumbersToPoints, &[]);
    graph.connect(numbers, incomplete, "values").unwrap();
    let invalid_mask = node(&mut graph, PointsToNumbers, &[("mask", 11.0)]);
    graph.connect(culled, invalid_mask, "points").unwrap();

    let assert_outputs = |graph: &Graph| {
        let result = Evaluator::default().evaluate(graph, &|| false);
        let ordered = [0.0, 0.75, 1.5, 2.25].map(|x| Value::Point([x, 30.0, 10.0]));
        assert_eq!(root(&result, sorted), &ordered);
        assert_eq!(
            root(&result, culled),
            &[ordered[0].clone(), ordered[2].clone()]
        );
        assert_eq!(
            root(&result, numbers),
            &[10.0, 0.0, 10.0, 1.5].map(Value::Number)
        );
        assert_eq!(
            root(&result, composed),
            &[
                Value::Point([10.0, 0.0, 0.0]),
                Value::Point([10.0, 0.0, 1.5])
            ]
        );
        assert!(
            result.outputs[&incomplete]
                .as_ref()
                .unwrap_err()
                .contains("倍数")
        );
        assert!(
            result.outputs[&invalid_mask]
                .as_ref()
                .unwrap_err()
                .contains("座標順序")
        );
    };
    assert_outputs(&graph);

    let path =
        std::env::temp_dir().join(format!("nodes-point-adapters-{}.morph", std::process::id()));
    let project = Project::new("点ノード", graph);
    persistence::save(&path, &project).unwrap();
    let loaded = persistence::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, project);
    assert_outputs(&loaded.graph);
}

#[test]
fn point_list_processing_is_branch_local_and_api_reports_actual_access() {
    let mut graph = Graph::default();
    let series = node(
        &mut graph,
        Series,
        &[("start", 2.0), ("step", -1.0), ("count", 2.0)],
    );
    let points = point(&mut graph, [0.0, 4.0, 6.0]);
    graph.connect(series, points, "x").unwrap();
    let graft = node(&mut graph, Graft, &[]);
    graph.connect(points, graft, "geometry").unwrap();
    let convert = node(&mut graph, PointsToNumbers, &[]);
    graph.connect(graft, convert, "points").unwrap();

    let result = Evaluator::default().evaluate(&graph, &|| false);
    let branches = &result.outputs[&convert].as_ref().unwrap().branches;
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[&[0, 0][..]], [2.0, 4.0, 6.0].map(Value::Number));
    assert_eq!(branches[&[0, 1][..]], [1.0, 4.0, 6.0].map(Value::Number));

    let catalog = graph_api::catalog();
    let entry = catalog
        .nodes
        .iter()
        .find(|n| n.kind == PointsToNumbers)
        .unwrap();
    assert_eq!(entry.inputs[0].access, "List");
    assert_eq!(entry.inputs[1].access, "First item");
    assert!(!entry.item_matching_configurable);
    let entry = catalog.nodes.iter().find(|n| n.kind == PointPolar).unwrap();
    assert!(entry.inputs.iter().all(|input| input.access == "Item"));
    assert!(entry.item_matching_configurable);
}
