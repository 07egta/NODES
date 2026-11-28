//! Offline console entry point. Keep the Windows console subsystem default:
//! the separate NODES GUI binary intentionally uses the Windows GUI subsystem.
use nodes::diagnostics::{self, DiagnosticReport};
use nodes::engine::Evaluation;
use nodes::ghx;
use nodes::graph_api::{self, Request};
use nodes::model::{NodeKind, Tree, Value};
use nodes::node_package::Package;
use nodes::persistence::{self, Project};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

const CLI_VERSION: u32 = 1;
// The graph API independently enforces the same request limit.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const PREVIEW_ITEMS: usize = 8;
const MAX_ERROR_CHARS: usize = 2048;
const HELP: &str = "\
NODES offline console CLI
Usage:
  nodes-cli catalog
  nodes-cli validate PROJECT.morph
  nodes-cli evaluate PROJECT.morph
  nodes-cli diagnose PROJECT.morph
  nodes-cli diff BEFORE.morph AFTER.morph
  nodes-cli diff-content BEFORE.morph AFTER.morph
  nodes-cli build REQUEST.json OUTPUT.morph
  nodes-cli apply PROJECT.morph REQUEST.json OUTPUT.morph
  nodes-cli export-stl PROJECT.morph OUTPUT.stl
  nodes-cli inspect-ghx INPUT.ghx REPORT.json
  nodes-cli convert-ghx INPUT.ghx OUTPUT.morph
  nodes-cli pack-part MANIFEST.json PART.morphpart OUTPUT.nodespkg
  nodes-cli inspect-package PACKAGE.nodespkg
  nodes-cli unpack-part PACKAGE.nodespkg OUTPUT.morphpart
  nodes-cli --help

Commands return one JSON document on stdout, including operational errors.
Exit codes: 0 success, 1 project/evaluation/I/O failure, 2 invalid command usage.
validate checks structure and the restricted capability policy without evaluation.
diagnose evaluates once and reports root failure candidates and measured node costs.
diff compares native files without evaluating them; diff-content ignores presentation and metadata.
Evaluation failures also include error.diagnostics; exit code remains 1.
build and apply evaluate every node before an atomic project save.
apply preserves project metadata and supports saving back to the input project.
export-stl requires explicit, enabled, nonempty Output nodes containing only Mesh.
Scripts, expressions, imports, clusters and external plugins are unavailable for Graph API evaluation.
GHX inspection writes an inert report; conversion evaluates the supported scalar subset.
Package commands inspect or copy data-only Cluster parts without evaluating them.
Request limit: 1 MiB. Project and GHX limit: 8 MiB. Parent output directories must exist.
";

#[derive(Debug)]
struct CliError {
    code: &'static str,
    message: String,
    nodes: Vec<Json>,
    diagnostics: Option<Box<DiagnosticReport>>,
}

fn problem(code: &'static str, message: impl AsRef<str>) -> CliError {
    CliError {
        code,
        message: message.as_ref().chars().take(MAX_ERROR_CHARS).collect(),
        nodes: Vec::new(),
        diagnostics: None,
    }
}

fn node_problem(code: &'static str, node_id: u64, message: &str) -> CliError {
    let mut error = problem(code, message);
    error
        .nodes
        .push(json!({"node_id": node_id, "message": error.message}));
    error
}

fn load_project(path: &Path) -> Result<Project, CliError> {
    graph_api::load_safe(path).map_err(|message| problem("invalid_project", message))
}

fn read_request(path: &Path) -> Result<Request, CliError> {
    let metadata = fs::metadata(path).map_err(|error| problem("request_io", error.to_string()))?;
    if !metadata.is_file() {
        return Err(problem(
            "invalid_request",
            "Request must be a regular file.",
        ));
    }
    if metadata.len() > MAX_REQUEST_BYTES as u64 {
        return Err(problem("request_size", "Request exceeds the 1 MiB limit."));
    }
    let file = File::open(path).map_err(|error| problem("request_io", error.to_string()))?;
    if !file
        .metadata()
        .map_err(|error| problem("request_io", error.to_string()))?
        .is_file()
    {
        return Err(problem(
            "invalid_request",
            "Request must be a regular file.",
        ));
    }
    let mut bytes = Vec::new();
    file.take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| problem("request_io", error.to_string()))?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(problem("request_size", "Request exceeds the 1 MiB limit."));
    }
    graph_api::parse_request(&bytes).map_err(|message| problem("invalid_request", message))
}

