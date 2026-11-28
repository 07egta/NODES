//! Analytic fixtures for OMEGA 16. Geometry is a skeleton/profile, not a load-rated assembly.
use nodes::engine::Evaluator;
use nodes::mechanism_nodes;
use nodes::model::{Graph, NodeKind as K, Value};
use nodes::persistence::Project;
use std::f64::consts::PI;

fn evaluate(kind: K, numbers: &[f64]) -> Result<Vec<Value>, String> {
    let values: Vec<_> = numbers.iter().copied().map(Value::Number).collect();
    let refs: Vec<_> = values.iter().collect();
    mechanism_nodes::run(kind, &refs, &|| false)
}

fn points(values: &[Value]) -> &[[f64; 3]] {
    match &values[0] {
        Value::Curve(c) => &c.points,
        _ => panic!("expected curve"),
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[test]
fn four_bar_preserves_all_four_lengths_in_both_assembly_modes() {
    for branch in [0., 1.] {
        for angle in [0., 45., 90., 180., 270.] {
            let result = evaluate(K::MechFourBar, &[60., 20., 60., 40., angle, branch]).unwrap();
            let p = points(&result);
            close(distance(p[0], p[1]), 20.);
            close(distance(p[1], p[2]), 60.);
            close(distance(p[2], p[3]), 40.);
            close(distance(p[3], p[0]), 60.);
        }
    }
    assert!(evaluate(K::MechFourBar, &[100., 20., 10., 10., 90., 0.]).is_err());
    assert!(evaluate(K::MechFourBar, &[20., 20., 10., 10., 0., 0.]).is_err());
}

#[test]
fn slider_stroke_and_offset_are_analytic() {
    let at_zero = evaluate(K::MechSliderCrank, &[20., 60., 0., 0., 0.]).unwrap();
    let at_pi = evaluate(K::MechSliderCrank, &[20., 60., 0., 180., 0.]).unwrap();
    close(points(&at_zero)[2][0] - points(&at_pi)[2][0], 40.);
    for branch in [0., 1.] {
        let result = evaluate(K::MechSliderCrank, &[20., 60., 12., 70., branch]).unwrap();
        let p = points(&result);
        close(p[2][1], 12.);
        close(distance(p[0], p[1]), 20.);
        close(distance(p[1], p[2]), 60.);
    }
    assert!(evaluate(K::MechSliderCrank, &[20., 10., 0., 90., 0.]).is_err());
}

#[test]
fn inverse_reaches_target_on_both_branches_and_rejects_unreachable() {
    for branch in [0., 1.] {
        let result = evaluate(K::MechArm2rInverse, &[40., 30., 40., 30., branch]).unwrap();
        let q1 = result[0].number().unwrap();
        let q2 = result[1].number().unwrap();
        let fk = evaluate(K::MechArm2r, &[40., 30., q1, q2]).unwrap();
        close(points(&fk)[2][0], 40.);
        close(points(&fk)[2][1], 30.);
        close(q2.abs(), 90.);
    }
    assert!(evaluate(K::MechArm2rInverse, &[40., 30., 71., 0., 0.]).is_err());
    assert!(evaluate(K::MechArm2rInverse, &[40., 30., 9., 0., 0.]).is_err());
    assert!(evaluate(K::MechArm2rInverse, &[40., 40., 0., 0., 0.]).is_err());
}

#[test]
fn jacobian_matches_centered_finite_difference_per_radian() {
    let p = [40., 30., 30., 60.];
    let j = evaluate(K::MechJacobian2r, &p).unwrap();
    let h = 1e-5_f64;
    for col in 0..2 {
        let mut plus = p;
        let mut minus = p;
        plus[2 + col] += h.to_degrees();
        minus[2 + col] -= h.to_degrees();
        let a = evaluate(K::MechArm2r, &plus).unwrap();
        let b = evaluate(K::MechArm2r, &minus).unwrap();
        for row in 0..2 {
            let derivative = (points(&a)[2][row] - points(&b)[2][row]) / (2. * h);
            close(derivative, j[row * 2 + col].number().unwrap());
        }
    }
    let straight = evaluate(K::MechJacobian2r, &[40., 30., 0., 0.]).unwrap();
    close(straight[0].number().unwrap(), 0.);
    close(straight[1].number().unwrap(), 0.);
}

#[test]
fn gear_pitch_limits_and_periodicity() {
    let result = evaluate(K::MechSpurGearProfile, &[2., 24., 20., 0., 8.]).unwrap();
    let Value::Curve(c) = &result[0] else {
        panic!("expected gear curve");
    };
    assert!(c.closed);
    let p = &c.points;
    assert_eq!(p.len() % 24, 0);
    let per_tooth = p.len() / 24;
    let min = p
        .iter()
        .map(|v| v[0].hypot(v[1]))
        .fold(f64::INFINITY, f64::min);
    let max = p.iter().map(|v| v[0].hypot(v[1])).fold(0., f64::max);
    close(min, 21.5);
    close(max, 26.);
    let (s, c) = (2. * PI / 24.).sin_cos();
    for (a, b) in p[..per_tooth].iter().zip(&p[per_tooth..2 * per_tooth]) {
        close(a[0] * c - a[1] * s, b[0]);
        close(a[0] * s + a[1] * c, b[1]);
    }
    for (a, b) in p.iter().zip(p.iter().cycle().skip(1)) {
        assert!(distance(*a, *b) > 1e-10);
    }
    assert!(evaluate(K::MechSpurGearProfile, &[2., 12., 20., 0., 8.]).is_err());
    assert!(evaluate(K::MechSpurGearProfile, &[2., 24.5, 20., 0., 8.]).is_err());
    assert!(evaluate(K::MechSpurGearProfile, &[2., 24., 20., 10., 8.]).is_err());
}

#[test]
fn motion_relations_and_cancellation() {
    let v = evaluate(K::MechGearAngle, &[20., 40., 90., 0.]).unwrap();
    close(v[0].number().unwrap(), -45.);
    let v = evaluate(K::MechGearAngle, &[20., 40., 90., 1.]).unwrap();
    close(v[0].number().unwrap(), 45.);
    let v = evaluate(K::MechRackTravel, &[2., 20., 360.]).unwrap();
    close(v[0].number().unwrap(), 40. * PI);
    let values = [2., 24., 20., 0., 8.].map(Value::Number);
    let refs: Vec<_> = values.iter().collect();
    assert!(mechanism_nodes::run(K::MechSpurGearProfile, &refs, &|| true).is_err());
}

#[test]
fn all_new_nodes_evaluate_and_survive_project_roundtrip() {
    let mut graph = Graph::default();
    let ids: Vec<_> = K::all()
        .iter()
        .copied()
        .filter(|k| mechanism_nodes::is_kind(*k))
        .map(|k| graph.add(k, [0., 0.]).unwrap())
        .collect();
    assert_eq!(ids.len(), 8);
    let project = Project::new("機構", graph);
    project.validate().unwrap();
    let loaded: Project = serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
    loaded.validate().unwrap();
    assert_eq!(project, loaded);
    let result = Evaluator::default().evaluate(&loaded.graph, &|| false);
    for id in ids {
        assert!(result.outputs[&id].is_ok(), "node {id} failed");
        assert!(result.outputs[&id].as_ref().unwrap().item_count() > 0);
    }
}
