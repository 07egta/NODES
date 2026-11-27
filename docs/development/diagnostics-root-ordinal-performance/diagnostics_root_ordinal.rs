//! Opt-in unchanged-module comparison with isolated per-variant process RSS.
//! --mode verify compares full reports; --mode measure runs one diagnosis only.
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, MAX_NODES, Node, NodeId, NodeKind};
use morphlet::{ordinal_after, ordinal_before};
use serde_json::{Value, json};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const ROOT_CAP: usize = 64;
const AFFECTED_CAP: usize = 256;
const BEFORE_SOURCE: &[u8] = include_bytes!("../benchmark-snapshots/diagnostics_before.rs");
const AFTER_SOURCE: &[u8] = include_bytes!("../benchmark-snapshots/diagnostics_after.rs");

#[derive(Clone, Copy)]
enum Case {
    HighIdTwoRoots,
    Success10000,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::HighIdTwoRoots => "high-id-two-roots",
            Self::Success10000 => "success-10000",
        }
    }
    fn nodes(self) -> usize {
        match self {
            Self::HighIdTwoRoots => 16_384,
            Self::Success10000 => 10_000,
        }
    }
    fn roots(self) -> usize {
        match self {
            Self::HighIdTwoRoots => 2,
            Self::Success10000 => 0,
        }
    }
}

#[derive(Clone, Copy)]
enum Version {
    Before,
    After,
}