// Request JSON is an input artifact, unlike a project explicitly edited in place.
fn protect_request(request: &Path, destination: &Path) -> Result<(), CliError> {
    if request == destination
        || matches!((fs::canonicalize(request), fs::canonicalize(destination)),
            (Ok(source), Ok(target)) if source == target)
    {
        return Err(problem(
            "invalid_destination",
            "Project output must differ from the input request.",
        ));
    }
    Ok(())
}

fn checked_evaluate(project: &Project) -> Result<Evaluation, CliError> {
    let result = graph_api::evaluate(&project.graph, &|| false)
        .map_err(|message| problem("invalid_graph", message))?;
    let mut errors = Vec::new();
    for node in &project.graph.nodes {
        let message = match result.outputs.get(&node.id) {
            Some(Ok(_)) => continue,
            Some(Err(message)) => message.as_str(),
            None => "No evaluation result was produced for this node.",
        };
        errors.push(json!({
            "node_id": node.id,
            "kind": node.kind,
            "message": message.chars().take(MAX_ERROR_CHARS).collect::<String>()
        }));
    }
    if result.cancelled || !errors.is_empty() {
        return Err(CliError {
            code: if result.cancelled {
                "evaluation_cancelled"
            } else {
                "evaluation_failed"
            },
            message: format!(
                "Evaluation did not complete successfully; {} node error(s).",
                errors.len()
            ),
            nodes: errors,
            diagnostics: Some(Box::new(diagnostics::diagnose(&project.graph, &result))),
        });
    }
    Ok(result)
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Number(_) => "number",
        Value::Point(_) => "point",
        Value::Curve(_) => "curve",
        Value::Mesh(_) => "mesh",
        Value::Surface(_) => "surface",
        Value::Solid(_) => "solid",
        Value::Brep(_) => "brep",
        Value::Cage(_) => "cage",
        Value::Gcode(_) => "gcode",
    }
}

fn preview(value: &Value) -> Json {
    match value {
        Value::Number(number) => json!({"kind": "number", "value": number}),
        Value::Point(point) => json!({"kind": "point", "value": point}),
        Value::Curve(curve) => json!({
            "kind": "curve", "display_points": curve.points.len(), "closed": curve.closed,
            "has_curve_geometry": curve.geometry.is_some()
        }),
        Value::Mesh(mesh) => json!({
            "kind": "mesh", "vertex_count": mesh.vertices.len(), "triangle_count": mesh.triangles.len()
        }),
        Value::Surface(_) => json!({"kind": "surface"}),
        Value::Solid(_) => json!({"kind": "solid"}),
        Value::Brep(b) => json!({
            "kind": "brep", "faces": b.face_count(), "closed": b.is_closed(),
            "triangle_count": b.preview().triangles.len()
        }),
        Value::Cage(c) => json!({
            "kind": "cage", "control_points": c.vertices().len(), "faces": c.faces().len()
        }),
        Value::Gcode(job) => json!({
            "kind": "gcode", "path_count": job.paths.len(), "layers": job.layer_count()
        }),
    }
}

fn tree_summary(tree: &Tree) -> Json {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for value in tree.branches.values().flatten() {
        *kinds.entry(value_kind(value)).or_default() += 1;
    }
    let items = tree.item_count();
    let preview: Vec<_> = tree
        .branches
        .values()
        .flatten()
        .take(PREVIEW_ITEMS)
        .map(preview)
        .collect();
    json!({
        "branch_count": tree.branches.len(),
        "empty_branches": tree.branches.values().filter(|items| items.is_empty()).count(),
        "item_count": items,
        "kinds": kinds,
        "preview": preview,
        "preview_truncated": items > PREVIEW_ITEMS
    })
}

fn project_summary(project: &Project) -> Json {
    json!({
        "name": project.name,
        "schema_version": project.schema_version,
        "node_count": project.graph.nodes.len()
    })
}

fn evaluation_summary(project: &Project, evaluation: &Evaluation) -> Json {
    let nodes: Vec<_> = project
        .graph
        .nodes
        .iter()
        .map(|node| {
            let mut result = json!({
                "node_id": node.id, "kind": node.kind, "disabled": node.off,
                "elapsed_ms": evaluation.node_timings.get(&node.id).map(|timing| timing.elapsed_ms)
            });
            if let Some(Ok(tree)) = evaluation.outputs.get(&node.id) {
                result["data"] = tree_summary(tree);
            }
            result
        })
        .collect();
    json!({
        "computed": evaluation.computed, "cached": evaluation.cached,
        "elapsed_ms": evaluation.elapsed_ms, "cancelled": evaluation.cancelled,
        "preview_limit_per_node": PREVIEW_ITEMS, "nodes": nodes
    })
}

