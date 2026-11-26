//! Reproducible, opt-in graph diagnostics measurements over real evaluation results.
//! cargo run --release --locked --example diagnostics_perf -- --quick
//! cargo run --release --locked --example diagnostics_perf -- --full
//! Use --case prefix-1024 to select a fixed workload without running larger cases.
use morphlet::diagnostics::{
    DiagnosticReport, MAX_REPORTED_AFFECTED, MAX_REPORTED_ROOTS, NodeFailure, diagnose,
};
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, MAX_NODES, Node, NodeId, NodeKind};
use serde_json::{Value, json};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
enum Shape {
    SuccessfulChain(usize),
    FailedChain(usize),
    BalancedJoin { roots: usize, nodes: usize },
    PrefixJoin(usize),
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    shape: Shape,
}

impl Case {
    fn nodes(self) -> usize {
        match self.shape {
            Shape::SuccessfulChain(nodes)
            | Shape::FailedChain(nodes)
            | Shape::BalancedJoin { nodes, .. } => nodes,
            Shape::PrefixJoin(roots) => 2 * roots - 1,
        }
    }

    fn roots(self) -> usize {
        match self.shape {
            Shape::SuccessfulChain(_) => 0,
            Shape::FailedChain(_) => 1,
            Shape::BalancedJoin { roots, .. } | Shape::PrefixJoin(roots) => roots,
        }
    }

    fn description(self) -> &'static str {
        match self.shape {
            Shape::SuccessfulChain(_) => "Number(2.5) followed by Add(1) nodes",
            Shape::FailedChain(_) => "Divide-by-zero root followed by one-input Add chain",
            Shape::BalancedJoin { .. } => {
                "independent Divide-by-zero roots, balanced binary Add joins, then a long chain"
            }
            Shape::PrefixJoin(_) => {
                "independent Divide-by-zero roots, incremental Add joins adding one root per step"
            }
        }
    }
}

fn cases(full: bool) -> Vec<Case> {
    let mut cases = vec![
        Case {
            name: "success-1000",
            shape: Shape::SuccessfulChain(1_000),
        },
        Case {
            name: "success-10000",
            shape: Shape::SuccessfulChain(10_000),
        },
        Case {
            name: if full {
                "failure-chain-16384"
            } else {
                "failure-chain-1000"
            },
            shape: Shape::FailedChain(if full { 16_384 } else { 1_000 }),
        },
        Case {
            name: if full {
                "balanced-1024-tail"
            } else {
                "balanced-128-tail"
            },
            shape: Shape::BalancedJoin {
                roots: if full { 1_024 } else { 128 },
                nodes: if full { 16_384 } else { 4_000 },
            },
        },
    ];
    for (name, roots) in if full {
        [
            ("prefix-1024", 1_024),
            ("prefix-4096", 4_096),
            ("prefix-8192", 8_192),
        ]
    } else {
        [
            ("prefix-512", 512),
            ("prefix-1024", 1_024),
            ("prefix-2048", 2_048),
        ]
    } {
        cases.push(Case {
            name,
            shape: Shape::PrefixJoin(roots),
        });
    }
    cases
}

fn timed<T>(operation: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let result = std::hint::black_box(operation());
    (result, start.elapsed().as_secs_f64() * 1_000.)
}

