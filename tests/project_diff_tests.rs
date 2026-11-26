//! Stable-ID project revision comparisons, including the real read-only CLI.
use morphlet::annotations::{Group, Note};
use morphlet::model::{Graph, NodeId, NodeKind};
use morphlet::persistence::{self, Project};
use morphlet::project_diff::{CHANGE_PAGE_SIZE, ChangeKind, IMPACT_PREVIEW_SIZE, compare};
use serde_json::Value as Json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn node(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add(kind, [0.; 2]).unwrap()
}

fn project() -> Project {
    let mut graph = Graph::default();
    let source = node(&mut graph, NodeKind::Number);
    let downstream = node(&mut graph, NodeKind::Add);
    graph.connect(source, downstream, "a").unwrap();
    Project::new("比較する作品", graph)
}

#[test]
fn node_storage_reordering_is_not_a_change_and_inputs_stay_immutable() {
    let before = project();
    let mut after = before.clone();
    after.graph.nodes.reverse();
    let snapshots = (before.clone(), after.clone());
    let diff = compare(&before, &after, 0).unwrap();
    assert_eq!(diff.schema_version, 1);
    assert_eq!(diff.identity_basis, "native_node_id");
    assert!(!diff.has_changes);
    assert!(!diff.evaluation_performed);
    assert_eq!(diff.total_change_count, 0);
    assert_eq!(diff.summary.unchanged_count, 2);
    assert_eq!(diff.potential_dependency_impact.total_count, 0);
    assert!(diff.node_changes.is_empty());
    assert_eq!(diff.next_offset, None);
    assert!(!diff.truncated);
    assert_eq!((before, after), snapshots);
}

#[test]
fn presentation_and_project_metadata_changes_do_not_seed_calculation_impact() {
    let before = project();
    let mut after = before.clone();
    let changed = after.graph.node_mut(0).unwrap();
    changed.name = "見やすい名前".into();
    changed.position = [12., -7.];
    changed.preview = false;
    after.name = "作品名だけ変更".into();
    after.pan = [1., 2.];
    after.zoom = 2.;
    after.graph.next_id += 10;
    after.graph.groups.push(Group {
        name: "入力".into(),
        color: [1, 2, 3],
        nodes: vec![0],
    });
    after.graph.notes.push(Note {
        text: "値は同じです".into(),
        position: [4., 5.],
    });
    let diff = compare(&before, &after, 0).unwrap();
    assert!(diff.has_changes && diff.summary.metadata_changed);
    assert_eq!(diff.total_change_count, 1);
    assert_eq!(diff.summary.presentation_only_count, 1);
    assert_eq!(diff.summary.calculation_modified_count, 0);
    assert!(diff.metadata.name && diff.metadata.pan && diff.metadata.zoom);
    assert!(diff.metadata.next_id && diff.metadata.groups && diff.metadata.notes);
    assert_eq!(diff.potential_dependency_impact.seed_count, 0);
    assert_eq!(diff.potential_dependency_impact.total_count, 0);
    let change = &diff.node_changes[0];
    assert_eq!(change.change, ChangeKind::Modified);
    assert!(!change.calculation_changed && change.presentation_changed);
    assert!(
        change.presentation.name && change.presentation.position && change.presentation.preview
    );
    assert_eq!(
        change.before.as_ref().unwrap().name,
        before.graph.nodes[0].name
    );
    assert_eq!(change.after.as_ref().unwrap().position, [12., -7.]);
    assert_eq!(change.calculation.parameter_change_count, 0);
    assert_eq!(change.calculation.input_change_count, 0);
}

#[test]
fn shared_descendants_and_disabled_nodes_have_deduplicated_structural_impact() {
    let mut graph = Graph::default();
    let a = node(&mut graph, NodeKind::Number);
    let b = node(&mut graph, NodeKind::Number);
    let merged = node(&mut graph, NodeKind::Add);
    let tail = node(&mut graph, NodeKind::Multiply);
    let unrelated = node(&mut graph, NodeKind::Number);
    graph.connect(a, merged, "a").unwrap();
    graph.connect(b, merged, "b").unwrap();
    graph.connect(merged, tail, "a").unwrap();
    graph.connect(merged, tail, "b").unwrap();
    graph.node_mut(tail).unwrap().off = true;
    let before = Project::new("合流", graph);
    let mut after = before.clone();
    for id in [a, b] {
        after
            .graph
            .node_mut(id)
            .unwrap()
            .params
            .insert("value".into(), 3.);
    }
    let diff = compare(&before, &after, 0).unwrap();
    assert_eq!(diff.summary.calculation_modified_count, 2);
    assert_eq!(
        diff.potential_dependency_impact.meaning,
        "potential_dependency_impact"
    );
    assert_eq!(diff.potential_dependency_impact.graph, "after");
    assert!(diff.potential_dependency_impact.includes_seeds);
    assert_eq!(diff.potential_dependency_impact.seed_count, 2);
    assert_eq!(diff.potential_dependency_impact.total_count, 4);
    assert_eq!(
        diff.potential_dependency_impact.node_ids,
        vec![a, b, merged, tail]
    );
    assert!(
        !diff
            .potential_dependency_impact
            .node_ids
            .contains(&unrelated)
    );
}

