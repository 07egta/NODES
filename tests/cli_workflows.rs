//! Integration tests execute the real console binary, including on Windows.
use morphlet::graph_api;
use morphlet::model::{Graph, NodeKind, Tree, Value};
use morphlet::persistence::{self, Project};
use serde_json::{Value as Json, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nodes-cli-test-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invoke(command: &str, paths: &[&Path]) -> (Output, Json) {
    let output = Command::new(env!("CARGO_BIN_EXE_nodes-cli"))
        .arg(command)
        .args(paths)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{command} did not return JSON: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output, json)
}

fn successful(command: &str, paths: &[&Path]) -> Json {
    let (output, report) = invoke(command, paths);
    assert!(
        output.status.success(),
        "{command}: {report}; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report["ok"], true);
    assert_eq!(report["cli_version"], 1);
    assert_eq!(report["api_version"], graph_api::API_VERSION);
    report["result"].clone()
}

fn failed(command: &str, paths: &[&Path]) -> Json {
    let (output, report) = invoke(command, paths);
    assert_eq!(output.status.code(), Some(1), "{command}: {report}");
    assert_eq!(report["ok"], false);
    assert!(report["error"]["code"].is_string());
    report["error"].clone()
}

fn node(report: &Json, id: u64) -> &Json {
    report["evaluation"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == id)
        .unwrap()
}

fn save_graph(path: &Path, graph: Graph) {
    persistence::save(path, &Project::new("CLI fixture", graph)).unwrap();
}

fn mesh_graph(with_output: bool) -> Graph {
    let mut graph = Graph::default();
    let shape = graph.add(NodeKind::Box, [0.0, 0.0]).unwrap();
    for (key, value) in [("width", 20.0), ("depth", 10.0), ("height", 12.0)] {
        graph
            .node_mut(shape)
            .unwrap()
            .params
            .insert(key.into(), value);
    }
    if with_output {
        let output = graph.add(NodeKind::Output, [240.0, 0.0]).unwrap();
        graph.connect(shape, output, "geometry").unwrap();
    }
    graph
}

#[test]
fn console_help_catalog_and_usage_have_real_stdout_and_exit_codes() {
    let help = Command::new(env!("CARGO_BIN_EXE_nodes-cli"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help_text = String::from_utf8(help.stdout).unwrap();
    for command in [
        "catalog",
        "validate",
        "evaluate",
        "build",
        "apply",
        "export-stl",
    ] {
        assert!(help_text.contains(command));
    }
    let catalog = successful("catalog", &[]);
    let kinds: Vec<_> = catalog["catalog"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"box"));
    assert!(kinds.contains(&"add"));
    for blocked in [
        "script",
        "expression",
        "import_mesh",
        "cluster",
        "path_mapper",
    ] {
        assert!(
            !kinds.contains(&blocked),
            "{blocked} must not be exposed by the safe CLI"
        );
    }
    for (command, paths) in [
        ("not-a-command", Vec::<&Path>::new()),
        ("evaluate", Vec::new()),
        ("catalog", vec![Path::new("unexpected")]),
    ] {
        let (output, response) = invoke(command, &paths);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(response["error"]["code"], "usage");
    }
}

#[test]
fn build_edit_save_reload_and_evaluate_preserve_project_metadata() {
    let temporary = TemporaryDirectory::new();
    let request = temporary.path("build.json");
    let source = temporary.path("original.morph");
    let edited = temporary.path("日本語の編集.morph");
    fs::write(&request, include_bytes!("../examples/interop/ai-add.json")).unwrap();
    let built = successful("build", &[&request, &source]);
    assert_eq!(node(&built, 2)["data"]["preview"][0]["value"], 12.0);

    let mut project = persistence::load(&source).unwrap();
    project.name = "引き継ぐ作品名".into();
    project.pan = [88.0, -9.0];
    project.zoom = 1.75;
    project.graph.node_mut(0).unwrap().name = "既存の入力".into();
    project.graph.node_mut(0).unwrap().position = [123.0, 456.0];
    project.graph.node_mut(0).unwrap().preview = false;
    persistence::save(&source, &project).unwrap();
    let source_bytes = fs::read(&source).unwrap();
    let edit = temporary.path("edit.json");
    fs::write(
        &edit,
        serde_json::to_vec(&json!({
            "api_version": 1,
            "edits": [{"op":"set", "node":"0", "input":"value", "value":11}]
        }))
        .unwrap(),
    )
    .unwrap();
    let applied = successful("apply", &[&source, &edit, &edited]);
    assert_eq!(node(&applied, 2)["data"]["preview"][0]["value"], 16.0);
    project
        .graph
        .node_mut(0)
        .unwrap()
        .params
        .insert("value".into(), 11.0);
    assert_eq!(persistence::load(&edited).unwrap(), project);
    assert_eq!(fs::read(&source).unwrap(), source_bytes);

    let validation = successful("validate", &[&edited]);
    assert_eq!(validation["evaluation_performed"], false);
    assert_eq!(validation["project"]["name"], "引き継ぐ作品名");
    let reloaded = successful("evaluate", &[&edited]);
    assert_eq!(node(&reloaded, 2)["data"]["preview"][0]["value"], 16.0);
    assert_eq!(reloaded["evaluation"]["cached"], 0);
}

#[test]
fn evaluation_errors_have_node_ids_and_failed_apply_preserves_existing_destination() {
    let temporary = TemporaryDirectory::new();
    let path = temporary.path("divide.morph");
    let mut graph = Graph::default();
    let divide = graph.add(NodeKind::Divide, [0.0, 0.0]).unwrap();
    save_graph(&path, graph.clone());
    let before = fs::read(&path).unwrap();
    let request = temporary.path("zero.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "api_version": 1,
            "edits": [{"op":"set", "node":divide.to_string(), "input":"b", "value":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let error = failed("apply", &[&path, &request, &path]);
    assert_eq!(error["code"], "evaluation_failed");
    assert!(
        error["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["node_id"] == divide)
    );
    assert_eq!(fs::read(&path).unwrap(), before);

    graph
        .node_mut(divide)
        .unwrap()
        .params
        .insert("b".into(), 0.0);
    let invalid = temporary.path("evaluation-error.morph");
    save_graph(&invalid, graph);
    successful("validate", &[&invalid]);
    let error = failed("evaluate", &[&invalid]);
    assert_eq!(error["code"], "evaluation_failed");
    assert_eq!(error["nodes"][0]["node_id"], divide);
    assert!(
        error["nodes"][0]["message"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

#[test]
fn blocked_capabilities_and_oversized_requests_never_replace_destination() {
    let temporary = TemporaryDirectory::new();
    let request = temporary.path("blocked.json");
    let destination = temporary.path("keep.morph");
    fs::write(&destination, b"KEEP EXISTING DESTINATION").unwrap();
    for kind in ["script", "expression", "import_mesh", "cluster"] {
        fs::write(
            &request,
            serde_json::to_vec(&json!({
                "api_version":1,
                "edits":[{"op":"add","alias":"blocked","kind":kind,"position":[0,0]}]
            }))
            .unwrap(),
        )
        .unwrap();
        failed("build", &[&request, &destination]);
        assert_eq!(
            fs::read(&destination).unwrap(),
            b"KEEP EXISTING DESTINATION"
        );
    }
    fs::write(&request, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert_eq!(
        failed("build", &[&request, &destination])["code"],
        "request_size"
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"KEEP EXISTING DESTINATION"
    );

    let mut graph = Graph::default();
    graph.add(NodeKind::Script, [0.0, 0.0]).unwrap();
    let script_project = temporary.path("script.morph");
    save_graph(&script_project, graph);
    for command in ["validate", "evaluate"] {
        assert_eq!(
            failed(command, &[&script_project])["code"],
            "invalid_project"
        );
    }
    failed("export-stl", &[&script_project, &destination]);
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"KEEP EXISTING DESTINATION"
    );
}

#[test]
fn explicit_mesh_output_roundtrips_stl_and_excludes_intermediate_meshes() {
    let temporary = TemporaryDirectory::new();
    let project = temporary.path("box.morph");
    let stl = temporary.path("box.stl");
    let mut graph = mesh_graph(true);
    // This unselected shape must never be exported merely because it is a mesh.
    graph.add(NodeKind::Box, [0.0, 200.0]).unwrap();
    save_graph(&project, graph);
    let evaluated = successful("evaluate", &[&project]);
    let preview = &node(&evaluated, 0)["data"]["preview"][0];
    assert_eq!(preview["kind"], "mesh");
    assert_eq!(preview["triangle_count"], 12);
    assert!(preview.get("vertices").is_none());
    assert!(preview.get("triangles").is_none());

    let report = successful("export-stl", &[&project, &stl]);
    assert_eq!(report["output_node_ids"], json!([1]));
    assert_eq!(report["mesh_count"], 1);
    assert_eq!(report["triangles"], 12);
    assert_eq!(report["skipped_values"], 0);
    assert_eq!(report["printability_verified"], false);
    let bytes = fs::read(stl).unwrap();
    assert_eq!(bytes.len(), 84 + 50 * 12);
    let mesh = morphlet::import::parse_stl(&bytes, &|| false).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
    let imported = persistence::mesh_stats(&[Tree::single(Value::Mesh(Arc::new(mesh)))]).unwrap();
    assert!(imported.is_closed());
    assert_eq!(
        imported.bounds.unwrap(),
        [[-10.0, -5.0, 0.0], [10.0, 5.0, 12.0]]
    );
    assert!((imported.signed_volume_mm3 - 2400.0).abs() < 1e-8);
}

#[test]
fn export_rejects_missing_mixed_and_disabled_outputs_before_writing() {
    let temporary = TemporaryDirectory::new();
    let project = temporary.path("selection.morph");
    let destination = temporary.path("keep.stl");
    fs::write(&destination, b"KEEP STL").unwrap();
    save_graph(&project, mesh_graph(false));
    assert_eq!(
        failed("export-stl", &[&project, &destination])["code"],
        "output_required"
    );

    let mut mixed = mesh_graph(true);
    let circle = mixed.add(NodeKind::Circle, [0.0, 200.0]).unwrap();
    let curve_output = mixed.add(NodeKind::Output, [240.0, 200.0]).unwrap();
    mixed.connect(circle, curve_output, "geometry").unwrap();
    save_graph(&project, mixed);
    let error = failed("export-stl", &[&project, &destination]);
    assert_eq!(error["code"], "unsupported_output");
    assert_eq!(error["nodes"][0]["node_id"], curve_output);
    assert_eq!(fs::read(&destination).unwrap(), b"KEEP STL");

    let mut disabled = mesh_graph(true);
    disabled.node_mut(1).unwrap().off = true;
    save_graph(&project, disabled);
    assert_eq!(
        failed("export-stl", &[&project, &destination])["code"],
        "disabled_output"
    );
    assert_eq!(fs::read(&destination).unwrap(), b"KEEP STL");
    let before = fs::read(&project).unwrap();
    assert_eq!(
        failed("export-stl", &[&project, &project])["code"],
        "invalid_destination"
    );
    assert_eq!(fs::read(&project).unwrap(), before);
}

#[test]
fn evaluation_previews_are_bounded_even_for_large_number_lists() {
    let temporary = TemporaryDirectory::new();
    let project = temporary.path("series.morph");
    let mut graph = Graph::default();
    let series = graph.add(NodeKind::Series, [0.0, 0.0]).unwrap();
    graph
        .node_mut(series)
        .unwrap()
        .params
        .insert("count".into(), 25.0);
    save_graph(&project, graph);
    let report = successful("evaluate", &[&project]);
    let data = &node(&report, series)["data"];
    assert_eq!(data["item_count"], 25);
    assert_eq!(data["preview"].as_array().unwrap().len(), 8);
    assert_eq!(data["preview_truncated"], true);
}
