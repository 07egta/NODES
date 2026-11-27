use nodes::engine::Evaluator;
use nodes::model::{Graph, NodeKind, Value};
use nodes::nurbs_nodes::write_step;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDirectory(PathBuf);

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn step_graph_evaluation_leaves_legacy_destination_untouched() {
    let sequence = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = TempDirectory(
        std::env::temp_dir().join(format!("nodes-step-{}-{sequence}", std::process::id())),
    );
    fs::create_dir(&directory.0).unwrap();
    let destination = directory.0.join("box.STP");

    let mut graph = Graph::default();
    let shape = graph.add(NodeKind::ExactBox, [0., 0.]).unwrap();
    let export = graph.add(NodeKind::ExactStepExport, [0., 0.]).unwrap();
    let import = graph.add(NodeKind::ExactStepImport, [0., 0.]).unwrap();
    graph.connect(shape, export, "breps").unwrap();
    graph.node_mut(export).unwrap().script = destination.to_str().unwrap().to_owned();

    let mut evaluator = Evaluator::default();
    let evaluation = evaluator.evaluate(&graph, &|| false);
    assert!(!destination.exists());
    assert!(evaluation.outputs[&import].is_err());
    let export_tree = evaluation.outputs[&export].as_ref().unwrap();
    let Value::Number(evaluated_bytes) = export_tree.branches[&vec![0]][0] else {
        panic!("expected STEP byte count");
    };
    assert!(evaluated_bytes > 0.);

    fs::write(&destination, "existing file").unwrap();
    let evaluation = evaluator.evaluate(&graph, &|| false);
    assert_eq!(fs::read_to_string(&destination).unwrap(), "existing file");
    let bytes = write_step(&destination, graph.node(export).unwrap(), &evaluation).unwrap();
    assert_eq!(bytes as f64, evaluated_bytes);
    assert_eq!(fs::metadata(&destination).unwrap().len(), bytes as u64);

    graph.node_mut(import).unwrap().script = destination.to_str().unwrap().to_owned();
    let evaluation = evaluator.evaluate(&graph, &|| false);
    let imported = evaluation.outputs[&import].as_ref().unwrap();
    assert!(matches!(&imported.branches[&vec![0]][..], [Value::Brep(_)]));

    fs::write(&destination, "invalid STEP file").unwrap();
    let evaluation = evaluator.evaluate(&graph, &|| false);
    assert!(evaluation.outputs[&import].is_err());
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "invalid STEP file"
    );
}
