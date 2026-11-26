//! Registry and compatibility-catalog contracts. These are NODES tests, not
//! Grasshopper oracle comparisons. No Rhino installation is required.
use morphlet::engine::Evaluator;
use morphlet::model::{DataType, Graph, NodeId, NodeKind, Tree, Value, MAX_NODES};
use morphlet::persistence::Project;
use serde_json::Value as Json;
use std::collections::BTreeSet;

fn node_id(kind: NodeKind) -> String {
    serde_json::to_value(kind).unwrap().as_str().unwrap().to_owned()
}

fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add(kind, [0., 0.]).unwrap()
}

fn set(graph: &mut Graph, id: NodeId, key: &str, value: f64) {
    graph.node_mut(id).unwrap().params.insert(key.into(), value);
}

fn numbers(tree: &Tree) -> Vec<f64> {
    tree.branches.values().flatten().map(|v| v.number().unwrap()).collect()
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
        assert!(!spec.description.trim().is_empty(), "{id}: missing description");
        let mut keys = BTreeSet::new();
        for input in spec.inputs {
            assert!(!input.key.is_empty() && keys.insert(input.key), "{id}: invalid port key");
            assert!(!input.label.trim().is_empty(), "{id}: missing port label");
            if let Some(default) = input.default {
                assert_eq!(input.kind, DataType::Number, "{id}: non-number default");
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
    let catalog: Json = serde_json::from_str(include_str!("../docs/compatibility/gh_catalog.json")).unwrap();
    assert_eq!(catalog["schema_version"], 1);
    let registry: BTreeSet<_> = NodeKind::all().iter().copied().chain([NodeKind::Cluster])
        .map(node_id).collect();
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
        assert!(seen.insert((family, category, name)), "duplicate candidate {name}");
        assert!(matches!(status, "VERIFIED" | "PARTIAL" | "APPROXIMATE" |
            "IMPLEMENTED_UNVERIFIED" | "MISSING" | "BLOCKED" | "UNKNOWN"));
        for field in ["exact_build", "component_guid", "classification", "inputs", "outputs",
                      "defaults", "tree_behavior", "nodes_id", "test_status", "limitation",
                      "reference_url", "official_sdk_url"] {
            assert!(row.get(field).is_some(), "{name}: missing field {field}");
        }
        if let Some(guid) = row["component_guid"].as_str() {
            assert!(guids.insert((family, guid)), "duplicate component GUID {guid}");
        }
        let mapped = row["nodes_id"].as_str();
        if let Some(id) = mapped {
            assert!(registry.contains(id), "{name}: unknown NODES ID {id}");
        }
        if matches!(status, "VERIFIED" | "PARTIAL" | "APPROXIMATE" | "IMPLEMENTED_UNVERIFIED") {
            assert!(mapped.is_some(), "{name}: status requires a mapped node");
        }
        if status == "VERIFIED" {
            // A real build/GUID and archived oracle evidence are mandatory.
            assert!(row["exact_build"].is_string());
            assert!(row["component_guid"].is_string());
            assert_eq!(row["test_status"], "GH_REFERENCE_PASSED");
            assert!(row["oracle_evidence"].as_str().is_some_and(|s| !s.is_empty()));
        }
        assert!(row["reference_url"].as_str().unwrap().starts_with("https://"));
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
    assert_eq!(numbers(changed.outputs[&sum].as_ref().unwrap()), vec![5., 8., 11., 14.]);
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
    assert_eq!(numbers(repaired.outputs[&add_id].as_ref().unwrap()), vec![10.5]);
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
    while oversized.nodes.len() < 10_000 {
        let mut node = template.clone();
        node.id = oversized.next_id;
        oversized.next_id += 1;
        oversized.nodes.push(node);
    }
    assert!(oversized.validate().is_err());
}
