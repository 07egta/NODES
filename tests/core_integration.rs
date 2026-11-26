//! Core integration contracts: these tests intentionally use the public model API.
use morphlet::cluster::{self, ClusterDef};
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, MAX_NODES, NodeId, NodeKind, Tree, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add(kind, [0., 0.]).unwrap()
}

fn number(ev: &Evaluation, id: NodeId) -> f64 {
    ev.outputs[&id].as_ref().unwrap().branches[&vec![0]][0]
        .number()
        .unwrap()
}

#[test]
fn indexed_topology_handles_sparse_ids_repeated_parents_and_reordered_storage() {
    let mut graph = Graph::default();
    let a = add(&mut graph, NodeKind::Number);
    let removed = add(&mut graph, NodeKind::Number);
    let b = add(&mut graph, NodeKind::Add);
    let c = add(&mut graph, NodeKind::Add);
    graph.remove(removed);
    graph.connect(a, b, "a").unwrap();
    graph.connect(a, b, "b").unwrap();
    graph.connect(b, c, "a").unwrap();
    graph.connect(a, c, "b").unwrap();
    graph.nodes.reverse();

    assert_eq!(graph.topological_order().unwrap(), vec![a, b, c]);
    let mut evaluator = Evaluator::default();
    let ev = evaluator.evaluate(&graph, &|| false);
    assert_eq!(ev.computed, 3);
    assert_eq!(number(&ev, c), 3.);
    graph.nodes.reverse();
    let ev = evaluator.evaluate(&graph, &|| false);
    assert_eq!((ev.computed, ev.cached), (0, 3));
    assert_eq!(number(&ev, c), 3.);
    let before = graph.clone();
    assert!(graph.connect(c, b, "a").is_err());
    assert_eq!(graph, before);
}

fn sparse_edit_graph(count: usize) -> (Graph, NodeId, NodeId) {
    assert!(count >= 2);
    let mut graph = Graph::default();
    let source = add(&mut graph, NodeKind::Number);
    let child = add(&mut graph, NodeKind::Add);
    graph.connect(source, child, "a").unwrap();
    for _ in 2..count {
        add(&mut graph, NodeKind::Number);
    }
    (graph, source, child)
}

#[test]
fn sparse_slider_edit_does_not_recompute_unrelated_nodes_at_supported_scales() {
    for count in [100, 1_000, MAX_NODES] {
        let (mut graph, source, child) = sparse_edit_graph(count);
        let mut evaluator = Evaluator::default();
        let cold = evaluator.evaluate(&graph, &|| false);
        assert_eq!((cold.computed, cold.cached), (count, 0));
        assert_eq!(number(&cold, child), 11.);
        assert!(cold.outputs.values().all(Result::is_ok));
        let warm = evaluator.evaluate(&graph, &|| false);
        assert_eq!((warm.computed, warm.cached), (0, count));
        graph.node_mut(source).unwrap().params.insert("value".into(), 7.);
        let edited = evaluator.evaluate(&graph, &|| false);
        assert_eq!((edited.computed, edited.cached), (2, count - 2));
        assert_eq!(number(&edited, child), 17.);
    }
}

#[test]
fn ten_thousand_nodes_are_rejected_at_current_resource_limit() {
    let mut graph = Graph::default();
    let id = add(&mut graph, NodeKind::Number);
    let prototype = graph.node(id).unwrap().clone();
    graph.nodes = (0..10_000)
        .map(|id| {
            let mut node = prototype.clone();
            node.id = id;
            node
        })
        .collect();
    graph.next_id = 10_000;
    assert!(graph.topological_order().unwrap_err().contains(&MAX_NODES.to_string()));
    let ev = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(ev.computed, 0);
    assert!(ev.outputs.values().all(Result::is_err));
}

fn nested_cluster(inner: Graph, output: NodeId) -> (Graph, NodeId) {
    let mut outer = Graph::default();
    outer.clusters.push(ClusterDef {
        id: 1,
        name: "Inner".into(),
        graph: inner,
        inputs: vec![],
        output,
    });

    let mut middle = Graph::default();
    let inner_instance = add(&mut middle, NodeKind::Cluster);
    middle.node_mut(inner_instance).unwrap().cluster = Some(1);
    outer.clusters.push(ClusterDef {
        id: 2,
        name: "Middle".into(),
        graph: middle,
        inputs: vec![],
        output: inner_instance,
    });

    let instance = cluster::instantiate(&mut outer, 2, [0., 0.]).unwrap();
    outer.validate().unwrap();
    (outer, instance)
}

