//! Independently known binary64 sum/mean answers, rounded once to nearest/even.
use nodes::{
    math_nodes,
    model::{NodeKind, Value},
};

fn aggregate(kind: NodeKind, input: &[f64]) -> Result<f64, String> {
    let values: Vec<_> = input.iter().copied().map(Value::Number).collect();
    let result = math_nodes::run_list(kind, &[&values], &|| false)?;
    assert_eq!(result.len(), 1);
    result[0].number()
}

#[test]
fn cancellation_preserves_every_exponent_including_the_smallest_subnormal() {
    for tiny in [f64::from_bits(1), f64::MIN_POSITIVE, 1e-200, 1.0] {
        for input in [
            [f64::MAX, tiny, -f64::MAX],
            [tiny, f64::MAX, -f64::MAX],
            [-f64::MAX, f64::MAX, tiny],
        ] {
            assert_eq!(
                aggregate(NodeKind::Summation, &input).unwrap().to_bits(),
                tiny.to_bits()
            );
        }
    }
}

#[test]
fn mean_divides_the_exact_total_before_rounding() {
    let tiny = 1e-200;
    assert_eq!(
        aggregate(NodeKind::Average, &[f64::MAX, tiny, -f64::MAX]).unwrap(),
        tiny / 3.0
    );
    assert_eq!(
        aggregate(NodeKind::Average, &[f64::MAX, f64::MAX]).unwrap(),
        f64::MAX
    );
    assert_eq!(
        aggregate(NodeKind::Average, &[-f64::MAX, -f64::MAX]).unwrap(),
        -f64::MAX
    );
}

#[test]
fn subnormal_mean_ties_are_even_and_negative_underflow_keeps_its_sign() {
    let tiny = f64::from_bits(1);
    for (a, b, expected) in [(1, 2, 2), (2, 3, 2), (0, 1, 0), (0, 3, 2)] {
        assert_eq!(
            aggregate(NodeKind::Average, &[f64::from_bits(a), f64::from_bits(b)])
                .unwrap()
                .to_bits(),
            expected
        );
    }
    assert_eq!(
        aggregate(NodeKind::Average, &[-tiny, 0.0])
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        aggregate(NodeKind::Average, &[-f64::MAX, -tiny, f64::MAX])
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
}

#[test]
fn mean_rounds_normal_and_subnormal_boundaries_once() {
    let one = 1.0_f64.to_bits();
    assert_eq!(
        aggregate(NodeKind::Average, &[1.0, f64::from_bits(one + 1)])
            .unwrap()
            .to_bits(),
        one
    );
    assert_eq!(
        aggregate(
            NodeKind::Average,
            &[f64::from_bits(one + 1), f64::from_bits(one + 2)]
        )
        .unwrap()
        .to_bits(),
        one + 2
    );
    let min = f64::MIN_POSITIVE.to_bits();
    assert_eq!(
        aggregate(
            NodeKind::Average,
            &[f64::from_bits(min - 1), f64::MIN_POSITIVE]
        )
        .unwrap()
        .to_bits(),
        min
    );
}

#[test]
fn final_overflow_is_decided_at_the_rounding_boundary() {
    let half = 2.0_f64.powi(970);
    let tiny = f64::from_bits(1);
    for input in [
        [f64::MAX, half, -tiny],
        [half, -tiny, f64::MAX],
        [-tiny, f64::MAX, half],
    ] {
        assert_eq!(aggregate(NodeKind::Summation, &input).unwrap(), f64::MAX);
    }
    assert!(aggregate(NodeKind::Summation, &[f64::MAX, half]).is_err());
    assert!(aggregate(NodeKind::Summation, &[f64::MAX, half, tiny]).is_err());
    assert!(aggregate(NodeKind::Summation, &[-f64::MAX, -half]).is_err());
    assert_eq!(
        aggregate(NodeKind::Summation, &[f64::MAX, half * 0.5]).unwrap(),
        f64::MAX
    );
}

#[test]
fn cancellation_zero_empty_invalid_and_cancelled_contracts_are_preserved() {
    assert_eq!(
        aggregate(NodeKind::Summation, &[f64::MAX, -f64::MAX, -0.0])
            .unwrap()
            .to_bits(),
        0
    );
    assert_eq!(aggregate(NodeKind::Summation, &[]).unwrap(), 0.0);
    assert!(
        math_nodes::run_list(NodeKind::Average, &[&[]], &|| false)
            .unwrap()
            .is_empty()
    );
    for kind in [NodeKind::Summation, NodeKind::Average] {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(aggregate(kind, &[invalid]).is_err());
        }
        assert!(math_nodes::run_list(kind, &[&[Value::Number(1.0)]], &|| true).is_err());
    }
}

#[test]
fn mean_remainder_resolves_normal_rounding_ties() {
    let one = 1.0_f64.to_bits();
    let tiny = f64::from_bits(1);
    for (third, tail, expected) in [
        (one + 1, tiny, 0x3fe8_0000_0000_0001),
        (one + 1, 0.0, 0x3fe8_0000_0000_0000),
        (one + 1, -tiny, 0x3fe8_0000_0000_0000),
        (one + 3, -tiny, 0x3fe8_0000_0000_0001),
        (one + 3, 0.0, 0x3fe8_0000_0000_0002),
        (one + 3, tiny, 0x3fe8_0000_0000_0002),
    ] {
        assert_eq!(
            aggregate(NodeKind::Average, &[1.0, 1.0, f64::from_bits(third), tail])
                .unwrap()
                .to_bits(),
            expected
        );
    }
}

#[test]
fn guard_and_sticky_bits_cross_limb_boundaries() {
    let tiny = f64::from_bits(1);
    for shift in [1, 63, 64, 65, 127, 128, 129, 2044, 2045] {
        let base_bits = (shift + 1) << 52;
        let base = f64::from_bits(base_bits);
        // Half an ULP of base, constructed without an intermediate underflow.
        let half = if shift <= 52 {
            f64::from_bits(1 << (shift - 1))
        } else {
            f64::from_bits((shift - 52) << 52)
        };
        for (tail, expected) in [(-tiny, base_bits), (0.0, base_bits), (tiny, base_bits + 1)] {
            assert_eq!(
                aggregate(NodeKind::Summation, &[base, half, tail])
                    .unwrap()
                    .to_bits(),
                expected,
                "positive shift={shift}, tail={tail:e}"
            );
            assert_eq!(
                aggregate(NodeKind::Summation, &[-base, -half, -tail])
                    .unwrap()
                    .to_bits(),
                expected | (1 << 63),
                "negative shift={shift}, tail={tail:e}"
            );
        }
    }
}

#[test]
fn exact_fraction_oracle_matches_sum_and_mean_in_both_orders() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/sum_oracle.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 128);
    for case in cases {
        let mut values: Vec<_> = case["input_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bits| f64::from_bits(u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap()))
            .collect();
        for order in ["original", "reversed"] {
            for (kind, key) in [
                (NodeKind::Summation, "sum_bits"),
                (NodeKind::Average, "mean_bits"),
            ] {
                let actual = aggregate(kind, &values);
                if let Some(expected) = case[key].as_str() {
                    assert_eq!(
                        actual.unwrap().to_bits(),
                        u64::from_str_radix(expected, 16).unwrap(),
                        "case={} operation={key} order={order}",
                        case["id"]
                    );
                } else {
                    assert!(actual.is_err(), "expected overflow: {} {key}", case["id"]);
                }
            }
            values.reverse();
        }
    }
}
