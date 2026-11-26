//! Script node (Rhai sandbox) and cluster (まとめノード) tests.
//! NOTE: written without being compiled or run locally; CI is the first real check.
use morphlet::cluster::{self, ClusterDef};
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, NodeId, NodeKind, Tree, Value};
use morphlet::persistence::{self, Project};
use morphlet::script::{self, ScriptInputs};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let n = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("morphlet-script-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn inputs(a: f64, b: f64, c: f64, d: f64) -> ScriptInputs {
    ScriptInputs {
        numbers: [vec![a], vec![b], vec![c], vec![d]],
        geometry: None,
    }
}
fn run(source: &str) -> Result<Tree, String> {
    script::run_script(source, &inputs(1., 2., 3., 4.), &|| false)
}
fn values(tree: &Tree) -> Vec<Value> {
    tree.branches.values().flatten().cloned().collect()
}
fn numbers(tree: &Tree) -> Vec<f64> {
    values(tree)
        .iter()
        .map(|v| match v {
            Value::Number(n) => *n,
            other => panic!("number expected, got {other:?}"),
        })
        .collect()
}
fn evaluate(graph: &Graph) -> morphlet::engine::Evaluation {
    Evaluator::default().evaluate(graph, &|| false)
}
fn script_graph(source: &str) -> (Graph, NodeId) {
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Script, [0., 0.]).unwrap();
    graph.node_mut(id).unwrap().script = source.into();
    (graph, id)
}

// ------------------------------------------------------------------ script: results

#[test]
fn simple_script_returns_a_number_and_integers_work_in_math() {
    assert_eq!(numbers(&run("a + b * c").unwrap()), vec![7.]);
    assert_eq!(numbers(&run("sqrt(16)").unwrap()), vec![4.]);
    assert_eq!(numbers(&run("max(a, 10)").unwrap()), vec![10.]);
    let list = numbers(&run("linspace(0, 1, 5)").unwrap());
    assert_eq!(list, vec![0., 0.25, 0.5, 0.75, 1.]);
}

#[test]
fn script_can_return_a_point_and_a_list() {
    let tree = run("point(1, 2, 3)").unwrap();
    assert_eq!(values(&tree), vec![Value::Point([1., 2., 3.])]);
    let tree = run("let out = []; for i in 0..4 { out.push(i * 2); } out").unwrap();
    assert_eq!(numbers(&tree), vec![0., 2., 4., 6.]);
}

#[test]
fn simple_script_produces_a_closed_outward_mesh_through_the_engine() {
    let (graph, id) =
        script_graph("let m = mesh_box(10, 20, 30);\nlet n = mesh_move(m, 0, 0, 0);\nm");
    let evaluation = evaluate(&graph);
    let tree = evaluation.outputs.get(&id).unwrap().clone().unwrap();
    let report = persistence::mesh_stats(&[tree]).unwrap();
    assert_eq!(report.vertices, 8);
    assert_eq!(report.triangles, 12);
    assert!(report.is_closed());
    assert!((report.signed_volume_mm3 - 6000.).abs() < 1e-6);
}

#[test]
fn cylinder_and_merge_make_valid_meshes() {
    let (graph, id) = script_graph(
        "let parts = [];\nfor i in 0..3 { parts.push(mesh_move(mesh_cylinder(5, 10, 64), i * 20, 0, 0)); }\nmesh_merge(parts)",
    );
    let tree = evaluate(&graph).outputs.get(&id).unwrap().clone().unwrap();
    let report = persistence::mesh_stats(&[tree]).unwrap();
    assert!(report.is_closed(), "{report:?}");
    assert!(report.signed_volume_mm3 > 0.);
    let one = std::f64::consts::PI * 25. * 10.;
    assert!((report.signed_volume_mm3 / 3. - one).abs() < one * 0.02);
}