#[test]
fn disabled_nodes_inside_nested_clusters_match_top_level_semantics() {
    let mut inner = Graph::default();
    let output = add(&mut inner, NodeKind::Number);
    inner.node_mut(output).unwrap().off = true;
    let (mut graph, instance) = nested_cluster(inner, output);
    let mut evaluator = Evaluator::default();
    let disabled = evaluator.evaluate(&graph, &|| false);
    assert_eq!(disabled.outputs[&instance], Ok(Tree::default()));

    graph.clusters[0].graph.node_mut(output).unwrap().off = false;
    let enabled = evaluator.evaluate(&graph, &|| false);
    assert_eq!((enabled.computed, enabled.cached), (1, 0));
    assert_eq!(number(&enabled, instance), 1.);
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn nested_import_recomputes_when_missing_file_appears_and_is_removed() {
    let directory = TempDir(std::env::temp_dir().join(format!(
        "morphlet-core-import-{}",
        std::process::id()
    )));
    fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("mesh.obj");

    let mut inner = Graph::default();
    let output = add(&mut inner, NodeKind::ImportMesh);
    inner.node_mut(output).unwrap().script = path.to_str().unwrap().into();
    let (graph, instance) = nested_cluster(inner, output);
    let mut evaluator = Evaluator::default();
    assert!(evaluator.evaluate(&graph, &|| false).outputs[&instance].is_err());
    let warm = evaluator.evaluate(&graph, &|| false);
    assert_eq!((warm.computed, warm.cached), (0, 1));

    fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let loaded = evaluator.evaluate(&graph, &|| false);
    assert_eq!((loaded.computed, loaded.cached), (1, 0));
    assert!(matches!(
        loaded.outputs[&instance].as_ref().unwrap().branches[&vec![0]].first(),
        Some(Value::Mesh(_))
    ));

    fs::remove_file(&path).unwrap();
    let missing = evaluator.evaluate(&graph, &|| false);
    assert_eq!((missing.computed, missing.cached), (1, 0));
    assert!(missing.outputs[&instance].is_err());
}

#[test]
fn registered_node_contracts_have_unique_serialized_ids_and_input_keys() {
    let mut ids = BTreeSet::new();
    for kind in NodeKind::all().iter().copied().chain([NodeKind::Cluster]) {
        let id = serde_json::to_string(&kind).unwrap();
        assert!(ids.insert(id.clone()), "duplicate node id: {id}");
        assert_eq!(serde_json::from_str::<NodeKind>(&id).unwrap(), kind);
        let spec = kind.spec();
        assert!(!spec.label.is_empty(), "{id}");
        assert!(!spec.category.is_empty(), "{id}");
        assert!(!spec.description.is_empty(), "{id}");
        assert!(!spec.keywords.is_empty(), "{id}");
        let mut keys = BTreeSet::new();
        for input in spec.inputs {
            assert!(!input.key.is_empty(), "{id}");
            assert!(!input.label.is_empty(), "{id}");
            assert!(keys.insert(input.key), "{id}: {}", input.key);
            if let Some(default) = input.default {
                assert!(default.is_finite() && input.min.is_finite() && input.max.is_finite());
                assert!((input.min..=input.max).contains(&default), "{id}");
                assert!(input.step.is_finite() && input.step > 0., "{id}");
                if input.integer {
                    assert_eq!(default.fract(), 0., "{id}");
                }
            }
        }
    }
}

/// Run manually on the same machine before and after changes. Construction is excluded.
/// No hardware-dependent time threshold is used in CI.
#[test]
#[ignore = "manual release-mode performance measurement"]
fn benchmark_sparse_slider_edit() {
    println!("nodes,cold_ms,warm_ms,edit_ms,computed,cached");
    for count in [100, 1_000, MAX_NODES] {
        let (mut graph, source, _) = sparse_edit_graph(count);
        for iteration in 0..5 {
            let mut evaluator = Evaluator::default();
            let start = Instant::now();
            let cold = evaluator.evaluate(&graph, &|| false);
            let cold_ms = start.elapsed().as_secs_f64() * 1_000.;
            assert_eq!(cold.computed, count);
            drop(cold);
            let start = Instant::now();
            let warm = evaluator.evaluate(&graph, &|| false);
            let warm_ms = start.elapsed().as_secs_f64() * 1_000.;
            assert_eq!(warm.cached, count);
            drop(warm);
            graph.node_mut(source).unwrap().params.insert("value".into(), 2. + iteration as f64);
            let start = Instant::now();
            let edited = evaluator.evaluate(&graph, &|| false);
            let edit_ms = start.elapsed().as_secs_f64() * 1_000.;
            assert_eq!((edited.computed, edited.cached), (2, count - 2));
            println!("{count},{cold_ms:.3},{warm_ms:.3},{edit_ms:.3},{},{}", edited.computed, edited.cached);
        }
    }
}

#[test]
fn shared_definition_edits_recompute_only_dependent_instances() {
    let mut inner = Graph::default();
    let output = add(&mut inner, NodeKind::Number);
    let (mut graph, instance) = nested_cluster(inner, output);
    let independent = add(&mut graph, NodeKind::Number);
    let second = cluster::instantiate(&mut graph, 2, [0., 0.]).unwrap();
    let third = cluster::instantiate(&mut graph, 1, [0., 0.]).unwrap();
    let mut evaluator = Evaluator::default();
    assert_eq!(evaluator.evaluate(&graph, &|| false).computed, 4);
    assert_eq!(evaluator.evaluate(&graph, &|| false).cached, 4);
    graph.clusters[0].graph.node_mut(output).unwrap().params.insert("value".into(), 9.);
    let changed = evaluator.evaluate(&graph, &|| false);
    assert_eq!((changed.computed, changed.cached), (3, 1));
    for id in [instance, second, third] {
        assert_eq!(number(&changed, id), 9.);
    }
    assert_eq!(number(&changed, independent), 1.);
    // A new work may reuse every definition ID. Compare the actual content.
    let mut next = graph.clone();
    next.clusters[0].graph.node_mut(output).unwrap().params.insert("value".into(), 3.);
    assert_eq!(number(&evaluator.evaluate(&next, &|| false), instance), 3.);
}

#[test]
fn overdeep_definition_graph_is_rejected_before_exponential_expansion() {
    let mut graph = Graph::default();
    for id in 1..=12u64 {
        let mut inner = Graph::default();
        let mut output = add(&mut inner, NodeKind::Number);
        if id < 12 {
            for _ in 0..16 {
                output = add(&mut inner, NodeKind::Cluster);
                inner.node_mut(output).unwrap().cluster = Some(id + 1);
            }
        }
        graph.clusters.push(ClusterDef {
            id, name: format!("definition {id}"), graph: inner, inputs: vec![], output,
        });
    }
    assert!(graph.validate().unwrap_err().contains("深すぎ"));
    let mut stack = Vec::new();
    assert!(cluster::def_depth(&graph.clusters, 1, &mut stack).is_err());
    assert!(stack.is_empty(), "failed depth checks must unwind their stack");
}

#[test]
fn cluster_id_exhaustion_is_an_error_and_preserves_the_target() {
    let mut inner = Graph::default();
    let output = add(&mut inner, NodeKind::Number);
    let (source, _) = nested_cluster(inner.clone(), output);
    let bytes = cluster::export_part(&source, 2).unwrap();
    let mut target = Graph::default();
    target.clusters.push(ClusterDef {
        id: u64::MAX - 1, name: "existing".into(), graph: inner, inputs: vec![], output,
    });
    target.validate().unwrap();
    let before = target.clone();
    assert!(cluster::import_part(&mut target, bytes.as_bytes()).is_err());
    assert_eq!(target, before);
    target.clusters[0].id = u64::MAX;
    let standalone = add(&mut target, NodeKind::Number);
    let before = target.clone();
    assert!(cluster::group_nodes(&mut target, &[standalone], "new").is_err());
    assert_eq!(target, before);
}

#[test]
fn part_save_replaces_only_with_complete_valid_data() {
    let directory = TempDir(std::env::temp_dir().join(format!(
        "morphlet-core-part-{}", std::process::id()
    )));
    fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("part.morphpart");
    let mut inner = Graph::default();
    let output = add(&mut inner, NodeKind::Number);
    let (mut graph, _) = nested_cluster(inner, output);
    cluster::save_part(&path, &graph, 2).unwrap();
    let previous = fs::read(&path).unwrap();
    graph.clusters[0].output = 999;
    assert!(cluster::save_part(&path, &graph, 2).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    graph.clusters[0].output = output;
    graph.clusters[0].name = "updated".into();
    cluster::save_part(&path, &graph, 2).unwrap();
    let updated = fs::read(&path).unwrap();
    assert_ne!(updated, previous);
    assert!(cluster::parse_part(&updated).is_ok());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}
