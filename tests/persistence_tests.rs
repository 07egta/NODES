use morphlet::engine::Evaluator;
use morphlet::model::{Graph, Mesh, NodeId, NodeKind, Tree, Value};
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
fn non_file_sources_are_rejected_without_replacing_destinations() {
    let temp = TempDir::new();
    let source = temp.file("directory.ghx");
    fs::create_dir(&source).unwrap();
    let destination = temp.file("preserved.json");
    fs::write(&destination, b"last good document").unwrap();

    assert!(
        persistence::load(&source)
            .unwrap_err()
            .contains("通常のファイル")
    );
    assert!(
        morphlet::graph_api::load_safe(&source)
            .unwrap_err()
            .contains("regular file")
    );
    assert!(
        morphlet::graph_api::build_file(&source, &destination)
            .unwrap_err()
            .contains("regular file")
    );
    assert!(
        morphlet::ghx::inspect_file(&source, &destination)
            .unwrap_err()
            .contains("regular file")
    );
    assert_eq!(fs::read(&destination).unwrap(), b"last good document");
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

// ---- Nodes added in the "more nodes" update ----

const NEW_KINDS: [(NodeKind, &str); 14] = [
    (NodeKind::Subtract, "subtract"),
    (NodeKind::Power, "power"),
    (NodeKind::Abs, "abs"),
    (NodeKind::Min, "min"),
    (NodeKind::Max, "max"),
    (NodeKind::Modulo, "modulo"),
    (NodeKind::Sine, "sine"),
    (NodeKind::Range, "range"),
    (NodeKind::Cylinder, "cylinder"),
    (NodeKind::Cone, "cone"),
    (NodeKind::Torus, "torus"),
    (NodeKind::Plane, "plane"),
    (NodeKind::Mirror, "mirror"),
    (NodeKind::ScaleXyz, "scale_xyz"),
];

const OLD_KIND_NAMES: [(NodeKind, &str); 28] = [
    (NodeKind::Number, "number"),
    (NodeKind::Add, "add"),
    (NodeKind::Multiply, "multiply"),
    (NodeKind::Divide, "divide"),
    (NodeKind::Series, "series"),
    (NodeKind::Point, "point"),
    (NodeKind::Circle, "circle"),
    (NodeKind::Polygon, "polygon"),
    (NodeKind::Star, "star"),
    (NodeKind::Rectangle, "rectangle"),
    (NodeKind::Helix, "helix"),
    (NodeKind::Extrude, "extrude"),
    (NodeKind::Loft, "loft"),
    (NodeKind::Revolve, "revolve"),
    (NodeKind::Vase, "vase"),
    (NodeKind::Sphere, "sphere"),
    (NodeKind::Box, "box"),
    (NodeKind::Move, "move"),
    (NodeKind::Rotate, "rotate"),
    (NodeKind::Scale, "scale"),
    (NodeKind::Twist, "twist"),
    (NodeKind::Wave, "wave"),
    (NodeKind::LinearArray, "linear_array"),
    (NodeKind::RadialArray, "radial_array"),
    (NodeKind::Merge, "merge"),
    (NodeKind::Flatten, "flatten"),
    (NodeKind::Graft, "graft"),
    (NodeKind::Output, "output"),
];

fn evaluate_one(graph: &Graph, id: NodeId) -> Result<Tree, String> {
    let result = Evaluator::default().evaluate(graph, &|| false);
    result.outputs.get(&id).unwrap().clone()
}

/// Evaluates a single number node (inputs left unconnected) and returns its numbers.
fn eval_numbers(kind: NodeKind, params: &[(&str, f64)]) -> Result<Vec<f64>, String> {
    let mut graph = Graph::default();
    let id = graph.add(kind, [0., 0.]).unwrap();
    for (key, value) in params {
        graph
            .node_mut(id)
            .unwrap()
            .params
            .insert((*key).to_string(), *value);
    }
    let tree = evaluate_one(&graph, id)?;
    Ok(tree
        .branches
        .values()
        .flatten()
        .map(|v| match v {
            Value::Number(n) => *n,
            other => panic!("number expected, got {other:?}"),
        })
        .collect())
}

fn first_mesh_stats(graph: &Graph, id: NodeId) -> morphlet::persistence::ExportReport {
    let tree = evaluate_one(graph, id).unwrap();
    persistence::mesh_stats(&[tree]).unwrap()
}

#[test]
fn new_node_kinds_keep_stable_serde_names_and_old_names_are_unchanged() {
    for (kind, name) in NEW_KINDS.iter().chain(OLD_KIND_NAMES.iter()) {
        assert_eq!(serde_json::to_string(kind).unwrap(), format!("\"{name}\""));
        let back: NodeKind = serde_json::from_str(&format!("\"{name}\"")).unwrap();
        assert_eq!(back, *kind);
        assert!(NodeKind::all().contains(kind), "{name} missing from all()");
    }
    // 42 geometry/data nodes, the Script node, the 20 list/pattern/print nodes and the
    // All baseline and Data Tree registrations remain present; subsequent kernels append.
    assert!(NodeKind::all().len() >= 87);
    let mut seen = std::collections::BTreeSet::new();
    assert!(NodeKind::all().iter().all(|k| seen.insert(*k)));
}

#[test]
fn new_nodes_evaluate_to_finite_non_empty_results() {
    let mut graph = Graph::default();
    let source = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    let mut ids = Vec::new();
    for (kind, _) in NEW_KINDS {
        let id = graph.add(kind, [0., 0.]).unwrap();
        if kind.spec().inputs.iter().any(|s| s.key == "geometry") {
            graph.connect(source, id, "geometry").unwrap();
        }
        ids.push((kind, id));
    }
    graph.validate().unwrap();
    let result = Evaluator::default().evaluate(&graph, &|| false);
    for (kind, id) in ids {
        let tree = result
            .outputs
            .get(&id)
            .unwrap()
            .as_ref()
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        let values: Vec<_> = tree.branches.values().flatten().collect();
        assert!(!values.is_empty(), "{kind:?} produced nothing");
        for value in values {
            match value {
                Value::Number(n) => assert!(n.is_finite(), "{kind:?}: {n}"),
                Value::Mesh(m) => {
                    assert!(
                        !m.vertices.is_empty() && !m.triangles.is_empty(),
                        "{kind:?}"
                    );
                    assert!(m.vertices.iter().flatten().all(|c| c.is_finite()));
                    assert!(
                        m.triangles
                            .iter()
                            .flatten()
                            .all(|i| (*i as usize) < m.vertices.len())
                    );
                }
                other => panic!("{kind:?}: unexpected {other:?}"),
            }
        }
    }
}

#[test]
fn simple_number_nodes_compute_expected_values() {
    let one = |kind, params: &[(&str, f64)]| eval_numbers(kind, params).unwrap();
    assert_eq!(one(NodeKind::Subtract, &[("a", 10.), ("b", 3.)]), [7.]);
    assert_eq!(one(NodeKind::Abs, &[("value", -3.5)]), [3.5]);
    assert_eq!(one(NodeKind::Min, &[("a", 2.), ("b", 5.)]), [2.]);
    assert_eq!(one(NodeKind::Max, &[("a", 2.), ("b", 5.)]), [5.]);
    assert_eq!(
        one(NodeKind::Power, &[("base", 2.), ("exponent", 10.)]),
        [1024.]
    );
    assert_eq!(one(NodeKind::Modulo, &[("a", 10.), ("b", 3.)]), [1.]);
    assert_eq!(one(NodeKind::Modulo, &[("a", -7.), ("b", 3.)]), [2.]);
    assert_eq!(one(NodeKind::Modulo, &[("a", 7.), ("b", -3.)]), [1.]);
    let sine = one(
        NodeKind::Sine,
        &[("x", 0.25), ("amp", 2.), ("frequency", 1.), ("phase", 0.)],
    );
    assert!((sine[0] - 2.).abs() < 1e-12);
    let shifted = one(
        NodeKind::Sine,
        &[("x", 0.), ("amp", 1.), ("frequency", 1.), ("phase", 90.)],
    );
    assert!((shifted[0] - 1.).abs() < 1e-12);
    assert_eq!(
        one(
            NodeKind::Range,
            &[("start", 0.), ("end", 10.), ("count", 3.)]
        ),
        [0., 5., 10.]
    );
    assert_eq!(
        one(
            NodeKind::Range,
            &[("start", 4.), ("end", 9.), ("count", 1.)]
        ),
        [4.]
    );
}

#[test]
fn range_count_is_capped_by_the_input_limit() {
    let spec = NodeKind::Range
        .spec()
        .inputs
        .iter()
        .find(|s| s.key == "count")
        .unwrap();
    assert!(spec.max <= 512.);
    let too_many = eval_numbers(NodeKind::Range, &[("count", spec.max + 1.)]);
    assert!(too_many.is_err());
    let at_cap = eval_numbers(NodeKind::Range, &[("count", spec.max)]).unwrap();
    assert_eq!(at_cap.len(), spec.max as usize);
    assert!(at_cap.iter().all(|v| v.is_finite()));
}

#[test]
fn divide_modulo_and_power_never_produce_nan_or_infinity() {
    let samples = [
        -1e6, -1000., -2.5, -1., -0.5, -1e-9, 0., 1e-9, 0.5, 1., 2., 3.5, 1000., 1e6,
    ];
    for kind in [NodeKind::Divide, NodeKind::Modulo, NodeKind::Subtract] {
        for a in samples {
            for b in samples {
                if let Ok(values) = eval_numbers(kind, &[("a", a), ("b", b)]) {
                    assert!(
                        !values.is_empty() && values.iter().all(|v| v.is_finite()),
                        "{kind:?}({a}, {b}) = {values:?}"
                    );
                }
            }
        }
    }
    let exponents = [-64., -3., -0.5, 0., 0.5, 1., 2., 10., 64.];
    for base in samples {
        for exponent in exponents {
            if let Ok(values) =
                eval_numbers(NodeKind::Power, &[("base", base), ("exponent", exponent)])
            {
                assert!(
                    !values.is_empty() && values.iter().all(|v| v.is_finite()),
                    "power({base}, {exponent}) = {values:?}"
                );
            }
        }
    }
    // The dangerous cases must be reported, not silently turned into NaN/inf.
    assert!(eval_numbers(NodeKind::Divide, &[("a", 1.), ("b", 0.)]).is_err());
    assert!(eval_numbers(NodeKind::Modulo, &[("a", 1.), ("b", 0.)]).is_err());
    assert!(eval_numbers(NodeKind::Power, &[("base", 0.), ("exponent", -1.)]).is_err());
    assert!(eval_numbers(NodeKind::Power, &[("base", -8.), ("exponent", 0.5)]).is_err());
    assert!(eval_numbers(NodeKind::Power, &[("base", 1e6), ("exponent", 64.)]).is_err());
    // Out-of-range or non-finite parameters are rejected before any maths runs.
    assert!(eval_numbers(NodeKind::Power, &[("exponent", 65.)]).is_err());
    assert!(eval_numbers(NodeKind::Sine, &[("amp", f64::NAN)]).is_err());
    assert!(eval_numbers(NodeKind::Sine, &[("frequency", f64::INFINITY)]).is_err());
}

#[test]
fn sine_over_a_range_gives_a_finite_list_even_at_extreme_settings() {
    let mut graph = Graph::default();
    let range = graph.add(NodeKind::Range, [0., 0.]).unwrap();
    let sine = graph.add(NodeKind::Sine, [0., 0.]).unwrap();
    graph.connect(range, sine, "x").unwrap();
    {
        let node = graph.node_mut(range).unwrap();
        node.params.insert("start".into(), -1e5);
        node.params.insert("end".into(), 1e5);
        node.params.insert("count".into(), 512.);
    }
    {
        let node = graph.node_mut(sine).unwrap();
        node.params.insert("amp".into(), 1e6);
        node.params.insert("frequency".into(), 1000.);
    }
    let tree = evaluate_one(&graph, sine).unwrap();
    assert_eq!(tree.item_count(), 512);
    assert!(tree.branches.values().flatten().all(|v| match v {
        Value::Number(n) => n.is_finite() && n.abs() <= 1e6 + 1e-6,
        _ => false,
    }));
}

#[test]
fn cylinder_cone_and_torus_are_closed_with_correct_volume() {
    let single = |kind: NodeKind, params: &[(&str, f64)]| {
        let mut graph = Graph::default();
        let id = graph.add(kind, [0., 0.]).unwrap();
        for (key, value) in params {
            graph
                .node_mut(id)
                .unwrap()
                .params
                .insert((*key).to_string(), *value);
        }
        first_mesh_stats(&graph, id)
    };
    let (r, h, n) = (12.0f64, 30.0f64, 36.0f64);
    let polygon = n / 2. * r * r * (std::f64::consts::TAU / n).sin();
    let cylinder = single(
        NodeKind::Cylinder,
        &[("radius", r), ("height", h), ("segments", n)],
    );
    assert!(cylinder.is_closed(), "{cylinder:?}");
    assert!((cylinder.signed_volume_mm3 - polygon * h).abs() < polygon * h * 1e-9);
    let cone = single(
        NodeKind::Cone,
        &[("radius", r), ("height", h), ("segments", n)],
    );
    assert!(cone.is_closed(), "{cone:?}");
    assert!((cone.signed_volume_mm3 - polygon * h / 3.).abs() < polygon * h * 1e-9);
    // Segments 3 and 4 give a tetrahedron and a square pyramid.
    for sides in [3., 4.] {
        let pyramid = single(
            NodeKind::Cone,
            &[("radius", 10.), ("height", 20.), ("segments", sides)],
        );
        assert!(
            pyramid.is_closed() && pyramid.signed_volume_mm3 > 0.,
            "{pyramid:?}"
        );
    }
    let torus = single(NodeKind::Torus, &[]);
    assert!(torus.is_closed(), "{torus:?}");
    // Pappus: 2*pi^2*R*r^2 for the smooth torus; the faceted one differs by a few percent.
    let smooth = 2. * std::f64::consts::PI.powi(2) * 30. * 8. * 8.;
    assert!(torus.signed_volume_mm3 > 0.9 * smooth && torus.signed_volume_mm3 < 1.01 * smooth);
    // A minor radius that would make the ring self-overlap is an error, not a bad mesh.
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Torus, [0., 0.]).unwrap();
    for (key, value) in [("major", 5.), ("minor", 6.)] {
        graph.node_mut(id).unwrap().params.insert(key.into(), value);
    }
    assert!(evaluate_one(&graph, id).is_err());
}

