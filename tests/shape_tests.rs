//! Curve, tube, mesh-repair, transform and gyroid nodes (`src/nodes_shape.rs`).
//! Written without compiling or running; CI is the first check.
use nodes::engine::Evaluator;
use nodes::model::{Graph, Mesh, NodeId, NodeKind, Tree, Value};
use std::collections::BTreeMap;

/// Adds a node and sets some of its numbers.
fn node(g: &mut Graph, kind: NodeKind, params: &[(&str, f64)]) -> NodeId {
    let id = g.add(kind, [0., 0.]).unwrap();
    for (key, value) in params {
        g.node_mut(id)
            .unwrap()
            .params
            .insert((*key).to_string(), *value);
    }
    id
}

fn wire(g: &mut Graph, from: NodeId, to: NodeId, input: &str) {
    g.connect(from, to, input).unwrap();
}

/// Evaluates the whole graph and returns the result of one node.
fn result(g: &Graph, id: NodeId) -> Result<Tree, String> {
    let evaluation = Evaluator::default().evaluate(g, &|| false);
    evaluation
        .outputs
        .get(&id)
        .cloned()
        .unwrap_or_else(|| Err("結果がありません".into()))
}

fn mesh_of(tree: &Tree) -> Mesh {
    tree.branches
        .values()
        .flatten()
        .find_map(|v| match v {
            Value::Mesh(m) => Some((**m).clone()),
            _ => None,
        })
        .expect("the result should contain a mesh")
}

/// Every undirected edge is used by exactly two faces.
fn is_closed(m: &Mesh) -> bool {
    let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &m.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *uses.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    !uses.is_empty() && uses.values().all(|&c| c == 2)
}

fn signed_volume(m: &Mesh) -> f64 {
    m.triangles
        .iter()
        .map(|t| {
            let (p, q, r) = (
                m.vertices[t[0] as usize],
                m.vertices[t[1] as usize],
                m.vertices[t[2] as usize],
            );
            p[0] * (q[1] * r[2] - q[2] * r[1]) - p[1] * (q[0] * r[2] - q[2] * r[0])
                + p[2] * (q[0] * r[1] - q[1] * r[0])
        })
        .sum::<f64>()
        / 6.
}

/// A graph whose output is the node under test, fed by simple sources.
fn sample(kind: NodeKind) -> (Graph, NodeId) {
    let mut g = Graph::default();
    let out = match kind {
        NodeKind::ArcCurve => node(&mut g, kind, &[]),
        NodeKind::Polyline | NodeKind::SmoothCurve => {
            let points = node(
                &mut g,
                NodeKind::Populate,
                &[
                    ("count", 6.),
                    ("width", 40.),
                    ("depth", 40.),
                    ("height", 20.),
                    ("seed", 2.),
                ],
            );
            let out = node(&mut g, kind, &[]);
            wire(&mut g, points, out, "points");
            out
        }
        NodeKind::OffsetCurve => {
            let rect = node(
                &mut g,
                NodeKind::Rectangle,
                &[("width", 20.), ("depth", 20.)],
            );
            let out = node(&mut g, kind, &[("distance", 2.)]);
            wire(&mut g, rect, out, "curve");
            out
        }
        NodeKind::PointOnCurve | NodeKind::DivideCurve => {
            let circle = node(&mut g, NodeKind::Circle, &[("radius", 10.)]);
            let out = node(&mut g, kind, &[]);
            wire(&mut g, circle, out, "curve");
            out
        }
        NodeKind::Pipe => {
            let helix = node(
                &mut g,
                NodeKind::Helix,
                &[
                    ("radius", 20.),
                    ("height", 40.),
                    ("turns", 2.),
                    ("segments", 64.),
                ],
            );
            let out = node(&mut g, kind, &[("radius", 2.), ("sides", 8.)]);
            wire(&mut g, helix, out, "curve");
            out
        }
        NodeKind::Sweep => {
            let profile = node(
                &mut g,
                NodeKind::Circle,
                &[("radius", 2.), ("segments", 8.)],
            );
            let path = node(
                &mut g,
                NodeKind::Helix,
                &[
                    ("radius", 20.),
                    ("height", 40.),
                    ("turns", 1.),
                    ("segments", 32.),
                ],
            );
            let out = node(&mut g, kind, &[]);
            wire(&mut g, profile, out, "profile");
            wire(&mut g, path, out, "path");
            out
        }
        NodeKind::RevolvePoints => {
            let points = node(
                &mut g,
                NodeKind::Populate,
                &[
                    ("count", 8.),
                    ("width", 30.),
                    ("depth", 30.),
                    ("height", 60.),
                    ("seed", 5.),
                ],
            );
            let out = node(&mut g, kind, &[("steps", 8.)]);
            wire(&mut g, points, out, "points");
            out
        }
        NodeKind::Gyroid => node(&mut g, kind, &[("resolution", 24.)]),
        // Mesh and geometry inputs: a box feeds every node that takes "geometry".
        _ => {
            let source = node(&mut g, NodeKind::Box, &[]);
            let out = node(&mut g, kind, &[]);
            wire(&mut g, source, out, "geometry");
            out
        }
    };
    (g, out)
}

