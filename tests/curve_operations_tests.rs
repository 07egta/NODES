use morphlet::curve_geometry::{self as cg, CurveGeometry, NurbsCurve, Point3};
use morphlet::curve_nodes::{geometry, run_item, run_list};
use morphlet::engine::Evaluator;
use morphlet::model::{Curve, Graph, NodeKind, Value};
use morphlet::node_access::{InputAccess, for_input};
use morphlet::persistence::{self, Project};
use std::{f64::consts::PI, sync::Arc};

const NEVER_CANCEL: &dyn Fn() -> bool = &|| false;

fn evaluate(kind: NodeKind, input: &[Value]) -> Vec<Value> {
    let args: Vec<_> = input.iter().collect();
    run_item(kind, &args, NEVER_CANCEL).unwrap()
}

fn point(value: &Value) -> Point3 {
    let Value::Point(p) = value else {
        panic!("expected point, got {value:?}");
    };
    *p
}

fn curve(value: &Value) -> &Curve {
    let Value::Curve(c) = value else {
        panic!("expected curve, got {value:?}");
    };
    c
}

fn geom(value: &Value) -> CurveGeometry {
    geometry(curve(value)).unwrap()
}

fn from_geometry(g: CurveGeometry) -> Value {
    Value::Curve(Arc::new(Curve {
        closed: g.closed().unwrap(),
        points: g.preview(8, NEVER_CANCEL).unwrap(),
        geometry: Some(g),
    }))
}

fn source_polyline(points: &[Point3], closed: bool) -> Value {
    from_geometry(CurveGeometry::nurbs(
        NurbsCurve::polyline(points.to_vec(), closed).unwrap(),
    ))
}