#[test]
fn plane_is_an_upward_facing_open_grid() {
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Plane, [0., 0.]).unwrap();
    {
        let node = graph.node_mut(id).unwrap();
        node.params.insert("width".into(), 20.);
        node.params.insert("depth".into(), 10.);
        node.params.insert("divisions".into(), 4.);
    }
    let report = first_mesh_stats(&graph, id);
    assert_eq!(report.vertices, 25);
    assert_eq!(report.triangles, 32);
    assert_eq!(report.open_edges, 16);
    assert_eq!(report.inconsistent_edges, 0);
    assert_eq!(report.non_manifold_edges, 0);
    let bounds = report.bounds.unwrap();
    assert_eq!(bounds[0], [-10., -5., 0.]);
    assert_eq!(bounds[1], [10., 5., 0.]);
    let tree = evaluate_one(&graph, id).unwrap();
    let Some(Value::Mesh(mesh)) = tree.branches.values().flatten().next() else {
        panic!("mesh expected");
    };
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
        let nz = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        assert!(nz > 0., "triangle faces down");
    }
}

#[test]
fn mirror_and_non_uniform_scale_keep_solids_closed_and_outward() {
    let mut graph = Graph::default();
    let cube = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    // Box is 40 mm and spans x -20..20; move it off the mirror plane first.
    let moved = graph.add(NodeKind::Move, [0., 0.]).unwrap();
    graph
        .node_mut(moved)
        .unwrap()
        .params
        .insert("x".into(), 50.);
    graph.connect(cube, moved, "geometry").unwrap();
    let kept = graph.add(NodeKind::Mirror, [0., 0.]).unwrap();
    graph.connect(moved, kept, "geometry").unwrap();
    let only = graph.add(NodeKind::Mirror, [0., 0.]).unwrap();
    graph.connect(moved, only, "geometry").unwrap();
    graph
        .node_mut(only)
        .unwrap()
        .params
        .insert("keep".into(), 0.);
    let stretched = graph.add(NodeKind::ScaleXyz, [0., 0.]).unwrap();
    graph.connect(cube, stretched, "geometry").unwrap();
    for (key, value) in [("sx", 2.), ("sy", 3.), ("sz", 0.5)] {
        graph
            .node_mut(stretched)
            .unwrap()
            .params
            .insert(key.into(), value);
    }
    let both = first_mesh_stats(&graph, kept);
    assert!(both.is_closed(), "{both:?}");
    assert_eq!(both.vertices, 16);
    assert_eq!(both.triangles, 24);
    assert!((both.signed_volume_mm3 - 2. * 64_000.).abs() < 1e-6);
    let bounds = both.bounds.unwrap();
    assert!((bounds[0][0] + 70.).abs() < 1e-9 && (bounds[1][0] - 70.).abs() < 1e-9);
    let half = first_mesh_stats(&graph, only);
    assert!(half.is_closed(), "{half:?}");
    assert!((half.signed_volume_mm3 - 64_000.).abs() < 1e-6);
    let half_bounds = half.bounds.unwrap();
    assert!(
        half_bounds[1][0] < 0.,
        "mirrored copy must be on the negative side"
    );
    let scaled = first_mesh_stats(&graph, stretched);
    assert!(scaled.is_closed(), "{scaled:?}");
    assert!((scaled.signed_volume_mm3 - 64_000. * 3.).abs() < 1e-6);
}

