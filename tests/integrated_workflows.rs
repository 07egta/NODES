//! Cross-mission regressions: persisted ports, exact geometry and editable examples.
use morphlet::{engine::Evaluator, model::{Graph, NodeKind as K, Value}, persistence::{self, Project}};
use std::path::{Path, PathBuf};

fn closest_graph() -> (Graph, u64, u64, u64) {
    let mut g = Graph::default();
    let a = g.add(K::Point, [0.; 2]).unwrap();
    let q = g.add(K::Point, [0.; 2]).unwrap();
    g.node_mut(a).unwrap().params.insert("x".into(), 4.);
    g.node_mut(q).unwrap().params.insert("x".into(), 5.);
    let c = g.add(K::ClosestPoint, [0.; 2]).unwrap();
    g.connect(a, c, "points").unwrap();
    g.connect(q, c, "targets").unwrap();
    (g, a, q, c)
}

#[test]
fn closest_point_mission_aliases_connect_and_disconnect_canonically() {
    let (mut graph, a, q, c) = closest_graph();
    assert_eq!(graph.node(c).unwrap().inputs, [("candidates".into(), a), ("queries".into(), q)].into());
    let result = Evaluator::default().evaluate(&graph, &|| false);
    let values: Vec<_> = result.outputs[&c].as_ref().unwrap().branches.values().flatten().collect();
    assert_eq!(values, vec![&Value::Point([4., 0., 0.])]);
    graph.disconnect(c, "points");
    assert!(!graph.node(c).unwrap().inputs.contains_key("candidates"));
}

#[test]
fn saved_generative_closest_point_ports_migrate_without_losing_wires() {
    let (graph, _, _, c) = closest_graph();
    let expected = Project::new("legacy ports", graph);
    let mut json = serde_json::to_value(&expected).unwrap();
    let node = json["graph"]["nodes"].as_array_mut().unwrap().iter_mut()
        .find(|n| n["id"] == c).unwrap();
    let inputs = node["inputs"].as_object_mut().unwrap();
    let a = inputs.remove("candidates").unwrap();
    let q = inputs.remove("queries").unwrap();
    inputs.insert("points".into(), a);
    inputs.insert("targets".into(), q);
    let decoded = persistence::decode_project(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(decoded, expected);
    assert_eq!(persistence::decode_project(&serde_json::to_vec(&decoded).unwrap()).unwrap(), expected);
}

#[test]
fn conflicting_legacy_connections_are_rejected() {
    let (mut graph, _, q, c) = closest_graph();
    graph.node_mut(c).unwrap().inputs.insert("points".into(), q);
    let bytes = serde_json::to_vec(&Project::new("conflict", graph)).unwrap();
    let error = persistence::decode_project(&bytes).unwrap_err();
    assert!(error.contains("競合"), "{error}");
}

#[test]
fn affine_generated_curves_keep_exact_geometry_and_length() {
    for segments in [8., 64.] {
        let mut graph = Graph::default();
        let circle = graph.add(K::Circle, [0.; 2]).unwrap();
        graph.node_mut(circle).unwrap().params.extend([
            ("radius".into(), 3.), ("segments".into(), segments),
        ]);
        let center = graph.add(K::Point, [0.; 2]).unwrap();
        let rotate = graph.add(K::Rotate3d, [0.; 2]).unwrap();
        graph.connect(circle, rotate, "geometry").unwrap();
        graph.connect(center, rotate, "center").unwrap();
        graph.node_mut(rotate).unwrap().params.insert("angle".into(), 37.);
        let shear = graph.add(K::Shear, [0.; 2]).unwrap();
        graph.connect(rotate, shear, "geometry").unwrap();
        let array = graph.add(K::GridArray, [0.; 2]).unwrap();
        graph.connect(shear, array, "geometry").unwrap();
        graph.node_mut(array).unwrap().params.extend([("nx".into(), 2.), ("ny".into(), 2.)]);
        let length = graph.add(K::CurveLength, [0.; 2]).unwrap();
        graph.connect(array, length, "curve").unwrap();
        let result = Evaluator::default().evaluate(&graph, &|| false);
        for value in result.outputs[&array].as_ref().unwrap().branches.values().flatten() {
            let Value::Curve(curve) = value else { panic!("curve required") };
            assert!(curve.geometry.is_some());
        }
        let lengths: Vec<_> = result.outputs[&length].as_ref().unwrap().branches.values().flatten().collect();
        assert_eq!(lengths.len(), 4);
        for v in lengths {
            assert!((v.number().unwrap() - 3. * std::f64::consts::TAU).abs() < 1e-8);
        }
    }
}

fn examples(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { examples(&path, out); }
        else if path.extension().is_some_and(|e| e == "morph") { out.push(path); }
    }
}