#[test]
fn geometry_input_g_is_available_to_scripts() {
    let mut graph = Graph::default();
    let b = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    let s = graph.add(NodeKind::Script, [1., 0.]).unwrap();
    graph.node_mut(s).unwrap().script = "mesh_triangles(g)".into();
    graph.connect(b, s, "g").unwrap();
    let tree = evaluate(&graph).outputs.get(&s).unwrap().clone().unwrap();
    assert_eq!(numbers(&tree), vec![12.]);
}

#[test]
fn script_node_uses_connected_lists_for_a() {
    let mut graph = Graph::default();
    let r = graph.add(NodeKind::Range, [0., 0.]).unwrap();
    let s = graph.add(NodeKind::Script, [1., 0.]).unwrap();
    graph.node_mut(s).unwrap().script = "let t = 0.0; for v in a { t += v; } t".into();
    graph.connect(r, s, "a").unwrap();
    let tree = evaluate(&graph).outputs.get(&s).unwrap().clone().unwrap();
    // Range defaults: 0..100 in 11 steps -> sum = 550.
    assert_eq!(numbers(&tree), vec![550.]);
}

#[test]
fn both_examples_work_with_the_default_inputs() {
    let (graph, id) = script_graph(script::EXAMPLE_STAIRS);
    let tree = evaluate(&graph).outputs.get(&id).unwrap().clone().unwrap();
    let report = persistence::mesh_stats(&[tree]).unwrap();
    assert_eq!(report.triangles, 8 * 12);
    let (graph, id) = script_graph(script::EXAMPLE_WAVE);
    let tree = evaluate(&graph).outputs.get(&id).unwrap().clone().unwrap();
    assert_eq!(numbers(&tree).len(), 8);
}

#[test]
fn bad_results_are_japanese_errors_not_panics() {
    for source in [
        "\"text\"",
        "let x = 1;",
        "true",
        "[[1, 2], [3]]",
        "1 +",
        "undefined_name",
        "1 / 0",
    ] {
        let error = run(source).unwrap_err();
        assert!(!error.is_empty(), "{source}");
    }
    assert!(run("").unwrap_err().contains("空"));
    assert!(run("mesh_box(0, 1, 1)").is_err());
    assert!(run("mesh_box(1e12, 1, 1)").is_err());
    assert!(run("mesh_merge([])").is_err());
    assert!(run("mesh_cylinder(1, 1, 2)").is_err());
}

#[test]
fn an_erroring_script_marks_only_its_node() {
    let (graph, id) = script_graph("this is not a script");
    let evaluation = evaluate(&graph);
    let message = evaluation.outputs.get(&id).unwrap().clone().unwrap_err();
    assert!(message.contains("書き方"), "{message}");
}

// ------------------------------------------------------------------ script: limits

#[test]
fn infinite_loops_are_stopped() {
    let start = Instant::now();
    for source in [
        "let i = 0; loop { i += 1; }",
        "while true { }",
        "let x = 0; while x >= 0 { x += 1; }",
        "fn f(n) { f(n + 1) } f(0)",
    ] {
        assert!(run(source).is_err(), "{source}");
    }
    assert!(start.elapsed() < Duration::from_secs(30));
    let error = run("let i = 0; loop { i += 1; }").unwrap_err();
    assert!(error.contains("上限"), "{error}");
}

#[test]
fn the_cancel_callback_stops_a_script() {
    let started = Instant::now();
    let cancel = || started.elapsed() > Duration::from_millis(1);
    let result = script::run_script(
        "let i = 0; loop { i += 1; }",
        &inputs(0., 0., 0., 0.),
        &cancel,
    );
    assert!(result.is_err());
    assert!(script::run_script("1", &inputs(0., 0., 0., 0.), &|| true).is_err());
}

#[test]
fn huge_arrays_and_strings_are_rejected() {
    let error = run("let a = []; for i in 0..100000 { a.push(i); } a.len()").unwrap_err();
    assert!(
        error.contains("大きすぎ") || error.contains("上限"),
        "{error}"
    );
    assert!(run("let s = \"x\"; for i in 0..30 { s += s; } s.len()").is_err());
    assert!(run("let m = #{}; for i in 0..5000 { m[`k${i}`] = i; } m.len()").is_err());
    assert!(run("linspace(0, 1, 1000000)").is_err());
}