#[test]
fn mirror_handles_curves_and_rejects_numbers() {
    let mut graph = Graph::default();
    let point = graph.add(NodeKind::Point, [0., 0.]).unwrap();
    graph.node_mut(point).unwrap().params.insert("x".into(), 5.);
    let mirror = graph.add(NodeKind::Mirror, [0., 0.]).unwrap();
    graph.connect(point, mirror, "geometry").unwrap();
    let tree = evaluate_one(&graph, mirror).unwrap();
    let points: Vec<_> = tree.branches.values().flatten().collect();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0], &Value::Point([5., 0., 0.]));
    assert_eq!(points[1], &Value::Point([-5., 0., 0.]));
    let number = graph.add(NodeKind::Number, [0., 0.]).unwrap();
    let bad = graph.add(NodeKind::Mirror, [0., 0.]).unwrap();
    // Number -> Geometry is rejected at connection time, and a hand-edited file that
    // forces it is rejected at validation time.
    assert!(graph.connect(number, bad, "geometry").is_err());
    graph
        .node_mut(bad)
        .unwrap()
        .inputs
        .insert("geometry".into(), number);
    assert!(graph.validate().is_err());
}

#[test]
fn old_files_without_the_new_nodes_still_load_and_evaluate() {
    let temp = TempDir::new();
    let path = temp.file("old.morph");
    let old = r##"{
  "schema_version": 1,
  "name": "旧作",
  "graph": {
    "nodes": [
      {"id": 0, "kind": "number", "name": "数値", "position": [0.0, 0.0],
       "params": {"value": 5.0}, "inputs": {}, "preview": false},
      {"id": 1, "kind": "divide", "name": "割り算", "position": [200.0, 0.0],
       "params": {"a": 1.0, "b": 2.0}, "inputs": {"a": 0}, "preview": false},
      {"id": 2, "kind": "box", "name": "箱", "position": [0.0, 200.0],
       "params": {"depth": 40.0, "height": 40.0, "width": 40.0}, "inputs": {}, "preview": false},
      {"id": 3, "kind": "scale", "name": "拡大縮小", "position": [200.0, 200.0],
       "params": {"factor": 1.2}, "inputs": {"geometry": 2}, "preview": false},
      {"id": 4, "kind": "output", "name": "出力", "position": [400.0, 200.0],
       "params": {}, "inputs": {"geometry": 3}, "preview": true}
    ],
    "next_id": 5
  },
  "pan": [0.0, 0.0],
  "zoom": 1.0
}"##;
    fs::write(&path, old).unwrap();
    let project = persistence::load(&path).unwrap();
    assert_eq!(project.graph.nodes.len(), 5);
    assert_eq!(project.graph.nodes[1].kind, NodeKind::Divide);
    let result = Evaluator::default().evaluate(&project.graph, &|| false);
    assert!(result.outputs.values().all(|r| r.is_ok()));
    // A file written by this version round-trips with the new kinds inside it.
    let mut graph = Graph::default();
    let range = graph.add(NodeKind::Range, [0., 0.]).unwrap();
    let sine = graph.add(NodeKind::Sine, [200., 0.]).unwrap();
    graph.connect(range, sine, "x").unwrap();
    let new_project = Project::new("新", graph);
    let new_path = temp.file("new.morph");
    persistence::save(&new_path, &new_project).unwrap();
    assert_eq!(persistence::load(&new_path).unwrap(), new_project);
    assert!(
        fs::read_to_string(&new_path)
            .unwrap()
            .contains("\"kind\": \"range\"")
    );
}