#[test]
fn every_new_node_is_finite_and_non_empty() {
    let kinds = [
        NodeKind::ArcCurve,
        NodeKind::Polyline,
        NodeKind::SmoothCurve,
        NodeKind::OffsetCurve,
        NodeKind::PointOnCurve,
        NodeKind::DivideCurve,
        NodeKind::Pipe,
        NodeKind::Sweep,
        NodeKind::SmoothMesh,
        NodeKind::Subdivide,
        NodeKind::WeldMesh,
        NodeKind::UnifyNormals,
        NodeKind::MeshInfo,
        NodeKind::TaperScale,
        NodeKind::Bend,
        NodeKind::GridArray3d,
        NodeKind::Gyroid,
        NodeKind::RevolvePoints,
    ];
    for kind in kinds {
        let (g, out) = sample(kind);
        let tree = result(&g, out).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        assert!(tree.item_count() > 0, "{kind:?} produced nothing");
        for value in tree.branches.values().flatten() {
            match value {
                Value::Number(v) => assert!(v.is_finite(), "{kind:?}"),
                Value::Point(p) => assert!(p.iter().all(|c| c.is_finite()), "{kind:?}"),
                Value::Curve(c) => {
                    assert!(!c.points.is_empty(), "{kind:?}");
                    assert!(c.points.iter().flatten().all(|v| v.is_finite()), "{kind:?}");
                }
                Value::Mesh(m) => {
                    assert!(!m.triangles.is_empty(), "{kind:?}");
                    assert!(
                        m.vertices.iter().flatten().all(|v| v.is_finite()),
                        "{kind:?}"
                    );
                }
                Value::Surface(_) | Value::Solid(_) | Value::Gcode(_) => {}
            }
        }
    }
}

#[test]
fn pipe_and_gyroid_are_closed_solids() {
    let (g, out) = sample(NodeKind::Pipe);
    let pipe = mesh_of(&result(&g, out).unwrap());
    assert!(is_closed(&pipe), "a capped pipe must be closed");
    assert!(signed_volume(&pipe) > 0., "a pipe must face outward");

    let (g, out) = sample(NodeKind::Gyroid);
    let gyroid = mesh_of(&result(&g, out).unwrap());
    assert!(is_closed(&gyroid), "the gyroid solid must be closed");
    assert!(signed_volume(&gyroid) > 0., "the gyroid must face outward");
}

