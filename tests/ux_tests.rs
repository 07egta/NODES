//! Node on/off, groups and notes: file compatibility, evaluation and size caps.
use morphlet::annotations::{self, Group, MAX_GROUPS, MAX_NOTE_BYTES, MAX_NOTES, Note};
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, NodeKind, Tree, Value};
use morphlet::persistence::{self, Project};
use morphlet::templates;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let number = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("morphlet-ux-test-{}-{number}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const OLD_FILE: &str = r#"{
  "schema_version": 1,
  "name": "old",
  "graph": {
    "nodes": [
      {"id": 0, "kind": "number", "name": "数値", "position": [0.0, 0.0],
       "params": {"value": 2.0}, "inputs": {}, "preview": true}
    ],
    "next_id": 1
  },
  "pan": [0.0, 0.0],
  "zoom": 1.0
}"#;

#[test]
fn old_files_load_with_defaults_and_new_fields_are_not_written_when_unused() {
    let p: Project = serde_json::from_str(OLD_FILE).unwrap();
    p.validate().unwrap();
    assert!(!p.graph.nodes[0].off);
    assert!(p.graph.groups.is_empty());
    assert!(p.graph.notes.is_empty());
    let text = serde_json::to_string(&p).unwrap();
    assert!(!text.contains("\"off\""));
    assert!(!text.contains("\"groups\""));
    assert!(!text.contains("\"notes\""));
    // Every bundled work still round-trips unchanged.
    for index in 0..templates::TEMPLATES.len() {
        let t = templates::create(index);
        let text = serde_json::to_string(&t).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(back, t);
    }
}

#[test]
fn an_off_node_outputs_an_empty_tree_and_comes_back_when_turned_on() {
    let mut g = Graph::default();
    let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
    let b = g.add(NodeKind::Add, [300., 0.]).unwrap();
    g.connect(a, b, "a").unwrap();
    let mut engine = Evaluator::default();
    let on = engine.evaluate(&g, &|| false);
    assert_eq!(on.outputs[&b].as_ref().unwrap().item_count(), 1);

    g.node_mut(b).unwrap().off = true;
    let off = engine.evaluate(&g, &|| false);
    assert_eq!(off.outputs[&b], Ok(Tree::default()));
    // The upstream node is untouched.
    assert_eq!(off.outputs[&a].as_ref().unwrap().item_count(), 1);

    g.node_mut(b).unwrap().off = false;
    let back = engine.evaluate(&g, &|| false);
    let tree = back.outputs[&b].as_ref().unwrap();
    assert_eq!(tree.item_count(), 1);
    assert!(matches!(
        tree.branches.values().flatten().next(),
        Some(Value::Number(_))
    ));
}

#[test]
fn an_off_flag_is_saved_and_loaded() {
    let dir = TempDir::new();
    let mut p = templates::create(0);
    p.graph.nodes[0].off = true;
    let path = dir.file("off.morph");
    persistence::save(&path, &p).unwrap();
    assert!(fs::read_to_string(&path).unwrap().contains("\"off\": true"));
    assert_eq!(persistence::load(&path).unwrap(), p);
}

#[test]
fn groups_and_notes_round_trip_through_a_file() {
    let dir = TempDir::new();
    let mut p = templates::create(0);
    let ids: Vec<_> = p.graph.nodes.iter().map(|n| n.id).take(2).collect();
    annotations::add_group(&mut p.graph, &ids, "花器まわり").unwrap();
    annotations::add_note(&mut p.graph, "ここで高さを決める\n2 行目", [12.5, -40.]).unwrap();
    let path = dir.file("annotated.morph");
    persistence::save(&path, &p).unwrap();
    let back = persistence::load(&path).unwrap();
    assert_eq!(back, p);
    assert_eq!(back.graph.groups[0].name, "花器まわり");
    assert_eq!(back.graph.groups[0].nodes.len(), 2);
    assert_eq!(back.graph.notes[0].position, [12.5, -40.]);
}

#[test]
fn groups_and_notes_are_capped_and_pruned() {
    let mut g = Graph::default();
    let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
    let b = g.add(NodeKind::Number, [0., 200.]).unwrap();

    // Hostile files are rejected by validation.
    let mut many = g.clone();
    many.groups = (0..=MAX_GROUPS)
        .map(|_| Group {
            name: "g".into(),
            color: [0, 0, 0],
            nodes: vec![a],
        })
        .collect();
    assert!(many.validate().is_err());
    let mut notes = g.clone();
    notes.notes = (0..=MAX_NOTES)
        .map(|_| Note {
            text: String::new(),
            position: [0., 0.],
        })
        .collect();
    assert!(notes.validate().is_err());
    let mut long = g.clone();
    long.notes.push(Note {
        text: "あ".repeat(MAX_NOTE_BYTES),
        position: [0., 0.],
    });
    assert!(long.validate().is_err());
    let mut bad = g.clone();
    bad.notes.push(Note {
        text: "x".into(),
        position: [f32::NAN, 0.],
    });
    assert!(bad.validate().is_err());

    // The helpers never build an invalid graph.
    assert!(annotations::add_note(&mut g, "x", [f32::INFINITY, 0.]).is_err());
    let index = annotations::add_note(&mut g, &"漢".repeat(MAX_NOTE_BYTES), [0., 0.]).unwrap();
    assert!(g.notes[index].text.len() <= MAX_NOTE_BYTES);
    assert!(annotations::add_group(&mut g, &[], "空").is_err());
    annotations::add_group(&mut g, &[a, b, a, 999], "二つ").unwrap();
    assert_eq!(g.groups[0].nodes, vec![a, b]);
    g.validate().unwrap();

    // Deleting nodes prunes membership, then empty groups.
    g.remove(a);
    annotations::prune(&mut g);
    assert_eq!(g.groups[0].nodes, vec![b]);
    g.remove(b);
    annotations::prune(&mut g);
    assert!(g.groups.is_empty());
}