#[test]
fn shipped_example_files_load_and_evaluate() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    for number in 1..=templates::TEMPLATES.len() {
        let path = dir.join(format!("{number:02}.morph"));
        let project = persistence::load(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let result = Evaluator::default().evaluate(&project.graph, &|| false);
        for (id, output) in &result.outputs {
            assert!(output.is_ok(), "{path:?} node {id}: {output:?}");
        }
    }
    // The newest example files are written to match their templates exactly.
    // If this fails after a template edit, regenerate with `Morphlet --export-demos examples`.
    for index in [6, 7, 8, 9] {
        let path = dir.join(format!("{:02}.morph", index + 1));
        assert_eq!(persistence::load(&path).unwrap(), templates::create(index));
    }
}

#[test]
fn new_templates_use_the_new_nodes_and_old_ones_are_unchanged() {
    assert_eq!(templates::TEMPLATES.len(), 10);
    let old_names = [
        "ねじれの花器",
        "光のリング",
        "星のトーテム",
        "螺旋のスケッチ",
        "波打つブロック",
        "輪郭からロフト",
    ];
    for (index, name) in old_names.iter().enumerate() {
        assert_eq!(templates::TEMPLATES[index].name, *name);
    }
    for index in [6, 7] {
        let project = templates::create(index);
        assert_eq!(project.name, templates::TEMPLATES[index].name);
        let used: Vec<_> = project.graph.nodes.iter().map(|n| n.kind).collect();
        assert!(
            NEW_KINDS.iter().filter(|(k, _)| used.contains(k)).count() >= 4,
            "template {index} should exercise several new nodes"
        );
    }
}

// ---- List, formula, pattern and print-checking nodes (gh_nodes.rs, expr.rs) ----

const GH_KINDS: [(NodeKind, &str); 20] = [
    (NodeKind::ListItem, "list_item"),
    (NodeKind::ListLength, "list_length"),
    (NodeKind::ReverseList, "reverse_list"),
    (NodeKind::SortList, "sort_list"),
    (NodeKind::CullPattern, "cull_pattern"),
    (NodeKind::ShiftList, "shift_list"),
    (NodeKind::ConcatList, "concat_list"),
    (NodeKind::SubList, "sub_list"),
    (NodeKind::Bounds, "bounds"),
    (NodeKind::Remap, "remap"),
    (NodeKind::Expression, "expression"),
    (NodeKind::Random, "random"),
    (NodeKind::Populate, "populate"),
    (NodeKind::Noise, "noise"),
    (NodeKind::NoiseDeform, "noise_deform"),
    (NodeKind::GraphProfile, "graph_profile"),
    (NodeKind::Shell, "shell"),
    (NodeKind::OverhangCheck, "overhang_check"),
    (NodeKind::Distance, "distance"),
    (NodeKind::PointComponent, "point_component"),
];

