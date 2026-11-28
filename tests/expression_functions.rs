//! Native Expression regressions: function domains, arity, precision and graph usage.
use nodes::engine::Evaluator;
use nodes::expr::{Expr, Func, parse};
use nodes::model::Value;
use nodes::persistence::decode_project;

fn eval(source: &str) -> f64 {
    parse(source).unwrap().eval([0.; 4]).unwrap()
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.),
        "{actual} != {expected}"
    );
}

#[test]
fn inverse_trigonometry_and_hyperbolic_functions() {
    near(eval("asin(1)"), std::f64::consts::FRAC_PI_2);
    near(eval("acos(-1)"), std::f64::consts::PI);
    near(eval("atan(1)"), std::f64::consts::FRAC_PI_4);
    near(eval("atan2(1,-1)"), 3. * std::f64::consts::FRAC_PI_4);
    near(eval("atan2(-1,-1)"), -3. * std::f64::consts::FRAC_PI_4);
    assert_eq!(
        eval("atan2(-0,-1)").to_bits(),
        (-std::f64::consts::PI).to_bits()
    );
    near(eval("sinh(ln(2))"), 0.75);
    near(eval("cosh(ln(2))"), 1.25);
    near(eval("tanh(ln(2))"), 0.6);
}

#[test]
fn roots_logs_and_stable_small_values() {
    near(eval("cbrt(-27)"), -3.);
    near(eval("hypot(3,4)"), 5.);
    near(eval("hypot(3e200,4e200)"), 5e200);
    near(eval("log2(1024)"), 10.);
    near(eval("log10(1000)"), 3.);
    assert!((eval("log1p(1e-16)") / 1e-16 - 1.).abs() < 1e-14);
    assert!((eval("expm1(1e-16)") / 1e-16 - 1.).abs() < 1e-14);
}

#[test]
fn interpolation_handles_extreme_finite_inputs() {
    assert_eq!(eval("clamp(2,0,1)"), 1.);
    assert_eq!(eval("clamp(-2,0,1)"), 0.);
    assert_eq!(eval("clamp(2,1,1)"), 1.);
    assert_eq!(eval("lerp(-1e308,1e308,0.5)"), 0.);
    assert_eq!(eval("lerp(2,4,1.5)"), 5.);
    assert_eq!(eval("lerp(-0,1,0)").to_bits(), (-0.0_f64).to_bits());
    assert_eq!(eval("smoothstep(0,-1e308,1e308)"), 0.5);
    assert_eq!(eval("smoothstep(0.25,0,1)"), 0.15625);
    assert_eq!(eval("smoothstep(-1,0,1)"), 0.);
    assert_eq!(eval("smoothstep(2,0,1)"), 1.);
    near(eval("lerp(20,45,smoothstep(0.5,0,1))"), 32.5);
}

#[test]
fn rejects_wrong_arity_domains_and_nonfinite_values() {
    for source in [
        "clamp(1,2)",
        "clamp(1,2,3,4)",
        "atan2(1)",
        "log2(1,2)",
        "lerp(1,2,)",
        "smoothstep(1,2,3,4)",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
    for source in [
        "asin(2)",
        "acos(-2)",
        "log10(0)",
        "log2(-1)",
        "log1p(-1)",
        "expm1(1000)",
        "clamp(0,1,-1)",
        "smoothstep(0,1,1)",
        "smoothstep(0,1,-1)",
        "hypot(1.7e308,1.7e308)",
        "lerp(-1e308,1e308,2)",
    ] {
        assert!(parse(source).unwrap().eval([0.; 4]).is_err(), "{source}");
    }
    let invalid = [f64::NAN, 0., 0., 0.];
    for source in ["x", "-x", "min(x,1)", "clamp(x,0,1)", "lerp(x,1,1)"] {
        assert!(parse(source).unwrap().eval(invalid).is_err(), "{source}");
    }
    assert!(Expr::Num(f64::INFINITY).eval([0.; 4]).is_err());
    assert!(Expr::Var(4).eval([0.; 4]).is_err());
    assert!(Expr::Call(Func::Clamp, vec![]).eval([0.; 4]).is_err());
}

#[test]
fn legacy_expression_precedence_and_japanese_input() {
    near(eval("-2^2 + 2^3^2"), 508.);
    near(eval("ｍａｘ（２，３）×４"), 12.);
    near(eval("ＬＥＲＰ（２０，４５，０．５）"), 32.5);
    assert!(parse(&"(".repeat(30)).is_err());
}

#[test]
fn editable_vase_graph_evaluates_after_parameter_changes() {
    let mut project = decode_project(include_bytes!("../examples/math/smooth-vase-radius.morph"))
        .expect("valid example project");
    let mut evaluator = Evaluator::default();
    for (x, expected) in [(0., 20.), (0.5, 32.5), (1., 45.)] {
        project
            .graph
            .node_mut(3)
            .unwrap()
            .params
            .insert("x".into(), x);
        let evaluation = evaluator.evaluate(&project.graph, &|| false);
        assert!(evaluation.outputs.values().all(Result::is_ok));
        let tree = evaluation.outputs[&3].as_ref().unwrap();
        assert_eq!(tree.branches[&vec![0]][0], Value::Number(expected));
    }
    let bytes = serde_json::to_vec(&project).unwrap();
    let restored = decode_project(&bytes).unwrap();
    assert_eq!(restored.graph, project.graph);
}
