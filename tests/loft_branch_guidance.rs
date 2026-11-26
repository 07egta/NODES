use morphlet::{
    engine::Evaluator,
    model::{NodeId, NodeKind},
    persistence::{self, History, Project},
};
use std::{path::PathBuf, process::Command};

fn four_separate_sections() -> (Project, NodeId) {
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/06.morph");
    let mut p = persistence::load(&sample).unwrap();
    for id in [0, 1] {
        p.graph
            .node_mut(id)
            .unwrap()
            .params
            .insert("count".into(), 4.);
    }
    let graft = p.graph.add(NodeKind::Graft, [720., 80.]).unwrap();
    p.graph.connect(3, graft, "geometry").unwrap();
    p.graph.connect(graft, 4, "curves").unwrap();
    (p, graft)
}

#[test]
fn cli_points_to_explicit_flatten_and_a_saved_edit_repairs_the_same_loft() {
    let (mut p, graft) = four_separate_sections();
    let mut e = Evaluator::default();
    let mut history = History::default();
    let bad_graph = p.graph.clone();
    let diagnostics = e.evaluate(&p.graph, &|| false);
    let source = diagnostics.outputs[&graft].as_ref().unwrap();
    assert_eq!(
        source.branches.keys().cloned().collect::<Vec<_>>(),
        (0..4).map(|i| vec![0, i]).collect::<Vec<_>>()
    );
    assert!(source.branches.values().all(|section| section.len() == 1));
    assert!(diagnostics.outputs[&4].is_err(), "do not auto-flatten");
    assert_eq!(p.graph, bad_graph);

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nodes-loft-qa-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let files = TempDir(dir);
    let separate = files.0.join("separate.morph");
    let flat = files.0.join("flat.morph");
    persistence::save(&separate, &p).unwrap();
    let validate = |path: &PathBuf| {
        Command::new(env!("CARGO_BIN_EXE_Morphlet"))
            .arg("--validate")
            .arg(path)
            .output()
            .unwrap()
    };
    let failure = validate(&separate);
    assert!(!failure.status.success());
    let stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(
        stderr.contains("合計 4 本")
            && stderr.contains("4 本の枝に 1 本ずつ")
            && stderr.contains("{0;0}"),
        "{stderr}"
    );
    assert!(
        stderr.contains("「断面リスト」の手前に「フラット化」"),
        "{stderr}"
    );

    history.checkpoint(&p);
    let flatten = p.graph.add(NodeKind::Flatten, [920., 80.]).unwrap();
    p.graph.connect(graft, flatten, "geometry").unwrap();
    p.graph.connect(flatten, 4, "curves").unwrap();
    persistence::save(&flat, &p).unwrap();
    let pass = validate(&flat);
    assert!(
        pass.status.success(),
        "{}",
        String::from_utf8_lossy(&pass.stderr)
    );
    let loaded = persistence::load(&flat).unwrap();
    assert_eq!(p, loaded);
    let evaluation = e.evaluate(&loaded.graph, &|| false);
    assert_eq!(
        evaluation.outputs[&flatten]
            .as_ref()
            .unwrap()
            .branches
            .len(),
        1
    );
    let mesh = evaluation.outputs[&4].as_ref().unwrap();
    let report = persistence::mesh_stats(std::slice::from_ref(mesh)).unwrap();
    assert_eq!(
        (report.mesh_count, report.triangles, report.open_edges),
        (1, 508, 0)
    );
    assert!(report.signed_volume_mm3 > 0.);

    assert!(history.undo(&mut p));
    assert_eq!(p.graph, bad_graph);
    assert!(
        e.evaluate(&p.graph, &|| false).outputs[&4]
            .as_ref()
            .unwrap_err()
            .contains("4 本の枝に 1 本ずつ")
    );
    assert!(history.redo(&mut p));
    assert_eq!(p, loaded);
}

#[test]
fn mixed_branches_report_the_single_bad_path_without_suggesting_global_flatten() {
    let (mut p, graft) = four_separate_sections();
    let join = p.graph.add(NodeKind::MergeTrees, [920., 80.]).unwrap();
    p.graph.connect(3, join, "a").unwrap();
    p.graph.connect(graft, join, "b").unwrap();
    p.graph.connect(join, 4, "curves").unwrap();
    let evaluation = Evaluator::default().evaluate(&p.graph, &|| false);
    let source = evaluation.outputs[&join].as_ref().unwrap();
    assert_eq!(source.branches[&vec![0]].len(), 4);
    assert_eq!(source.branches[&vec![0, 0]].len(), 1);
    let error = evaluation.outputs[&4].as_ref().unwrap_err();
    assert!(error.contains("枝 {0;0} の断面は 1 本"), "{error}");
    assert!(!error.contains("フラット化"), "{error}");
}