fn set(graph: &mut Graph, id: NodeId, params: &[(&str, f64)]) {
    for (key, value) in params {
        graph
            .node_mut(id)
            .unwrap()
            .params
            .insert((*key).to_string(), *value);
    }
}
fn numbers_of(tree: &Tree) -> Vec<f64> {
    tree.branches
        .values()
        .flatten()
        .map(|v| match v {
            Value::Number(n) => *n,
            other => panic!("number expected, got {other:?}"),
        })
        .collect()
}
/// A list node fed by 数列 (start, step, count) on its first input.
fn list_result(
    kind: NodeKind,
    series: (f64, f64, f64),
    params: &[(&str, f64)],
) -> Result<Vec<f64>, String> {
    let mut graph = Graph::default();
    let source = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    set(
        &mut graph,
        source,
        &[("start", series.0), ("step", series.1), ("count", series.2)],
    );
    let id = graph.add(kind, [0., 0.]).unwrap();
    let first = kind.spec().inputs[0].key;
    graph.connect(source, id, first).unwrap();
    set(&mut graph, id, params);
    evaluate_one(&graph, id).map(|t| numbers_of(&t))
}

#[test]
fn gh_node_kinds_keep_stable_serde_names_and_have_japanese_specs() {
    for (kind, name) in GH_KINDS {
        assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{name}\""));
        let back: NodeKind = serde_json::from_str(&format!("\"{name}\"")).unwrap();
        assert_eq!(back, kind);
        assert!(NodeKind::all().contains(&kind), "{name} missing from all()");
        let spec = kind.spec();
        assert_ne!(spec.label, "不明なノード", "{name} has no spec");
        assert!(!spec.description.is_empty() && !spec.keywords.is_empty());
    }
}

#[test]
fn gh_nodes_evaluate_to_finite_non_empty_results_with_defaults() {
    let mut graph = Graph::default();
    let series = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    let cube = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    let points = graph.add(NodeKind::Populate, [0., 0.]).unwrap();
    let mut ids = Vec::new();
    for (kind, _) in GH_KINDS {
        let id = graph.add(kind, [0., 0.]).unwrap();
        for spec in kind.spec().inputs.iter().filter(|s| s.default.is_none()) {
            let from = match spec.kind {
                morphlet::model::DataType::Point => points,
                morphlet::model::DataType::Mesh | morphlet::model::DataType::Geometry => cube,
                _ => series,
            };
            graph.connect(from, id, spec.key).unwrap();
        }
        ids.push((kind, id));
    }
    graph.validate().unwrap();
    let result = Evaluator::default().evaluate(&graph, &|| false);
    for (kind, id) in ids {
        let tree = result.outputs[&id]
            .as_ref()
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        let values: Vec<_> = tree.branches.values().flatten().collect();
        assert!(!values.is_empty(), "{kind:?} produced nothing");
        for value in values {
            match value {
                Value::Number(n) => assert!(n.is_finite(), "{kind:?}: {n}"),
                Value::Point(p) => assert!(p.iter().all(|c| c.is_finite()), "{kind:?}"),
                Value::Mesh(m) => {
                    assert!(!m.triangles.is_empty(), "{kind:?}");
                    assert!(m.vertices.iter().flatten().all(|c| c.is_finite()));
                }
                Value::Curve(c) => assert!(c.points.iter().flatten().all(|v| v.is_finite())),
                other => panic!("unexpected precise geometry in legacy node: {other:?}"),
            }
        }
    }
}

#[test]
fn list_nodes_compute_expected_values() {
    let s = (0., 1., 5.); // 0 1 2 3 4
    let item = |params: &[(&str, f64)]| list_result(NodeKind::ListItem, s, params);
    assert_eq!(item(&[("index", 2.)]).unwrap(), [2.]);
    assert_eq!(item(&[("index", -1.)]).unwrap(), [4.]);
    assert_eq!(item(&[("index", 7.)]).unwrap(), [2.]);
    assert!(item(&[("index", 7.), ("wrap", 0.)]).is_err());
    assert!(item(&[("index", -1.), ("wrap", 0.)]).is_err());
    assert_eq!(list_result(NodeKind::ListLength, s, &[]).unwrap(), [5.]);
    assert_eq!(
        list_result(NodeKind::ListLength, (0., 1., 0.), &[]).unwrap(),
        [0.]
    );
    assert_eq!(
        list_result(NodeKind::ReverseList, s, &[]).unwrap(),
        [4., 3., 2., 1., 0.]
    );
    assert_eq!(
        list_result(NodeKind::SortList, (4., -1.5, 4.), &[]).unwrap(),
        [-0.5, 1., 2.5, 4.]
    );
    let shift = |params: &[(&str, f64)]| list_result(NodeKind::ShiftList, s, params).unwrap();
    assert_eq!(shift(&[("offset", 1.)]), [1., 2., 3., 4., 0.]);
    assert_eq!(shift(&[("offset", -1.)]), [4., 0., 1., 2., 3.]);
    assert_eq!(shift(&[("offset", 1.), ("wrap", 0.)]), [1., 2., 3., 4.]);
    assert_eq!(shift(&[("offset", -2.), ("wrap", 0.)]), [0., 1., 2.]);
    assert!(shift(&[("offset", 9.), ("wrap", 0.)]).is_empty());
    let sub = |params: &[(&str, f64)]| list_result(NodeKind::SubList, s, params).unwrap();
    assert_eq!(sub(&[("start", 1.), ("count", 2.)]), [1., 2.]);
    assert_eq!(sub(&[("start", 4.), ("count", 10.)]), [4.]);
    assert!(sub(&[("start", 8.), ("count", 2.)]).is_empty());
    assert_eq!(
        list_result(NodeKind::Bounds, (3., -2., 4.), &[]).unwrap(),
        [-3., 3.]
    );
    assert_eq!(
        list_result(
            NodeKind::Remap,
            s,
            &[("target_min", 0.), ("target_max", 100.)]
        )
        .unwrap(),
        [0., 25., 50., 75., 100.]
    );
    assert_eq!(
        list_result(
            NodeKind::Remap,
            s,
            &[
                ("target_min", 0.),
                ("target_max", 1.),
                ("use_source", 1.),
                ("source_min", 0.),
                ("source_max", 2.),
                ("clamp", 1.),
            ]
        )
        .unwrap(),
        [0., 0.5, 1., 1., 1.]
    );
    // A list of equal values has no width: every value goes to the middle.
    assert_eq!(
        list_result(NodeKind::Remap, (7., 0., 3.), &[("target_max", 10.)]).unwrap(),
        [5., 5., 5.]
    );

    // 間引きパターン with a 0/1 pattern made by 剰余, and 連結 of two lists.
    let mut graph = Graph::default();
    let series = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    set(
        &mut graph,
        series,
        &[("start", 0.), ("step", 1.), ("count", 5.)],
    );
    let pattern = graph.add(NodeKind::Modulo, [0., 0.]).unwrap();
    set(&mut graph, pattern, &[("b", 2.)]);
    graph.connect(series, pattern, "a").unwrap();
    let cull = graph.add(NodeKind::CullPattern, [0., 0.]).unwrap();
    graph.connect(series, cull, "list").unwrap();
    graph.connect(pattern, cull, "pattern").unwrap();
    let concat = graph.add(NodeKind::ConcatList, [0., 0.]).unwrap();
    graph.connect(series, concat, "a").unwrap();
    graph.connect(cull, concat, "b").unwrap();
    let result = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(
        numbers_of(result.outputs[&cull].as_ref().unwrap()),
        [1., 3.]
    );
    assert_eq!(
        numbers_of(result.outputs[&concat].as_ref().unwrap()),
        [0., 1., 2., 3., 4., 1., 3.]
    );
}