#[test]
fn added_removed_and_rewired_nodes_are_matched_by_id() {
    let before = project();
    let mut after = before.clone();
    after.graph.remove(0);
    let added = node(&mut after.graph, NodeKind::Number);
    after.graph.connect(added, 1, "a").unwrap();
    let diff = compare(&before, &after, 0).unwrap();
    assert_eq!(diff.total_change_count, 3);
    assert_eq!(diff.summary.added_count, 1);
    assert_eq!(diff.summary.removed_count, 1);
    assert_eq!(diff.summary.modified_count, 1);
    assert_eq!(
        diff.node_changes
            .iter()
            .map(|change| change.node_id)
            .collect::<Vec<_>>(),
        vec![0, 1, added]
    );
    assert_eq!(diff.node_changes[0].change, ChangeKind::Removed);
    assert!(diff.node_changes[0].after.is_none());
    assert_eq!(diff.node_changes[2].change, ChangeKind::Added);
    assert!(diff.node_changes[2].before.is_none());
    let wire = &diff.node_changes[1].calculation.inputs[0];
    assert_eq!(wire.key, "a");
    assert_eq!(wire.before, Some(0));
    assert_eq!(wire.after, Some(added));
    assert_eq!(diff.potential_dependency_impact.node_ids, vec![1, added]);
    assert_eq!(diff.potential_dependency_impact.seed_count, 2);
}

#[test]
fn signed_zero_and_an_unstored_default_are_calculation_field_changes() {
    let mut before = project();
    before
        .graph
        .node_mut(0)
        .unwrap()
        .params
        .insert("value".into(), 0.);
    let mut after = before.clone();
    after
        .graph
        .node_mut(0)
        .unwrap()
        .params
        .insert("value".into(), -0.);
    // Removing a stored default must not be normalized away, even if its runtime
    // fallback would currently produce the same numerical result.
    after.graph.node_mut(1).unwrap().params.remove("b");
    let diff = compare(&before, &after, 0).unwrap();
    assert_eq!(diff.total_change_count, 2);
    assert_eq!(diff.summary.calculation_modified_count, 2);
    let zero = &diff.node_changes[0].calculation.parameters[0];
    assert_eq!(
        zero.before.as_ref().unwrap().ieee754_hex,
        "0000000000000000"
    );
    assert_eq!(zero.after.as_ref().unwrap().ieee754_hex, "8000000000000000");
    assert!(!zero.before.as_ref().unwrap().value.is_sign_negative());
    assert!(zero.after.as_ref().unwrap().value.is_sign_negative());
    let default = &diff.node_changes[1].calculation.parameters[0];
    assert_eq!(default.key, "b");
    assert_eq!(default.before.as_ref().unwrap().value, 10.);
    assert!(default.after.is_none());
    assert_eq!(diff.potential_dependency_impact.node_ids, vec![0, 1]);
}

#[test]
fn calculation_kind_and_disabled_changes_are_explicit_without_evaluation() {
    let before = project();
    let mut after = before.clone();
    let changed = after.graph.node_mut(1).unwrap();
    changed.kind = NodeKind::Multiply;
    changed.off = true;
    let diff = compare(&before, &after, 0).unwrap();
    assert_eq!(diff.total_change_count, 1);
    let fields = &diff.node_changes[0].calculation;
    assert!(fields.kind && fields.off);
    assert_eq!(
        diff.node_changes[0].before.as_ref().unwrap().kind,
        NodeKind::Add
    );
    assert_eq!(
        diff.node_changes[0].after.as_ref().unwrap().kind,
        NodeKind::Multiply
    );
    assert!(diff.node_changes[0].after.as_ref().unwrap().off);
    assert_eq!(diff.potential_dependency_impact.node_ids, vec![1]);
    assert!(!diff.evaluation_performed);
}

#[test]
fn every_changed_node_is_reachable_through_bounded_deterministic_pages() {
    let before = Project::new("ページ", Graph::default());
    let mut after = before.clone();
    for _ in 0..CHANGE_PAGE_SIZE + 21 {
        node(&mut after.graph, NodeKind::Number);
    }
    after.graph.nodes.reverse();
    let first = compare(&before, &after, 0).unwrap();
    assert_eq!(first.total_change_count, CHANGE_PAGE_SIZE + 21);
    assert_eq!(first.node_changes.len(), CHANGE_PAGE_SIZE);
    assert_eq!(first.next_offset, Some(CHANGE_PAGE_SIZE));
    assert!(first.truncated);
    assert_eq!(
        first.potential_dependency_impact.total_count,
        CHANGE_PAGE_SIZE + 21
    );
    assert_eq!(
        first.potential_dependency_impact.node_ids.len(),
        IMPACT_PREVIEW_SIZE
    );
    assert!(first.potential_dependency_impact.truncated);
    let second = compare(&before, &after, first.next_offset.unwrap()).unwrap();
    assert_eq!(second.node_changes.len(), 21);
    assert_eq!(second.total_change_count, first.total_change_count);
    assert!(!second.truncated);
    assert_eq!(second.next_offset, None);
    let all: Vec<_> = first
        .node_changes
        .iter()
        .chain(&second.node_changes)
        .map(|node| node.node_id)
        .collect();
    assert_eq!(all, (0..after.graph.next_id).collect::<Vec<_>>());
    let beyond = compare(&before, &after, usize::MAX).unwrap();
    assert!(beyond.node_changes.is_empty());
    assert_eq!(beyond.next_offset, None);
    assert_eq!(beyond.total_change_count, first.total_change_count);
}

