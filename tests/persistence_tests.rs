use morphlet::model::{Graph, Mesh, NodeKind, Tree, Value};
use morphlet::persistence::{self, HISTORY_LIMIT, History, MAX_PROJECT_BYTES, Project};
use morphlet::templates;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let number = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("morphlet-io-test-{}-{number}", std::process::id()));
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

fn tetrahedron() -> Mesh {
    Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    }
}
fn tree(mesh: Mesh) -> Tree {
    Tree::single(Value::Mesh(Arc::new(mesh)))
}

#[test]
fn project_roundtrip_preserves_geometry_inputs_and_view() {
    let temp = TempDir::new();
    let path = temp.file("作品.morphlet");
    let mut project = templates::create(0);
    project.name = "自分の花器：01".into();
    project.pan = [-152.25, 82.5];
    project.zoom = 0.65;
    persistence::save(&path, &project).unwrap();
    assert_eq!(persistence::load(&path).unwrap(), project);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn replacing_an_existing_save_keeps_only_the_complete_new_document() {
    let temp = TempDir::new();
    let path = temp.file("作品.morphlet");
    persistence::save(&path, &templates::create(0)).unwrap();
    let project = templates::create(1);
    persistence::save(&path, &project).unwrap();
    assert_eq!(persistence::load(&path).unwrap(), project);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn invalid_save_leaves_last_good_file_untouched() {
    let temp = TempDir::new();
    let path = temp.file("作品.morphlet");
    let mut project = templates::create(0);
    persistence::save(&path, &project).unwrap();
    let previous = fs::read(&path).unwrap();
    project.zoom = f32::NAN;
    assert!(persistence::save(&path, &project).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn invalid_files_do_not_replace_the_current_project() {
    let temp = TempDir::new();
    let path = temp.file("broken.morphlet");
    let mut current = templates::create(0);
    let before = current.clone();
    for bytes in [b"{".to_vec(), b"null".to_vec(), b"[]".to_vec()] {
        fs::write(&path, bytes).unwrap();
        if let Ok(opened) = persistence::load(&path) {
            current = opened;
        }
        assert_eq!(current, before);
    }
    let mut future = serde_json::to_value(&before).unwrap();
    future["schema_version"] = 42.into();
    fs::write(&path, serde_json::to_vec(&future).unwrap()).unwrap();
    assert!(persistence::load(&path).unwrap_err().contains("v42"));
    assert_eq!(current, before);
}

#[test]
fn oversized_and_cyclic_projects_are_rejected() {
    let temp = TempDir::new();
    let path = temp.file("oversized.morphlet");
    fs::write(&path, vec![b' '; MAX_PROJECT_BYTES + 1]).unwrap();
    assert!(persistence::load(&path).unwrap_err().contains("8 MB"));
    let mut graph = Graph::default();
    let a = graph.add(NodeKind::Add, [0., 0.]).unwrap();
    let b = graph.add(NodeKind::Add, [200., 0.]).unwrap();
    graph.node_mut(a).unwrap().inputs.insert("a".into(), b);
    graph.node_mut(b).unwrap().inputs.insert("a".into(), a);
    let project = Project::new("cycle", graph);
    fs::write(&path, serde_json::to_vec(&project).unwrap()).unwrap();
    assert!(persistence::load(&path).is_err());
}

#[test]
fn undo_redo_restores_parameters_wires_and_deleted_nodes() {
    let mut project = templates::create(0);
    let initial = project.clone();
    let mut history = History::default();
    history.checkpoint(&project);
    project.graph.nodes[0].params.insert("radius".into(), 41.0);
    let resized = project.clone();
    history.checkpoint(&project);
    let removed = project.graph.nodes[0].id;
    project.graph.remove(removed);
    let deleted = project.clone();
    assert!(history.undo(&mut project));
    assert_eq!(project, resized);
    assert!(history.undo(&mut project));
    assert_eq!(project, initial);
    assert!(!history.undo(&mut project));
    assert!(history.redo(&mut project));
    assert_eq!(project, resized);
    assert!(history.redo(&mut project));
    assert_eq!(project, deleted);
    assert!(!history.redo(&mut project));
}

#[test]
fn history_is_bounded_and_new_edits_clear_redo() {
    let mut project = templates::create(0);
    let mut history = History::default();
    for i in 0..HISTORY_LIMIT + 10 {
        history.checkpoint(&project);
        project.name = format!("Edit {i}");
    }
    let mut restored = 0;
    while history.undo(&mut project) {
        restored += 1;
    }
    assert_eq!(restored, HISTORY_LIMIT);
    assert!(history.can_redo());
    history.checkpoint(&project);
    project.name = "New direction".into();
    assert!(!history.can_redo());
}

#[test]
fn topology_detects_open_inconsistent_and_nonmanifold_edges() {
    let closed = persistence::mesh_stats(&[tree(tetrahedron())]).unwrap();
    assert!(closed.is_closed());
    assert!((closed.signed_volume_mm3 - 1. / 6.).abs() < 1e-12);
    let mut open = tetrahedron();
    open.triangles.remove(0);
    let report = persistence::mesh_stats(&[tree(open)]).unwrap();
    assert_eq!(report.open_edges, 3);
    let mut flipped = tetrahedron();
    flipped.triangles[0].swap(0, 1);
    let report = persistence::mesh_stats(&[tree(flipped)]).unwrap();
    assert_eq!(report.open_edges, 0);
    assert_eq!(report.inconsistent_edges, 3);
    let mut duplicate = tetrahedron();
    duplicate.triangles.push(duplicate.triangles[0]);
    let report = persistence::mesh_stats(&[tree(duplicate)]).unwrap();
    assert_eq!(report.non_manifold_edges, 3);
}

#[test]
fn topology_welds_separate_vertices_at_the_same_position() {
    let original = tetrahedron();
    let mut soup = Mesh::default();
    for triangle in original.triangles {
        let base = soup.vertices.len() as u32;
        for index in triangle {
            soup.vertices.push(original.vertices[index as usize]);
        }
        soup.triangles.push([base, base + 1, base + 2]);
    }
    let report = persistence::mesh_stats(&[tree(soup)]).unwrap();
    assert_eq!(report.vertices, 12);
    assert!(report.is_closed());
}

#[test]
fn invalid_meshes_and_degenerate_faces_are_not_silently_exported() {
    let temp = TempDir::new();
    let path = temp.file("shape.stl");
    fs::write(&path, b"last good file").unwrap();
    let mut invalid_index = tetrahedron();
    invalid_index.triangles[0][0] = 400;
    assert!(persistence::export_stl(&path, &[tree(invalid_index)]).is_err());
    let mut nan = tetrahedron();
    nan.vertices[0][2] = f64::NAN;
    assert!(persistence::export_stl(&path, &[tree(nan)]).is_err());
    let mut degenerate = tetrahedron();
    degenerate.triangles.push([0, 0, 1]);
    assert!(persistence::export_stl(&path, &[tree(degenerate)]).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"last good file");
}

#[test]
fn binary_stl_has_valid_triangle_count_length_and_unit_normals() {
    let temp = TempDir::new();
    let path = temp.file("tetra.stl");
    let report = persistence::export_stl(&path, &[tree(tetrahedron())]).unwrap();
    let bytes = fs::read(path).unwrap();
    assert_eq!(report.triangles, 4);
    assert_eq!(bytes.len(), 84 + 50 * 4);
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 4);
    for t in 0..4 {
        let start = 84 + 50 * t;
        let mut length = 0.0f32;
        for c in 0..3 {
            let offset = start + c * 4;
            let component = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            assert!(component.is_finite());
            length += component * component;
        }
        assert!((length - 1.).abs() < 1e-6);
    }
}

#[test]
fn stl_rejects_faces_that_collapse_at_float32_precision() {
    let temp = TempDir::new();
    let mut mesh = tetrahedron();
    for v in &mut mesh.vertices {
        v[0] += 1e8;
    }
    let error = persistence::export_stl(&temp.file("collapsed.stl"), &[tree(mesh)]).unwrap_err();
    assert!(error.contains("STL") || error.contains("大きすぎる"));
}

#[test]
fn obj_offsets_indices_between_multiple_objects() {
    let temp = TempDir::new();
    let path = temp.file("two.obj");
    let one = tree(tetrahedron());
    let mut shifted = tetrahedron();
    for v in &mut shifted.vertices {
        v[0] += 4.;
    }
    let report = persistence::export_obj(&path, &[one, tree(shifted)]).unwrap();
    let text = fs::read_to_string(path).unwrap();
    let faces: Vec<_> = text.lines().filter(|line| line.starts_with("f ")).collect();
    assert_eq!(report.vertices, 8);
    assert_eq!(faces.len(), 8);
    assert_eq!(faces[0], "f 1 3 2");
    assert_eq!(faces[4], "f 5 7 6");
    assert!(report.is_closed());
}

#[test]
fn every_template_is_valid_and_names_and_defaults_are_preserved() {
    for (index, spec) in templates::TEMPLATES.iter().enumerate() {
        let project = templates::create(index);
        project.validate().unwrap();
        assert_eq!(project.name, spec.name);
        assert!(!project.graph.nodes.is_empty());
        let outputs: Vec<_> = project
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Output)
            .collect();
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].preview);
        assert!(
            project
                .graph
                .nodes
                .iter()
                .filter(|n| n.kind != NodeKind::Output)
                .all(|n| !n.preview)
        );
    }
}

#[test]
fn templates_evaluate_and_solid_presets_are_closed() {
    let mut evaluator = morphlet::engine::Evaluator::default();
    for index in 0..templates::TEMPLATES.len() {
        let project = templates::create(index);
        let result = evaluator.evaluate(&project.graph, &|| false);
        for (id, output) in &result.outputs {
            assert!(output.is_ok(), "template {index}, node {id}: {output:?}");
        }
        if index == 3 {
            continue;
        } // The helix is intentionally a curve.
        let trees: Vec<_> = project
            .graph
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Output)
            .map(|node| {
                result
                    .outputs
                    .get(&node.id)
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .clone()
            })
            .collect();
        let report = persistence::mesh_stats(&trees).unwrap();
        assert!(report.is_closed(), "template {index}: {report:?}");
        assert!(
            report.signed_volume_mm3 > 0.,
            "template {index}: {report:?}"
        );
    }
}
