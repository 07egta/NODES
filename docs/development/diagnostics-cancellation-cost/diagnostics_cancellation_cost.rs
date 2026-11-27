//! Cancellation instrumentation cost comparison. This is an opt-in experiment,
//! compiled in a disposable worktree with the accompanying library module shim.
//! Both diagnostic source snapshots are included unchanged in the same library.
use morphlet::engine::{Evaluation, Evaluator};
use morphlet::model::{Graph, MAX_NODES, Node, NodeId, NodeKind};
use morphlet::{cost_after, cost_before};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const ROOT_CAP: usize = 64;
const AFFECTED_CAP: usize = 256;
const BEFORE_SOURCE: &[u8] = include_bytes!("../benchmark-snapshots/diagnostics_before.rs");
const AFTER_SOURCE: &[u8] = include_bytes!("../benchmark-snapshots/diagnostics_after.rs");

#[derive(Clone, Copy)]
enum Shape {
    Success(usize),
    Prefix(usize),
    Interleaved(usize),
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    shape: Shape,
}

const CASES: [Case; 3] = [
    Case {
        name: "success-10000",
        shape: Shape::Success(10_000),
    },
    Case {
        name: "prefix-1024",
        shape: Shape::Prefix(1_024),
    },
    Case {
        name: "interleaved-512",
        shape: Shape::Interleaved(512),
    },
];

#[derive(Clone, Copy)]
enum Version {
    Before,
    AfterWrapper,
    AfterAtomic,
}

impl Version {
    fn label(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::AfterWrapper => "after_wrapper",
            Self::AfterAtomic => "after_atomic",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Before => 0,
            Self::AfterWrapper => 1,
            Self::AfterAtomic => 2,
        }
    }
}

enum Report {
    Before(cost_before::DiagnosticReport),
    After(cost_after::DiagnosticReport),
}

impl Report {
    fn encode(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Before(report) => serde_json::to_vec(report),
            Self::After(report) => serde_json::to_vec(report),
        }
        .map_err(|error| error.to_string())
    }
}

struct ExpectedRoot {
    affected_count: usize,
    affected_preview: Vec<NodeId>,
}

struct ExpectedBlocked {
    root_count: usize,
    root_preview: Vec<NodeId>,
}

#[derive(Default)]
struct Oracle {
    roots: BTreeMap<NodeId, ExpectedRoot>,
    blocked: BTreeMap<NodeId, ExpectedBlocked>,
    success_tip: Option<(NodeId, f64)>,
    core_roots: usize,
}

