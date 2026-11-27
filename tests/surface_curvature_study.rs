//! Analytical graph-level checks for the newly recreated curvature study.
//! This sample is not claimed to reproduce the unavailable local JSON bytewise.
use nodes::engine::Evaluator;
use nodes::persistence;
use std::f64::consts::TAU;

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn torus_study_has_nine_curvature_branches_and_roundtrips() {
    let mut project =
        persistence::decode_project(include_bytes!("../examples/surface-curvature-study.morph"))
            .unwrap();
    project.validate().unwrap();
    let encoded = serde_json::to_vec(&project).unwrap();
    let reread = persistence::decode_project(&encoded).unwrap();
    assert_eq!(reread, project);
    let mut evaluator = Evaluator::default();
    let evaluation = evaluator.evaluate(&project.graph, &|| false);
    for (id, value) in &evaluation.outputs {
        assert!(value.is_ok(), "node {id}: {value:?}");
    }
    for id in [2_u64, 3, 4, 5, 6, 7, 8] {
        assert_eq!(
            evaluation.outputs[&id].as_ref().unwrap().branches.len(),
            9,
            "node {id}"
        );
    }
    let gaussian = evaluation.outputs[&4].as_ref().unwrap();
    let mean = evaluation.outputs[&5].as_ref().unwrap();
    let principal = evaluation.outputs[&6].as_ref().unwrap();
    let (major, minor) = (35., 8.);
    for (i, (path, values)) in gaussian.branches.iter().enumerate() {
        assert_eq!(values.len(), 1);
        let cosine = (TAU * i as f64 / 8.).cos();
        let radial = major + minor * cosine;
        near(values[0].number().unwrap(), cosine / (minor * radial));
        near(
            mean.branches[path][0].number().unwrap(),
            -(major + 2. * minor * cosine) / (2. * minor * radial),
        );
        let values = &principal.branches[path];
        assert_eq!(values.len(), 2);
        let mut expected = [-1. / minor, -cosine / radial];
        expected.sort_by(f64::total_cmp);
        near(values[0].number().unwrap(), expected[0]);
        near(values[1].number().unwrap(), expected[1]);
    }
    assert_eq!(evaluator.evaluate(&project.graph, &|| false).computed, 0);
    project
        .graph
        .node_mut(0)
        .unwrap()
        .params
        .insert("major".into(), 42.);
    let changed = evaluator.evaluate(&project.graph, &|| false);
    assert!(changed.computed > 0);
    let first = changed.outputs[&4]
        .as_ref()
        .unwrap()
        .branches
        .values()
        .next()
        .unwrap();
    near(first[0].number().unwrap(), 1. / (8. * (42. + 8.)));
}
