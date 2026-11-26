use morphlet::{
    engine::Evaluator,
    model::{Graph, NodeKind, Value},
    persistence::{self, Project},
};
use std::{path::Path, process::Command};

#[test]
fn actual_cli_loads_tree_branches_and_the_requested_path_keeps_its_identity() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tree-branches.morph");
    let run = Command::new(env!("CARGO_BIN_EXE_Morphlet"))
        .arg("--validate")
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let project = persistence::load(&fixture).unwrap();
    let reloaded: Project = serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
    assert_eq!(reloaded, project);
    let out = Evaluator::default().evaluate(&reloaded.graph, &|| false);
    let data = |id| &out.outputs[&id].as_ref().unwrap().branches;
    assert_eq!(
        data(2).keys().collect::<Vec<_>>(),
        [&vec![0], &vec![0, 0], &vec![0, 1], &vec![0, 2]]
    );
    assert_eq!(
        data(2)[&vec![0]],
        [Value::Number(10.), Value::Number(20.), Value::Number(30.)]
    );
    assert_eq!(data(4).keys().collect::<Vec<_>>(), [&vec![0, 2]]);
    assert_eq!(data(4)[&vec![0, 2]], [Value::Number(30.)]);
    assert_eq!(data(5)[&vec![0]], [Value::Number(10.), Value::Number(30.)]);
    let report = persistence::mesh_stats(&[out.outputs[&8].as_ref().unwrap().clone()]).unwrap();
    assert_eq!(
        (report.mesh_count, report.triangles, report.open_edges),
        (2, 24, 0)
    );
}

#[test]
fn graft_no_longer_erases_a_value_when_a_new_path_matches_a_former_empty_branch() {
    let mut g = Graph::default();
    let value = g.add(NodeKind::Number, [0., 0.]).unwrap();
    let graft_before = g.add(NodeKind::Graft, [0., 0.]).unwrap();
    let empty = g.add(NodeKind::ShiftList, [0., 0.]).unwrap();
    let merge = g.add(NodeKind::MergeTrees, [0., 0.]).unwrap();
    let graft_after = g.add(NodeKind::Graft, [0., 0.]).unwrap();
    g.node_mut(value)
        .unwrap()
        .params
        .insert("value".into(), 42.);
    g.node_mut(empty).unwrap().params.insert("wrap".into(), 0.);
    g.connect(value, graft_before, "geometry").unwrap();
    g.connect(graft_before, empty, "list").unwrap();
    g.connect(value, merge, "a").unwrap();
    g.connect(empty, merge, "b").unwrap();
    g.connect(merge, graft_after, "geometry").unwrap();
    let out = Evaluator::default().evaluate(&g, &|| false);
    let joined = &out.outputs[&merge].as_ref().unwrap().branches;
    assert!(joined[&vec![0, 0]].is_empty());
    let grafted = &out.outputs[&graft_after].as_ref().unwrap().branches;
    assert_eq!(grafted.len(), 1);
    assert_eq!(grafted[&vec![0, 0]], [Value::Number(42.)]);
}