fn ensure(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn timed<T>(operation: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let value = std::hint::black_box(operation());
    (value, start.elapsed().as_secs_f64() * 1_000.)
}

fn prototype(kind: NodeKind) -> Result<Node, String> {
    let mut graph = Graph::default();
    graph.add(kind, [0.; 2])?;
    graph.nodes.pop().ok_or_else(|| "missing prototype".into())
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
    graph.nodes.push(node);
    Ok(id)
}

fn fixture(case: Case) -> Result<(Graph, Oracle), String> {
    let mut graph = Graph::default();
    let mut oracle = Oracle::default();
    let mut number = prototype(NodeKind::Number)?;
    number.params.insert("value".into(), 2.5);
    let mut divide = prototype(NodeKind::Divide)?;
    divide.params.insert("b".into(), 0.);
    let mut add = prototype(NodeKind::Add)?;
    add.params.insert("b".into(), 1.);

    match case.shape {
        Shape::Success(nodes) => {
            let mut last = append(&mut graph, &number, &[])?;
            for _ in 1..nodes {
                last = append(&mut graph, &add, &[("a", last)])?;
            }
            oracle.success_tip = Some((last, 2.5 + nodes as f64 - 1.));
        }
        Shape::Prefix(roots) => {
            oracle.core_roots = roots;
            let ids = (0..roots)
                .map(|_| append(&mut graph, &divide, &[]))
                .collect::<Result<Vec<_>, _>>()?;
            let mut last = ids[0];
            for id in ids.iter().skip(1) {
                last = append(&mut graph, &add, &[("a", last), ("b", *id)])?;
            }
            // Analytic prefix intervals, independent of diagnostic traversal.
            for root in 0..roots {
                let first = roots + root.max(1) - 1;
                let end = 2 * roots - 1;
                oracle.roots.insert(
                    root as NodeId,
                    ExpectedRoot {
                        affected_count: end - first,
                        affected_preview: (first..end)
                            .take(AFFECTED_CAP)
                            .map(|id| id as NodeId)
                            .collect(),
                    },
                );
            }
            for join in roots..2 * roots - 1 {
                let count = join - roots + 2;
                oracle.blocked.insert(
                    join as NodeId,
                    ExpectedBlocked {
                        root_count: count,
                        root_preview: (0..count).take(ROOT_CAP).map(|id| id as NodeId).collect(),
                    },
                );
            }
        }
        Shape::Interleaved(roots) => {
            ensure(
                roots.is_power_of_two() && roots >= 2,
                "balanced roots must be a power of two",
            )?;
            oracle.core_roots = roots;
            let core_ids = (0..roots)
                .map(|_| append(&mut graph, &divide, &[]))
                .collect::<Result<Vec<_>, _>>()?;
            let mut level = core_ids.clone();
            while level.len() > 1 {
                level = level
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| append(&mut graph, &add, &[("a", pair[0]), ("b", pair[1])]))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            let tip = level[0];
            let shared_count = AFFECTED_CAP + 17;
            let additional_count = AFFECTED_CAP - 1;
            let mut shared = vec![tip];
            let mut last = tip;
            for _ in 1..shared_count {
                last = append(&mut graph, &add, &[("a", last)])?;
                shared.push(last);
            }
            let mut extra_roots = Vec::new();
            let mut low_joins = Vec::new();
            for _ in 0..additional_count {
                let root = append(&mut graph, &divide, &[])?;
                extra_roots.push(root);
                low_joins.push(append(&mut graph, &add, &[("a", tip), ("b", root)])?);
            }

            // Exactly the committed interleaved test's ID permutation. Topology
            // stays balanced; the smallest affected IDs are in separate groups.
            let mut remap = vec![NodeId::MAX; graph.nodes.len()];
            remap[tip as usize] = 0;
            for (index, id) in low_joins.iter().enumerate() {
                remap[*id as usize] = (index + 1) as NodeId;
            }
            let mut next = AFFECTED_CAP as NodeId;
            for id in &mut remap {
                if *id == NodeId::MAX {
                    *id = next;
                    next += 1;
                }
            }
            ensure(
                next == graph.next_id,
                "permutation does not preserve node count",
            )?;
            for node in &mut graph.nodes {
                node.id = remap[node.id as usize];
                for source in node.inputs.values_mut() {
                    *source = remap[*source as usize];
                }
            }
            graph.nodes.rotate_left(17);
            graph.nodes.reverse();

            let mapped_core: Vec<_> = core_ids.iter().map(|id| remap[*id as usize]).collect();
            let affected_count = shared_count + additional_count + roots.ilog2() as usize - 1;
            for &root in &mapped_core {
                oracle.roots.insert(
                    root,
                    ExpectedRoot {
                        affected_count,
                        affected_preview: (0..AFFECTED_CAP as NodeId).collect(),
                    },
                );
            }
            for (index, &root) in extra_roots.iter().enumerate() {
                oracle.roots.insert(
                    remap[root as usize],
                    ExpectedRoot {
                        affected_count: 1,
                        affected_preview: vec![(index + 1) as NodeId],
                    },
                );
                let mut expected = mapped_core.clone();
                expected.push(remap[root as usize]);
                expected.sort_unstable();
                oracle.blocked.insert(
                    (index + 1) as NodeId,
                    ExpectedBlocked {
                        root_count: roots + 1,
                        root_preview: expected.into_iter().take(ROOT_CAP).collect(),
                    },
                );
            }
            // A balanced join covers one exact contiguous interval of core-root
            // ordinals; the shared tail covers all of them. No reachability walk.
            let mut start = roots;
            let mut width = 2;
            while width <= roots {
                for ordinal in 0..roots / width {
                    let expected = &mapped_core[ordinal * width..(ordinal + 1) * width];
                    oracle.blocked.insert(
                        remap[start + ordinal],
                        ExpectedBlocked {
                            root_count: width,
                            root_preview: expected.iter().copied().take(ROOT_CAP).collect(),
                        },
                    );
                }
                start += roots / width;
                width *= 2;
            }
            for &id in shared.iter().skip(1) {
                oracle.blocked.insert(
                    remap[id as usize],
                    ExpectedBlocked {
                        root_count: roots,
                        root_preview: mapped_core.iter().copied().take(ROOT_CAP).collect(),
                    },
                );
            }
            ensure(
                graph.nodes.len() == 2 * roots + 781,
                "interleaved analytic node count differs",
            )?;
        }
    }
    graph.validate()?;
    ensure(
        oracle.success_tip.is_some()
            || oracle.roots.len() + oracle.blocked.len() == graph.nodes.len(),
        "oracle does not cover every failed node",
    )?;
    Ok((graph, oracle))
}

fn count(value: &Value, key: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| format!("missing count field {key}"))
}

