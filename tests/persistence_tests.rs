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
    // 42 geometry/data nodes plus the Script node (Cluster is created from a definition).
    assert_eq!(NodeKind::all().len(), 43);
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
    // The two newest example files are written to match their templates exactly.
    // If this fails after a template edit, regenerate with `Morphlet --export-demos examples`.
    for index in [6, 7] {
        let path = dir.join(format!("{:02}.morph", index + 1));
        assert_eq!(persistence::load(&path).unwrap(), templates::create(index));
    }
}

#[test]
fn new_templates_use_the_new_nodes_and_old_ones_are_unchanged() {
    assert_eq!(templates::TEMPLATES.len(), 8);
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