#[test]
fn script_text_length_is_capped() {
    let long = format!("1{}", " ".repeat(script::MAX_SCRIPT_BYTES));
    assert!(run(&long).unwrap_err().contains("長すぎ"));
    let ok = format!("1{}", " ".repeat(script::MAX_SCRIPT_BYTES - 1));
    assert!(run(&ok).is_ok());
}

#[test]
fn too_many_meshes_hit_the_budget() {
    let error = run("let k = 0; for i in 0..2000 { let m = mesh_cylinder(1, 1, 256); k += 1; } k")
        .unwrap_err();
    assert!(
        error.contains("大きすぎ") || error.contains("合計"),
        "{error}"
    );
}

#[test]
fn forbidden_features_are_errors() {
    assert!(run("import \"std/fs\" as fs; 1").is_err());
    assert!(run("eval(\"1 + 1\")").is_err());
    assert!(run("open(\"/etc/passwd\")").is_err());
    assert!(run("read_file(\"Cargo.toml\")").is_err());
    assert!(run("system(\"echo hi\")").is_err());
    assert!(run("timestamp()").is_err());
}

// ------------------------------------------------------------------ persistence

#[test]
fn old_files_without_the_new_fields_still_load() {
    let old = r#"{
      "schema_version": 1,
      "name": "旧作品",
      "graph": {
        "nodes": [{
          "id": 0, "kind": "number", "name": "数値", "position": [0.0, 0.0],
          "params": {"value": 3.0}, "inputs": {}, "preview": true
        }],
        "next_id": 1
      },
      "pan": [0.0, 0.0],
      "zoom": 1.0
    }"#;
    let project: Project = serde_json::from_str(old).unwrap();
    project.validate().unwrap();
    assert!(project.graph.clusters.is_empty());
    assert!(project.graph.nodes[0].script.is_empty());
    // Files without scripts/clusters are written exactly as before.
    let text = serde_json::to_string(&project).unwrap();
    assert!(!text.contains("script") && !text.contains("clusters") && !text.contains("cluster"));
}

#[test]
fn script_text_round_trips_through_a_morph_file() {
    let temp = TempDir::new();
    let path = temp.file("s.morph");
    let (graph, id) = script_graph(script::EXAMPLE_WAVE);
    let project = Project::new("スクリプト作品", graph);
    persistence::save(&path, &project).unwrap();
    let loaded = persistence::load(&path).unwrap();
    assert_eq!(loaded, project);
    assert_eq!(loaded.graph.node(id).unwrap().script, script::EXAMPLE_WAVE);
}

#[test]
fn an_oversized_script_in_a_file_is_rejected() {
    let (mut graph, id) = script_graph("1");
    graph.node_mut(id).unwrap().script = "x".repeat(script::MAX_SCRIPT_BYTES + 1);
    assert!(Project::new("x", graph).validate().is_err());
}

// ------------------------------------------------------------------ clusters

/// Box -> Move -> Scale -> Output. Returns (graph, [box, move, scale, output]).
fn chain() -> (Graph, [NodeId; 4]) {
    let mut graph = Graph::default();
    let b = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    let m = graph.add(NodeKind::Move, [100., 0.]).unwrap();
    let s = graph.add(NodeKind::Scale, [200., 0.]).unwrap();
    let o = graph.add(NodeKind::Output, [300., 0.]).unwrap();
    graph.connect(b, m, "geometry").unwrap();
    graph.connect(m, s, "geometry").unwrap();
    graph.connect(s, o, "geometry").unwrap();
    graph.node_mut(m).unwrap().params.insert("z".into(), 5.);
    graph
        .node_mut(s)
        .unwrap()
        .params
        .insert("factor".into(), 1.5);
    (graph, [b, m, s, o])
}
fn output_tree(graph: &Graph, id: NodeId) -> Tree {
    evaluate(graph).outputs.get(&id).unwrap().clone().unwrap()
}

