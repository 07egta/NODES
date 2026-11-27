//! Script interpolation uses the same finite endpoint behavior as math nodes.
use nodes::model::{Tree, Value};
use nodes::script::{ScriptInputs, run_script};

fn run(source: &str) -> Result<Tree, String> {
    run_script(source, &ScriptInputs::default(), &|| false)
}

fn numbers(tree: &Tree) -> Vec<f64> {
    tree.branches
        .values()
        .flatten()
        .map(|v| match v {
            Value::Number(n) => *n,
            other => panic!("expected number, got {other:?}"),
        })
        .collect()
}

#[test]
fn script_lerp_handles_opposite_extrema_without_intermediate_overflow() {
    for source in ["lerp(-1e308, 1e308, 0.5)", "lerp(1e308, -1e308, 0.5)"] {
        assert_eq!(numbers(&run(source).unwrap()), vec![0.]);
    }
    assert_eq!(
        numbers(&run("lerp(-1e308, 1e308, 0.25)").unwrap()),
        vec![-5e307]
    );
}

#[test]
fn script_lerp_preserves_extreme_endpoints_and_single_item_ranges() {
    for (source, expected) in [
        ("lerp(-1e308, 1e308, 0)", -1e308),
        ("lerp(-1e308, 1e308, 1)", 1e308),
        ("linspace(-1e308, 1e308, 1)", -1e308),
    ] {
        assert_eq!(numbers(&run(source).unwrap()), vec![expected]);
    }
    assert_eq!(
        numbers(&run("lerp(-0.0, 1.0, 0)").unwrap())[0].to_bits(),
        (-0.0_f64).to_bits()
    );
}

#[test]
fn script_linspace_spans_extreme_finite_values_in_both_directions() {
    let expected = [-1e308, -5e307, 0., 5e307, 1e308];
    for (source, reverse) in [
        ("linspace(-1e308, 1e308, 5)", false),
        ("linspace(1e308, -1e308, 5)", true),
    ] {
        let mut actual = numbers(&run(source).unwrap());
        if reverse {
            actual.reverse();
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn script_interpolation_still_rejects_true_overflow_and_excessive_counts() {
    assert!(run("lerp(1e308, -1e308, 2)").is_err());
    assert!(run("linspace(0, 1, 1000000)").is_err());
    assert!(run("linspace(0, 1, 0)").is_err());
    assert_eq!(numbers(&run("lerp(10, 20, 0.25)").unwrap()), vec![12.5]);
}

#[test]
fn shared_lerp_keeps_finite_extrapolation_and_tiny_constants() {
    for (a, b, t, expected) in [
        (1e308, 1e308, 2., 1e308),
        (1e308, 1.0000000000000002e308, 2., 1.0000000000000004e308),
        (1e308, 7e307, 5., -5e307),
        (f64::from_bits(1), f64::from_bits(1), 0.5, f64::from_bits(1)),
        (
            -f64::from_bits(1),
            -f64::from_bits(1),
            0.5,
            -f64::from_bits(1),
        ),
        (-0., -0., 0.5, -0.),
    ] {
        let inputs = ScriptInputs {
            numbers: [vec![a], vec![b], vec![t], vec![]],
            geometry: None,
        };
        let result = run_script("lerp(a, b, c)", &inputs, &|| false).unwrap();
        let actual = numbers(&result)[0];
        if expected == 0. || expected.abs() < f64::MIN_POSITIVE {
            assert_eq!(actual.to_bits(), expected.to_bits());
        } else {
            assert!((actual - expected).abs() <= expected.abs() * 4e-16);
        }
        // The node and Script paths must use the same numeric contract.
        let node_result = nodes::math_nodes::run_item(
            nodes::model::NodeKind::Lerp,
            &[&Value::Number(a), &Value::Number(b), &Value::Number(t)],
            &|| false,
        )
        .unwrap();
        assert_eq!(node_result.len(), 1);
        assert_eq!(node_result[0].number().unwrap().to_bits(), actual.to_bits());
    }
}

#[test]
fn shared_lerp_retains_the_low_part_of_rounded_endpoint_differences() {
    let before_one = f64::from_bits(1.0_f64.to_bits() - 1);
    let after_one = f64::from_bits(1.0_f64.to_bits() + 1);
    // Independent exact-rational answers, rounded once. In every case b-a
    // loses a small endpoint, which matters near t=1 even with an FMA.
    for (a, b, t, expected) in [
        (-1., 2.0_f64.powi(-53), before_one, -2.0_f64.powi(-106)),
        (1., 2.0_f64.powi(-54), before_one, 3. * 2.0_f64.powi(-54)),
        (2.0_f64.powi(54), 1., after_one, -3.),
    ] {
        let inputs = ScriptInputs {
            numbers: [vec![a], vec![b], vec![t], vec![]],
            geometry: None,
        };
        let scripted = run_script("lerp(a, b, c)", &inputs, &|| false).unwrap();
        let node = nodes::math_nodes::run_item(
            nodes::model::NodeKind::Lerp,
            &[&Value::Number(a), &Value::Number(b), &Value::Number(t)],
            &|| false,
        )
        .unwrap();
        assert_eq!(numbers(&scripted)[0].to_bits(), expected.to_bits());
        assert_eq!(node[0].number().unwrap().to_bits(), expected.to_bits());
    }
}