fn near(a: Point3, b: Point3) {
    assert!(cg::norm(cg::sub(a, b)) < 1e-9, "{a:?} != {b:?}");
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn length_measurements_and_inverse_respect_nonlinear_parameterization() {
    let g = CurveGeometry::nurbs(
        NurbsCurve::new(
            2,
            vec![[0.; 3], [1., 0., 0.], [9., 0., 0.]],
            vec![1.; 3],
            vec![3., 3., 3., 5., 5., 5.],
        )
        .unwrap(),
    );
    let c = from_geometry(g.reversed());
    let half = Value::Number(0.5);
    let quarter = Value::Number(0.25);
    let out = evaluate(NodeKind::CurveLengthParameter, &[c.clone(), half.clone()]);
    close(out[0].number().unwrap(), 6.25);
    close(out[1].number().unwrap(), 2.75);
    let interval = evaluate(
        NodeKind::CurveLengthDomain,
        &[c.clone(), half.clone(), quarter],
    );
    close(interval[0].number().unwrap(), 2.6875);
    let empty = evaluate(
        NodeKind::CurveLengthDomain,
        &[c.clone(), half.clone(), half.clone()],
    );
    assert_eq!(empty, [Value::Number(0.)]);

    for (length, normalized) in [
        (half, Value::Number(1.)),
        (Value::Number(4.5), Value::Number(0.)),
    ] {
        let args = [c.clone(), length, normalized];
        let t = evaluate(NodeKind::CurveParameterAtLength, &args)[0]
            .number()
            .unwrap();
        near(geom(&c).point(t).unwrap(), [4.5, 0., 0.]);
        near(
            point(&evaluate(NodeKind::CurveTangentAtLength, &args)[0]),
            [-1., 0., 0.],
        );
    }
    assert!(
        run_item(
            NodeKind::CurveLengthDomain,
            &[&c, &Value::Number(-0.1), &Value::Number(0.5)],
            NEVER_CANCEL
        )
        .is_err()
    );
}

#[test]
fn periodicity_and_control_polygon_report_the_basis() {
    let controls = vec![[0.; 3], [2., 0., 0.], [2., 3., 0.], [0., 2., 0.]];
    let g = CurveGeometry::nurbs(NurbsCurve::uniform(controls.clone(), 2, true).unwrap());
    assert_eq!(
        evaluate(NodeKind::CurvePeriodic, &[from_geometry(g.reversed())]),
        [Value::Number(1.)]
    );
    assert_eq!(
        evaluate(
            NodeKind::CurvePeriodic,
            &[from_geometry(g.subcurve(0.1, 0.9).unwrap())]
        ),
        [Value::Number(0.)]
    );
    let values = evaluate(NodeKind::CurveControlPolygon, &[from_geometry(g)]);
    assert_eq!(curve(&values[0]).points, controls);
    assert!(curve(&values[0]).closed);
    let repeated =
        CurveGeometry::nurbs(NurbsCurve::bezier(vec![[0.; 3], [0.; 3], [2., 0., 0.]]).unwrap());
    let values = evaluate(NodeKind::CurveControlPolygon, &[from_geometry(repeated)]);
    assert_eq!(curve(&values[0]).points.len(), 2);
}

#[test]
fn dash_pattern_keeps_curvature_and_clips_the_last_interval() {
    let radius = 2.;
    let g = CurveGeometry::arc([0.; 3], [radius, 0., 0.], [0., radius, 0.], 0., PI / 2.).unwrap();
    let curves = [from_geometry(g.clone())];
    let odd_pattern = [Value::Number(1.)];
    let segments = [Value::Number(8.)];
    let items = [&curves[..], &odd_pattern[..], &segments[..]];
    let dashes = run_list(NodeKind::CurveDashPattern, &items, NEVER_CANCEL).unwrap();
    let gaps = run_list(NodeKind::CurveDashGaps, &items, NEVER_CANCEL).unwrap();
    assert_eq!((dashes.len(), gaps.len()), (2, 2));
    let ordered = [&dashes[0], &gaps[0], &dashes[1], &gaps[1]];
    for (i, item) in ordered.into_iter().enumerate() {
        let seg = geom(item);
        close(
            seg.length_table(NEVER_CANCEL).unwrap().total(),
            if i == 3 { PI - 3. } else { 1. },
        );
        close(seg.curvature(0.5).unwrap(), 0.5);
    }
    near(geom(&dashes[0]).point(0.).unwrap(), g.point(0.).unwrap());
    near(geom(&gaps[1]).point(1.).unwrap(), g.point(1.).unwrap());

    for invalid in [vec![], vec![Value::Number(0.)], vec![Value::Number(0.001)]] {
        assert!(
            run_list(
                NodeKind::CurveDashPattern,
                &[&curves, &invalid, &segments],
                NEVER_CANCEL
            )
            .is_err()
        );
    }
    assert!(run_list(NodeKind::CurveDashPattern, &items, &|| true).is_err());
}

#[test]
fn centers_and_segment_extrema_use_active_vertices_and_closed_edges() {
    let vertices = [[0.; 3], [4., 0., 0.], [4., 3., 0.]];
    let closed = source_polyline(&vertices, true);
    let triangle = geom(&closed)
        .affine(|[x, y, z]| [7. + z, 10. + x, -50. + y])
        .unwrap();
    let transformed = from_geometry(triangle.reversed());
    let center = |kind| point(&evaluate(kind, std::slice::from_ref(&transformed))[0]);
    near(
        center(NodeKind::CurvePolygonCenterVertices),
        [7., 10. + 8. / 3., -49.],
    );
    near(center(NodeKind::CurvePolygonCenterEdges), [7., 12.5, -49.]);
    near(
        center(NodeKind::CurvePolygonCenterArea),
        [7., 10. + 8. / 3., -49.],
    );
    close(
        evaluate(
            NodeKind::CurveShortestSegment,
            std::slice::from_ref(&transformed),
        )[0]
        .number()
        .unwrap(),
        3.,
    );
    close(
        evaluate(NodeKind::CurveLongestSegment, &[transformed])[0]
            .number()
            .unwrap(),
        5.,
    );
    let g = geom(&source_polyline(&vertices, false));
    let trimmed = from_geometry(g.subcurve(0.25, 0.75).unwrap());
    close(
        evaluate(NodeKind::CurveShortestSegment, &[trimmed])[0]
            .number()
            .unwrap(),
        1.5,
    );

    let open = source_polyline(&vertices, false);
    assert!(run_item(NodeKind::CurvePolygonCenterArea, &[&open], NEVER_CANCEL).is_err());
    let nonplanar = source_polyline(&[[0.; 3], [1., 0., 0.], [1., 1., 1.], [0., 1., 0.]], true);
    assert!(
        run_item(
            NodeKind::CurvePolygonCenterArea,
            &[&nonplanar],
            NEVER_CANCEL
        )
        .is_err()
    );
}

#[test]
fn polyline_editors_preserve_open_endpoints_and_reject_smooth_inputs() {
    let c = source_polyline(&[[0.; 3], [1., 1., 0.], [2., 0., 0.]], false);
    let smooth = evaluate(
        NodeKind::CurveSmoothPolyline,
        &[c, Value::Number(0.5), Value::Number(1.)],
    );
    assert_eq!(
        curve(&smooth[0]).points,
        [[0.; 3], [1., 0.5, 0.], [2., 0., 0.]]
    );
    let c = source_polyline(
        &[[0.; 3], [1., 0.02, 0.], [2., 0., 0.], [2., 1., 0.]],
        false,
    );
    let reduced = evaluate(NodeKind::CurveReducePolyline, &[c, Value::Number(0.05)]);
    assert_eq!(
        curve(&reduced[0]).points,
        [[0.; 3], [2., 0., 0.], [2., 1., 0.]]
    );
    let c = source_polyline(
        &[
            [0.; 3],
            [1., 0.02, 0.],
            [2., 0., 0.],
            [2., 1., 0.],
            [0., 1., 0.],
        ],
        true,
    );
    let reduced = evaluate(
        NodeKind::CurveReducePolyline,
        &[c.clone(), Value::Number(0.05)],
    );
    assert!(curve(&reduced[0]).closed);
    assert_eq!(curve(&reduced[0]).points.len(), 4);
    assert!(
        run_item(
            NodeKind::CurveReducePolyline,
            &[&c, &Value::Number(5.)],
            NEVER_CANCEL
        )
        .is_err()
    );
    let circle = from_geometry(CurveGeometry::circle([0.; 3], [0., 0., 1.], 2.).unwrap());
    assert!(
        run_item(
            NodeKind::CurveReducePolyline,
            &[&circle, &Value::Number(0.05)],
            NEVER_CANCEL
        )
        .is_err()
    );
}

#[test]
fn rectangles_polygons_and_projected_lines_use_spatial_inputs() {
    let a = [0., 1., 0.];
    let b = [0., 1., 4.];
    let c = [3., 1., 1.];
    let points = [Value::Point(a), Value::Point(b), Value::Point(c)];
    let rectangle = evaluate(NodeKind::CurveRectangleThree, &points);
    assert_eq!(
        curve(&rectangle[0]).points,
        [a, b, [3., 1., 4.], [3., 1., 0.]]
    );
    let poly = evaluate(
        NodeKind::CurvePolygonEdge,
        &[
            points[0].clone(),
            points[1].clone(),
            points[2].clone(),
            Value::Number(6.),
        ],
    );
    let vertices = &curve(&poly[0]).points;
    assert_eq!(&vertices[..2], &[a, b]);
    assert_eq!(vertices.len(), 6);
    for (&v, &w) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        close(v[1], 1.);
        assert!(v[0] >= 0.);
        close(cg::norm(cg::sub(w, v)), 4.);
    }
    let line = source_polyline(&[a, b], false);
    let projected = evaluate(
        NodeKind::CurveLineFour,
        &[
            line,
            Value::Point([2., 2., -1.]),
            Value::Point([3., 1., 7.]),
        ],
    );
    assert_eq!(curve(&projected[0]).points, [[0., 1., -1.], [0., 1., 7.]]);
}

