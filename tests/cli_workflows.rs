//! Integration tests execute the real console binary, including on Windows.
use nodes::graph_api;
use nodes::model::{Graph, NodeKind, Tree, Value};
use nodes::persistence::{self, Project};
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
        "diagnose",
        "build",
        "apply",
        "export-stl",
        "inspect-ghx",
        "convert-ghx",
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
        ("diagnose", Vec::new()),
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
fn console_apply_supports_transactional_structural_edits() {
    let temporary = TemporaryDirectory::new();
    let source = temporary.path("original.morph");
    let edited = temporary.path("edited.morph");
    let request = temporary.path("edit.json");
    let mut project = graph_api::build(
        &graph_api::parse_request(include_bytes!("../examples/interop/ai-add.json")).unwrap(),
    )
    .unwrap();
    project.name = "構造編集".into();
    nodes::annotations::add_group(&mut project.graph, &[0, 1], "inputs").unwrap();
    persistence::save(&source, &project).unwrap();
    let source_bytes = fs::read(&source).unwrap();

    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "api_version": 1,
            "edits": [
                {"op":"disconnect","to":"2","input":"a"},
                {"op":"remove","node":"1"},
                {"op":"set","node":"2","input":"b","value":17},
                {"op":"reset","node":"2","input":"b"},
                {"op":"move","node":"0","position":[123,-50]}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let result = successful("apply", &[&source, &request, &edited]);
    assert_eq!(node(&result, 2)["data"]["preview"][0]["value"], 10.0);
    assert_eq!(result["node_ids"], json!({"0":0,"2":2}));
    let reloaded = persistence::load(&edited).unwrap();
    assert_eq!(reloaded.name, project.name);
    assert_eq!(reloaded.graph.nodes.len(), 2);
    assert_eq!(reloaded.graph.next_id, project.graph.next_id);
    assert_eq!(reloaded.graph.groups[0].nodes, vec![0]);
    assert_eq!(reloaded.graph.node(0).unwrap().position, [123., -50.]);
    assert!(reloaded.graph.node(2).unwrap().inputs.is_empty());
    assert_eq!(fs::read(&source).unwrap(), source_bytes);

    let edited_bytes = fs::read(&edited).unwrap();
    fs::write(
        &request,
        br#"{"api_version":1,"edits":[
            {"op":"remove","node":"0"},
            {"op":"move","node":"0","position":[0,0]}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        failed("apply", &[&edited, &request, &edited])["code"],
        "apply_failed"
    );
    assert_eq!(fs::read(&edited).unwrap(), edited_bytes);
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
    assert_eq!(error["diagnostics"]["summary"]["root_failure_count"], 1);
    assert_eq!(
        error["diagnostics"]["root_failures"][0]["node"]["id"],
        divide
    );
    assert!(
        error["nodes"][0]["message"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

#[test]
fn diagnose_reports_independent_causes_and_downstream_errors_without_writing() {
    let temporary = TemporaryDirectory::new();
    let path = temporary.path("原因を探す.morph");
    let mut graph = Graph::default();
    let first = graph.add(NodeKind::Divide, [0.0, 0.0]).unwrap();
    let second = graph.add(NodeKind::Divide, [0.0, 200.0]).unwrap();
    for id in [first, second] {
        graph.node_mut(id).unwrap().params.insert("b".into(), 0.0);
    }
    let join = graph.add(NodeKind::Add, [240.0, 0.0]).unwrap();
    graph.connect(first, join, "a").unwrap();
    graph.connect(second, join, "b").unwrap();
    let downstream = graph.add(NodeKind::Multiply, [480.0, 0.0]).unwrap();
    graph.connect(join, downstream, "a").unwrap();
    let unrelated = graph.add(NodeKind::Number, [0.0, 400.0]).unwrap();
    let disabled = graph.add(NodeKind::Divide, [240.0, 400.0]).unwrap();
    graph.node_mut(disabled).unwrap().off = true;
    graph
        .node_mut(disabled)
        .unwrap()
        .params
        .insert("b".into(), 0.0);
    save_graph(&path, graph);
    let before = fs::read(&path).unwrap();

    let error = failed("diagnose", &[&path]);
    assert_eq!(error["code"], "evaluation_failed");
    let report = &error["diagnostics"];
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["causality"], "dependency_inference");
    assert_eq!(report["summary"]["node_count"], 6);
    assert_eq!(report["summary"]["failed_count"], 4);
    assert_eq!(report["summary"]["root_failure_count"], 2);
    assert_eq!(report["summary"]["blocked_count"], 2);
    assert_eq!(report["summary"]["disabled_count"], 1);
    assert_eq!(report["summary"]["unevaluated_count"], 0);
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["summary"]["cancelled"], false);
    assert_eq!(report["root_failures"][0]["node"]["id"], first);
    assert_eq!(report["root_failures"][1]["node"]["id"], second);
    for root in report["root_failures"].as_array().unwrap() {
        assert_eq!(root["affected_nodes"], json!([join, downstream]));
        assert_eq!(root["affected_count"], 2);
        assert_eq!(root["affected_nodes_truncated"], false);
    }
    for blocked in report["blocked_nodes"].as_array().unwrap() {
        assert_eq!(blocked["upstream_roots"], json!([first, second]));
        assert_eq!(blocked["upstream_root_count"], 2);
        assert!(!blocked["message"].as_str().unwrap().is_empty());
        assert_ne!(blocked["node"]["id"], unrelated);
        assert_ne!(blocked["node"]["id"], disabled);
    }
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn diagnose_success_returns_compact_measured_report_without_geometry_payloads() {
    let temporary = TemporaryDirectory::new();
    let path = temporary.path("box.morph");
    save_graph(&path, mesh_graph(true));
    let before = fs::read(&path).unwrap();
    let result = successful("diagnose", &[&path]);
    let report = &result["diagnostics"];
    assert_eq!(result["project"]["node_count"], 2);
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["summary"]["failed_count"], 0);
    assert_eq!(report["summary"]["computed_count"], 2);
    assert_eq!(report["summary"]["cached_count"], 0);
    assert!(
        report["summary"]["elapsed_ms"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    assert_eq!(report["hotspots"].as_array().unwrap().len(), 2);
    for hotspot in report["hotspots"].as_array().unwrap() {
        assert!(hotspot["elapsed_ms"].as_f64().unwrap() >= 0.0);
        assert_eq!(hotspot["cached"], false);
        assert_eq!(hotspot["completed"], true);
        assert!(hotspot.get("data").is_none());
    }
    assert!(report["root_failures"].as_array().unwrap().is_empty());
    assert!(report["blocked_nodes"].as_array().unwrap().is_empty());
    assert_eq!(fs::read(&path).unwrap(), before);
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
    for command in ["validate", "evaluate", "diagnose"] {
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
    let mesh = nodes::import::parse_stl(&bytes, &|| false).unwrap();
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

#[cfg(unix)]
#[test]
fn fifo_projects_return_json_errors_without_waiting_for_a_writer() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let temporary = TemporaryDirectory::new();
    let fifo = temporary.path("input.morph");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("Unix CLI tests require mkfifo")
            .success()
    );

    for command in ["validate", "evaluate", "diagnose"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_nodes-cli"))
            .arg(command)
            .arg(&fifo)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("Could not poll {command}: {error}");
                }
            }
            if Instant::now() >= deadline {
                // Reap even the old blocking implementation so regression runs
                // fail promptly instead of hanging the entire test suite.
                let killed = child.kill();
                let waited = child.wait();
                panic!("{command} waited for a FIFO writer; kill={killed:?}; wait={waited:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        let report: Json = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{command} did not return JSON: {error}; stderr={}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
        assert_eq!(output.status.code(), Some(1), "{command}: {report}");
        assert_eq!(report["ok"], false);
        assert_eq!(report["error"]["code"], "invalid_project");
        assert_eq!(report["error"]["message"], "project must be a regular file");
    }
}

#[test]
fn ghx_console_inspects_and_converts_without_modifying_source() {
    let temporary = TemporaryDirectory::new();
    let input = temporary.path("移行.ghx");
    let report = temporary.path("report.json");
    let output = temporary.path("converted.morph");
    let bytes = include_bytes!("../examples/interop/scalar-point.ghx");
    fs::write(&input, bytes).unwrap();

    let inspected = successful("inspect-ghx", &[&input, &report]);
    assert_eq!(inspected["evaluation_performed"], false);
    let details: Json = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(details["objects"].as_array().unwrap().len(), 2);
    assert_eq!(details["connections"].as_array().unwrap().len(), 1);
    assert_eq!(details["source_xml"], std::str::from_utf8(bytes).unwrap());

    let converted = successful("convert-ghx", &[&input, &output]);
    assert_eq!(converted["project"]["node_count"], 2);
    let project = persistence::load(&output).unwrap();
    let evaluation = graph_api::evaluate(&project.graph, &|| false).unwrap();
    assert_eq!(
        evaluation.outputs[&project.graph.nodes[1].id]
            .as_ref()
            .unwrap()
            .branches
            .values()
            .flatten()
            .next(),
        Some(&Value::Point([7., 2., 3.]))
    );
    assert_eq!(fs::read(&input).unwrap(), bytes);
}

#[test]
fn ghx_console_rejects_unsupported_graph_without_overwriting_output() {
    let temporary = TemporaryDirectory::new();
    let input = temporary.path("unknown.ghx");
    let report = temporary.path("report.json");
    let output = temporary.path("existing.morph");
    let bytes = include_str!("../examples/interop/scalar-point.ghx").replace(
        "3581f42a-9592-4549-bd6b-1c0fc39d067b",
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
    );
    fs::write(&input, &bytes).unwrap();
    fs::write(&output, b"keep existing destination").unwrap();
    successful("inspect-ghx", &[&input, &report]);
    assert_eq!(
        failed("convert-ghx", &[&input, &output])["code"],
        "ghx_conversion_failed"
    );
    assert_eq!(fs::read(&output).unwrap(), b"keep existing destination");
    assert_eq!(fs::read_to_string(&input).unwrap(), bytes);
}
