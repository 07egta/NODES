//! Mathematical expectations and graph regressions. CI must compile and execute these.
use morphlet::curve_geometry::{self as cg, CurveGeometry, NurbsCurve};
use morphlet::curve_nodes;
use morphlet::engine::Evaluator;
use morphlet::model::{Curve, Graph, NodeKind, Tree, Value};
use morphlet::persistence;
use std::{
    f64::consts::{PI, TAU},
    sync::Arc,
};
fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, tol {t}");
}
fn near(a: [f64; 3], b: [f64; 3], t: f64) {
    assert!(cg::norm(cg::sub(a, b)) <= t, "{a:?} != {b:?}");
}
fn val(g: CurveGeometry) -> Value {
    Value::Curve(Arc::new(Curve {
        closed: g.closed().unwrap(),
        points: g.preview(32, &|| false).unwrap(),
        geometry: Some(g),
    }))
}
fn line(a: [f64; 3], b: [f64; 3]) -> Value {
    val(CurveGeometry::nurbs(
        NurbsCurve::polyline(vec![a, b], false).unwrap(),
    ))
}
fn number(v: &Value) -> f64 {
    v.number().unwrap()
}
fn first(t: &Tree) -> &Value {
    t.branches.values().flatten().next().unwrap()
}
fn points(v: &[Value]) -> Vec<[f64; 3]> {
    v.iter()
        .map(|v| {
            if let Value::Point(p) = v {
                *p
            } else {
                panic!("point expected")
            }
        })
        .collect()
}
#[test]
fn rational_quarter_circle_has_exact_locus_and_analytic_curvature() {
    let n = NurbsCurve::new(
        2,
        vec![[1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
        vec![1., 0.5_f64.sqrt(), 1.],
        vec![0., 0., 0., 1., 1., 1.],
    )
    .unwrap();
    let g = CurveGeometry::nurbs(n);
    near(
        g.point(0.5).unwrap(),
        [0.5_f64.sqrt(), 0.5_f64.sqrt(), 0.],
        1e-13,
    );
    for i in 0..=100 {
        let t = i as f64 / 100.;
        let p = g.point(t).unwrap();
        close(cg::norm(p), 1., 2e-14);
        close(cg::dot(p, g.tangent(t).unwrap()), 0., 2e-13);
        close(g.curvature(t).unwrap(), 1., 1e-12);
    }
    close(g.length_table(&|| false).unwrap().total(), PI / 2., 1e-9);
}
#[test]
fn de_boor_knot_insertion_preserves_points_and_derivatives() {
    let g = CurveGeometry::arc([1., 2., 3.], [4., 0., 0.], [0., 4., 0.], 0., PI / 2.).unwrap();
    let old = g.as_nurbs().unwrap();
    let new = old.insert_knot(0.37).unwrap();
    assert_eq!(new.knots().len(), old.knots().len() + 1);
    for i in 0..=100 {
        let t = i as f64 / 100.;
        for (a, b) in old
            .derivatives(t)
            .unwrap()
            .into_iter()
            .zip(new.derivatives(t).unwrap())
        {
            near(a, b, 2e-11);
        }
    }
}
#[test]
fn periodic_uniform_curve_has_continuous_seam() {
    let n = NurbsCurve::uniform(
        vec![[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [0., 2., 0.]],
        3,
        true,
    )
    .unwrap();
    let g = CurveGeometry::nurbs(n);
    assert!(g.periodic());
    assert!(g.closed().unwrap());
    near(g.point(0.).unwrap(), g.point(1.).unwrap(), 1e-12);
    near(
        g.derivatives(0.).unwrap()[1],
        g.derivatives(1.).unwrap()[1],
        1e-12,
    );
    near(
        g.derivatives(0.).unwrap()[2],
        g.derivatives(1.).unwrap()[2],
        1e-12,
    );
}
#[test]
fn interpolation_passes_through_chord_parameter_points() {
    let p = [[0., 0., 0.], [1., 2., 0.], [3., 4., 1.], [5., 0., 2.]];
    let n = NurbsCurve::interpolate(&p, 3, &|| false).unwrap();
    let total: f64 = p.windows(2).map(|w| cg::norm(cg::sub(w[1], w[0]))).sum();
    let mut length = 0.;
    for i in 0..p.len() {
        if i > 0 {
            length += cg::norm(cg::sub(p[i], p[i - 1]));
        }
        near(n.point(length / total).unwrap(), p[i], 1e-10);
    }
}
#[test]
fn parameter_is_not_arc_length() {
    let g = CurveGeometry::nurbs(
        NurbsCurve::bezier(vec![[0., 0., 0.], [1., 0., 0.], [9., 0., 0.]]).unwrap(),
    );
    near(g.point(0.5).unwrap(), [2.75, 0., 0.], 1e-12);
    let tab = g.length_table(&|| false).unwrap();
    close(tab.total(), 9., 1e-10);
    let t = tab.parameter(&g, 4.5, &|| false).unwrap();
    assert!((t - 0.5).abs() > 0.1);
    near(g.point(t).unwrap(), [4.5, 0., 0.], 1e-10);
}
#[test]
fn helix_length_and_tangent_are_analytic() {
    let r = 3.;
    let h = 7.;
    let turns = 2.;
    let g =
        CurveGeometry::helix([0.; 3], [r, 0., 0.], [0., r, 0.], [0., 0., h], turns, 0.).unwrap();
    close(
        g.length_table(&|| false).unwrap().total(),
        (TAU * r * turns).hypot(h),
        1e-9,
    );
    near(g.point(0.25).unwrap(), [-r, 0., h * 0.25], 1e-12);
    let rev = g.reversed();
    near(rev.point(0.2).unwrap(), g.point(0.8).unwrap(), 1e-12);
    near(
        rev.tangent(0.2).unwrap(),
        cg::scale(g.tangent(0.8).unwrap(), -1.),
        1e-12,
    );
}
#[test]
fn three_points_circle_and_arc_and_degenerate_input() {
    let g = CurveGeometry::through_three([2., 0., 3.], [0., 2., 3.], [-2., 0., 3.], false).unwrap();
    near(g.point(0.).unwrap(), [2., 0., 3.], 1e-12);
    near(g.point(0.5).unwrap(), [0., 2., 3.], 1e-12);
    near(g.point(1.).unwrap(), [-2., 0., 3.], 1e-12);
    close(g.length_table(&|| false).unwrap().total(), 2. * PI, 1e-8);
    assert!(CurveGeometry::through_three([0.; 3], [1., 0., 0.], [2., 0., 0.], false).is_err());
    assert!(
        NurbsCurve::new(
            2,
            vec![[0.; 3]; 3],
            vec![1., 0., 1.],
            vec![0., 0., 0., 1., 1., 1.]
        )
        .is_err()
    );
    assert!(
        NurbsCurve::new(
            2,
            vec![[0.; 3]; 3],
            vec![1.; 3],
            vec![0., 0., 1., 0., 1., 1.]
        )
        .is_err()
    );
    assert!(g.point(-0.01).is_err());
    assert!(g.subcurve(0.2, 0.2).is_err());
    assert!(g.length_table(&|| true).is_err());
}
#[test]
fn intersections_include_endpoint_skew_and_parallel_cases() {
    let a = line([0., 0., 0.], [2., 0., 0.]);
    let b = line([1., -1., 0.], [1., 1., 0.]);
    let tol = Value::Number(1e-7);
    let run = |b: &Value| curve_nodes::run_item(NodeKind::CurveLineLine, &[&a, b, &tol], &|| false);
    near(points(&run(&b).unwrap())[0], [1., 0., 0.], 1e-12);
    assert!(run(&line([1., -1., 1.], [1., 1., 1.])).unwrap().is_empty());
    assert!(run(&line([0., 1., 0.], [2., 1., 0.])).is_err());
    assert!(run(&line([1., 0., 0.], [3., 0., 0.])).is_err());
    near(
        points(&run(&line([2., 0., 0.], [2., 1., 0.])).unwrap())[0],
        [2., 0., 0.],
        1e-12,
    );
    let o = Value::Point([1., 0., 0.]);
    let normal = Value::Point([1., 0., 0.]);
    near(
        points(
            &curve_nodes::run_item(NodeKind::CurveLinePlane, &[&a, &o, &normal, &tol], &|| {
                false
            })
            .unwrap(),
        )[0],
        [1., 0., 0.],
        1e-12,
    );
}
#[test]
fn closest_parameter_and_exact_subcurve() {
    let g = CurveGeometry::circle([0.; 3], [0., 0., 1.], 10.).unwrap();
    let t = g.closest_parameter([0., 15., 0.], &|| false).unwrap();
    near(g.point(t).unwrap(), [0., 10., 0.], 1e-6);
    let s = g.subcurve(0.2, 0.8).unwrap();
    for i in 0..=10 {
        let t = i as f64 / 10.;
        near(s.point(t).unwrap(), g.point(0.2 + 0.6 * t).unwrap(), 1e-12);
    }
}
#[test]
fn graph_circle_resolution_independent_and_affine_retains_precision() {
    let mut graph = Graph::default();
    let c = graph.add(NodeKind::Circle, [0.; 2]).unwrap();
    let l = graph.add(NodeKind::CurveLength, [0.; 2]).unwrap();
    let m = graph.add(NodeKind::Move, [0.; 2]).unwrap();
    let w = graph.add(NodeKind::Wave, [0.; 2]).unwrap();
    graph.connect(c, l, "curve").unwrap();
    graph.connect(c, m, "geometry").unwrap();
    graph.connect(c, w, "geometry").unwrap();
    graph
        .node_mut(c)
        .unwrap()
        .params
        .insert("segments".into(), 8.);
    graph.node_mut(m).unwrap().params.insert("x".into(), 10.);
    let ev = Evaluator::default().evaluate(&graph, &|| false);
    close(
        number(first(ev.outputs[&l].as_ref().unwrap())),
        30. * TAU,
        1e-6,
    );
    if let Value::Curve(c) = first(ev.outputs[&m].as_ref().unwrap()) {
        assert_eq!(c.points.len(), 8);
        near(
            c.geometry.as_ref().unwrap().point(0.).unwrap(),
            [40., 0., 0.],
            1e-12,
        );
    } else {
        panic!("curve expected");
    }
    if let Value::Curve(c) = first(ev.outputs[&w].as_ref().unwrap()) {
        assert!(c.geometry.is_none());
    } else {
        panic!("curve expected");
    }
}
#[test]
fn tree_branches_preserved_and_examples_round_trip() {
    let mut graph = Graph::default();
    let c = graph.add(NodeKind::Circle, [0.; 2]).unwrap();
    let range = graph.add(NodeKind::Range, [0.; 2]).unwrap();
    graph
        .node_mut(range)
        .unwrap()
        .params
        .insert("start".into(), 10.);
    graph
        .node_mut(range)
        .unwrap()
        .params
        .insert("end".into(), 20.);
    graph
        .node_mut(range)
        .unwrap()
        .params
        .insert("count".into(), 2.);
    graph.connect(range, c, "radius").unwrap();
    let graft = graph.add(NodeKind::Graft, [0.; 2]).unwrap();
    let input = NodeKind::Graft.spec().inputs[0].key;
    graph.connect(c, graft, input).unwrap();
    let divide = graph.add(NodeKind::CurveDivide, [0.; 2]).unwrap();
    graph.connect(graft, divide, "curve").unwrap();
    let ev = Evaluator::default().evaluate(&graph, &|| false);
    let branches = &ev.outputs[&divide].as_ref().unwrap().branches;
    assert_eq!(branches.len(), 2);
    for values in branches.values() {
        assert_eq!(values.len(), 10);
    }
    for file in [
        "11-circle-divisions.morph",
        "12-helix-frames.morph",
        "13-furniture-profile.morph",
        "14-offset-preview.morph",
        "15-line-intersection.morph",
        "16-trim-loft.morph",
    ] {
        let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/curves")
            .join(file);
        let project = persistence::load(&original).unwrap_or_else(|e| panic!("{file}: {e}"));
        let ev = Evaluator::default().evaluate(&project.graph, &|| false);
        assert!(!ev.outputs.is_empty());
        for (id, result) in &ev.outputs {
            assert!(result.is_ok(), "{file} node {id}: {result:?}");
        }
        let path =
            std::env::temp_dir().join(format!("morphlet-curve-{}-{file}", std::process::id()));
        persistence::save(&path, &project).unwrap();
        let loaded = persistence::load(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded, project);
        let again = Evaluator::default().evaluate(&loaded.graph, &|| false);
        assert_eq!(again.outputs, ev.outputs);
    }
}
