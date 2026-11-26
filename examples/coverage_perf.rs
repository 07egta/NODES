//! Opt-in headless evaluator, kernel and native project I/O measurements.
//! cargo run --release --locked --example coverage_perf
//! One JSON document on stdout; failed invariants produce a nonzero exit status.
use morphlet::data_tree as dt;
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::mesh_fabrication;
use morphlet::model::{
    Graph, MAX_ITEMS, MAX_NODES, Mesh, NodeId, NodeKind, Tree, Value as ModelValue,
};
use morphlet::persistence::{self, Project};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn timed<T>(operation: impl FnOnce() -> T) -> (T, f64) {
    let started = Instant::now();
    let value = std::hint::black_box(operation());
    (value, started.elapsed().as_secs_f64() * 1000.0)
}

fn make_chain(n: usize) -> Result<(Graph, NodeId), String> {
    let mut graph = Graph::default();
    let slider = graph.add(NodeKind::Number, [0., 0.])?;
    let mut previous = slider;
    for _ in 1..n {
        let next = graph.add(NodeKind::Add, [0., 0.])?;
        graph.node_mut(next).unwrap().params.insert("b".into(), 1.);
        // No per-wire full graph scan while constructing the benchmark fixture.
        graph
            .node_mut(next)
            .unwrap()
            .inputs
            .insert("a".into(), previous);
        previous = next;
    }
    graph.validate()?;
    Ok((graph, slider))
}

fn final_number(result: &Evaluation, id: NodeId) -> f64 {
    assert!(!result.cancelled && result.outputs.values().all(Result::is_ok));
    let tree = result.outputs[&id].as_ref().unwrap();
    assert_eq!(tree.item_count(), 1);
    tree.branches
        .values()
        .flatten()
        .next()
        .unwrap()
        .number()
        .unwrap()
}

fn sample(n: usize) -> Value {
    let build_started = Instant::now();
    let (mut graph, slider) = match make_chain(n) {
        Ok(fixture) => fixture,
        Err(e) => return json!({"requested_nodes": n, "status": "REJECTED", "reason": e}),
    };
    let fixture_ms = build_started.elapsed().as_secs_f64() * 1000.;
    let last = graph.nodes.last().unwrap().id;
    let mut engine = Evaluator::default();
    let cold = engine.evaluate(&graph, &|| false);
    assert!(!cold.cancelled && cold.outputs.values().all(Result::is_ok));
    let warm = engine.evaluate(&graph, &|| false);
    assert!(!warm.cancelled && warm.outputs.values().all(Result::is_ok));
    assert_eq!(warm.cached, n);
    let mut times = Vec::new();
    for step in 0..50 {
        let number = step as f64 + 2.;
        graph
            .node_mut(slider)
            .unwrap()
            .params
            .insert("value".into(), number);
        let start = Instant::now();
        let result = engine.evaluate(&graph, &|| false);
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(final_number(&result, last), number + n as f64 - 1.);
        times.push(elapsed);
    }
    times.sort_by(f64::total_cmp);
    json!({
        "requested_nodes": n,
        "status": "MEASURED",
        "fixture_ms": fixture_ms,
        "cold_evaluate_ms": cold.elapsed_ms,
        "warm_evaluate_ms": warm.elapsed_ms,
        "warm_cache_hits": warm.cached,
        "slider_samples": times.len(),
        "slider_p50_ms": times[24],
        "slider_p95_ms": times[47],
        "slider_max_ms": times[49]
    })
}