#[test]
fn inscribed_conics_and_endpoint_tangent_arc_have_correct_loci() {
    let a = Value::Point([2., 0., 0.]);
    let b = Value::Point([2., 4., 0.]);
    let c = Value::Point([2., 0., 3.]);
    let segments = Value::Number(12.);
    let args = [a, b, c, segments.clone()];
    let circle = evaluate(NodeKind::CurveIncircle, &args);
    let g = geom(&circle[0]);
    close(g.length_table(NEVER_CANCEL).unwrap().total(), 2. * PI);
    for t in [0., 0.125, 0.25, 0.625] {
        close(cg::norm(cg::sub(g.point(t).unwrap(), [2., 1., 1.])), 1.);
    }

    let ellipse = evaluate(NodeKind::CurveInEllipse, &args);
    let g = geom(&ellipse[0]);
    for (midpoint, side) in [
        ([2., 2., 0.], [0., 1., 0.]),
        ([2., 0., 1.5], [0., 0., 1.]),
        ([2., 2., 1.5], [0., -0.8, 0.6]),
    ] {
        // The existing nearest-parameter search only resolves t approximately.
        let t = g.closest_parameter(midpoint, NEVER_CANCEL).unwrap();
        assert!(cg::norm(cg::sub(g.point(t).unwrap(), midpoint)) < 1e-7);
        assert!(cg::norm(cg::cross(g.tangent(t).unwrap(), side)) < 1e-7);
    }

    let start = [5., 1., 0.];
    let end = [5., 0., 1.];
    let tangent = [0., 0., -2.];
    let arc = evaluate(
        NodeKind::CurveArcSed,
        &[
            Value::Point(start),
            Value::Point(end),
            Value::Point(tangent),
            segments.clone(),
        ],
    );
    let g = geom(&arc[0]);
    near(g.point(0.).unwrap(), start);
    near(g.point(1.).unwrap(), end);
    near(g.tangent(0.).unwrap(), [0., 0., -1.]);
    close(g.length_table(NEVER_CANCEL).unwrap().total(), 1.5 * PI);

    let shallow_start = [0., 0.00001, 0.];
    let shallow_end = [1e6, 0.00101, 0.];
    let shallow = evaluate(
        NodeKind::CurveArcSed,
        &[
            Value::Point(shallow_start),
            Value::Point(shallow_end),
            Value::Point([1., 0., 0.]),
            segments.clone(),
        ],
    );
    let g = geom(&shallow[0]);
    assert_eq!(g.point(0.).unwrap(), shallow_start);
    assert_eq!(g.point(1.).unwrap(), shallow_end);
    near(g.tangent(0.).unwrap(), [1., 0., 0.]);
    close(g.point(0.5).unwrap()[1], 0.00026);
    assert!(
        run_item(
            NodeKind::CurveArcSed,
            &[
                &Value::Point(start),
                &Value::Point([5., 1., 1.]),
                &Value::Point(tangent),
                &segments
            ],
            NEVER_CANCEL
        )
        .is_err()
    );
}