#[test]
fn list_nodes_keep_branches_separate() {
    let mut graph = Graph::default();
    let series = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    set(&mut graph, series, &[("count", 3.)]);
    let grafted = graph.add(NodeKind::Graft, [0., 0.]).unwrap();
    graph.connect(series, grafted, "geometry").unwrap();
    let length = graph.add(NodeKind::ListLength, [0., 0.]).unwrap();
    graph.connect(grafted, length, "list").unwrap();
    let tree = evaluate_one(&graph, length).unwrap();
    assert_eq!(tree.branches.len(), 3);
    assert_eq!(numbers_of(&tree), [1., 1., 1.]);
}

#[test]
fn expression_parses_evaluates_and_rejects_garbage() {
    use morphlet::expr::{MAX_EXPR_LEN, parse};
    let eval = |src: &str, vars: [f64; 4]| parse(src).and_then(|e| e.eval(vars));
    assert_eq!(eval("x * 2 + y", [3., 1., 0., 0.]).unwrap(), 7.);
    assert_eq!(eval("x + y + z + w", [1., 2., 3., 4.]).unwrap(), 10.);
    assert_eq!(eval("2 ^ 3 ^ 2", [0.; 4]).unwrap(), 512.);
    assert_eq!(eval("-2 ^ 2", [0.; 4]).unwrap(), -4.);
    assert_eq!(eval("(1 + 2) * 3 % 4", [0.; 4]).unwrap(), 1.);
    assert_eq!(eval("max(x, 3) - min(2, 5)", [1., 0., 0., 0.]).unwrap(), 1.);
    assert!((eval("sin(pi / 2)", [0.; 4]).unwrap() - 1.).abs() < 1e-12);
    // Full-width characters typed through a Japanese IME are accepted.
    assert_eq!(eval("ｘ＊２＋１", [4., 0., 0., 0.]).unwrap(), 9.);
    assert!(parse(morphlet::expr::DEFAULT_EXPRESSION).is_ok());
    // Bad text is a Japanese error, never a panic.
    for bad in [
        "",
        "   ",
        "x +",
        "* x",
        "(x",
        "x)",
        "foo(1)",
        "min(1)",
        "sin(1, 2)",
        "1.2.3",
        "1e400",
        "x; drop",
        "日本語",
        "a + b",
        "\u{0}",
    ] {
        let error = parse(bad).unwrap_err();
        assert!(!error.is_empty(), "{bad:?}");
    }
    // Domain errors at evaluation time are errors, never NaN or infinity.
    for bad in [
        "x / 0",
        "x % 0",
        "sqrt(-1)",
        "ln(0)",
        "0 ^ -1",
        "(-8) ^ 0.5",
        "exp(1000)",
    ] {
        assert!(eval(bad, [1., 0., 0., 0.]).is_err(), "{bad}");
    }
    // Length, nesting and size are capped; deep input does not overflow the stack.
    assert!(parse(&"1+".repeat(MAX_EXPR_LEN)).is_err());
    assert!(parse(&format!("{}x{}", "(".repeat(100), ")".repeat(100))).is_err());
    assert!(parse(&format!("{}x", "-".repeat(200))).is_err());
    assert!(parse(&format!("{}x", "2^".repeat(100))).is_err());
    assert!(parse(&"x".repeat(100_000)).is_err());
    assert!(parse(&format!("{}x{}", "sin(".repeat(60), ")".repeat(60))).is_err());
    assert!(parse(&format!("{}x{}", "(".repeat(10), ")".repeat(10))).is_ok());
}

#[test]
fn expression_node_maps_lists_and_keeps_its_text_in_files() {
    let temp = TempDir::new();
    let mut graph = Graph::default();
    let range = graph.add(NodeKind::Range, [0., 0.]).unwrap();
    set(
        &mut graph,
        range,
        &[("start", 0.), ("end", 4.), ("count", 5.)],
    );
    let formula = graph.add(NodeKind::Expression, [0., 0.]).unwrap();
    assert_eq!(
        graph.node(formula).unwrap().script,
        morphlet::expr::DEFAULT_EXPRESSION
    );
    graph.node_mut(formula).unwrap().script = "x * x + y".into();
    set(&mut graph, formula, &[("y", 1.)]);
    graph.connect(range, formula, "x").unwrap();
    assert_eq!(
        numbers_of(&evaluate_one(&graph, formula).unwrap()),
        [1., 2., 5., 10., 17.]
    );
    let path = temp.file("expr.morph");
    let project = Project::new("式", graph.clone());
    persistence::save(&path, &project).unwrap();
    assert_eq!(persistence::load(&path).unwrap(), project);
    // A broken formula marks only its own node, and editing the text recomputes it.
    let cube = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    graph.node_mut(formula).unwrap().script = "x +* 2".into();
    let mut engine = Evaluator::default();
    let result = engine.evaluate(&graph, &|| false);
    assert!(result.outputs[&formula].is_err());
    assert!(result.outputs[&cube].is_ok());
    graph.node_mut(formula).unwrap().script = "x".into();
    let result = engine.evaluate(&graph, &|| false);
    assert_eq!(
        numbers_of(result.outputs[&formula].as_ref().unwrap()),
        [0., 1., 2., 3., 4.]
    );
}