fn data_tree_sample() -> Value {
    const ROWS: usize = 128;
    const COLS: usize = 96;
    const EMPTY_BRANCHES: usize = 8;
    let ((matrix, tree), fixture_ms) = timed(|| {
        let matrix = Tree {
            branches: (0..ROWS)
                .map(|row| {
                    let values = (0..COLS)
                        .map(|col| ModelValue::Number((row * COLS + col) as f64))
                        .collect();
                    (vec![7, row as u32], values)
                })
                .collect::<BTreeMap<_, _>>(),
        };
        let mut tree = matrix.clone();
        for branch in 0..EMPTY_BRANCHES {
            tree.branches.insert(vec![9, branch as u32], Vec::new());
        }
        (matrix, tree)
    });
    let ((), validate_ms) = timed(|| dt::validate_topology(&tree).expect("valid tree fixture"));
    let (flat, flatten_ms) = timed(|| dt::flatten(&tree).expect("flatten bounded tree"));
    assert_eq!(flat.branches.len(), 1);
    assert_eq!(flat.item_count(), ROWS * COLS);
    let values = dt::branch(&flat, &[0]).expect("flatten path {0}");
    for (index, value) in values.iter().enumerate() {
        assert_eq!(value.number().unwrap(), index as f64);
    }
    let sum: f64 = values.iter().map(|value| value.number().unwrap()).sum();
    let item_count = ROWS * COLS;
    let expected_sum = (item_count * (item_count - 1) / 2) as f64;
    assert_eq!(sum, expected_sum);

    let (grafted, graft_ms) = timed(|| dt::graft(&tree).expect("graft bounded tree"));
    assert_eq!(grafted.branches.len(), item_count + EMPTY_BRANCHES);
    assert_eq!(grafted.item_count(), item_count);
    for row in 0..ROWS {
        for col in 0..COLS {
            let branch =
                dt::branch(&grafted, &[7, row as u32, col as u32]).expect("grafted item path");
            assert_eq!(branch.len(), 1);
            assert_eq!(branch[0].number().unwrap(), (row * COLS + col) as f64);
        }
    }
    for branch in 0..EMPTY_BRANCHES {
        assert!(
            dt::branch(&grafted, &[9, branch as u32])
                .unwrap()
                .is_empty()
        );
    }

    // Only the rectangular matrix is transposed; explicit empty branches are
    // tested by graft/unflatten instead of being silently removed for the call.
    let (transposed, transpose_ms) =
        timed(|| dt::flip_matrix(&matrix).expect("transpose rectangular tree"));
    assert_eq!(transposed.branches.len(), COLS);
    for col in 0..COLS {
        let branch = dt::branch(&transposed, &[7, col as u32]).unwrap();
        assert_eq!(branch.len(), ROWS);
        for (row, value) in branch.iter().enumerate() {
            assert_eq!(value.number().unwrap(), (row * COLS + col) as f64);
        }
    }
    let (restored, unflatten_ms) =
        timed(|| dt::unflatten(&flat, &tree).expect("restore tree topology"));
    assert!(
        restored == tree,
        "unflatten must preserve all paths, values and empty branches"
    );

    json!({
        "status": "MEASURED",
        "scope": "Data Tree kernels including input checks; expected-result assertions excluded",
        "operation_repetitions": 1,
        "matrix_rows": ROWS,
        "matrix_columns": COLS,
        "input_branches": tree.branches.len(),
        "empty_branches": EMPTY_BRANCHES,
        "items": item_count,
        "max_items": MAX_ITEMS,
        "input_weight": tree.weight(),
        "grafted_branches": grafted.branches.len(),
        "expected_sum": expected_sum,
        "observed_sum": sum,
        "fixture_ms": fixture_ms,
        "validate_topology_ms": validate_ms,
        "flatten_ms": flatten_ms,
        "graft_ms": graft_ms,
        "transpose_ms": transpose_ms,
        "unflatten_ms": unflatten_ms,
        "invariants": "all item values and paths checked; empty branches preserved"
    })
}

fn near(actual: f64, expected: f64) {
    assert!(actual.is_finite(), "nonfinite mesh result");
    let tolerance = 1.0e-10 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, observed {actual}, tolerance {tolerance}"
    );
}