#[test]
fn dash_pattern_uses_list_access_and_survives_save_reload() {
    assert_eq!(
        for_input(NodeKind::CurveDashPattern, 0),
        Some(InputAccess::List)
    );
    assert_eq!(
        for_input(NodeKind::CurveDashPattern, 1),
        Some(InputAccess::List)
    );
    assert_eq!(
        for_input(NodeKind::CurveDashPattern, 2),
        Some(InputAccess::FirstItem)
    );
    let mut graph = Graph::default();
    let pattern = graph.add(NodeKind::Range, [0.; 2]).unwrap();
    let circle = graph.add(NodeKind::Circle, [0.; 2]).unwrap();
    let dash = graph.add(NodeKind::CurveDashPattern, [0.; 2]).unwrap();
    let length = graph.add(NodeKind::CurveLength, [0.; 2]).unwrap();
    let p = &mut graph.node_mut(pattern).unwrap().params;
    p.insert("start".into(), 4.);
    p.insert("end".into(), 2.);
    p.insert("count".into(), 2.);
    graph
        .node_mut(circle)
        .unwrap()
        .params
        .insert("radius".into(), 2.);
    graph.connect(pattern, dash, "pattern").unwrap();
    graph.connect(circle, dash, "curve").unwrap();
    graph.connect(dash, length, "curve").unwrap();

    let project = Project::new("破線", graph);
    let path = std::env::temp_dir().join(format!("nodes-curve-dash-{}.morph", std::process::id()));
    persistence::save(&path, &project).unwrap();
    let reloaded = persistence::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(project, reloaded);
    let evaluation = Evaluator::default().evaluate(&reloaded.graph, NEVER_CANCEL);
    let lengths: Vec<_> = evaluation.outputs[&length].as_ref().unwrap().branches[&vec![0]]
        .iter()
        .map(|v| v.number().unwrap())
        .collect();
    assert_eq!(lengths.len(), 3);
    for (&length, expected) in lengths.iter().zip([4., 4., 4. * PI - 12.]) {
        close(length, expected);
    }
}

#[test]
fn weighted_polyline_omits_the_geometrically_closed_seam() {
    let g = CurveGeometry::nurbs(
        NurbsCurve::new(
            1,
            vec![[0.1, 0., 0.], [4.1, 0., 0.], [0.1, 3., 0.], [0.1, 0., 0.]],
            vec![1., 1., 1., 0.1],
            vec![0., 0., 1. / 3., 2. / 3., 1., 1.],
        )
        .unwrap(),
    );
    assert_ne!(g.point(0.).unwrap(), g.point(1.).unwrap());
    assert!(g.closed().unwrap());
    let c = from_geometry(g);
    for kind in [
        NodeKind::CurvePolygonCenterVertices,
        NodeKind::CurvePolygonCenterArea,
    ] {
        near(
            point(&evaluate(kind, std::slice::from_ref(&c))[0]),
            [0.1 + 4. / 3., 1., 0.],
        );
    }
}

#[test]
fn closed_polyline_smoothing_rejects_two_alternating_positions() {
    let c = source_polyline(&[[0.; 3], [4., 0., 0.], [3., 2., 0.], [0., 2., 0.]], true);
    let iterations = Value::Number(1.);
    assert!(
        run_item(
            NodeKind::CurveSmoothPolyline,
            &[&c, &Value::Number(1.), &iterations],
            NEVER_CANCEL,
        )
        .is_err()
    );
    let out = evaluate(
        NodeKind::CurveSmoothPolyline,
        &[c, Value::Number(0.5), iterations],
    );
    assert!(curve(&out[0]).closed);
    assert_eq!(curve(&out[0]).points.len(), 4);
    near(curve(&out[0]).points[0], [1., 0.5, 0.]);
}