fn ids(value: &Value, key: &str) -> Result<Vec<NodeId>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing ID array {key}"))?
        .iter()
        .map(|id| {
            id.as_u64()
                .ok_or_else(|| format!("non-integer ID in {key}"))
        })
        .collect()
}

fn verify(
    report: &Value,
    graph: &Graph,
    oracle: &Oracle,
    evaluation: &Evaluation,
) -> Result<(), String> {
    let summary = &report["summary"];
    let nodes = graph.nodes.len();
    let failed = oracle.roots.len() + oracle.blocked.len();
    ensure(
        report["graph_error"].is_null()
            && summary["complete"] == true
            && summary["cancelled"] == false,
        "invalid/incomplete/cancelled diagnosis",
    )?;
    for (key, expected) in [
        ("node_count", nodes),
        ("completed_count", nodes),
        ("computed_count", nodes),
        ("cached_count", 0),
        ("disabled_count", 0),
        ("unevaluated_count", 0),
        ("failed_count", failed),
        ("successful_count", nodes - failed),
        ("root_failure_count", oracle.roots.len()),
        ("blocked_count", oracle.blocked.len()),
        (
            "connection_count",
            graph.nodes.iter().map(|node| node.inputs.len()).sum(),
        ),
    ] {
        ensure(
            count(summary, key)? == expected,
            format!("summary {key} differs from analytic oracle"),
        )?;
    }
    ensure(
        report["causality"] == "dependency_inference",
        "unexpected causality label",
    )?;
    let roots = report["root_failures"]
        .as_array()
        .ok_or("missing root records")?;
    let blocked = report["blocked_nodes"]
        .as_array()
        .ok_or("missing blocked records")?;
    ensure(
        roots.len() == oracle.roots.len() && blocked.len() == oracle.blocked.len(),
        "record counts differ",
    )?;
    for (record, (&id, expected)) in roots.iter().zip(&oracle.roots) {
        ensure(
            record["node"]["id"].as_u64() == Some(id),
            "root IDs/order differ",
        )?;
        ensure(
            count(record, "affected_count")? == expected.affected_count
                && ids(record, "affected_nodes")? == expected.affected_preview,
            format!("root {id}: affected count/smallest IDs differ"),
        )?;
        ensure(
            record["affected_nodes_truncated"] == (expected.affected_count > AFFECTED_CAP),
            format!("root {id}: affected truncation differs"),
        )?;
        ensure(
            count(record, "upstream_root_count")? == 0
                && ids(record, "upstream_roots")?.is_empty()
                && record["upstream_roots_truncated"] == false,
            format!("root {id}: unexpected upstream IDs"),
        )?;
    }
    for (record, (&id, expected)) in blocked.iter().zip(&oracle.blocked) {
        ensure(
            record["node"]["id"].as_u64() == Some(id),
            "blocked IDs/order differ",
        )?;
        ensure(
            count(record, "upstream_root_count")? == expected.root_count
                && ids(record, "upstream_roots")? == expected.root_preview,
            format!("blocked {id}: root count/smallest IDs differ"),
        )?;
        ensure(
            record["upstream_roots_truncated"] == (expected.root_count > ROOT_CAP),
            format!("blocked {id}: root truncation differs"),
        )?;
        ensure(
            count(record, "affected_count")? == 0
                && ids(record, "affected_nodes")?.is_empty()
                && record["affected_nodes_truncated"] == false,
            format!("blocked {id}: unexpected affected IDs"),
        )?;
    }
    for record in roots.iter().chain(blocked) {
        let id = record["node"]["id"].as_u64().ok_or("missing node ID")?;
        let expected = evaluation
            .outputs
            .get(&id)
            .and_then(|result| result.as_ref().err())
            .ok_or("missing real evaluation error")?;
        ensure(
            record["message"].as_str() == Some(expected.as_str())
                && record["message_truncated"] == false,
            format!("node {id}: real evaluator error differs"),
        )?;
    }
    let hotspots = report["hotspots"]
        .as_array()
        .ok_or("missing timing records")?;
    ensure(
        hotspots.len() == nodes
            && hotspots
                .iter()
                .all(|item| item["completed"] == true && item["cached"] == false),
        "cold evaluation timing records differ",
    )?;
    let measured_ids: BTreeSet<_> = hotspots
        .iter()
        .filter_map(|item| item["node"]["id"].as_u64())
        .collect();
    ensure(
        measured_ids == graph.nodes.iter().map(|node| node.id).collect(),
        "timings do not cover each node exactly once",
    )?;
    if let Some((tip, expected)) = oracle.success_tip {
        let actual = evaluation
            .outputs
            .get(&tip)
            .and_then(|result| result.as_ref().ok())
            .and_then(|tree| tree.branches.values().flatten().next())
            .and_then(|value| value.number().ok());
        ensure(
            actual == Some(expected),
            "successful chain's analytic value differs",
        )?;
    }
    Ok(())
}