#[test]
fn offset_of_a_square_has_the_expected_area() {
    // A 20 mm square grown by 2 mm with mitred corners is 24 mm square: 576 mm².
    let (g, out) = sample(NodeKind::OffsetCurve);
    let tree = result(&g, out).unwrap();
    let points: Vec<[f64; 3]> = tree
        .branches
        .values()
        .flatten()
        .find_map(|v| match v {
            Value::Curve(c) => Some(c.points.clone()),
            _ => None,
        })
        .expect("a curve");
    let n = points.len();
    let area = 0.5
        * (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>();
    assert!((area - 576.).abs() < 1e-6, "area was {area}");
}

#[test]
fn smoothing_keeps_the_vertex_and_triangle_counts() {
    let source = {
        let mut g = Graph::default();
        let boxed = node(&mut g, NodeKind::Box, &[]);
        let plain = node(&mut g, NodeKind::Output, &[]);
        wire(&mut g, boxed, plain, "geometry");
        mesh_of(&result(&g, plain).unwrap())
    };
    let (g, out) = sample(NodeKind::SmoothMesh);
    let smoothed = mesh_of(&result(&g, out).unwrap());
    assert_eq!(smoothed.vertices.len(), source.vertices.len());
    assert_eq!(smoothed.triangles, source.triangles);
    assert!(
        smoothed.vertices != source.vertices,
        "smoothing should move vertices"
    );
}

#[test]
fn hostile_parameters_are_refused() {
    // NaN and infinite numbers never reach the node's calculation.
    let mut g = Graph::default();
    let arc = node(&mut g, NodeKind::ArcCurve, &[("radius", f64::NAN)]);
    assert!(result(&g, arc).is_err());
    let mut g = Graph::default();
    let arc = node(&mut g, NodeKind::ArcCurve, &[("segments", f64::INFINITY)]);
    assert!(result(&g, arc).is_err());

    // Sizes and counts above the node's range or the engine's budget.
    let mut g = Graph::default();
    let gyroid = node(&mut g, NodeKind::Gyroid, &[("resolution", 200.)]);
    assert!(result(&g, gyroid).is_err());
    let mut g = Graph::default();
    let source = node(&mut g, NodeKind::Box, &[]);
    let grid = node(
        &mut g,
        NodeKind::GridArray3d,
        &[("nx", 16.), ("ny", 16.), ("nz", 16.)],
    );
    wire(&mut g, source, grid, "geometry");
    let err = result(&g, grid).unwrap_err();
    assert!(err.contains("個"), "{err}");

    // A 20 mm square shrunk by 10 mm collapses to a point: refused, not a degenerate curve.
    let mut g = Graph::default();
    let rect = node(
        &mut g,
        NodeKind::Rectangle,
        &[("width", 20.), ("depth", 20.)],
    );
    let off = node(&mut g, NodeKind::OffsetCurve, &[("distance", -10.)]);
    wire(&mut g, rect, off, "curve");
    assert!(result(&g, off).is_err());

    // Bending a 40 mm box through 330 degrees folds it onto itself.
    let mut g = Graph::default();
    let source = node(&mut g, NodeKind::Box, &[]);
    let bend = node(&mut g, NodeKind::Bend, &[("angle", 330.)]);
    wire(&mut g, source, bend, "geometry");
    let err = result(&g, bend).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn serde_names_are_stable_and_listed() {
    let pairs = [
        (NodeKind::ArcCurve, "arc_curve"),
        (NodeKind::Polyline, "polyline"),
        (NodeKind::SmoothCurve, "smooth_curve"),
        (NodeKind::OffsetCurve, "offset_curve"),
        (NodeKind::PointOnCurve, "point_on_curve"),
        (NodeKind::DivideCurve, "divide_curve"),
        (NodeKind::Pipe, "pipe"),
        (NodeKind::Sweep, "sweep"),
        (NodeKind::SmoothMesh, "smooth_mesh"),
        (NodeKind::Subdivide, "subdivide"),
        (NodeKind::WeldMesh, "weld_mesh"),
        (NodeKind::UnifyNormals, "unify_normals"),
        (NodeKind::MeshInfo, "mesh_info"),
        (NodeKind::TaperScale, "taper_scale"),
        (NodeKind::Bend, "bend"),
        (NodeKind::GridArray3d, "grid_array3d"),
        (NodeKind::Gyroid, "gyroid"),
        (NodeKind::RevolvePoints, "revolve_points"),
    ];
    for (kind, name) in pairs {
        assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{name}\""));
        let back: NodeKind = serde_json::from_str(&format!("\"{name}\"")).unwrap();
        assert_eq!(back, kind);
        assert!(NodeKind::all().contains(&kind), "{name} missing from all()");
    }
}