fn save_evaluated(project: &Project, destination: &Path) -> Result<Json, CliError> {
    let evaluation = checked_evaluate(project)?;
    persistence::save(destination, project).map_err(|message| problem("save_failed", message))?;
    Ok(json!({
        "destination": destination.to_string_lossy(),
        "project": project_summary(project),
        "evaluation": evaluation_summary(project, &evaluation)
    }))
}

fn export_stl(project_path: &Path, destination: &Path) -> Result<Json, CliError> {
    if project_path == destination
        || matches!((fs::canonicalize(project_path), fs::canonicalize(destination)),
            (Ok(source), Ok(target)) if source == target)
    {
        return Err(problem(
            "invalid_destination",
            "STL output must differ from the input project.",
        ));
    }
    let project = load_project(project_path)?;
    let mut evaluation = checked_evaluate(&project)?;
    let outputs: Vec<_> = project
        .graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Output)
        .collect();
    if outputs.is_empty() {
        return Err(problem(
            "output_required",
            "Connect the intended mesh to an explicit Output node.",
        ));
    }
    let mut trees = Vec::with_capacity(outputs.len());
    let mut output_ids = Vec::with_capacity(outputs.len());
    for node in outputs {
        if node.off {
            return Err(node_problem(
                "disabled_output",
                node.id,
                "An Output node is disabled.",
            ));
        }
        let tree = evaluation
            .outputs
            .remove(&node.id)
            .ok_or_else(|| {
                node_problem(
                    "missing_output",
                    node.id,
                    "The Output node has no evaluation result.",
                )
            })?
            .map_err(|message| node_problem("evaluation_failed", node.id, &message))?;
        if tree.item_count() == 0 {
            return Err(node_problem(
                "empty_output",
                node.id,
                "The Output node contains no mesh values.",
            ));
        }
        for value in tree.branches.values().flatten() {
            match value {
                Value::Mesh(mesh) if !mesh.vertices.is_empty() && !mesh.triangles.is_empty() => {}
                Value::Mesh(_) => {
                    return Err(node_problem(
                        "empty_mesh",
                        node.id,
                        "The Output node contains an empty mesh.",
                    ));
                }
                _ => {
                    return Err(node_problem(
                        "unsupported_output",
                        node.id,
                        &format!(
                            "Output contains {} data. Explicit mesh conversion is required for STL.",
                            value_kind(value)
                        ),
                    ));
                }
            }
        }
        output_ids.push(node.id);
        trees.push(tree);
    }
    // Every selected value was checked before any write. The shared exporter
    // validates indices, coordinates, degeneracy and f32 STL precision, then
    // uses persistence::atomic_write; no intermediate graph output is exported.
    let report = persistence::export_stl(destination, &trees)
        .map_err(|message| problem("export_failed", message))?;
    Ok(json!({
        "destination": destination.to_string_lossy(),
        "project": project_summary(&project),
        "output_node_ids": output_ids,
        "format": "binary_stl",
        "units": "millimeters",
        "mesh_count": report.mesh_count,
        "vertices": report.vertices,
        "triangles": report.triangles,
        "bounds": report.bounds,
        "signed_volume_mm3": report.signed_volume_mm3,
        "open_edges": report.open_edges,
        "non_manifold_edges": report.non_manifold_edges,
        "non_manifold_vertices": report.non_manifold_vertices,
        "inconsistent_edges": report.inconsistent_edges,
        "duplicate_faces": report.duplicate_faces,
        "degenerate_triangles": report.degenerate_triangles,
        "skipped_values": report.skipped_values,
        "topologically_closed": report.is_closed(),
        "printability_verified": false
    }))
}