#[test]
fn grouping_keeps_the_result_and_shows_one_node() {
    let (mut graph, [b, m, s, o]) = chain();
    let before = output_tree(&graph, o);
    let cluster_id = cluster::group_nodes(&mut graph, &[m, s], "移動と拡大").unwrap();
    assert_eq!(graph.nodes.len(), 3, "box + cluster + output");
    assert_eq!(graph.clusters.len(), 1);
    let node = graph.node(cluster_id).unwrap();
    assert_eq!(node.kind, NodeKind::Cluster);
    assert_eq!(
        node.inputs.get("in0"),
        Some(&b),
        "the box feeds the first slot"
    );
    graph.validate().unwrap();
    assert_eq!(output_tree(&graph, o), before);
    // Exposed numbers are live: change the cluster's Move Z slot and the result changes.
    let def = &graph.clusters[0];
    let z_slot = def
        .inputs
        .iter()
        .position(|p| p.node == m && p.key == "z")
        .unwrap();
    graph
        .node_mut(cluster_id)
        .unwrap()
        .params
        .insert(format!("in{z_slot}"), 20.);
    assert_ne!(output_tree(&graph, o), before);
}

#[test]
fn editing_the_definition_invalidates_the_cache() {
    let (mut graph, [_, m, s, o]) = chain();
    cluster::group_nodes(&mut graph, &[m, s], "部品").unwrap();
    let mut evaluator = Evaluator::default();
    let first = evaluator.evaluate(&graph, &|| false);
    let second = evaluator.evaluate(&graph, &|| false);
    assert_eq!(second.computed, 0);
    graph.clusters[0]
        .graph
        .node_mut(s)
        .unwrap()
        .params
        .insert("factor".into(), 3.);
    let third = evaluator.evaluate(&graph, &|| false);
    assert!(third.computed > 0);
    assert_ne!(third.outputs.get(&o), first.outputs.get(&o));
}

#[test]
fn cluster_round_trips_through_a_morph_file_and_still_evaluates() {
    let temp = TempDir::new();
    let path = temp.file("c.morph");
    let (mut graph, [_, m, s, o]) = chain();
    let before = output_tree(&graph, o);
    cluster::group_nodes(&mut graph, &[m, s], "部品").unwrap();
    let project = Project::new("まとめ", graph);
    persistence::save(&path, &project).unwrap();
    let loaded = persistence::load(&path).unwrap();
    assert_eq!(loaded, project);
    assert_eq!(output_tree(&loaded.graph, o), before);
}

#[test]
fn grouping_errors_leave_the_graph_untouched() {
    let (mut graph, [b, m, s, o]) = chain();
    let snapshot = graph.clone();
    assert!(cluster::group_nodes(&mut graph, &[], "x").is_err());
    assert!(cluster::group_nodes(&mut graph, &[999], "x").is_err());
    // box and scale without the move between them would make a loop
    assert!(cluster::group_nodes(&mut graph, &[b, s], "x").is_err());
    assert_eq!(graph, snapshot);
    // a middle result used outside the selection
    let extra = graph.add(NodeKind::Scale, [0., 100.]).unwrap();
    graph.connect(b, extra, "geometry").unwrap();
    let snapshot = graph.clone();
    assert!(cluster::group_nodes(&mut graph, &[b, m], "x").is_err());
    assert_eq!(graph, snapshot);
    let _ = (s, o);
}

#[test]
fn ambiguous_outputs_are_refused() {
    let mut graph = Graph::default();
    let sources: Vec<_> = (0..9)
        .map(|i| graph.add(NodeKind::Box, [i as f32, 0.]).unwrap())
        .collect();
    let merge_like: Vec<_> = (0..9)
        .map(|i| graph.add(NodeKind::Move, [i as f32, 50.]).unwrap())
        .collect();
    for (s, t) in sources.iter().zip(&merge_like) {
        graph.connect(*s, *t, "geometry").unwrap();
    }
    let snapshot = graph.clone();
    // nine independent results and no single output: ambiguous
    assert!(cluster::group_nodes(&mut graph, &merge_like, "x").is_err());
    assert_eq!(graph, snapshot);
}

