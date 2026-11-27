//! Independent Fraction-generated answers through both actual public paths.
use morphlet::{
    math_nodes,
    model::{NodeKind, Value},
    script::{ScriptInputs, run_script},
};

fn read_word(text: &serde_json::Value) -> u64 {
    u64::from_str_radix(text.as_str().unwrap(), 16).unwrap()
}

fn inputs(case: &serde_json::Value) -> [f64; 3] {
    let words = case["input_bits"].as_array().unwrap();
    assert_eq!(words.len(), 3);
    std::array::from_fn(|i| f64::from_bits(read_word(&words[i])))
}

fn check_result(case: &serde_json::Value, path: &str, actual: Result<f64, String>) {
    let id = case["id"].as_str().unwrap();
    if case["expected_bits"].is_null() {
        assert!(
            actual.is_err(),
            "{id}: {path} expected overflow, got {actual:?}"
        );
    } else {
        let expected = read_word(&case["expected_bits"]);
        let actual = actual.unwrap_or_else(|error| panic!("{id}: {path}: {error}"));
        assert_eq!(
            actual.to_bits(),
            expected,
            "{id}: {path}: actual {actual:e}, expected {:e}",
            f64::from_bits(expected)
        );
    }
}

#[test]
fn actual_lerp_node_matches_independent_exact_interpolation_oracle() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/interpolation_oracle.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 128);
    for case in cases {
        let values = inputs(case).map(Value::Number);
        let args = values.each_ref();
        let result = math_nodes::run_item(NodeKind::Lerp, &args, &|| false).and_then(|out| {
            assert_eq!(out.len(), 1, "{}: node output count", case["id"]);
            out[0].number()
        });
        check_result(case, "math_nodes::run_item", result);
    }
}

#[test]
fn actual_script_lerp_matches_independent_exact_interpolation_oracle() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/interpolation_oracle.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 128);
    for case in cases {
        let [a, b, t] = inputs(case);
        let inputs = ScriptInputs {
            numbers: [vec![a], vec![b], vec![t], vec![]],
            geometry: None,
        };
        let result = run_script("lerp(a, b, c)", &inputs, &|| false).and_then(|tree| {
            assert_eq!(tree.item_count(), 1, "{}: script output count", case["id"]);
            tree.branches.values().flatten().next().unwrap().number()
        });
        check_result(case, "Script lerp(a,b,c)", result);
    }
}