fn ensure(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn prototype(kind: NodeKind) -> Result<Node, String> {
    let mut graph = Graph::default();
    graph.add(kind, [0.; 2])?;
    graph
        .nodes
        .pop()
        .ok_or_else(|| "prototype node is missing".into())
}

fn append(
    graph: &mut Graph,
    prototype: &Node,
    inputs: &[(&str, NodeId)],
) -> Result<NodeId, String> {
    ensure(graph.nodes.len() < MAX_NODES, "fixture exceeds MAX_NODES")?;
    let mut node = prototype.clone();
    node.id = graph.next_id;
    graph.next_id = graph.next_id.checked_add(1).ok_or("fixture ID overflow")?;
    node.inputs = inputs
        .iter()
        .map(|(key, id)| ((*key).into(), *id))
        .collect();
    let id = node.id;
    // Sequential IDs and a final public validation avoid per-insertion graph scans.
    graph.nodes.push(node);
    Ok(id)
}

fn fixture(case: Case) -> Result<Graph, String> {
    ensure(
        case.nodes() <= MAX_NODES,
        "fixed workload exceeds current MAX_NODES",
    )?;
    let mut graph = Graph::default();
    let mut number = prototype(NodeKind::Number)?;
    number.params.insert("value".into(), 2.5);
    let mut divide = prototype(NodeKind::Divide)?;
    divide.params.insert("b".into(), 0.);
    let mut add = prototype(NodeKind::Add)?;
    add.params.insert("b".into(), 1.);
    match case.shape {
        Shape::SuccessfulChain(nodes) | Shape::FailedChain(nodes) => {
            let root = if case.roots() == 0 { &number } else { &divide };
            let mut last = append(&mut graph, root, &[])?;
            for _ in 1..nodes {
                last = append(&mut graph, &add, &[("a", last)])?;
            }
        }
        Shape::BalancedJoin { roots, nodes } => {
            ensure(
                roots.is_power_of_two(),
                "balanced root count must be a power of two",
            )?;
            ensure(
                nodes >= 2 * roots - 1,
                "balanced workload has no room for joins",
            )?;
            let mut level = (0..roots)
                .map(|_| append(&mut graph, &divide, &[]))
                .collect::<Result<Vec<_>, _>>()?;
            while level.len() > 1 {
                level = level
                    .chunks_exact(2)
                    .map(|pair| append(&mut graph, &add, &[("a", pair[0]), ("b", pair[1])]))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            let mut last = level[0];
            while graph.nodes.len() < nodes {
                last = append(&mut graph, &add, &[("a", last)])?;
            }
        }
        Shape::PrefixJoin(roots) => {
            let ids = (0..roots)
                .map(|_| append(&mut graph, &divide, &[]))
                .collect::<Result<Vec<_>, _>>()?;
            let mut last = ids[0];
            for id in ids.iter().skip(1) {
                last = append(&mut graph, &add, &[("a", last), ("b", *id)])?;
            }
        }
    }
    graph.validate()?;
    Ok(graph)
}

/// Analytic descendants for each fixed shape, independent of diagnostics traversal.
fn expected_affected(case: Case, root: usize) -> (usize, Vec<NodeId>) {
    match case.shape {
        Shape::SuccessfulChain(_) => (0, Vec::new()),
        Shape::FailedChain(nodes) => (
            nodes - 1,
            (1..nodes)
                .take(MAX_REPORTED_AFFECTED)
                .map(|id| id as NodeId)
                .collect(),
        ),
        Shape::BalancedJoin { roots, nodes } => {
            let mut ancestors = Vec::new();
            let mut level_start = roots;
            let mut width = 2;
            while width <= roots {
                ancestors.push((level_start + root / width) as NodeId);
                level_start += roots / width;
                width *= 2;
            }
            let count = ancestors.len() + nodes - (2 * roots - 1);
            ancestors.extend(
                ((2 * roots - 1)..nodes)
                    .map(|id| id as NodeId)
                    .take(MAX_REPORTED_AFFECTED.saturating_sub(ancestors.len())),
            );
            (count, ancestors)
        }
        Shape::PrefixJoin(roots) => {
            let first = roots + root.max(1) - 1;
            let end = 2 * roots - 1;
            (
                end - first,
                (first..end)
                    .take(MAX_REPORTED_AFFECTED)
                    .map(|id| id as NodeId)
                    .collect(),
            )
        }
    }
}

/// Every binary join has a contiguous analytic interval of independent roots.
fn expected_upstream(case: Case, id: usize) -> (usize, Vec<NodeId>) {
    let (start, count) = match case.shape {
        Shape::SuccessfulChain(_) => (0, 0),
        Shape::FailedChain(_) => (0, 1),
        Shape::PrefixJoin(roots) => (0, id - roots + 2),
        Shape::BalancedJoin { roots, .. } => {
            let mut level_start = roots;
            let mut width = 2;
            let mut interval = (0, roots);
            while width <= roots {
                let level_count = roots / width;
                if id < level_start + level_count {
                    interval = ((id - level_start) * width, width);
                    break;
                }
                level_start += level_count;
                width *= 2;
            }
            interval
        }
    };
    (
        count,
        (start..start + count)
            .take(MAX_REPORTED_ROOTS)
            .map(|id| id as NodeId)
            .collect(),
    )
}

fn verify_failure(
    case: Case,
    failure: &NodeFailure,
    evaluation: &Evaluation,
    root: bool,
) -> Result<(), String> {
    let id = failure.node.id;
    ensure(
        evaluation
            .outputs
            .get(&id)
            .and_then(|value| value.as_ref().err())
            == Some(&failure.message),
        format!("{id}: evaluator error message was changed"),
    )?;
    if root {
        let (count, preview) = expected_affected(case, id as usize);
        ensure(
            failure.affected_count == count && failure.affected_nodes == preview,
            format!("root {id}: analytic affected count/IDs differ"),
        )?;
        ensure(
            failure.affected_nodes_truncated == (count > MAX_REPORTED_AFFECTED),
            format!("root {id}: affected truncation flag differs"),
        )?;
        ensure(
            failure.upstream_roots.is_empty()
                && failure.upstream_root_count == 0
                && !failure.upstream_roots_truncated,
            format!("root {id}: unexpected upstream roots"),
        )?;
    } else {
        let (count, preview) = expected_upstream(case, id as usize);
        ensure(
            failure.upstream_root_count == count && failure.upstream_roots == preview,
            format!("blocked {id}: analytic root count/IDs differ"),
        )?;
        ensure(
            failure.upstream_roots_truncated == (count > MAX_REPORTED_ROOTS),
            format!("blocked {id}: root truncation flag differs"),
        )?;
        ensure(
            failure.affected_count == 0
                && failure.affected_nodes.is_empty()
                && !failure.affected_nodes_truncated,
            format!("blocked {id}: unexpected affected records"),
        )?;
    }
    Ok(())
}

fn verify(case: Case, report: &DiagnosticReport, evaluation: &Evaluation) -> Result<(), String> {
    let summary = &report.summary;
    let nodes = case.nodes();
    let roots = case.roots();
    let failed = if roots == 0 { 0 } else { nodes };
    ensure(
        report.graph_error.is_none() && summary.complete && !summary.cancelled,
        "diagnostics graph is invalid/incomplete/cancelled",
    )?;
    ensure(
        summary.node_count == nodes
            && summary.completed_count == nodes
            && summary.computed_count == nodes
            && summary.cached_count == 0
            && summary.disabled_count == 0
            && summary.unevaluated_count == 0,
        "cold evaluation/diagnostics summary counts differ",
    )?;
    ensure(
        summary.failed_count == failed
            && summary.successful_count == nodes - failed
            && summary.root_failure_count == roots
            && summary.blocked_count == failed - roots,
        "failure classification counts differ from analytic fixture",
    )?;
    ensure(
        report.root_failures.len() == roots && report.blocked_nodes.len() == failed - roots,
        "failure record lengths differ",
    )?;
    ensure(
        report.hotspots.len() == nodes && report.hotspots.iter().all(|h| h.completed && !h.cached),
        "cold completed node timing records differ",
    )?;
    for (expected, failure) in report.root_failures.iter().enumerate() {
        ensure(
            failure.node.id == expected as NodeId,
            "root ordering/IDs differ",
        )?;
        verify_failure(case, failure, evaluation, true)?;
    }
    for (index, failure) in report.blocked_nodes.iter().enumerate() {
        ensure(
            failure.node.id == (roots + index) as NodeId,
            "blocked ordering/IDs differ",
        )?;
        verify_failure(case, failure, evaluation, false)?;
    }
    if roots == 0 {
        let expected = 2.5 + nodes as f64 - 1.;
        let actual = evaluation
            .outputs
            .get(&((nodes - 1) as NodeId))
            .and_then(|result| result.as_ref().ok())
            .and_then(|tree| tree.branches.values().flatten().next())
            .and_then(|value| value.number().ok());
        ensure(
            actual == Some(expected),
            "successful chain's independently known number differs",
        )?;
    }
    Ok(())
}

fn measure(case: Case, repetitions: usize) -> Result<Value, String> {
    let (graph, fixture_build_ms) = timed(|| fixture(case));
    let graph = graph?;
    let (evaluation, evaluation_wall_ms) =
        timed(|| Evaluator::default().evaluate(&graph, &|| false));
    let (first, first_diagnostics_ms) = timed(|| {
        diagnose(
            std::hint::black_box(&graph),
            std::hint::black_box(&evaluation),
        )
    });
    verify(case, &first, &evaluation)?;
    let mut warm_ms = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        let (report, elapsed) = timed(|| {
            diagnose(
                std::hint::black_box(&graph),
                std::hint::black_box(&evaluation),
            )
        });
        verify(case, &report, &evaluation)?;
        warm_ms.push(elapsed);
        // Destruction and result assertions are deliberately outside the timer.
        drop(report);
    }
    let mut sorted = warm_ms.clone();
    sorted.sort_by(f64::total_cmp);
    let (encoded, serialization_ms) = timed(|| serde_json::to_vec(&first));
    let encoded_bytes = encoded.map_err(|error| error.to_string())?.len();
    let affected_min = first.root_failures.iter().map(|f| f.affected_count).min();
    let affected_max = first.root_failures.iter().map(|f| f.affected_count).max();
    let root_count_max = first
        .blocked_nodes
        .iter()
        .map(|f| f.upstream_root_count)
        .max();
    let expected_error_count = if case.roots() == 0 { 0 } else { case.nodes() };
    Ok(json!({
        "name": case.name,
        "status": "MEASURED",
        "shape": case.description(),
        "nodes": graph.nodes.len(),
        "expected_root_failures": case.roots(),
        "expected_failed_nodes": expected_error_count,
        "timing_ms": {
            "fixture_build_and_validate": fixture_build_ms,
            "evaluation_wall": evaluation_wall_ms,
            "evaluation_reported": evaluation.elapsed_ms,
            "diagnostics_first_call": first_diagnostics_ms,
            "diagnostics_warm_samples": warm_ms,
            "diagnostics_warm_median": sorted[sorted.len() / 2],
            "diagnostics_warm_min": sorted[0],
            "diagnostics_warm_max": sorted[sorted.len() - 1],
            "report_json_serialization": serialization_ms,
        },
        "report": {
            "summary": first.summary,
            "root_records": first.root_failures.len(),
            "blocked_records": first.blocked_nodes.len(),
            "affected_count_min": affected_min,
            "affected_count_max": affected_max,
            "upstream_root_count_max": root_count_max,
            "max_affected_preview_length": first.root_failures.iter().map(|f| f.affected_nodes.len()).max().unwrap_or(0),
            "max_root_preview_length": first.blocked_nodes.iter().map(|f| f.upstream_roots.len()).max().unwrap_or(0),
            "truncated_affected_records": first.root_failures.iter().filter(|f| f.affected_nodes_truncated).count(),
            "truncated_upstream_records": first.blocked_nodes.iter().filter(|f| f.upstream_roots_truncated).count(),
            "serialized_json_bytes": encoded_bytes,
            "first_errors": first.root_failures.iter().take(3).map(|f| json!({"node_id": f.node.id, "message": f.message})).collect::<Vec<_>>(),
        },
        "analytic_invariants_checked": true,
    }))
}

fn run() -> (Value, bool) {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let mut full = false;
    let mut selected = None;
    let mut repetitions = None;
    let mut parsed = arguments.iter();
    while let Some(argument) = parsed.next() {
        match argument.as_str() {
            "--quick" => full = false,
            "--full" => full = true,
            "--case" => selected = parsed.next().cloned(),
            "--repetitions" => {
                repetitions = parsed.next().and_then(|value| value.parse::<usize>().ok())
            }
            _ => {
                return (
                    json!({"status": "FAILED", "error": format!("Unknown argument: {argument}"), "usage": "diagnostics_perf [--quick|--full] [--case NAME] [--repetitions 3|5|7]"}),
                    false,
                );
            }
        }
        if (argument == "--case" && selected.is_none())
            || (argument == "--repetitions" && repetitions.is_none())
        {
            return (
                json!({"status": "FAILED", "error": format!("Missing/invalid value for {argument}")}),
                false,
            );
        }
    }
    let repetitions = repetitions.unwrap_or(if full { 5 } else { 3 });
    if ![3, 5, 7].contains(&repetitions) {
        return (
            json!({"status": "FAILED", "error": "--repetitions must be 3, 5 or 7"}),
            false,
        );
    }
    let cases: Vec<_> = cases(full)
        .into_iter()
        .filter(|case| selected.as_ref().is_none_or(|name| name == case.name))
        .collect();
    if cases.is_empty() {
        return (
            json!({"status": "FAILED", "error": "Requested case is not in the selected --quick/--full mode"}),
            false,
        );
    }
    let started = Instant::now();
    let mut results = Vec::new();
    let mut success = true;
    for case in cases {
        match measure(case, repetitions) {
            Ok(result) => results.push(result),
            Err(error) => {
                results.push(json!({"name": case.name, "status": "FAILED", "error": error}));
                success = false;
                break;
            }
        }
    }
    (
        json!({
            "workload_schema_version": 1,
            "workload": "NODES read-only graph diagnostics",
            "status": if success { "MEASURED" } else { "FAILED" },
            "mode": if full { "full" } else { "quick" },
            "arguments": arguments,
            "unix_time_seconds": SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs()),
            "source_revision_runtime": std::env::var("NODES_SOURCE_SHA").ok(),
            "source_revision_compile_time": option_env!("NODES_SOURCE_SHA"),
        "source_worktree_state": std::env::var("NODES_WORKTREE_STATE").ok(),
        "diagnostics_source_sha256": std::env::var("NODES_DIAGNOSTICS_SHA256").ok(),
            "compiler_version": std::env::var("NODES_RUSTC_VERSION").ok(),
            "package_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "pointer_bits": usize::BITS,
            "debug_assertions": cfg!(debug_assertions),
            "warm_repetitions": repetitions,
            "limits": {"max_nodes": MAX_NODES, "max_reported_roots": MAX_REPORTED_ROOTS, "max_reported_affected": MAX_REPORTED_AFFECTED},
            "measurement_scope": "Each graph is evaluated once. First and repeated diagnose calls use that same immutable real result. Diagnostic timers include report construction but exclude graph construction, evaluation, checks, serialization and destruction. Warm refers to repeated diagnostics only, not cached node evaluation. OS/allocator caches are not reset; first-call is not a cold process measurement.",
            "total_workload_ms": started.elapsed().as_secs_f64() * 1_000.,
            "cases": results,
        }),
        success,
    )
}

fn main() {
    let (report, success) = run();
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("Cannot serialize benchmark report: {error}");
            std::process::exit(1);
        }
    }
    if !success {
        std::process::exit(1);
    }
}