#[test]
fn unsafe_or_structurally_invalid_projects_are_rejected_by_the_module() {
    let safe = project();
    let mut unsafe_project = safe.clone();
    node(&mut unsafe_project.graph, NodeKind::Script);
    assert!(
        compare(&safe, &unsafe_project, 0)
            .unwrap_err()
            .contains("after project")
    );
    assert!(
        compare(&unsafe_project, &safe, 0)
            .unwrap_err()
            .contains("before project")
    );
    let mut invalid = safe.clone();
    invalid
        .graph
        .node_mut(0)
        .unwrap()
        .inputs
        .insert("unknown".into(), 1);
    assert!(compare(&safe, &invalid, 0).is_err());
}

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
            "nodes-diff-test-{}-{stamp}-{sequence}",
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

fn cli(before: &Path, after: &Path, offset: Option<&str>) -> (i32, Json) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nodes-cli"));
    command.arg("diff").arg(before).arg(after);
    if let Some(offset) = offset {
        command.arg(offset);
    }
    let output = command.output().unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "diff did not return JSON: {error}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap(), value)
}

#[test]
fn real_console_diff_is_read_only_and_does_not_evaluate_failing_nodes() {
    let temporary = TemporaryDirectory::new();
    let before_path = temporary.path("変更前.morph");
    let after_path = temporary.path("変更後.morph");
    let mut before = Project::new("計算しない", Graph::default());
    let divide = node(&mut before.graph, NodeKind::Divide);
    before
        .graph
        .node_mut(divide)
        .unwrap()
        .params
        .insert("b".into(), 0.);
    let mut after = before.clone();
    after.graph.node_mut(divide).unwrap().name = "ゼロ除算でも比較できる".into();
    persistence::save(&before_path, &before).unwrap();
    persistence::save(&after_path, &after).unwrap();
    let before_bytes = fs::read(&before_path).unwrap();
    let after_bytes = fs::read(&after_path).unwrap();
    let (code, report) = cli(&before_path, &after_path, None);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["command"], "diff");
    let diff = &report["result"]["diff"];
    assert_eq!(diff["evaluation_performed"], false);
    assert_eq!(diff["has_changes"], true);
    assert_eq!(diff["summary"]["presentation_only_count"], 1);
    assert_eq!(diff["potential_dependency_impact"]["total_count"], 0);
    assert_eq!(fs::read(&before_path).unwrap(), before_bytes);
    assert_eq!(fs::read(&after_path).unwrap(), after_bytes);
    assert_eq!(fs::read_dir(&temporary.0).unwrap().count(), 2);
}

#[test]
fn real_console_pagination_usage_and_unsafe_inputs_have_distinct_exit_codes() {
    let temporary = TemporaryDirectory::new();
    let before_path = temporary.path("before.morph");
    let after_path = temporary.path("after.morph");
    let before = Project::new("ページ", Graph::default());
    let mut after = before.clone();
    for _ in 0..CHANGE_PAGE_SIZE + 1 {
        node(&mut after.graph, NodeKind::Number);
    }
    persistence::save(&before_path, &before).unwrap();
    persistence::save(&after_path, &after).unwrap();
    let (code, last) = cli(&before_path, &after_path, Some("256"));
    assert_eq!(code, 0, "{last}");
    assert_eq!(
        last["result"]["diff"]["node_changes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(last["result"]["diff"]["node_changes"][0]["node_id"], 256);
    for offset in ["-1", "1.5", "", "184467440737095516160", "+2", "１"] {
        let (code, invalid) = cli(&before_path, &after_path, Some(offset));
        assert_eq!(code, 2, "{offset}: {invalid}");
        assert_eq!(invalid["error"]["code"], "usage");
    }
    let script = node(&mut after.graph, NodeKind::Script);
    after.graph.node_mut(script).unwrap().script = "throw(\"must never run\")".into();
    persistence::save(&after_path, &after).unwrap();
    let original = fs::read(&after_path).unwrap();
    let (code, unsafe_input) = cli(&before_path, &after_path, None);
    assert_eq!(code, 1);
    assert_eq!(unsafe_input["error"]["code"], "invalid_project");
    assert_eq!(fs::read(&after_path).unwrap(), original);
}