#[test]
fn random_and_populate_are_deterministic_and_bounded() {
    let random = |seed: f64| {
        eval_numbers(
            NodeKind::Random,
            &[("count", 200.), ("min", -5.), ("max", 5.), ("seed", seed)],
        )
        .unwrap()
    };
    let a = random(1.);
    assert_eq!(a.len(), 200);
    assert_eq!(a, random(1.));
    assert_ne!(a, random(2.));
    assert!(a.iter().all(|v| (-5. ..5.).contains(v)));
    assert!(eval_numbers(NodeKind::Random, &[("count", 513.)]).is_err());
    assert!(eval_numbers(NodeKind::Random, &[("min", f64::NAN)]).is_err());

    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Populate, [0., 0.]).unwrap();
    set(
        &mut graph,
        id,
        &[
            ("count", 100.),
            ("width", 40.),
            ("depth", 20.),
            ("height", 10.),
        ],
    );
    let tree = evaluate_one(&graph, id).unwrap();
    let points: Vec<_> = tree.branches.values().flatten().collect();
    assert_eq!(points.len(), 100);
    for p in points {
        let Value::Point(p) = p else {
            panic!("point expected")
        };
        assert!(p[0].abs() <= 20. && p[1].abs() <= 10. && (0. ..=10.).contains(&p[2]));
    }
    assert_eq!(evaluate_one(&graph, id).unwrap(), tree);
    set(&mut graph, id, &[("width", -1.)]);
    assert!(evaluate_one(&graph, id).is_err());
}

#[test]
fn noise_is_smooth_deterministic_and_finite_at_extremes() {
    let noise = |params: &[(&str, f64)]| eval_numbers(NodeKind::Noise, params).unwrap()[0];
    let a = noise(&[("x", 3.3), ("y", -1.2), ("z", 7.), ("amp", 5.)]);
    assert_eq!(a, noise(&[("x", 3.3), ("y", -1.2), ("z", 7.), ("amp", 5.)]));
    assert!(a.abs() <= 5.);
    let b = noise(&[("x", 3.31), ("y", -1.2), ("z", 7.), ("amp", 5.)]);
    assert!((a - b).abs() < 0.1, "noise should change smoothly");
    for x in [-1e5, -0.5, 0., 1e-9, 99_999.9, 1e5] {
        let v = noise(&[("x", x), ("y", x), ("z", -x), ("size", 0.1), ("amp", 1e4)]);
        assert!(v.is_finite() && v.abs() <= 1e4);
    }
    assert!(eval_numbers(NodeKind::Noise, &[("size", 0.)]).is_err());
    assert!(eval_numbers(NodeKind::Noise, &[("x", f64::INFINITY)]).is_err());
    for (x, y) in [(0., 0.), (1., 1.), (2., -3.)] {
        // On the integer lattice gradient noise is exactly zero.
        let v = noise(&[("x", x * 20.), ("y", y * 20.), ("z", 0.), ("amp", 1.)]);
        assert!(v.abs() < 1e-12);
    }
}

#[test]
fn noise_deform_keeps_a_vase_closed() {
    let mut graph = Graph::default();
    let vase = graph.add(NodeKind::Vase, [0., 0.]).unwrap();
    for mode in [0., 1., 2.] {
        let id = graph.add(NodeKind::NoiseDeform, [0., 0.]).unwrap();
        set(&mut graph, id, &[("mode", mode), ("amp", 1.)]);
        graph.connect(vase, id, "geometry").unwrap();
        let report = first_mesh_stats(&graph, id);
        assert!(report.is_closed(), "mode {mode}: {report:?}");
        assert!(report.signed_volume_mm3 > 0.);
    }
}

#[test]
fn graph_profile_shell_makes_a_closed_outward_printable_wall() {
    let mut graph = Graph::default();
    let radii = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    set(
        &mut graph,
        radii,
        &[("start", 20.), ("step", 4.), ("count", 5.)],
    );
    let body = graph.add(NodeKind::GraphProfile, [0., 0.]).unwrap();
    graph.connect(radii, body, "radii").unwrap();
    let open = first_mesh_stats(&graph, body);
    assert_eq!(open.open_edges, 64, "only the top rim is open: {open:?}");
    assert_eq!(open.inconsistent_edges + open.non_manifold_edges, 0);
    let shell = graph.add(NodeKind::Shell, [0., 0.]).unwrap();
    graph.connect(body, shell, "geometry").unwrap();
    let solid = first_mesh_stats(&graph, shell);
    assert!(solid.is_closed(), "{solid:?}");
    assert!(solid.signed_volume_mm3 > 0., "{solid:?}");
    // The wall is thin compared with the filled cup.
    set(&mut graph, body, &[("top", 1.)]);
    let cup = first_mesh_stats(&graph, body);
    assert!(cup.is_closed(), "{cup:?}");
    set(&mut graph, body, &[("top", 0.)]);
    assert!(solid.signed_volume_mm3 < cup.signed_volume_mm3 * 0.4);
    // Twisting and noise first, as in the example, still gives a closed wall.
    let noise = graph.add(NodeKind::NoiseDeform, [0., 0.]).unwrap();
    graph.connect(body, noise, "geometry").unwrap();
    let twist = graph.add(NodeKind::Twist, [0., 0.]).unwrap();
    graph.connect(noise, twist, "geometry").unwrap();
    let wall = graph.add(NodeKind::Shell, [0., 0.]).unwrap();
    graph.connect(twist, wall, "geometry").unwrap();
    let report = first_mesh_stats(&graph, wall);
    assert!(
        report.is_closed() && report.signed_volume_mm3 > 0.,
        "{report:?}"
    );
    // Too thick for the shape is an error, not a folded mesh.
    set(&mut graph, shell, &[("thickness", 50.)]);
    assert!(evaluate_one(&graph, shell).is_err());
    // A radius list that dips below 0.5 mm is refused.
    set(
        &mut graph,
        radii,
        &[("start", 40.), ("step", -10.), ("count", 5.)],
    );
    assert!(evaluate_one(&graph, body).is_err());
}