impl Version {
    fn label(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

enum Report {
    Before(ordinal_before::DiagnosticReport),
    After(ordinal_after::DiagnosticReport),
}

fn ensure(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn timed<T>(operation: impl FnOnce() -> T) -> (T, f64) {
    let started = Instant::now();
    let result = std::hint::black_box(operation());
    (result, started.elapsed().as_secs_f64() * 1_000.)
}

fn prototype(kind: NodeKind) -> Result<Node, String> {
    let mut graph = Graph::default();
    graph.add(kind, [0.; 2])?;
    graph
        .nodes
        .pop()
        .ok_or_else(|| "prototype is missing".into())
}

fn append(
    graph: &mut Graph,
    prototype: &Node,
    inputs: &[(&str, NodeId)],
) -> Result<NodeId, String> {
    ensure(graph.nodes.len() < MAX_NODES, "fixture exceeds MAX_NODES")?;
    let mut node = prototype.clone();
    node.id = graph.next_id;
    graph.next_id = graph.next_id.checked_add(1).ok_or("node ID overflow")?;
    node.inputs = inputs
        .iter()
        .map(|(key, id)| ((*key).into(), *id))
        .collect();
    let id = node.id;
    graph.nodes.push(node);
    Ok(id)
}

fn fixture(case: Case) -> Result<Graph, String> {
    let mut graph = Graph::default();
    let mut add = prototype(NodeKind::Add)?;
    add.params.insert("b".into(), 1.);
    match case {
        Case::HighIdTwoRoots => {
            let roots = [(case.nodes() - 2) as NodeId, (case.nodes() - 1) as NodeId];
            // Forward connections are validated only once the two roots exist.
            // Every low-ID join consumes both high-ID roots, with no other edges.
            for _ in 0..case.nodes() - 2 {
                append(&mut graph, &add, &[("a", roots[0]), ("b", roots[1])])?;
            }
            let mut divide = prototype(NodeKind::Divide)?;
            divide.params.insert("b".into(), 0.);
            ensure(
                append(&mut graph, &divide, &[])? == roots[0],
                "first root ID differs",
            )?;
            ensure(
                append(&mut graph, &divide, &[])? == roots[1],
                "second root ID differs",
            )?;
            graph.nodes.reverse();
            graph.nodes.rotate_left(17);
        }
        Case::Success10000 => {
            let mut number = prototype(NodeKind::Number)?;
            number.params.insert("value".into(), 2.5);
            let mut last = append(&mut graph, &number, &[])?;
            for _ in 1..case.nodes() {
                last = append(&mut graph, &add, &[("a", last)])?;
            }
        }
    }
    graph.validate()?;
    ensure(graph.nodes.len() == case.nodes(), "fixture size differs")?;
    Ok(graph)
}

// The macro checks concrete report types without constructing an additional
// full-size JSON value or cloning any relationship lists in measurement mode.
macro_rules! verify_report {
    ($report:expr, $case:expr, $graph:expr, $evaluation:expr) => {{
        let report = $report;
        let case = $case;
        let graph = $graph;
        let evaluation = $evaluation;
        let nodes = case.nodes();
        let roots = case.roots();
        let failed = if roots == 0 { 0 } else { nodes };
        let summary = &report.summary;
        ensure(report.graph_error.is_none() && summary.complete && !summary.cancelled, "diagnosis is structurally invalid/incomplete/cancelled")?;
        ensure(summary.node_count == nodes && summary.completed_count == nodes && summary.computed_count == nodes
            && summary.cached_count == 0 && summary.disabled_count == 0 && summary.unevaluated_count == 0,
            "cold evaluation summary counts differ")?;
        ensure(summary.successful_count == nodes - failed && summary.failed_count == failed
            && summary.root_failure_count == roots && summary.blocked_count == failed.saturating_sub(roots),
            "analytic failure classification differs")?;
        ensure(summary.connection_count == graph.nodes.iter().map(|node| node.inputs.len()).sum::<usize>(), "connection count differs")?;
        ensure(report.root_failures.len() == roots && report.blocked_nodes.len() == failed.saturating_sub(roots), "failure record counts differ")?;
        ensure(report.disabled_nodes.is_empty() && report.unevaluated_nodes.is_empty(), "unexpected unavailable nodes")?;
        ensure(report.causality == "dependency_inference", "causality contract differs")?;
        match case {
            Case::HighIdTwoRoots => {
                let root_ids = [(nodes - 2) as NodeId, (nodes - 1) as NodeId];
                for (failure, expected_id) in report.root_failures.iter().zip(root_ids) {
                    ensure(failure.node.id == expected_id && failure.affected_count == nodes - 2,
                        "high-ID root or exact affected count differs")?;
                    ensure(failure.affected_nodes.iter().copied().eq(0..AFFECTED_CAP as NodeId)
                        && failure.affected_nodes_truncated, "root smallest affected-ID preview differs")?;
                    ensure(failure.upstream_roots.is_empty() && failure.upstream_root_count == 0
                        && !failure.upstream_roots_truncated, "root unexpectedly has upstream failures")?;
                }
                for (index, failure) in report.blocked_nodes.iter().enumerate() {
                    ensure(failure.node.id == index as NodeId && failure.upstream_root_count == 2
                        && failure.upstream_roots.as_slice() == root_ids && !failure.upstream_roots_truncated,
                        "low-ID join does not have exactly the two high-ID roots")?;
                    ensure(failure.affected_count == 0 && failure.affected_nodes.is_empty()
                        && !failure.affected_nodes_truncated, "blocked node has unexpected affected records")?;
                }
                for failure in report.root_failures.iter().chain(&report.blocked_nodes) {
                    let expected = evaluation.outputs.get(&failure.node.id).and_then(|result| result.as_ref().err());
                    ensure(expected == Some(&failure.message) && !failure.message_truncated,
                        "real evaluator message differs")?;
                }
            }
            Case::Success10000 => {
                ensure(evaluation.outputs.values().all(Result::is_ok), "real successful graph contains an error")?;
                let actual = evaluation.outputs.get(&((nodes - 1) as NodeId)).and_then(|result| result.as_ref().ok())
                    .and_then(|tree| tree.branches.values().flatten().next()).and_then(|value| value.number().ok());
                ensure(actual == Some(2.5 + nodes as f64 - 1.), "successful chain final value differs")?;
            }
        }
        ensure(report.hotspots.len() == nodes, "timing coverage count differs")?;
        let mut visited = vec![false; nodes];
        for hotspot in &report.hotspots {
            let index = usize::try_from(hotspot.node.id).map_err(|error| error.to_string())?;
            ensure(index < nodes && !visited[index] && hotspot.completed && !hotspot.cached, "duplicate or incomplete timing record")?;
            visited[index] = true;
        }
        json!({"summary":summary,"root_records":report.root_failures.len(),"blocked_records":report.blocked_nodes.len(),
            "hotspot_records":report.hotspots.len(),"analytic_oracle_passed":true,
            "first_root_error":report.root_failures.first().map(|failure| failure.message.as_str())})
    }};
}

impl Report {
    fn verify(&self, case: Case, graph: &Graph, evaluation: &Evaluation) -> Result<Value, String> {
        Ok(match self {
            Self::Before(report) => verify_report!(report, case, graph, evaluation),
            Self::After(report) => verify_report!(report, case, graph, evaluation),
        })
    }

    fn encode(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Before(report) => serde_json::to_vec(report),
            Self::After(report) => serde_json::to_vec(report),
        }
        .map_err(|error| error.to_string())
    }
}

fn diagnose(version: Version, graph: &Graph, evaluation: &Evaluation) -> Report {
    let graph = std::hint::black_box(graph);
    let evaluation = std::hint::black_box(evaluation);
    match version {
        Version::Before => Report::Before(ordinal_before::diagnose(graph, evaluation)),
        Version::After => Report::After(ordinal_after::diagnose(graph, evaluation)),
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn run() -> Result<Value, String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let mut mode = "verify";
    let mut case = Case::HighIdTwoRoots;
    let mut variant = None;
    let mut parsed = arguments.iter();
    while let Some(argument) = parsed.next() {
        match argument.as_str() {
            "--mode" => {
                mode = match parsed.next().map(String::as_str) {
                    Some("measure") => "measure",
                    Some("verify") => "verify",
                    _ => return Err("--mode requires measure or verify".into()),
                }
            }
            "--case" => {
                case = match parsed.next().map(String::as_str) {
                    Some("high-id-two-roots") => Case::HighIdTwoRoots,
                    Some("success-10000") => Case::Success10000,
                    _ => return Err("--case requires high-id-two-roots or success-10000".into()),
                }
            }
            "--variant" => {
                variant = Some(match parsed.next().map(String::as_str) {
                    Some("before") => Version::Before,
                    Some("after") => Version::After,
                    _ => return Err("--variant requires before or after".into()),
                })
            }
            _ => return Err(format!("Unknown argument {argument}")),
        }
    }
    ensure(
        mode != "measure" || variant.is_some(),
        "measure mode requires one --variant",
    )?;
    ensure(
        mode != "verify" || variant.is_none(),
        "verify mode always compares both variants",
    )?;
    ensure(
        ordinal_before::MAX_REPORTED_ROOTS == ROOT_CAP
            && ordinal_after::MAX_REPORTED_ROOTS == ROOT_CAP
            && ordinal_before::MAX_REPORTED_AFFECTED == AFFECTED_CAP
            && ordinal_after::MAX_REPORTED_AFFECTED == AFFECTED_CAP,
        "snapshot caps differ from protocol",
    )?;
    let distinct_functions =
        ordinal_before::diagnose as *const () != ordinal_after::diagnose as *const ();
    ensure(
        distinct_functions,
        "the two diagnosis implementations must link distinct code bodies",
    )?;
    let (built, fixture_ms) = timed(|| fixture(case));
    let graph = built?;
    let (evaluation, evaluation_ms) = timed(|| Evaluator::default().evaluate(&graph, &|| false));
    let result = if let Some(version) = variant {
        let (report, diagnosis_ms) = timed(|| diagnose(version, &graph, &evaluation));
        let verified = report.verify(case, &graph, &evaluation)?;
        // No full report serialization occurs in this branch. The process exits
        // after one variant/diagnosis; RSS cannot inherit the other variant's peak.
        json!({"variant":version.label(),"diagnosis_calls":1,"diagnosis_first_ms":diagnosis_ms,
            "oracle":verified,"full_report_serialized":false})
    } else {
        let before = diagnose(Version::Before, &graph, &evaluation);
        let after = diagnose(Version::After, &graph, &evaluation);
        let before_verified = before.verify(case, &graph, &evaluation)?;
        let after_verified = after.verify(case, &graph, &evaluation)?;
        let before_bytes = before.encode()?;
        let after_bytes = after.encode()?;
        ensure(
            before_bytes == after_bytes,
            "complete reports are not byte-identical",
        )?;
        json!({"both_analytic_oracles_passed":true,"all_report_bytes_equal":true,
            "full_report_json_bytes":before_bytes.len(),"full_report_fnv1a64":format!("{:016x}",fnv1a64(&before_bytes)),
            "before_oracle":before_verified,"after_oracle":after_verified,
            "excluded_from_latency_and_rss_comparison":true})
    };
    Ok(json!({
        "workload_schema_version":1,"workload":"Compact failing-root ordinal comparison with per-variant fresh-process RSS",
        "status":if mode=="measure" {"MEASURED"} else {"VERIFIED"},"mode":mode,"case":case.name(),"arguments":arguments,
        "nodes":case.nodes(),"expected_roots":case.roots(),
        "fixture_build_and_validate_ms":fixture_ms,"real_evaluation_wall_ms":evaluation_ms,
        "real_evaluation_reported_ms":evaluation.elapsed_ms,"result":result,
        "unix_time_seconds":SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|value| value.as_secs()),
        "debug_assertions":cfg!(debug_assertions),"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "provenance":{
            "stable_library_revision":env!("NODES_ORDINAL_LIBRARY_SHA"),
            "before_revision":env!("NODES_ORDINAL_BEFORE_SHA"),"after_revision":env!("NODES_ORDINAL_AFTER_SHA"),
            "before_diagnostics_sha256":env!("NODES_ORDINAL_BEFORE_SHA256"),"after_diagnostics_sha256":env!("NODES_ORDINAL_AFTER_SHA256"),
            "before_embedded_snapshot_bytes":BEFORE_SOURCE.len(),"after_embedded_snapshot_bytes":AFTER_SOURCE.len(),
            "before_embedded_snapshot_fnv1a64":format!("{:016x}",fnv1a64(BEFORE_SOURCE)),
            "after_embedded_snapshot_fnv1a64":format!("{:016x}",fnv1a64(AFTER_SOURCE)),
            "distinct_linked_diagnosis_function_addresses":distinct_functions,
            "before_report_type":std::any::type_name::<ordinal_before::DiagnosticReport>(),
            "after_report_type":std::any::type_name::<ordinal_after::DiagnosticReport>(),
        },
        "analytic_bitset_payload_if_high_id_case":{
            "distinct_two_root_join_sets":16_382,"before_words_per_set":256,"after_words_per_set":1,
            "before_word_payload_bytes":33_550_336,"after_word_payload_bytes":131_056,
            "scope":"Analytic u64 root-membership word payload for the high-ID fixture only; excludes Arc, maps, groups, graph, evaluation, messages and report. Not a measured RSS value.",
        },
        "measurement_scope":"Measure mode runs one real graph evaluation then one diagnosis by one variant in a fresh process. Timing excludes fixture/evaluation/typed oracle checks/destruction. No full report JSON is allocated in measurement mode. wait4 peak RSS is the entire process, including graph/evaluation and one variant, not diagnosis-only live memory. Verification mode is a separate process excluded from comparisons.",
    }))
}

fn main() {
    let (report, success) = match run() {
        Ok(value) => (value, true),
        Err(error) => (json!({"status":"FAILED","error":error}), false),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("benchmark JSON must serialize")
    );
    if !success {
        std::process::exit(1);
    }
}