fn mesh_sample() -> Value {
    const CELLS: usize = 128;
    let (mesh, fixture_ms) = timed(|| {
        let mut mesh = Mesh {
            vertices: Vec::with_capacity((CELLS + 1) * (CELLS + 1)),
            triangles: Vec::with_capacity(2 * CELLS * CELLS),
        };
        for y in 0..=CELLS {
            for x in 0..=CELLS {
                mesh.vertices.push([x as f64, y as f64, 0.0]);
            }
        }
        for y in 0..CELLS {
            for x in 0..CELLS {
                let a = (y * (CELLS + 1) + x) as u32;
                let b = a + 1;
                let c = a + (CELLS + 1) as u32;
                let d = c + 1;
                mesh.triangles.extend([[a, b, d], [a, d, c]]);
            }
        }
        mesh
    });
    assert_eq!(mesh.vertices.len(), (CELLS + 1) * (CELLS + 1));
    assert_eq!(mesh.triangles.len(), 2 * CELLS * CELLS);
    let (audit, topology_ms) =
        timed(|| mesh_fabrication::audit(&mesh, &|| false).expect("audit planar grid"));
    assert_eq!(
        audit,
        mesh_fabrication::Audit {
            boundary_edges: 4 * CELLS,
            ..Default::default()
        }
    );
    let ((area, center), area_center_ms) =
        timed(|| mesh_fabrication::area_center(&mesh, &|| false).expect("grid area"));
    let expected_area = (CELLS * CELLS) as f64;
    let expected_center = [CELLS as f64 / 2.0, CELLS as f64 / 2.0, 0.0];
    near(area, expected_area);
    for (actual, expected) in center.into_iter().zip(expected_center) {
        near(actual, expected);
    }

    let (refined, subdivide_ms) =
        timed(|| mesh_fabrication::subdivide(&mesh, 1, &|| false).expect("one subdivision"));
    assert_eq!(refined.vertices.len(), (2 * CELLS + 1) * (2 * CELLS + 1));
    assert_eq!(refined.triangles.len(), 8 * CELLS * CELLS);
    let ((refined_area, refined_center), refined_area_center_ms) =
        timed(|| mesh_fabrication::area_center(&refined, &|| false).expect("refined area"));
    near(refined_area, expected_area);
    for (actual, expected) in refined_center.into_iter().zip(expected_center) {
        near(actual, expected);
    }

    let refined_vertices = refined.vertices.len();
    let refined_triangles = refined.triangles.len();
    let tree = Tree::single(ModelValue::Mesh(Arc::new(refined)));
    let (export_report, export_audit_ms) =
        timed(|| persistence::mesh_stats(&[tree]).expect("export topology audit"));
    assert_eq!(export_report.vertices, refined_vertices);
    assert_eq!(export_report.triangles, refined_triangles);
    assert_eq!(export_report.mesh_count, 1);
    assert_eq!(export_report.open_edges, 8 * CELLS);
    assert_eq!(export_report.non_manifold_edges, 0);
    assert_eq!(export_report.inconsistent_edges, 0);
    assert_eq!(export_report.degenerate_triangles, 0);
    assert_eq!(export_report.skipped_values, 0);
    assert_eq!(
        export_report.bounds,
        Some([[0.0, 0.0, 0.0], [CELLS as f64, CELLS as f64, 0.0]])
    );
    assert!(
        !export_report.is_closed(),
        "the grid is intentionally an open surface"
    );
    near(export_report.signed_volume_mm3, 0.0);

    json!({
        "status": "MEASURED",
        "scope": "open planar grid; indexed topology, area, midpoint subdivision and export audit",
        "operation_repetitions": 1,
        "grid_cells_per_side": CELLS,
        "input_vertices": mesh.vertices.len(),
        "input_triangles": mesh.triangles.len(),
        "output_vertices": refined_vertices,
        "output_triangles": refined_triangles,
        "expected_area_mm2": expected_area,
        "observed_area_mm2": area,
        "refined_area_mm2": refined_area,
        "expected_center_mm": expected_center,
        "observed_center_mm": center,
        "refined_center_mm": refined_center,
        "relative_tolerance": 1.0e-10,
        "expected_boundary_edges": 4 * CELLS,
        "refined_boundary_edges": export_report.open_edges,
        "fixture_ms": fixture_ms,
        "topology_audit_ms": topology_ms,
        "area_center_ms": area_center_ms,
        "subdivide_ms": subdivide_ms,
        "refined_area_center_ms": refined_area_center_ms,
        "export_audit_ms": export_audit_ms,
        "invariants": "analytic area/centroid, exact counts/bounds, open manifold topology"
    })
}

/// Own only a newly created directory and its one benchmark file. Never remove
/// arbitrary directory contents or overwrite a caller-provided destination.
struct ScratchProject {
    directory: PathBuf,
}

impl ScratchProject {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..16 {
            let directory = std::env::temp_dir().join(format!(
                "nodes-perf-{}-{stamp}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&directory) {
                Ok(()) => return Self { directory },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create isolated performance directory: {error}"),
            }
        }
        panic!("cannot reserve an isolated performance directory");
    }

    fn path(&self) -> PathBuf {
        self.directory.join("workload.morph")
    }

    fn cleanup(&self) {
        fs::remove_file(self.path()).expect("remove only the benchmark project");
        fs::remove_dir(&self.directory).expect("remove empty benchmark directory");
    }
}

impl Drop for ScratchProject {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path());
        let _ = fs::remove_dir(&self.directory);
    }
}