fn part_chain(depth: u64) -> Graph {
    let mut graph = Graph::default();
    for level in 1..=depth {
        let mut inner = Graph::default();
        let output = if level < depth {
            let id = inner.add(NodeKind::Cluster, [0., 0.]).unwrap();
            inner.node_mut(id).unwrap().cluster = Some(level + 1);
            id
        } else {
            inner.add(NodeKind::Number, [0., 0.]).unwrap()
        };
        graph.clusters.push(ClusterDef {
            id: level,
            name: format!("L{level}"),
            graph: inner,
            inputs: Vec::new(),
            output,
        });
    }
    let node = graph.add(NodeKind::Cluster, [0., 0.]).unwrap();
    graph.node_mut(node).unwrap().cluster = Some(1);
    graph
}

#[test]
fn nesting_up_to_four_levels_works_and_five_is_refused() {
    let ok = part_chain(4);
    ok.validate().unwrap();
    let id = ok.nodes[0].id;
    assert_eq!(numbers(&output_tree(&ok, id)), vec![1.]);

    let too_deep = part_chain(5);
    assert!(too_deep.validate().unwrap_err().contains("深すぎ"));
    // The engine refuses it as well, even for a file that was never validated.
    let id = too_deep.nodes[0].id;
    let message = evaluate(&too_deep)
        .outputs
        .get(&id)
        .unwrap()
        .clone()
        .unwrap_err();
    assert!(message.contains("深すぎ"), "{message}");
}

#[test]
fn definition_cycles_are_refused_and_never_overflow_the_stack() {
    let mut graph = part_chain(2);
    // make level 2 point back at level 1
    let mut inner = Graph::default();
    let id = inner.add(NodeKind::Cluster, [0., 0.]).unwrap();
    inner.node_mut(id).unwrap().cluster = Some(1);
    graph.clusters[1].graph = inner;
    graph.clusters[1].output = id;
    assert!(graph.validate().is_err());
    let id = graph.nodes[0].id;
    assert!(evaluate(&graph).outputs.get(&id).unwrap().is_err());
}

#[test]
fn the_total_inner_node_cap_stops_huge_definitions() {
    let mut graph = Graph::default();
    let mut leaf = Graph::default();
    let mut leaf_out = 0;
    for _ in 0..200 {
        leaf_out = leaf.add(NodeKind::Number, [0., 0.]).unwrap();
    }
    graph.clusters.push(ClusterDef {
        id: 2,
        name: "leaf".into(),
        graph: leaf,
        inputs: Vec::new(),
        output: leaf_out,
    });
    let mut top = Graph::default();
    let mut top_out = 0;
    for _ in 0..30 {
        top_out = top.add(NodeKind::Cluster, [0., 0.]).unwrap();
        top.node_mut(top_out).unwrap().cluster = Some(2);
    }
    graph.clusters.push(ClusterDef {
        id: 1,
        name: "top".into(),
        graph: top,
        inputs: Vec::new(),
        output: top_out,
    });
    let node = graph.add(NodeKind::Cluster, [0., 0.]).unwrap();
    graph.node_mut(node).unwrap().cluster = Some(1);
    graph.validate().unwrap();
    let message = evaluate(&graph)
        .outputs
        .get(&node)
        .unwrap()
        .clone()
        .unwrap_err();
    assert!(message.contains("上限"), "{message}");
}

#[test]
fn a_cluster_node_without_a_definition_is_invalid() {
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Cluster, [0., 0.]).unwrap();
    assert!(graph.validate().is_err());
    graph.node_mut(id).unwrap().cluster = Some(7);
    assert!(graph.validate().is_err());
}

// ------------------------------------------------------------------ .morphpart