#[test]
fn shell_of_a_box_has_the_exact_hollow_volume() {
    let mut graph = Graph::default();
    let cube = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    let shell = graph.add(NodeKind::Shell, [0., 0.]).unwrap();
    graph.connect(cube, shell, "geometry").unwrap();
    let report = first_mesh_stats(&graph, shell);
    assert!(report.is_closed(), "{report:?}");
    let expected = 40f64.powi(3) - 36f64.powi(3);
    assert!(
        (report.signed_volume_mm3 - expected).abs() < 1e-6,
        "{report:?}"
    );
    // An open plane becomes a closed slab.
    let plane = graph.add(NodeKind::Plane, [0., 0.]).unwrap();
    let slab = graph.add(NodeKind::Shell, [0., 0.]).unwrap();
    graph.connect(plane, slab, "geometry").unwrap();
    let report = first_mesh_stats(&graph, slab);
    assert!(report.is_closed(), "{report:?}");
    assert!((report.signed_volume_mm3 - 100. * 100. * 2.).abs() < 1e-6);
}

#[test]
fn overhang_check_reports_flat_downward_faces() {
    let check = |kind: NodeKind, angle: f64| {
        let mut graph = Graph::default();
        let shape = graph.add(kind, [0., 0.]).unwrap();
        let id = graph.add(NodeKind::OverhangCheck, [0., 0.]).unwrap();
        set(&mut graph, id, &[("angle", angle)]);
        graph.connect(shape, id, "geometry").unwrap();
        numbers_of(&evaluate_one(&graph, id).unwrap())
    };
    // A box only has vertical walls and a bottom on the plate.
    assert_eq!(check(NodeKind::Box, 45.), [0., 0., 0.]);
    // The lower half of a sphere overhangs.
    let sphere = check(NodeKind::Sphere, 45.);
    assert_eq!(sphere.len(), 3);
    assert!(sphere[0] > 5. && sphere[0] < 50., "{sphere:?}");
    assert!(sphere[1] > 0.);
    assert!(sphere[2] > 45. && sphere[2] <= 90.);
    let lenient = check(NodeKind::Sphere, 80.);
    assert!(lenient[0] < sphere[0]);
}

#[test]
fn distance_and_point_component() {
    let mut graph = Graph::default();
    let a = graph.add(NodeKind::Point, [0., 0.]).unwrap();
    set(&mut graph, a, &[("x", 3.), ("y", 4.), ("z", 0.)]);
    let b = graph.add(NodeKind::Point, [0., 0.]).unwrap();
    let d = graph.add(NodeKind::Distance, [0., 0.]).unwrap();
    graph.connect(a, d, "a").unwrap();
    graph.connect(b, d, "b").unwrap();
    let c = graph.add(NodeKind::PointComponent, [0., 0.]).unwrap();
    set(&mut graph, c, &[("axis", 1.)]);
    graph.connect(a, c, "point").unwrap();
    let result = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(numbers_of(result.outputs[&d].as_ref().unwrap()), [5.]);
    assert_eq!(numbers_of(result.outputs[&c].as_ref().unwrap()), [4.]);
    // A number cannot be wired into a point input.
    let n = graph.add(NodeKind::Number, [0., 0.]).unwrap();
    assert!(graph.connect(n, d, "a").is_err());
}

#[test]
fn new_example_works_are_closed_and_use_the_new_nodes() {
    for (index, needed) in [
        (
            8,
            &[
                NodeKind::GraphProfile,
                NodeKind::NoiseDeform,
                NodeKind::Twist,
                NodeKind::Shell,
                NodeKind::Expression,
            ][..],
        ),
        (
            9,
            &[
                NodeKind::Populate,
                NodeKind::Distance,
                NodeKind::Remap,
                NodeKind::Cylinder,
            ][..],
        ),
    ] {
        let project = templates::create(index);
        assert_eq!(project.name, templates::TEMPLATES[index].name);
        let used: Vec<_> = project.graph.nodes.iter().map(|n| n.kind).collect();
        for kind in needed {
            assert!(used.contains(kind), "template {index} lacks {kind:?}");
        }
        let output = project
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Output)
            .unwrap()
            .id;
        let report = first_mesh_stats(&project.graph, output);
        assert!(report.is_closed(), "template {index}: {report:?}");
        assert!(report.signed_volume_mm3 > 0.);
    }
    // The attractor example makes one column per random point.
    let project = templates::create(9);
    let output = project
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Output)
        .unwrap()
        .id;
    assert_eq!(
        evaluate_one(&project.graph, output).unwrap().item_count(),
        60
    );
}

#[test]
fn hostile_settings_on_new_nodes_are_errors_not_panics() {
    let samples = [
        f64::NAN,
        f64::INFINITY,
        -f64::INFINITY,
        -1e300,
        1e300,
        -1.,
        0.,
        0.5,
    ];
    let mut graph = Graph::default();
    let series = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    let cube = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    let points = graph.add(NodeKind::Populate, [0., 0.]).unwrap();
    for (kind, _) in GH_KINDS {
        for spec in kind.spec().inputs.iter().filter(|s| s.default.is_some()) {
            for value in samples {
                let mut g = graph.clone();
                let id = g.add(kind, [0., 0.]).unwrap();
                for input in kind.spec().inputs.iter().filter(|s| s.default.is_none()) {
                    let from = match input.kind {
                        morphlet::model::DataType::Point => points,
                        morphlet::model::DataType::Mesh | morphlet::model::DataType::Geometry => {
                            cube
                        }
                        _ => series,
                    };
                    g.connect(from, id, input.key).unwrap();
                }
                set(&mut g, id, &[(spec.key, value)]);
                let result = Evaluator::default().evaluate(&g, &|| false);
                if let Some(Ok(tree)) = result.outputs.get(&id) {
                    for v in tree.branches.values().flatten() {
                        if let Value::Number(n) = v {
                            assert!(n.is_finite(), "{kind:?} {} = {value}", spec.key);
                        }
                    }
                }
            }
        }
    }
}
