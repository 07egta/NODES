//! Analytic solutions and inverse identities across extreme row scales.
//! These are finite-precision tests, not an arbitrary-conditioning guarantee.
use morphlet::engine::Evaluator;
use morphlet::math_nodes;
use morphlet::model::{Graph, NodeKind, Value};
use std::cell::Cell;

fn solve_with(
    matrix: &[f64],
    rhs: &[f64],
    size: usize,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<f64>, String> {
    let matrix: Vec<_> = matrix.iter().copied().map(Value::Number).collect();
    let rhs: Vec<_> = rhs.iter().copied().map(Value::Number).collect();
    let size = [Value::Number(size as f64)];
    math_nodes::run_list(NodeKind::LinearSolve, &[&matrix, &rhs, &size], stop)?
        .iter()
        .map(Value::number)
        .collect()
}

fn solve(matrix: &[f64], rhs: &[f64], size: usize) -> Result<Vec<f64>, String> {
    solve_with(matrix, rhs, size, &|| false)
}

fn inverse(matrix: &[f64], size: usize) -> Result<Vec<f64>, String> {
    let matrix: Vec<_> = matrix.iter().copied().map(Value::Number).collect();
    let size = [Value::Number(size as f64)];
    math_nodes::run_list(NodeKind::MatrixInverse, &[&matrix, &size], &|| false)?
        .iter()
        .map(Value::number)
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!(actual.is_finite(), "nonfinite result {actual}");
    if expected == 0.0 {
        assert!(actual.abs() <= 2e-14, "{actual:e} != zero");
    } else {
        assert!(
            (actual / expected - 1.0).abs() <= 2e-14,
            "{actual:e} != {expected:e}"
        );
    }
}

#[test]
fn large_well_conditioned_rows_do_not_overflow_during_elimination() {
    // A is 1e308 times a 2x2 Hadamard matrix: its 2-norm condition number is one.
    // The former raw second-row subtraction formed -2e308 and rejected both
    // operations despite their finite, analytically known answers.
    let a = [1e308, 1e308, 1e308, -1e308];
    let solution = solve(&a, &[1e308, 0.0], 2).unwrap();
    assert_eq!(solution, vec![0.5, 0.5]);
    let inverted = inverse(&a, 2).unwrap();
    for (&actual, expected) in inverted.iter().zip([5e-309, 5e-309, 5e-309, -5e-309]) {
        close(actual, expected);
    }
    // Independent inverse identity, with each product about one half.
    for row in 0..2 {
        for column in 0..2 {
            let value = a[row * 2] * inverted[column] + a[row * 2 + 1] * inverted[2 + column];
            close(value, f64::from(row == column));
        }
    }
}

#[test]
fn row_scale_and_pivot_swaps_preserve_known_solutions() {
    // The unscaled system is [[4,7],[2,6]] * [2,-1] = [1,-2].
    for (left, right) in [(1.0, 1.0), (1e300, 1e-300), (1e-300, 1e300)] {
        let result = solve(
            &[4.0 * left, 7.0 * left, 2.0 * right, 6.0 * right],
            &[left, -2.0 * right],
            2,
        )
        .unwrap();
        close(result[0], 2.0);
        close(result[1], -1.0);
    }
    let swapped = solve(&[0.0, 1e308, 1e-308, 1e-308], &[1e308, 3e-308], 2).unwrap();
    close(swapped[0], 2.0);
    close(swapped[1], 1.0);

    let tiny = f64::from_bits(1);
    assert_eq!(
        solve(&[tiny, 0.0, 0.0, 2.0 * tiny], &[tiny, 4.0 * tiny], 2).unwrap(),
        vec![1.0, 2.0],
        "subnormal diagonal scales must not be multiplied by overflowing reciprocals"
    );
}

#[test]
fn scaling_neither_overflows_small_rows_nor_erases_meaningful_right_hand_sides() {
    // Unconditionally normalizing the second row would form 2/1e-308 = infinity.
    // Leaving it unchanged permits the second pivot 2e-308 and finite solution.
    let large = solve(&[1.0, -1.0, 1e-308, 1e-308], &[0.0, 2.0], 2).unwrap();
    close(large[0], 1e308);
    close(large[1], 1e308);

    // Normalizing the first row by 1e308 would erase its nonzero RHS, yet the
    // original pivot 2e296 produces the smallest representable positive answer.
    let tiny = solve(&[2e296, 1e308, 0.0, 1e308], &[1e-27, 0.0], 2).unwrap();
    assert_eq!(tiny[0].to_bits(), 1);
    assert_eq!(tiny[1], 0.0);

    // A nonzero normalized RHS is not sufficient: 1e-15/1e308 rounds to a
    // two-unit subnormal, changing the answer by about 1.2 percent. Retaining
    // this row's original scale avoids that precision regression.
    let subnormal = solve(&[2e296, 1e308, 0.0, 1e308], &[1e-15, 0.0], 2).unwrap();
    close(subnormal[0], 5e-312);
    assert_eq!(subnormal[1], 0.0);
}

#[test]
fn singular_unstable_and_unrepresentable_systems_remain_explicit_errors() {
    for matrix in [
        [1.0, 2.0, 2.0, 4.0],
        [1e308, 1e308, 1e-308, 1e-308],
        [0.0, 0.0, 1.0, 1.0],
    ] {
        assert!(solve(&matrix, &[1.0, 1.0], 2).is_err());
        assert!(inverse(&matrix, 2).is_err());
    }
    // Preserve the existing relative 1e-12 pivot rejection policy.
    assert!(solve(&[1.0, 1.0, 1.0, 1.0 + 1e-13], &[2.0, 2.0], 2).is_err());
    assert!(inverse(&[f64::from_bits(1)], 1).is_err());
    assert!(solve(&[f64::MIN_POSITIVE], &[f64::MAX], 1).is_err());
    assert!(solve(&[f64::NAN], &[1.0], 1).is_err());
    assert!(solve(&[1.0], &[f64::INFINITY], 1).is_err());
    assert!(solve(&[1.0, 2.0], &[1.0], 1).is_err());
}

#[test]
fn matrix_normalization_and_elimination_are_cooperatively_cancellable() {
    let size = 16;
    let mut matrix = vec![0.; size * size];
    for i in 0..size {
        matrix[i * size + i] = 1e300;
    }
    let rhs = vec![1e300; size];
    assert!(solve_with(&matrix, &rhs, size, &|| true).is_err());
    let calls = Cell::new(0);
    let result = solve_with(&matrix, &rhs, size, &|| {
        calls.set(calls.get() + 1);
        calls.get() == size + 3
    });
    assert!(result.is_err());
    assert_eq!(
        calls.get(),
        size + 3,
        "stop after normalization and the first pivot"
    );
    assert_eq!(solve(&matrix, &rhs, size).unwrap(), vec![1.; size]);
}

#[test]
fn full_graph_matrix_nodes_return_finite_results_and_reuse_their_cache() {
    let mut graph = Graph::default();
    let matrix = graph.add(NodeKind::Script, [0.; 2]).unwrap();
    graph.node_mut(matrix).unwrap().script = "[1.0e308, 1.0e308, 1.0e308, -1.0e308]".into();
    let rhs = graph.add(NodeKind::Script, [0., 100.]).unwrap();
    graph.node_mut(rhs).unwrap().script = "[1.0e308, 0.0]".into();
    let solution = graph.add(NodeKind::LinearSolve, [200., 0.]).unwrap();
    let inverted = graph.add(NodeKind::MatrixInverse, [200., 100.]).unwrap();
    graph.connect(matrix, solution, "matrix").unwrap();
    graph.connect(rhs, solution, "rhs").unwrap();
    graph.connect(matrix, inverted, "matrix").unwrap();
    graph.validate().unwrap();
    let mut evaluator = Evaluator::default();
    let evaluated = evaluator.evaluate(&graph, &|| false);
    for (id, expected) in [
        (solution, vec![0.5, 0.5]),
        (inverted, vec![5e-309, 5e-309, 5e-309, -5e-309]),
    ] {
        let output = evaluated.outputs[&id].as_ref().unwrap();
        assert_eq!(output.branches.len(), 1);
        let actual = &output.branches[&vec![0]];
        assert_eq!(actual.len(), expected.len());
        for (value, expected) in actual.iter().zip(expected) {
            close(value.number().unwrap(), expected);
        }
    }
    let cached = evaluator.evaluate(&graph, &|| false);
    assert_eq!(cached.outputs, evaluated.outputs);
    assert_eq!(cached.cached, 4);
    assert_eq!(cached.computed, 0);
}