fn project_sample() -> Value {
    const NODES: usize = 2_000;
    const INITIAL: f64 = 17.25;
    const EDITED: f64 = -3.5;
    let ((project, slider, last), fixture_ms) = timed(|| {
        let (mut graph, slider) = make_chain(NODES).expect("project graph fixture");
        graph
            .node_mut(slider)
            .unwrap()
            .params
            .insert("value".into(), INITIAL);
        let last = graph.nodes.last().unwrap().id;
        let mut project = Project::new("NODES 性能検証", graph);
        project.pan = [123.5, -45.25];
        project.zoom = 1.25;
        (project, slider, last)
    });
    let (bytes, serialize_ms) =
        timed(|| serde_json::to_vec_pretty(&project).expect("serialize native project"));
    assert!(bytes.len() < persistence::MAX_PROJECT_BYTES);
    let (decoded, decode_validate_ms) =
        timed(|| persistence::decode_project(&bytes).expect("decode native project"));
    assert!(
        decoded == project,
        "in-memory project roundtrip must preserve all fields"
    );

    let scratch = ScratchProject::new();
    let path = scratch.path();
    let ((), save_new_ms) =
        timed(|| persistence::save(&path, &project).expect("save native project"));
    let native_bytes = fs::metadata(&path).expect("saved project metadata").len();
    assert_eq!(native_bytes, (bytes.len() + 1) as u64);
    let (mut loaded, load_new_ms) =
        timed(|| persistence::load(&path).expect("load native project"));
    assert!(
        loaded == project,
        "native save/load must preserve all project fields"
    );
    let mut engine = Evaluator::default();
    let first = engine.evaluate(&loaded.graph, &|| false);
    assert_eq!(final_number(&first, last), INITIAL + NODES as f64 - 1.0);
    assert_eq!(first.computed, NODES);

    loaded
        .graph
        .node_mut(slider)
        .unwrap()
        .params
        .insert("value".into(), EDITED);
    loaded.name = "NODES 再保存検証".into();
    let ((), overwrite_ms) =
        timed(|| persistence::save(&path, &loaded).expect("replace native project"));
    let (reloaded, load_overwritten_ms) =
        timed(|| persistence::load(&path).expect("reload edited native project"));
    assert!(
        reloaded == loaded,
        "overwrite must preserve changed parameters and metadata"
    );
    let changed = engine.evaluate(&reloaded.graph, &|| false);
    assert_eq!(final_number(&changed, last), EDITED + NODES as f64 - 1.0);
    assert_eq!((changed.computed, changed.cached), (NODES, 0));
    scratch.cleanup();

    json!({
        "status": "MEASURED",
        "scope": "native v1 project JSON and persistence API; reads follow writes without cache eviction",
        "operation_repetitions": 1,
        "graph_nodes": NODES,
        "graph_edges": NODES - 1,
        "schema_version": project.schema_version,
        "serialized_json_bytes": bytes.len(),
        "native_file_bytes": native_bytes,
        "fixture_ms": fixture_ms,
        "serialize_pretty_ms": serialize_ms,
        "decode_validate_ms": decode_validate_ms,
        "save_new_file_ms": save_new_ms,
        "load_new_file_ms": load_new_ms,
        "overwrite_file_ms": overwrite_ms,
        "load_overwritten_file_ms": load_overwritten_ms,
        "loaded_evaluate_ms": first.elapsed_ms,
        "reloaded_edit_evaluate_ms": changed.elapsed_ms,
        "expected_initial_output": INITIAL + NODES as f64 - 1.0,
        "expected_edited_output": EDITED + NODES as f64 - 1.0,
        "save_scope": "project validation, pretty JSON, temporary file, flush/sync and rename",
        "cleanup": "own temporary project and empty directory removed",
        "invariants": "complete project equality, changed metadata/parameter persistence and independent arithmetic outputs"
    })
}

fn run() -> Value {
    let counts = [10, 100, 1_000, MAX_NODES, 10_000];
    let samples: Vec<_> = counts.into_iter().map(sample).collect();
    assert!(
        samples.iter().all(|sample| sample["status"] == "MEASURED"),
        "a requested chain workload was rejected"
    );
    json!({
        "samples": samples,
        "additional_workloads": {
            "data_tree": data_tree_sample(),
            "mesh": mesh_sample(),
            "native_project": project_sample()
        }
    })
}

fn main() {
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(run);
    let (mut report, exit_code) = match outcome {
        Ok(report) => (report, 0),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "benchmark invariant or operation failed".into());
            (json!({"error": message, "samples": []}), 1)
        }
    };
    let revision = std::env::var("NODES_SOURCE_SHA").ok();
    report["status"] = json!(if exit_code == 0 { "MEASURED" } else { "FAILED" });
    report["measurement"] = json!(
        "headless evaluator, Data Tree, mesh kernels and native project I/O; not GUI, startup or memory"
    );
    report["platform"] = json!(std::env::consts::OS);
    report["architecture"] = json!(std::env::consts::ARCH);
    report["max_nodes"] = json!(MAX_NODES);
    report["source_revision"] = json!(revision.as_deref().or(option_env!("NODES_SOURCE_SHA")));
    report["build_scope"] = json!({
        "package_version": env!("CARGO_PKG_VERSION"),
        "debug_assertions": cfg!(debug_assertions),
        "pointer_bits": usize::BITS,
        "compiled_source_revision": option_env!("NODES_SOURCE_SHA"),
        "runtime_source_revision": revision,
        "compiler_version_if_supplied": std::env::var("NODES_RUSTC_VERSION").ok(),
        "profile_note": "debug_assertions is observed; optimization settings are not inferred"
    });
    report["workload_schema_version"] = json!(2);
    report["total_workload_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
