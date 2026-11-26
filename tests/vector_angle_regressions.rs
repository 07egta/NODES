//! Direction-preserving angle calculations, including a representable tiny angle.
//! References use exact integer geometry; ordinary floating-point limits remain.
use morphlet::engine::Evaluator;
use morphlet::math_nodes;
use morphlet::model::{Graph, NodeId, NodeKind, Value};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

fn angle(a: [f64; 3], b: [f64; 3]) -> Result<f64, String> {
    let values = [Value::Point(a), Value::Point(b)];
    let output = math_nodes::run_item(NodeKind::VectorAngle, &[&values[0], &values[1]], &|| false)?;
    assert_eq!(output.len(), 1);
    output[0].number()
}

fn near_parallel() -> ([f64; 3], [f64; 3], f64) {
    const N: i128 = 1_000_000_000;
    let a = [N, N - 1, 0];
    let b = [N - 1, N - 2, 0];
    let determinant = a[0] * b[1] - a[1] * b[0];
    let dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<i128>();
    assert_eq!(determinant, -1);
    assert_eq!(dot, 2 * (N - 1) * (N - 1));
    let expected = (determinant.abs() as f64).atan2(dot as f64);
    (a.map(|x| x as f64), b.map(|x| x as f64), expected)
}

fn close_angle(actual: f64, expected: f64) {
    assert!(actual.is_finite());
    assert!(
        (actual - expected).abs() <= expected.abs() * 8.0 * f64::EPSILON,
        "angle {actual:e}, expected {expected:e}"
    );
}

#[test]
fn near_parallel_integer_vectors_retain_their_nonzero_angle() {
    let (a, b, expected) = near_parallel();
    assert!(expected > 0.0 && expected < 1e-18);
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let left = order.map(|i| a[i]);
        let right = order.map(|i| b[i]);
        close_angle(angle(left, right).unwrap(), expected);
        close_angle(angle(right, left).unwrap(), expected);
    }
}

#[test]
fn independent_scaling_and_subnormal_directions_preserve_angles() {
    let (a, b, expected) = near_parallel();
    close_angle(
        angle(
            a.map(|x| x * 2.0_f64.powi(-700)),
            b.map(|x| x * 2.0_f64.powi(-800)),
        )
        .unwrap(),
        expected,
    );
    // These bit patterns are the same integer vectors multiplied by the least
    // subnormal. Their common scale cancels in the analytic angle.
    let subnormal_a = [
        f64::from_bits(1_000_000_000),
        f64::from_bits(999_999_999),
        0.,
    ];
    let subnormal_b = [f64::from_bits(999_999_999), f64::from_bits(999_999_998), 0.];
    close_angle(angle(subnormal_a, subnormal_b).unwrap(), expected);
    close_angle(angle(a, subnormal_b).unwrap(), expected);

    let tiny = f64::from_bits(1);
    // The exact angle is approximately (2/3)*tiny, which rounds to tiny. Scaling
    // only into [1,2) loses the half-tiny product residual and incorrectly gives 0.
    assert_eq!(
        angle([1.5, 3.0 * tiny, 0.], [1.5, 2.0 * tiny, 0.])
            .unwrap()
            .to_bits(),
        1
    );
    for (a, b, expected) in [
        ([tiny, 0., 0.], [0., tiny, 0.], FRAC_PI_2),
        ([tiny, 0., 0.], [-tiny, 0., 0.], PI),
        ([tiny, 0., 0.], [2.0 * tiny, 0., 0.], 0.),
        ([1e9, 0., 0.], [0., tiny, 0.], FRAC_PI_2),
        ([1., 0., 0.], [1., 1., 0.], FRAC_PI_4),
        ([1., 0., 0.], [-1., 1., 0.], 3.0 * FRAC_PI_4),
        ([1., 2., 3.], [2., 4., 6.], 0.),
    ] {
        close_angle(angle(a, b).unwrap(), expected);
    }
}

#[test]
fn zero_invalid_and_cancelled_angle_inputs_still_fail() {
    let valid = [1., 0., 0.];
    for invalid in [
        [0.; 3],
        [-0.0; 3],
        [f64::NAN, 0., 0.],
        [f64::INFINITY, 0., 0.],
        [1e9 + 1., 0., 0.],
    ] {
        assert!(angle(invalid, valid).is_err());
        assert!(angle(valid, invalid).is_err());
    }
    let values = [Value::Point(valid), Value::Point([0., 1., 0.])];
    assert!(
        math_nodes::run_item(NodeKind::VectorAngle, &[&values[0], &values[1]], &|| true).is_err()
    );
}

fn vector(graph: &mut Graph, xyz: [f64; 3]) -> NodeId {
    let id = graph.add(NodeKind::ConstructVector, [0.; 2]).unwrap();
    graph.node_mut(id).unwrap().params.extend([
        ("x".into(), xyz[0]),
        ("y".into(), xyz[1]),
        ("z".into(), xyz[2]),
    ]);
    id
}

#[test]
fn native_angle_nodes_preserve_grafted_paths_broadcasting_and_cache() {
    let (left, right, expected) = near_parallel();
    let mut graph = Graph::default();
    let a = vector(&mut graph, left);
    let b = vector(&mut graph, right);
    let reversed = graph.add(NodeKind::ReverseVector, [0.; 2]).unwrap();
    graph.connect(a, reversed, "vector").unwrap();
    let list = graph.add(NodeKind::ConcatList, [0.; 2]).unwrap();
    graph.connect(a, list, "a").unwrap();
    graph.connect(reversed, list, "b").unwrap();
    let graft = graph.add(NodeKind::Graft, [0.; 2]).unwrap();
    graph.connect(list, graft, "geometry").unwrap();
    let angle = graph.add(NodeKind::VectorAngle, [0.; 2]).unwrap();
    graph.connect(graft, angle, "a").unwrap();
    graph.connect(b, angle, "b").unwrap();
    graph.validate().unwrap();
    let mut evaluator = Evaluator::default();
    let first = evaluator.evaluate(&graph, &|| false);
    let tree = first.outputs[&angle].as_ref().unwrap();
    assert_eq!(tree.branches.len(), 2);
    for (branch, expected) in [expected, PI - expected].into_iter().enumerate() {
        let values = &tree.branches[&vec![0, branch as u32]];
        assert_eq!(values.len(), 1);
        close_angle(values[0].number().unwrap(), expected);
    }
    let warm = evaluator.evaluate(&graph, &|| false);
    assert_eq!(warm.outputs, first.outputs);
    assert_eq!(warm.cached, graph.nodes.len());
    assert_eq!(warm.computed, 0);
}
