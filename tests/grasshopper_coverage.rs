//! Registry and compatibility-catalog contracts. These are NODES tests, not
//! Grasshopper oracle comparisons. No Rhino installation is required.
use nodes::engine::Evaluator;
use nodes::model::{DataType, Graph, MAX_NODES, NodeId, NodeKind, Tree, Value};
use nodes::persistence::Project;
use serde_json::Value as Json;
use std::collections::BTreeSet;

fn node_id(kind: NodeKind) -> String {
    serde_json::to_value(kind)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add(kind, [0., 0.]).unwrap()
}

fn set(graph: &mut Graph, id: NodeId, key: &str, value: f64) {
    graph.node_mut(id).unwrap().params.insert(key.into(), value);
}

fn numbers(tree: &Tree) -> Vec<f64> {
    tree.branches
        .values()
        .flatten()
        .map(|v| v.number().unwrap())
        .collect()
}

#[test]
fn every_registered_kind_has_unique_id_and_complete_metadata() {
    let mut ids = BTreeSet::new();
    let mut graph = Graph::default();
    assert!(!NodeKind::all().contains(&NodeKind::Cluster));
    for kind in NodeKind::all().iter().copied().chain([NodeKind::Cluster]) {
        let id = node_id(kind);
        assert!(ids.insert(id.clone()), "duplicate serialized ID: {id}");
        let decoded: NodeKind = serde_json::from_value(Json::String(id.clone())).unwrap();
        assert_eq!(decoded, kind);
        let spec = kind.spec();
        assert!(!spec.label.trim().is_empty(), "{id}: missing label");
        assert_ne!(spec.label, "不明なノード", "{id}: fallback specification");
        assert!(!spec.category.trim().is_empty(), "{id}: missing category");
        assert!(
            !spec.description.trim().is_empty(),
            "{id}: missing description"
        );
        let mut keys = BTreeSet::new();
        for input in spec.inputs {
            assert!(
                !input.key.is_empty() && keys.insert(input.key),
                "{id}: invalid port key"
            );
            assert!(!input.label.trim().is_empty(), "{id}: missing port label");
            if let Some(default) = input.default {
                // A polymorphic port (e.g. a cluster input) can accept a numeric
                // default without having the narrow Number type itself.
                assert!(
                    input.kind.accepts(DataType::Number),
                    "{id}: incompatible numeric default"
                );
                assert!(default.is_finite() && input.min.is_finite() && input.max.is_finite());
                assert!((input.min..=input.max).contains(&default));
                assert!(input.step.is_finite() && input.step > 0.);
            }
        }
        // A cluster instance needs its definition and is not a normal picker entry.
        if kind != NodeKind::Cluster {
            add(&mut graph, kind);
        }
    }
    let original = Project::new("全登録ノード", graph);
    original.validate().unwrap();
    let decoded: Project = serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn catalog_claims_are_conservative_and_mapped_nodes_exist() {
    let catalog: Json =
        serde_json::from_str(include_str!("../docs/compatibility/gh_catalog.json")).unwrap();
    assert_eq!(catalog["schema_version"], 1);
    let registry: BTreeSet<_> = NodeKind::all()
        .iter()
        .copied()
        .chain([NodeKind::Cluster])
        .map(node_id)
        .collect();
    let mut seen = BTreeSet::new();
    let mut guids = BTreeSet::new();
    let gh1 = catalog["components"].as_array().unwrap();
    let gh2 = catalog["gh2"]["components"].as_array().unwrap();
    assert!(!gh1.is_empty() && !gh2.is_empty());
    assert!(gh1.iter().all(|r| r["version_family"] == "GH1"));
    assert!(gh2.iter().all(|r| r["version_family"] == "GH2"));
    for row in gh1.iter().chain(gh2) {
        let name = row["name"].as_str().unwrap();
        let category = row["category"].as_str().unwrap();
        let family = row["version_family"].as_str().unwrap();
        let status = row["status"].as_str().unwrap();
        assert!(matches!(family, "GH1" | "GH2"));
        assert!(
            seen.insert((family, category, name)),
            "duplicate candidate {name}"
        );
        assert!(matches!(
            status,
            "VERIFIED"
                | "PARTIAL"
                | "APPROXIMATE"
                | "IMPLEMENTED"
                | "MISSING"
                | "BLOCKED"
                | "UNKNOWN"
        ));
        assert!(
            catalog["status_definitions"][status]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "{name}: undocumented status"
        );
        for field in [
            "exact_build",
            "component_guid",
            "classification",
            "inputs",
            "outputs",
            "defaults",
            "tree_behavior",
            "nodes_id",
            "test_status",
            "limitation",
            "reference_url",
            "official_sdk_url",
        ] {
            assert!(row.get(field).is_some(), "{name}: missing field {field}");
        }
        if let Some(guid) = row["component_guid"].as_str() {
            assert!(
                guids.insert((family, guid)),
                "duplicate component GUID {guid}"
            );
        }
        let mapped = row["nodes_id"].as_str();
        if let Some(id) = mapped {
            assert!(registry.contains(id), "{name}: unknown NODES ID {id}");
        }
        if matches!(
            status,
            "VERIFIED" | "PARTIAL" | "APPROXIMATE" | "IMPLEMENTED"
        ) {
            assert!(mapped.is_some(), "{name}: status requires a mapped node");
        }
        if status == "VERIFIED" {
            // A real build/GUID and archived oracle evidence are mandatory.
            assert!(
                row["exact_build"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
            );
            assert!(row["component_guid"].is_string());
            assert_eq!(row["test_status"], "GH_REFERENCE_PASSED");
            let evidence = row["oracle_evidence"]
                .as_str()
                .expect("VERIFIED requires evidence");
            let relative = std::path::Path::new(evidence);
            assert!(
                relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
                "{name}: evidence must be a repository-relative path"
            );
            assert!(
                evidence.starts_with("docs/compatibility/oracles/")
                    || evidence.starts_with("tests/fixtures/grasshopper/"),
                "{name}: reference evidence belongs in an explicit oracle fixture directory"
            );
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let contents = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("{name}: missing reference evidence: {e}"));
            let oracle: Json = serde_json::from_str(&contents)
                .unwrap_or_else(|e| panic!("{name}: reference evidence is not JSON: {e}"));
            assert_eq!(oracle["version_family"], row["version_family"]);
            assert_eq!(oracle["exact_build"], row["exact_build"]);
            assert_eq!(oracle["component_guid"], row["component_guid"]);
            let cases = oracle["cases"]
                .as_array()
                .filter(|cases| !cases.is_empty())
                .expect("reference evidence requires actual test cases");
            for case in cases {
                for field in ["input", "expected", "actual", "comparison"] {
                    assert!(
                        case.get(field).is_some(),
                        "{name}: oracle case missing {field}"
                    );
                }
                assert_eq!(
                    case["status"], "PASS",
                    "{name}: reference case did not pass"
                );
            }
        }
        assert!(
            row["reference_url"]
                .as_str()
                .unwrap()
                .starts_with("https://")
        );
    }
    assert!(catalog["gh2"]["components"].is_array());
}

#[test]
fn independent_number_to_list_tree_and_save_contract() {
    let mut graph = Graph::default();
    let slider = add(&mut graph, NodeKind::Number);
    let series = add(&mut graph, NodeKind::Series);
    let graft = add(&mut graph, NodeKind::Graft);
    let flatten = add(&mut graph, NodeKind::Flatten);
    let sum = add(&mut graph, NodeKind::Add);
    set(&mut graph, slider, "value", 2.);
    set(&mut graph, series, "step", 3.);
    set(&mut graph, series, "count", 4.);
    set(&mut graph, sum, "b", -2.);
    graph.connect(slider, series, "start").unwrap();
    graph.connect(series, graft, "geometry").unwrap();
    graph.connect(graft, flatten, "geometry").unwrap();
    graph.connect(flatten, sum, "a").unwrap();
    let mut eval = Evaluator::default();
    let first = eval.evaluate(&graph, &|| false);
    let expected = vec![0., 3., 6., 9.];
    assert_eq!(numbers(first.outputs[&sum].as_ref().unwrap()), expected);
    let grafted = first.outputs[&graft].as_ref().unwrap();
    assert_eq!(grafted.branches.len(), 4);
    for (i, n) in [2., 5., 8., 11.].into_iter().enumerate() {
        assert_eq!(grafted.branches[&vec![0, i as u32]], vec![Value::Number(n)]);
    }
    assert_eq!(first.outputs[&flatten].as_ref().unwrap().branches.len(), 1);
    let project = Project::new("数列と枝", graph.clone());
    let decoded: Project = serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
    assert_eq!(project, decoded);
    let fresh = Evaluator::default().evaluate(&decoded.graph, &|| false);
    assert_eq!(fresh.outputs, first.outputs);
    let warm = eval.evaluate(&graph, &|| false);
    assert_eq!(warm.cached, 5);
    assert_eq!(warm.computed, 0);
    set(&mut graph, slider, "value", 7.);
    let changed = eval.evaluate(&graph, &|| false);
    assert_eq!(
        numbers(changed.outputs[&sum].as_ref().unwrap()),
        vec![5., 8., 11., 14.]
    );
}

#[test]
fn invalid_inputs_and_cancelled_results_do_not_reuse_stale_values() {
    let mut graph = Graph::default();
    let divide = add(&mut graph, NodeKind::Divide);
    let add_id = add(&mut graph, NodeKind::Add);
    let unrelated = add(&mut graph, NodeKind::Number);
    graph.connect(divide, add_id, "a").unwrap();
    let mut eval = Evaluator::default();
    let initial = eval.evaluate(&graph, &|| false);
    assert!(initial.outputs[&add_id].is_ok());
    set(&mut graph, divide, "b", 0.);
    let broken = eval.evaluate(&graph, &|| false);
    assert!(broken.outputs[&divide].is_err());
    assert!(broken.outputs[&add_id].is_err());
    assert!(broken.outputs[&unrelated].is_ok());
    set(&mut graph, divide, "b", 2.);
    assert!(eval.evaluate(&graph, &|| true).cancelled);
    let repaired = eval.evaluate(&graph, &|| false);
    assert_eq!(
        numbers(repaired.outputs[&add_id].as_ref().unwrap()),
        vec![10.5]
    );
}

#[test]
fn node_budget_is_a_rejection_not_a_false_large_graph_success() {
    let mut graph = Graph::default();
    for _ in 0..MAX_NODES {
        add(&mut graph, NodeKind::Number);
    }
    let before = graph.clone();
    assert!(graph.add(NodeKind::Number, [0., 0.]).is_err());
    assert_eq!(graph, before);
    let result = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(result.outputs.len(), MAX_NODES);
    assert!(result.outputs.values().all(Result::is_ok));
    // Hostile deserialized input that bypassed Graph::add is also rejected.
    let mut oversized = graph;
    let template = oversized.nodes[0].clone();
    while oversized.nodes.len() <= MAX_NODES {
        let mut node = template.clone();
        node.id = oversized.next_id;
        oversized.next_id += 1;
        oversized.nodes.push(node);
    }
    assert!(oversized.validate().is_err());
}

#[test]
fn reroute_dot_passes_values_through_and_new_kinds_keep_snake_case_ids() {
    let mut graph = Graph::default();
    let slider = add(&mut graph, NodeKind::Number);
    let dot = add(&mut graph, NodeKind::Reroute);
    let sum = add(&mut graph, NodeKind::Add);
    set(&mut graph, slider, "value", 5.);
    graph.connect(slider, dot, "value").unwrap();
    graph.connect(dot, sum, "a").unwrap();
    assert!(!graph.node(dot).unwrap().preview);
    let result = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(numbers(result.outputs[&dot].as_ref().unwrap()), vec![5.]);
    assert_eq!(numbers(result.outputs[&sum].as_ref().unwrap()), vec![15.]);
    for (kind, id) in [
        (NodeKind::Reroute, "reroute"),
        (NodeKind::MathSquareRoot, "math_square_root"),
        (NodeKind::GateXnor, "gate_xnor"),
        (NodeKind::PointPolar, "point_polar"),
        (NodeKind::CurveAreaCentroid, "curve_area_centroid"),
        (NodeKind::RotateDirection, "rotate_direction"),
        (NodeKind::ProjectToZ, "project_to_z"),
    ] {
        assert_eq!(node_id(kind), id);
    }
}

#[test]
fn new_standard_components_evaluate_in_a_graph() {
    let mut graph = Graph::default();
    let value = add(&mut graph, NodeKind::Number);
    let root = add(&mut graph, NodeKind::MathSquareRoot);
    let polar = add(&mut graph, NodeKind::PointPolar);
    set(&mut graph, value, "value", 16.);
    graph.connect(value, root, "x").unwrap();
    graph.connect(root, polar, "radius").unwrap();
    set(&mut graph, polar, "angle", 90.);
    set(&mut graph, polar, "elevation", 0.);
    let result = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(numbers(result.outputs[&root].as_ref().unwrap()), vec![4.]);
    let tree = result.outputs[&polar].as_ref().unwrap();
    match tree.branches.values().flatten().next() {
        Some(Value::Point(p)) => assert!((p[1] - 4.).abs() < 1e-9 && p[0].abs() < 1e-9),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn standard_logic_gates_validate_both_operands() {
    for kind in [NodeKind::GateNand, NodeKind::GateNor, NodeKind::GateXnor] {
        for valid in [0., 1.] {
            for invalid in [-1., 0.5, 2.] {
                let valid = Value::Number(valid);
                let invalid = Value::Number(invalid);
                assert!(nodes::gh_gaps::run_item(kind, &[&valid, &invalid], &|| false).is_err());
                assert!(nodes::gh_gaps::run_item(kind, &[&invalid, &valid], &|| false).is_err());
            }
        }
    }
}

#[test]
fn standard_polygon_area_and_centroid_survive_large_translation() {
    for offset in [0., 1e9, -1e9] {
        for reversed in [false, true] {
            let mut points = vec![
                [offset, offset, 0.],
                [offset + 4., offset, 0.],
                [offset + 4., offset + 2., 0.],
                [offset, offset + 2., 0.],
            ];
            if reversed {
                points.reverse();
            }
            let curve = Value::Curve(std::sync::Arc::new(nodes::model::Curve {
                points,
                closed: true,
                geometry: None,
            }));
            let area = nodes::gh_gaps::run_item(NodeKind::CurveArea, &[&curve], &|| false).unwrap();
            assert_eq!(area, vec![Value::Number(8.)]);
            let centroid =
                nodes::gh_gaps::run_item(NodeKind::CurveAreaCentroid, &[&curve], &|| false)
                    .unwrap();
            assert_eq!(centroid, vec![Value::Point([offset + 2., offset + 1., 0.])]);
        }
    }
}

#[test]
fn second_standard_batch_keeps_snake_case_ids_and_evaluates() {
    for (kind, id) in [
        (NodeKind::MathIntDivide, "math_int_divide"),
        (NodeKind::SetFibonacci, "set_fibonacci"),
        (NodeKind::SetSortByKeys, "set_sort_by_keys"),
        (NodeKind::PointCullDuplicates, "point_cull_duplicates"),
        (NodeKind::CurveJoin, "curve_join"),
        (NodeKind::MoveAway, "move_away"),
        (NodeKind::MeshSplitParts, "mesh_split_parts"),
        (NodeKind::DisplayBoundingBox, "display_bounding_box"),
        (NodeKind::SurfaceBoundary, "surface_boundary"),
    ] {
        assert_eq!(node_id(kind), id);
    }
    let mut graph = Graph::default();
    let fib = add(&mut graph, NodeKind::SetFibonacci);
    set(&mut graph, fib, "count", 6.);
    let tail = add(&mut graph, NodeKind::SetSplitTail);
    set(&mut graph, tail, "index", 2.);
    graph.connect(fib, tail, "list").unwrap();
    let result = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(
        numbers(result.outputs[&fib].as_ref().unwrap()),
        vec![0., 1., 1., 2., 3., 5.]
    );
    assert_eq!(
        numbers(result.outputs[&tail].as_ref().unwrap()),
        vec![1., 2., 3., 5.]
    );
}
