//! Exact integer reference cases for cancellation inside the existing 1e9 bound.
//! Compensated products do not promise exact real arithmetic or eliminate underflow.
use nodes::engine::Evaluator;
use nodes::math_nodes;
use nodes::model::{Graph, NodeId, NodeKind, Value};

fn run(kind: NodeKind, a: [f64; 3], b: [f64; 3]) -> Result<Value, String> {
    let values = [Value::Point(a), Value::Point(b)];
    math_nodes::run_item(kind, &[&values[0], &values[1]], &|| false)?
        .into_iter()
        .next()
        .ok_or_else(|| "expected one vector result".into())
}

#[test]
fn dot_product_keeps_small_terms_and_recovers_product_rounding_residuals() {
    let n = 1e9;
    let a = [n, 1.0, n];
    let b = [n, 1.0, -n];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        assert_eq!(
            run(
                NodeKind::DotProduct,
                order.map(|i| a[i]),
                order.map(|i| b[i])
            )
            .unwrap(),
            Value::Number(1.0),
            "n² + 1 - n² is exactly one for these representable integer inputs"
        );
    }
    assert_eq!(
        run(NodeKind::DotProduct, a, b.map(|v| -v)).unwrap(),
        Value::Number(-1.0)
    );
    // Compensated addition alone cannot fix this: the two rounded products are
    // identical. Their FMA residuals recover n(n-2) - (n-1)² = -1.
    assert_eq!(
        run(
            NodeKind::DotProduct,
            [n, n - 1.0, 0.0],
            [n - 2.0, -(n - 1.0), 0.0]
        )
        .unwrap(),
        Value::Number(-1.0)
    );
}

#[test]
fn cross_product_keeps_the_exact_small_determinant_and_orientation() {
    let n = 1e9;
    let a = [n, n - 1.0, 0.0];
    let b = [n - 1.0, n - 2.0, 0.0];
    for (order, expected) in [
        ([0, 1, 2], [0., 0., -1.]),
        ([2, 0, 1], [-1., 0., 0.]),
        ([1, 2, 0], [0., -1., 0.]),
    ] {
        let left = order.map(|i| a[i]);
        let right = order.map(|i| b[i]);
        assert_eq!(
            run(NodeKind::CrossProduct, left, right).unwrap(),
            Value::Point(expected)
        );
        assert_eq!(
            run(NodeKind::CrossProduct, right, left).unwrap(),
            Value::Point(expected.map(|v| -v))
        );
    }
    assert_eq!(
        run(NodeKind::CrossProduct, a, a).unwrap(),
        Value::Point([0.; 3])
    );
}

#[test]
fn compensated_zero_results_preserve_existing_ieee_zero_signs() {
    let dot = run(NodeKind::DotProduct, [-0.0; 3], [1.0; 3])
        .unwrap()
        .number()
        .unwrap();
    assert_eq!(dot, 0.0);
    assert!(dot.is_sign_negative());
    let Value::Point(cross) =
        run(NodeKind::CrossProduct, [-0.0, 0.0, 0.0], [1.0, 1.0, 0.0]).unwrap()
    else {
        panic!("expected a vector");
    };
    assert_eq!(cross[2], 0.0);
    assert!(cross[2].is_sign_negative());
}

#[test]
fn representable_subnormal_products_survive_without_claiming_underflow_immunity() {
    let tiny = f64::from_bits(1);
    assert_eq!(
        run(NodeKind::DotProduct, [tiny, 0., 0.], [1., 0., 0.])
            .unwrap()
            .number()
            .unwrap()
            .to_bits(),
        1
    );
    assert_eq!(
        run(NodeKind::CrossProduct, [0., tiny, 0.], [0., 0., 1.]).unwrap(),
        Value::Point([tiny, 0., 0.])
    );
    // Half the least subnormal rounds to zero under ordinary ties-to-even.
    assert_eq!(
        run(NodeKind::DotProduct, [tiny, 0., 0.], [0.5, 0., 0.]).unwrap(),
        Value::Number(0.)
    );
}

#[test]
fn finite_bounds_and_cancellation_remain_in_force() {
    for kind in [NodeKind::DotProduct, NodeKind::CrossProduct] {
        for invalid in [
            [f64::NAN, 0., 0.],
            [f64::INFINITY, 0., 0.],
            [1e9 + 1., 0., 0.],
        ] {
            assert!(run(kind, invalid, [1., 0., 0.]).is_err());
        }
        let a = Value::Point([1., 0., 0.]);
        let b = Value::Point([0., 1., 0.]);
        assert!(math_nodes::run_item(kind, &[&a, &b], &|| true).is_err());
    }
    // Scalar dot values can exceed the point-coordinate limit; cross vectors
    // continue to obey the existing 1e9 coordinate bound after calculation.
    assert_eq!(
        run(NodeKind::DotProduct, [1e9, 0., 0.], [1e9, 0., 0.]).unwrap(),
        Value::Number(1e18)
    );
    assert!(run(NodeKind::CrossProduct, [1e9, 0., 0.], [0., 1e9, 0.]).is_err());
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
fn real_vector_nodes_preserve_grafted_paths_and_single_branch_broadcasting() {
    let n = 1e9;
    for (kind, left, right, expected) in [
        (
            NodeKind::DotProduct,
            [n, 1., n],
            [n, 1., -n],
            [Value::Number(1.), Value::Number(-1.)],
        ),
        (
            NodeKind::CrossProduct,
            [n, n - 1., 0.],
            [n - 1., n - 2., 0.],
            [Value::Point([0., 0., -1.]), Value::Point([0., 0., 1.])],
        ),
    ] {
        let mut graph = Graph::default();
        let a = vector(&mut graph, left);
        let b = vector(&mut graph, right);
        let negative = graph.add(NodeKind::ReverseVector, [0.; 2]).unwrap();
        graph.connect(a, negative, "vector").unwrap();
        let list = graph.add(NodeKind::ConcatList, [0.; 2]).unwrap();
        graph.connect(a, list, "a").unwrap();
        graph.connect(negative, list, "b").unwrap();
        let graft = graph.add(NodeKind::Graft, [0.; 2]).unwrap();
        graph.connect(list, graft, "geometry").unwrap();
        let product = graph.add(kind, [0.; 2]).unwrap();
        graph.connect(graft, product, "a").unwrap();
        graph.connect(b, product, "b").unwrap();
        graph.validate().unwrap();
        let mut evaluator = Evaluator::default();
        let first = evaluator.evaluate(&graph, &|| false);
        let tree = first.outputs[&product].as_ref().unwrap();
        assert_eq!(tree.branches.len(), 2);
        for (branch, expected) in expected.into_iter().enumerate() {
            assert_eq!(tree.branches[&vec![0, branch as u32]], vec![expected]);
        }
        let warm = evaluator.evaluate(&graph, &|| false);
        assert_eq!(warm.outputs, first.outputs);
        assert_eq!(warm.cached, graph.nodes.len());
        assert_eq!(warm.computed, 0);
    }
}