#[test]
fn parts_can_be_saved_and_loaded_into_another_work() {
    let temp = TempDir::new();
    let path = temp.file("部品.morphpart");
    let (mut graph, [_, m, s, o]) = chain();
    let before = output_tree(&graph, o);
    cluster::group_nodes(&mut graph, &[m, s], "移動と拡大").unwrap();
    cluster::save_part(&path, &graph, graph.clusters[0].id).unwrap();

    let mut other = Graph::default();
    let b = other.add(NodeKind::Box, [0., 0.]).unwrap();
    let out = other.add(NodeKind::Output, [300., 0.]).unwrap();
    let def_id = cluster::load_part(&path, &mut other).unwrap();
    let node = cluster::instantiate(&mut other, def_id, [100., 0.]).unwrap();
    other.connect(b, node, "in0").unwrap();
    other.connect(node, out, "geometry").unwrap();
    other.validate().unwrap();
    assert_eq!(output_tree(&other, out), before);

    // Importing twice keeps both definitions apart.
    let again = cluster::load_part(&path, &mut other).unwrap();
    assert_ne!(again, def_id);
    assert_eq!(other.clusters.len(), 2);
}

#[test]
fn parts_with_nested_definitions_are_renumbered_on_import() {
    let graph = part_chain(3);
    let text = cluster::export_part(&graph, 1).unwrap();
    let mut other = part_chain(1);
    let root = cluster::import_part(&mut other, text.as_bytes()).unwrap();
    other.validate().unwrap();
    assert_eq!(other.clusters.len(), 4);
    let node = cluster::instantiate(&mut other, root, [0., 0.]).unwrap();
    assert_eq!(numbers(&output_tree(&other, node)), vec![1.]);
}

#[test]
fn bad_part_files_are_rejected_without_changing_the_work() {
    let (mut graph, [_, m, s, _]) = chain();
    cluster::group_nodes(&mut graph, &[m, s], "部品").unwrap();
    let good = cluster::export_part(&graph, graph.clusters[0].id).unwrap();
    let mut target = Graph::default();
    let snapshot = target.clone();

    assert!(cluster::import_part(&mut target, b"not json").is_err());
    assert!(cluster::import_part(&mut target, b"{}").is_err());
    let wrong_format = good.replace("morphlet-part", "something-else");
    assert!(cluster::import_part(&mut target, wrong_format.as_bytes()).is_err());
    let wrong_version = good.replace("\"version\": 1", "\"version\": 99");
    assert!(cluster::import_part(&mut target, wrong_version.as_bytes()).is_err());
    let unknown_field = good.replacen('{', "{\"extra\": 1,", 1);
    assert!(cluster::import_part(&mut target, unknown_field.as_bytes()).is_err());
    let missing_root = good.replace("\"root\": 1", "\"root\": 42");
    assert!(cluster::import_part(&mut target, missing_root.as_bytes()).is_err());
    let oversized = vec![b' '; cluster::MAX_PART_BYTES + 1];
    assert!(cluster::import_part(&mut target, &oversized).is_err());
    assert!(cluster::export_part(&graph, 999).is_err());
    assert_eq!(target, snapshot);
    assert!(cluster::import_part(&mut target, good.as_bytes()).is_ok());
}

#[test]
fn a_part_file_cannot_smuggle_an_oversized_script() {
    let (mut graph, id) = script_graph("1");
    cluster::group_nodes(&mut graph, &[id], "s").unwrap();
    let mut file =
        cluster::parse_part(cluster::export_part(&graph, 1).unwrap().as_bytes()).unwrap();
    file.clusters[0].graph.nodes[0].script = "x".repeat(script::MAX_SCRIPT_BYTES + 1);
    let text = serde_json::to_string(&file).unwrap();
    assert!(cluster::parse_part(text.as_bytes()).is_err());
}

#[test]
fn scripts_inside_clusters_run() {
    let mut graph = Graph::default();
    let s = graph.add(NodeKind::Script, [0., 0.]).unwrap();
    graph.node_mut(s).unwrap().script = "a * 10".into();
    let cluster_id = cluster::group_nodes(&mut graph, &[s], "x10").unwrap();
    graph
        .node_mut(cluster_id)
        .unwrap()
        .params
        .insert("in0".into(), 4.);
    assert_eq!(numbers(&output_tree(&graph, cluster_id)), vec![40.]);
}
