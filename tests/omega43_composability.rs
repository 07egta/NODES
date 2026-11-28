//! Cross-category regression: exact curves must not become display chords.
use nodes::{
    engine::Evaluator,
    model::{Graph, NodeKind as K, Value},
    persistence::{self, Project},
};

#[test]
fn exact_circle_array_and_attractor_are_preview_independent() {
    let mut g = Graph::default();
    let radius = g.add(K::Number, [0.; 2]).unwrap();
    let circle = g.add(K::Circle, [0.; 2]).unwrap();
    let point = g.add(K::Point, [0.; 2]).unwrap();
    let array = g.add(K::CurveArray, [0.; 2]).unwrap();
    let graft = g.add(K::Graft, [0.; 2]).unwrap();
    let flatten = g.add(K::Flatten, [0.; 2]).unwrap();
    let reroute = g.add(K::Reroute, [0.; 2]).unwrap();
    let attractor = g.add(K::CurveAttractor, [0.; 2]).unwrap();
    let abs = g.add(K::Abs, [0.; 2]).unwrap();
    let add = g.add(K::Add, [0.; 2]).unwrap();
    let length = g.add(K::ListLength, [0.; 2]).unwrap();
    g.node_mut(radius).unwrap().params.insert("value".into(), 10.);
    g.node_mut(circle).unwrap().params.insert("segments".into(), 8.);
    g.node_mut(array).unwrap().params.insert("count".into(), 13.);
    g.node_mut(add).unwrap().params.insert("b".into(), 1.);
    g.connect(radius, circle, "radius").unwrap();
    g.connect(point, array, "geometry").unwrap();
    g.connect(circle, array, "curve").unwrap();
    g.connect(array, graft, "geometry").unwrap();
    g.connect(graft, flatten, "geometry").unwrap();
    g.connect(flatten, reroute, "value").unwrap();
    g.connect(reroute, attractor, "point").unwrap();
    g.connect(circle, attractor, "curve").unwrap();
    g.connect(attractor, abs, K::Abs.spec().inputs[0].key).unwrap();
    g.connect(abs, add, "a").unwrap();
    g.connect(add, length, K::ListLength.spec().inputs[0].key).unwrap();

    let mut evaluator = Evaluator::default();
    for segments in [8., 64.] {
        g.node_mut(circle).unwrap().params.insert("segments".into(), segments);
        let project = Project::new("exact array", g.clone());
        let loaded = persistence::decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
        let result = evaluator.evaluate(&loaded.graph, &|| false);
        for (id, value) in &result.outputs {
            assert!(value.is_ok(), "node {id}: {value:?}");
        }
        let points = result.outputs[&array].as_ref().unwrap();
        assert_eq!(points.item_count(), 13);
        for value in points.branches.values().flatten() {
            let Value::Point(p) = value else {
                panic!("expected point");
            };
            assert!((p[0].hypot(p[1]) - 10.).abs() < 1e-8);
        }
        for value in result.outputs[&add].as_ref().unwrap().branches.values().flatten() {
            assert!((value.number().unwrap() - 1.).abs() < 1e-5);
        }
        assert_eq!(
            result.outputs[&length].as_ref().unwrap().branches.values().flatten().next(),
            Some(&Value::Number(13.))
        );
        let warm = evaluator.evaluate(&loaded.graph, &|| false);
        assert_eq!(warm.outputs, result.outputs);
        assert_eq!(warm.computed, 0);
    }
}
