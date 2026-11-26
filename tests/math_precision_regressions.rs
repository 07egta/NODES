//! Independent numeric answers for representable spreads and extreme products.
use morphlet::{
    math_nodes,
    model::{NodeKind, Value},
};

fn aggregate(kind: NodeKind, input: &[f64]) -> Result<f64, String> {
    let values: Vec<Value> = input.iter().copied().map(Value::Number).collect();
    let output = math_nodes::run_list(kind, &[&values], &|| false)?;
    output.first().ok_or("Expected one aggregate")?.number()
}
fn relative(actual: f64, expected: f64, tolerance: f64) {
    assert!(actual.is_finite());
    if expected == 0.0 {
        assert_eq!(actual, expected);
    } else {
        assert!(
            (actual / expected - 1.0).abs() <= tolerance,
            "{actual:e} != {expected:e}"
        );
    }
}

#[test]
fn population_variance_retains_small_spread_after_large_translation() {
    // For two numbers separated by 2, population variance is exactly 1.
    for input in [[0.0, 2.0], [1e16, 1e16 + 2.0], [-1e16, -1e16 + 2.0]] {
        relative(aggregate(NodeKind::Variance, &input).unwrap(), 1.0, 2e-15);
        relative(
            aggregate(NodeKind::StdDeviation, &input).unwrap(),
            1.0,
            2e-15,
        );
    }
    relative(
        aggregate(NodeKind::Variance, &[1e12, 1e12 + 1.0]).unwrap(),
        0.25,
        2e-15,
    );
    // Invariance under an exactly representable binary translation.
    let shifted = [-2.0, -1.0, 0.0, 1.0, 2.0].map(|x| x + 2.0_f64.powi(40));
    relative(aggregate(NodeKind::Variance, &shifted).unwrap(), 2.0, 2e-15);
    assert_eq!(aggregate(NodeKind::Variance, &[1e308; 3]).unwrap(), 0.0);
}

#[test]
fn population_spread_handles_opposite_extrema_without_intermediate_overflow() {
    relative(
        aggregate(NodeKind::StdDeviation, &[-f64::MAX, f64::MAX]).unwrap(),
        f64::MAX,
        2e-15,
    );
    relative(
        aggregate(NodeKind::Variance, &[-1e154, 1e154]).unwrap(),
        1e308,
        2e-15,
    );
    relative(
        aggregate(NodeKind::StdDeviation, &[-1e200, 1e200]).unwrap(),
        1e200,
        2e-15,
    );
    assert!(aggregate(NodeKind::Variance, &[-1e200, 1e200]).is_err());
    assert!(aggregate(NodeKind::Variance, &[f64::NAN]).is_err());
    assert!(aggregate(NodeKind::StdDeviation, &[f64::INFINITY]).is_err());
}

#[test]
fn products_recover_from_extreme_intermediate_exponents_in_any_order() {
    for (a, b, expected) in [(1e-200, 1e200, 1e-200), (1e200, 1e-200, 1e200)] {
        for input in [[a, a, b], [a, b, a], [b, a, a]] {
            relative(
                aggregate(NodeKind::Product, &input).unwrap(),
                expected,
                2e-15,
            );
        }
    }
    // Powers of two have an exact mantissa and an exact expected product.
    let large = 2.0_f64.powi(1000);
    let small = 2.0_f64.powi(-1000);
    assert_eq!(
        aggregate(NodeKind::Product, &[large, large, small, small]).unwrap(),
        1.0
    );
    assert_eq!(
        aggregate(NodeKind::Product, &[-large, small]).unwrap(),
        -1.0
    );
    assert_eq!(aggregate(NodeKind::Product, &[]).unwrap(), 1.0);
}

#[test]
fn products_preserve_subnormal_inputs_and_outputs() {
    let tiny = f64::from_bits(1);
    assert_eq!(aggregate(NodeKind::Product, &[tiny]).unwrap().to_bits(), 1);
    assert_eq!(
        aggregate(NodeKind::Product, &[tiny, 2.0])
            .unwrap()
            .to_bits(),
        2
    );
    assert_eq!(
        aggregate(NodeKind::Product, &[tiny, 1.5])
            .unwrap()
            .to_bits(),
        2
    );
    assert_eq!(
        aggregate(NodeKind::Product, &[tiny, 0.75])
            .unwrap()
            .to_bits(),
        1
    );
    assert_eq!(
        aggregate(NodeKind::Product, &[f64::MIN_POSITIVE, 0.5])
            .unwrap()
            .to_bits(),
        1_u64 << 51
    );
    assert_eq!(
        aggregate(NodeKind::Product, &[f64::MAX, 1.0]).unwrap(),
        f64::MAX
    );
    // An intermediate below all f64 values can return to a normal result later.
    relative(
        aggregate(NodeKind::Product, &[tiny, tiny, f64::MAX, f64::MAX]).unwrap(),
        2.0_f64.powi(-100) * (1.0 - f64::EPSILON),
        3e-15,
    );
}

#[test]
fn products_report_true_range_loss_but_zero_factors_are_exact() {
    let tiny = f64::from_bits(1);
    assert!(aggregate(NodeKind::Product, &[f64::MAX, 2.0]).is_err());
    assert!(aggregate(NodeKind::Product, &[tiny, 0.5]).is_err());
    assert!(aggregate(NodeKind::Product, &[tiny, tiny]).is_err());
    assert_eq!(
        aggregate(NodeKind::Product, &[f64::MAX, f64::MAX, 0.0]).unwrap(),
        0.0
    );
    assert_eq!(
        aggregate(NodeKind::Product, &[tiny, tiny, 0.0]).unwrap(),
        0.0
    );
    assert!(
        aggregate(NodeKind::Product, &[-0.0, 2.0])
            .unwrap()
            .is_sign_negative()
    );
    assert!(aggregate(NodeKind::Product, &[0.0, f64::NAN]).is_err());
    assert!(aggregate(NodeKind::Product, &[f64::INFINITY, 0.0]).is_err());
}

#[test]
fn precision_aggregations_are_cancellable_and_keep_existing_empty_semantics() {
    for kind in [
        NodeKind::Product,
        NodeKind::Variance,
        NodeKind::StdDeviation,
    ] {
        let values = [Value::Number(1.0), Value::Number(2.0)];
        assert!(math_nodes::run_list(kind, &[&values], &|| true).is_err());
    }
    for kind in [NodeKind::Variance, NodeKind::StdDeviation] {
        assert!(
            math_nodes::run_list(kind, &[&[]], &|| false)
                .unwrap()
                .is_empty()
        );
    }
}