#[test]
fn every_editable_example_roundtrips_evaluates_and_uses_the_cache() {
    let mut files = Vec::new();
    examples(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples"), &mut files);
    files.sort();
    assert!(files.len() >= 20);
    for path in files {
        let project = persistence::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let decoded = persistence::decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
        assert_eq!(decoded, project, "{}", path.display());
        let mut evaluator = Evaluator::default();
        let cold = evaluator.evaluate(&decoded.graph, &|| false);
        assert!(!cold.cancelled);
        assert_eq!(cold.outputs.len(), decoded.graph.nodes.len());
        for (id, result) in &cold.outputs {
            assert!(result.is_ok(), "{} node {id}: {result:?}", path.display());
        }
        let warm = evaluator.evaluate(&decoded.graph, &|| false);
        assert_eq!(warm.outputs, cold.outputs, "{}", path.display());
        assert_eq!(warm.computed, 0, "{}", path.display());
    }
}

#[test]
fn api_catalog_exposes_pure_integrated_nodes_with_access_and_keeps_execution_restricted() {
    let catalog = morphlet::graph_api::catalog();
    for kind in [K::Variance, K::CurveNurbs, K::SurfaceExtrude, K::SurfaceGaussianCurvature, K::MeshCatmullClark, K::Delaunay2d, K::PartitionList] {
        let item = catalog.nodes.iter().find(|n| n.kind == kind).unwrap();
        assert!(item.inputs.iter().all(|p| !p.access.is_empty()));
    }
    for kind in [K::Script, K::ImportMesh, K::Cluster, K::Expression, K::PathMapper] {
        assert!(!morphlet::graph_api::permitted(kind));
    }
}

#[test]
fn integer_inputs_near_an_integer_do_not_silently_truncate() {
    let mut graph = Graph::default();
    let id = graph.add(K::GhRange, [0.; 2]).unwrap();
    let port = K::GhRange.spec().inputs.iter().find(|p| p.integer).unwrap();
    graph.node_mut(id).unwrap().params.insert(port.key.into(), 1.999999999);
    let e = Evaluator::default().evaluate(&graph, &|| false);
    assert!(e.outputs[&id].is_err());
}

#[test]
fn obsolete_list_matching_option_migrates_to_the_shared_ordinal_default() {
    let mut graph = Graph::default();
    let source = graph.add(K::Series, [0.; 2]).unwrap();
    let sum = graph.add(K::Summation, [0.; 2]).unwrap();
    let key = K::Summation.spec().inputs[0].key;
    graph.connect(source, sum, key).unwrap();
    graph.node_mut(sum).unwrap().params.insert(morphlet::data_tree::MATCH_PARAM.into(), 2.);
    let bytes = serde_json::to_vec(&Project::new("legacy list mode", graph)).unwrap();
    let project = persistence::decode_project(&bytes).unwrap();
    assert!(!project.graph.node(sum).unwrap().params.contains_key(morphlet::data_tree::MATCH_PARAM));
    assert!(Evaluator::default().evaluate(&project.graph, &|| false).outputs[&sum].is_ok());
}