fn run(command: &str, arguments: &[OsString]) -> Result<Json, CliError> {
    let expected = match command {
        "catalog" => 0,
        "validate" | "evaluate" | "diagnose" | "inspect-package" => 1,
        "build" | "export-stl" | "diff" | "diff-content" | "inspect-ghx" | "convert-ghx"
        | "unpack-part" => 2,
        "apply" | "pack-part" => 3,
        _ => return Err(problem("usage", "Unknown command. Run nodes-cli --help.")),
    };
    if arguments.len() != expected {
        return Err(problem(
            "usage",
            "Incorrect argument count. Run nodes-cli --help.",
        ));
    }
    match command {
        "catalog" => Ok(json!({"catalog": graph_api::catalog()})),
        "validate" => {
            let project = load_project(Path::new(&arguments[0]))?;
            Ok(json!({"project": project_summary(&project), "evaluation_performed": false}))
        }
        "evaluate" => {
            let project = load_project(Path::new(&arguments[0]))?;
            let evaluation = checked_evaluate(&project)?;
            Ok(
                json!({"project": project_summary(&project), "evaluation": evaluation_summary(&project, &evaluation)}),
            )
        }
        "diagnose" => {
            let project = load_project(Path::new(&arguments[0]))?;
            let evaluation = checked_evaluate(&project)?;
            Ok(json!({
                "project": project_summary(&project),
                "diagnostics": diagnostics::diagnose(&project.graph, &evaluation)
            }))
        }
        "diff" | "diff-content" => {
            let before = persistence::load(Path::new(&arguments[0]))
                .map_err(|message| problem("invalid_project", message))?;
            let after = persistence::load(Path::new(&arguments[1]))
                .map_err(|message| problem("invalid_project", message))?;
            let report = nodes::project_diff::compare(&before, &after, command == "diff-content")
                .map_err(|message| problem("diff_failed", message))?;
            Ok(json!(report))
        }
        "build" => {
            protect_request(Path::new(&arguments[0]), Path::new(&arguments[1]))?;
            let request = read_request(Path::new(&arguments[0]))?;
            let project =
                graph_api::build(&request).map_err(|message| problem("build_failed", message))?;
            save_evaluated(&project, Path::new(&arguments[1]))
        }
        "apply" => {
            protect_request(Path::new(&arguments[1]), Path::new(&arguments[2]))?;
            let mut project = load_project(Path::new(&arguments[0]))?;
            let request = read_request(Path::new(&arguments[1]))?;
            let aliases = graph_api::apply(&mut project.graph, &request)
                .map_err(|message| problem("apply_failed", message))?;
            let mut result = save_evaluated(&project, Path::new(&arguments[2]))?;
            result["node_ids"] = json!(aliases);
            Ok(result)
        }
        "inspect-ghx" => {
            let input = Path::new(&arguments[0]);
            let destination = Path::new(&arguments[1]);
            ghx::inspect_file(input, destination)
                .map_err(|message| problem("ghx_inspection_failed", message))?;
            Ok(json!({
                "destination": destination.to_string_lossy(),
                "evaluation_performed": false
            }))
        }
        "convert-ghx" => {
            let input = Path::new(&arguments[0]);
            let destination = Path::new(&arguments[1]);
            if !destination
                .extension()
                .and_then(|v| v.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("morph"))
            {
                return Err(problem(
                    "invalid_destination",
                    "GHX conversion output must be .morph.",
                ));
            }
            let project = ghx::convert_path(input)
                .map_err(|message| problem("ghx_conversion_failed", message))?;
            save_evaluated(&project, destination)
        }
        "pack-part" => {
            let package = Package::pack_files(Path::new(&arguments[0]), Path::new(&arguments[1]))
                .map_err(|message| problem("invalid_package", message))?;
            package
                .save(Path::new(&arguments[2]))
                .map_err(|message| problem("package_io", message))?;
            Ok(json!({"package": package.info()}))
        }
        "inspect-package" | "unpack-part" => {
            let package = Package::load(Path::new(&arguments[0]))
                .map_err(|message| problem("invalid_package", message))?;
            if command == "unpack-part" {
                package
                    .unpack_part(Path::new(&arguments[1]))
                    .map_err(|message| problem("package_io", message))?;
            }
            Ok(json!({"package": package.info()}))
        }
        "export-stl" => export_stl(Path::new(&arguments[0]), Path::new(&arguments[1])),
        _ => unreachable!(),
    }
}

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.is_empty()
        || (arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "-h" | "help")))
    {
        if io::stdout().lock().write_all(HELP.as_bytes()).is_err() {
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    let command = arguments[0].to_str().unwrap_or("<invalid-command>");
    let (result, status) = match run(command, &arguments[1..]) {
        Ok(value) => (
            json!({
                "cli_version": CLI_VERSION, "api_version": graph_api::API_VERSION,
                "command": command, "ok": true, "result": value
            }),
            ExitCode::SUCCESS,
        ),
        Err(error) => {
            let status = if error.code == "usage" {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            };
            let mut response = json!({
                "cli_version": CLI_VERSION, "api_version": graph_api::API_VERSION,
                "command": command, "ok": false,
                "error": {"code": error.code, "message": error.message, "nodes": error.nodes}
            });
            if let Some(diagnostics) = error.diagnostics {
                response["error"]["diagnostics"] = json!(diagnostics);
            }
            (response, status)
        }
    };
    let mut stdout = io::stdout().lock();
    if let Err(error) = serde_json::to_writer(&mut stdout, &result)
        .map_err(io::Error::other)
        .and_then(|()| writeln!(stdout))
    {
        eprintln!("Unable to write CLI response: {error}");
        return ExitCode::FAILURE;
    }
    status
}