fn run_diagnostics(
    version: Version,
    graph: &Graph,
    evaluation: &Evaluation,
    latest: &AtomicU64,
    request_revision: u64,
) -> Result<Report, String> {
    let graph = std::hint::black_box(graph);
    let evaluation = std::hint::black_box(evaluation);
    match version {
        Version::Before => Ok(Report::Before(cost_before::diagnose(graph, evaluation))),
        Version::AfterWrapper => Ok(Report::After(cost_after::diagnose(graph, evaluation))),
        Version::AfterAtomic => cost_after::diagnose_cancellable(graph, evaluation, || {
            latest.load(Ordering::Relaxed) != request_revision
        })
        .map(Report::After)
        .ok_or_else(|| "unchanged runtime revision unexpectedly cancelled diagnosis".into()),
    }
}

fn stats(samples: &[f64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({"samples_ms": samples, "median_ms": sorted[sorted.len() / 2], "min_ms": sorted[0], "max_ms": sorted[sorted.len() - 1]})
}

fn measure(case: Case, repetitions: usize, reverse: bool) -> Result<Value, String> {
    let (built, fixture_build_ms) = timed(|| fixture(case));
    let (graph, oracle) = built?;
    let (evaluation, evaluation_wall_ms) =
        timed(|| Evaluator::default().evaluate(&graph, &|| false));
    // The Worker also reads a shared AtomicU64 revision with Relaxed ordering.
    // Allocate the token and obscure its reference once OUTSIDE diagnostic timers.
    // The timed callback performs only the runtime atomic load/comparison, with no
    // Cell/counter instrumentation, Arc cloning, or black_box inside the callback.
    let request_revision = std::hint::black_box(17_u64);
    let latest = Arc::new(AtomicU64::new(request_revision));
    let observed_latest = std::hint::black_box(latest.as_ref());
    let mut first_ms = [0.; 3];
    let mut serialization_ms = [0.; 3];
    let mut samples = [Vec::new(), Vec::new(), Vec::new()];
    let mut canonical: Option<Vec<u8>> = None;
    let mut rounds = Vec::new();
    let mut independently_verified = BTreeSet::new();
    for round in 0..=repetitions {
        let mut order = [Version::Before, Version::AfterWrapper, Version::AfterAtomic];
        order.rotate_left(round % 3);
        // The second process reverses each corresponding round exactly, balancing
        // all pairwise orderings while rotating each implementation's position.
        if reverse {
            order.reverse();
        }
        let mut measured = Vec::new();
        for version in order {
            let (report, elapsed) = timed(|| {
                run_diagnostics(
                    version,
                    &graph,
                    &evaluation,
                    observed_latest,
                    request_revision,
                )
            });
            let report = report?;
            let (encoded, serialization) = timed(|| report.encode());
            let encoded = encoded?;
            if round == 0 {
                let parsed: Value =
                    serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
                verify(&parsed, &graph, &oracle, &evaluation)?;
                independently_verified.insert(version.label());
                first_ms[version.index()] = elapsed;
                serialization_ms[version.index()] = serialization;
            } else {
                samples[version.index()].push(elapsed);
            }
            if let Some(expected) = &canonical {
                ensure(
                    &encoded == expected,
                    format!(
                        "{} round {round}: complete serialized report differs",
                        version.label()
                    ),
                )?;
            } else {
                canonical = Some(encoded);
            }
            measured.push(json!({"implementation":version.label(),"diagnostics_ms":elapsed}));
            drop(report);
        }
        rounds.push(json!({"round":round,"phase":if round==0 {"first_report"} else {"repeated"},"measurements":measured}));
    }
    ensure(
        observed_latest.load(Ordering::Relaxed) == request_revision,
        "runtime revision changed unexpectedly",
    )?;
    let max_affected = oracle
        .roots
        .values()
        .map(|root| root.affected_count)
        .max()
        .unwrap_or(0);
    let max_upstream = oracle
        .blocked
        .values()
        .map(|node| node.root_count)
        .max()
        .unwrap_or(0);
    let implementations = [Version::Before, Version::AfterWrapper, Version::AfterAtomic].into_iter().map(|version| {
        (version.label(),json!({"first_report_call_ms":first_ms[version.index()],"repeated":stats(&samples[version.index()]),"first_report_serialization_ms":serialization_ms[version.index()]}))
    }).collect::<BTreeMap<_,_>>();
    Ok(json!({
        "name":case.name,"status":"MEASURED","nodes":graph.nodes.len(),"core_roots":oracle.core_roots,
        "expected_roots":oracle.roots.len(),"expected_blocked":oracle.blocked.len(),
        "fixture_build_and_validate_ms":fixture_build_ms,"real_evaluation_wall_ms":evaluation_wall_ms,
        "real_evaluation_reported_ms":evaluation.elapsed_ms,"implementations":implementations,"rounds":rounds,
        "oracle":{
            "method":"Analytic prefix intervals or balanced-tree root intervals plus explicit ID permutation; no diagnostic traversal reused",
            "independently_verified_implementations":independently_verified,"all_calls_equal_complete_serialized_report":true,
            "full_report_json_bytes":canonical.as_ref().map(Vec::len),"maximum_affected_count":max_affected,"maximum_upstream_root_count":max_upstream,
            "truncated_affected_records":oracle.roots.values().filter(|root| root.affected_count>AFFECTED_CAP).count(),
            "truncated_upstream_records":oracle.blocked.values().filter(|node| node.root_count>ROOT_CAP).count(),
            "checked_fields":"All node/root/blocked counts; all sorted capped relationship IDs and exact counts; all truncation flags; real error messages; complete timing coverage; successful chain value",
        },
    }))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn run() -> Result<Value, String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let mut selected: Option<String> = None;
    let mut repetitions = 5;
    let mut reverse = false;
    let mut parsed = arguments.iter();
    while let Some(argument) = parsed.next() {
        match argument.as_str() {
            "--case" => selected = Some(parsed.next().ok_or("--case needs a name")?.clone()),
            "--repetitions" => {
                repetitions = parsed
                    .next()
                    .ok_or("--repetitions needs 5")?
                    .parse::<usize>()
                    .map_err(|error| error.to_string())?
            }
            "--reverse" => reverse = true,
            _ => return Err(format!("Unknown argument {argument}")),
        }
    }
    ensure(
        repetitions == 5,
        "this bounded protocol uses exactly five repeated calls per process",
    )?;
    ensure(
        cost_before::MAX_REPORTED_ROOTS == ROOT_CAP
            && cost_after::MAX_REPORTED_ROOTS == ROOT_CAP
            && cost_before::MAX_REPORTED_AFFECTED == AFFECTED_CAP
            && cost_after::MAX_REPORTED_AFFECTED == AFFECTED_CAP,
        "snapshot caps differ from protocol",
    )?;
    let distinct_functions =
        cost_before::diagnose as *const () != cost_after::diagnose as *const ();
    ensure(
        distinct_functions,
        "before/after wrappers must link distinct code bodies",
    )?;
    let cases: Vec<_> = CASES
        .into_iter()
        .filter(|case| selected.as_ref().is_none_or(|name| name == case.name))
        .collect();
    ensure(!cases.is_empty(), "unknown case name")?;
    let started = Instant::now();
    let mut results = Vec::new();
    for case in cases {
        results.push(measure(case, repetitions, reverse)?);
    }
    Ok(json!({
        "workload_schema_version":1,"workload":"Ordinary-path cost of cooperative diagnostic cancellation instrumentation",
        "status":"MEASURED","arguments":arguments,"repeated_rounds":repetitions,"reverse_order":reverse,
        "unix_time_seconds":SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|value| value.as_secs()),
        "debug_assertions":cfg!(debug_assertions),"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "api_paths":{
            "before":"before::diagnose(Graph, Evaluation)",
            "after_wrapper":"after::diagnose(Graph, Evaluation), the library's literal-false wrapper",
            "after_atomic":"after::diagnose_cancellable(Graph, Evaluation, || latest.load(Relaxed) != request_revision), with an unchanged runtime Arc<AtomicU64> token",
        },
        "runtime_callback_setup":"Arc allocation and one black_box of its AtomicU64 reference occur outside timing; callback contains only Relaxed load and revision comparison, without a counter, Arc clone, or black_box. No thread changes the token during diagnosis.",
        "provenance":{
            "stable_library_revision":env!("NODES_COST_LIBRARY_SHA"),"before_revision":env!("NODES_COST_BEFORE_SHA"),"after_revision":env!("NODES_COST_AFTER_SHA"),
            "before_diagnostics_sha256":env!("NODES_COST_BEFORE_SHA256"),"after_diagnostics_sha256":env!("NODES_COST_AFTER_SHA256"),
            "before_embedded_snapshot_bytes":BEFORE_SOURCE.len(),"after_embedded_snapshot_bytes":AFTER_SOURCE.len(),
            "before_embedded_snapshot_fnv1a64":format!("{:016x}",fnv1a64(BEFORE_SOURCE)),"after_embedded_snapshot_fnv1a64":format!("{:016x}",fnv1a64(AFTER_SOURCE)),
            "distinct_linked_wrapper_function_addresses":distinct_functions,
            "before_report_type":std::any::type_name::<cost_before::DiagnosticReport>(),"after_report_type":std::any::type_name::<cost_after::DiagnosticReport>(),
        },
        "measurement_scope":"One real evaluation per case; all three API paths diagnose the same immutable graph/result. One initial report call/path is recorded/excluded; repeated order rotates each round and reverses exactly in the second process. Timers exclude fixture/oracle construction, evaluation, JSON serialization, checks and destruction. Atomic token setup is outside timers. Repeated diagnosis is not cached node evaluation. OS/allocator state is not reset; no zero-overhead, CAD evaluation or UI claim is inferred.",
        "total_workload_ms":started.elapsed().as_secs_f64()*1_000.,"cases":results,
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
