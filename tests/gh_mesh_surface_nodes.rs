use morphlet::engine::{Evaluation, Evaluator};
use morphlet::mesh_fabrication::audit;
use morphlet::model::{Graph, Mesh, NodeId, NodeKind as K, Value};
use morphlet::node_access::{InputAccess, for_input};
use morphlet::persistence::{self, Project};
use morphlet::surface_kernel::{NurbsSurface, PlaneSurface, Surface};
use std::f64::consts::PI;

fn add(graph: &mut Graph, kind: K, params: &[(&str, f64)]) -> NodeId {
    let id = graph.add(kind, [0.; 2]).unwrap();
    graph
        .node_mut(id)
        .unwrap()
        .params
        .extend(params.iter().map(|&(key, value)| (key.to_owned(), value)));
    id
}

fn connected(graph: &mut Graph, kind: K, source: NodeId, port: &str) -> NodeId {
    let id = add(graph, kind, &[]);
    graph.connect(source, id, port).unwrap();
    id
}

fn point(graph: &mut Graph, [x, y, z]: [f64; 3]) -> NodeId {
    add(graph, K::Point, &[("x", x), ("y", y), ("z", z)])
}

fn values(evaluation: &Evaluation, id: NodeId) -> &[Value] {
    &evaluation.outputs[&id].as_ref().unwrap().branches[&vec![0]]
}

fn mesh(value: &Value) -> &Mesh {
    let Value::Mesh(mesh) = value else {
        panic!("mesh expected");
    };
    mesh
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

fn point_near(actual: [f64; 3], expected: [f64; 3]) {
    for (a, b) in actual.into_iter().zip(expected) {
        near(a, b);
    }
}

fn quad(graph: &mut Graph) -> NodeId {
    let points = add(
        graph,
        K::RectGrid,
        &[("nx", 2.), ("ny", 2.), ("dx", 3.), ("dy", 4.)],
    );
    let faces = add(graph, K::MeshQuad, &[("c", 3.), ("d", 2.)]);
    let constructor = connected(graph, K::MeshConstruct, points, "vertices");
    graph.connect(faces, constructor, "faces").unwrap();
    constructor
}

#[test]
fn face_index_and_analysis_nodes_evaluate_after_native_save_and_reload() {
    let mut graph = Graph::default();
    let mesh_id = quad(&mut graph);
    let triangle = add(
        &mut graph,
        K::MeshTriangle,
        &[("a", 8.), ("b", 2.), ("c", 11.)],
    );
    let invalid_triangle = add(&mut graph, K::MeshTriangle, &[("a", 2.)]);
    let [boundaries, centers, circles, ratios, edges, normals] = [
        K::MeshFaceBoundaries,
        K::MeshFaceCenters,
        K::MeshFaceCircles,
        K::MeshFaceAspectRatio,
        K::MeshInteriorEdges,
        K::MeshVertexNormals,
    ]
    .map(|kind| connected(&mut graph, kind, mesh_id, "mesh"));

    let shift = add(&mut graph, K::Move, &[("z", 7.)]);
    graph.connect(circles, shift, "geometry").unwrap();
    let lengths = connected(&mut graph, K::CurveLength, shift, "curve");

    let project = Project::new("mesh face adapters", graph);
    let path = std::env::temp_dir().join(format!(
        "nodes-gh-mesh-surface-{}.morph",
        std::process::id()
    ));
    persistence::save(&path, &project).unwrap();
    let loaded = persistence::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(project, loaded);
    let e = Evaluator::default().evaluate(&loaded.graph, &|| false);

    let quad = mesh(&values(&e, mesh_id)[0]);
    assert_eq!(quad.triangles, [[0, 1, 3], [0, 3, 2]]);
    assert_eq!(
        values(&e, triangle),
        [Value::Number(8.), Value::Number(2.), Value::Number(11.)]
    );
    assert!(e.outputs[&invalid_triangle].is_err());
    assert_eq!(
        values(&e, centers),
        [
            Value::Point([2., 4. / 3., 0.]),
            Value::Point([1., 8. / 3., 0.])
        ]
    );
    assert_eq!(values(&e, ratios), vec![Value::Number(12. / 25.); 2]);
    assert_eq!(values(&e, normals), vec![Value::Point([0., 0., 1.]); 4]);
    let [Value::Curve(edge)] = values(&e, edges) else {
        panic!("one interior edge expected");
    };
    assert_eq!(edge.points, [[0., 0., 0.], [3., 4., 0.]]);
    for (value, indices) in values(&e, boundaries).iter().zip(&quad.triangles) {
        let Value::Curve(boundary) = value else {
            panic!("curve expected");
        };
        assert!(boundary.closed);
        assert!(boundary.geometry.is_some());
        assert_eq!(boundary.points, indices.map(|i| quad.vertices[i as usize]));
    }
    assert_eq!(values(&e, lengths).len(), 2);
    for (curve, length) in values(&e, shift).iter().zip(values(&e, lengths)) {
        let Value::Curve(circle) = curve else {
            panic!("curve expected");
        };
        assert!(circle.closed);
        assert!(circle.geometry.is_some());
        near(length.number().unwrap(), 5. * PI);
        for &p in &circle.points {
            near((p[0] - 1.5).hypot(p[1] - 2.), 2.5);
            near(p[2], 7.);
        }
    }
}

#[test]
fn simultaneous_mesh_edits_remap_vertices_and_preserve_branch_contract() {
    let mut graph = Graph::default();
    let mesh_id = quad(&mut graph);
    let both = add(
        &mut graph,
        K::Series,
        &[("start", 0.), ("step", 1.), ("count", 2.)],
    );
    let one = add(&mut graph, K::Number, &[("value", 1.)]);
    let non_boolean = add(&mut graph, K::Number, &[("value", 2.)]);
    let [
        delete_faces,
        cull_faces,
        delete_vertices,
        cull_vertices,
        invalid_cull,
    ] = [
        (K::MeshDeleteFaces, "indices", both),
        (K::MeshCullFaces, "pattern", both),
        (K::MeshDeleteVertices, "indices", one),
        (K::MeshCullVertices, "pattern", both),
        (K::MeshCullFaces, "pattern", non_boolean),
    ]
    .map(|(kind, key, selection)| {
        assert_eq!(for_input(kind, 0), Some(InputAccess::SingleItem));
        assert_eq!(for_input(kind, 1), Some(InputAccess::List));
        let id = connected(&mut graph, kind, mesh_id, "mesh");
        graph.connect(selection, id, key).unwrap();
        id
    });
    let array = add(&mut graph, K::LinearArray, &[("count", 2.), ("x", 15.)]);
    graph.connect(mesh_id, array, "geometry").unwrap();
    let invalid_delete = connected(&mut graph, K::MeshDeleteFaces, array, "mesh");
    graph.connect(both, invalid_delete, "indices").unwrap();
    let graft = connected(&mut graph, K::Graft, array, "geometry");
    let valid_delete = connected(&mut graph, K::MeshDeleteFaces, graft, "mesh");
    graph.connect(one, valid_delete, "indices").unwrap();

    let e = Evaluator::default().evaluate(&graph, &|| false);
    assert!(mesh(&values(&e, delete_faces)[0]).triangles.is_empty());
    assert_eq!(mesh(&values(&e, cull_faces)[0]).triangles, [[0, 1, 3]]);
    assert_eq!(
        mesh(&values(&e, delete_vertices)[0]).vertices,
        [[0., 0., 0.], [0., 4., 0.], [3., 4., 0.]]
    );
    assert_eq!(mesh(&values(&e, delete_vertices)[0]).triangles, [[0, 2, 1]]);
    assert_eq!(
        mesh(&values(&e, cull_vertices)[0]).vertices,
        [[0., 0., 0.], [0., 4., 0.]]
    );
    assert!(mesh(&values(&e, cull_vertices)[0]).triangles.is_empty());
    assert!(e.outputs[&invalid_cull].is_err());
    assert!(e.outputs[&invalid_delete].is_err());
    let branches = &e.outputs[&valid_delete].as_ref().unwrap().branches;
    assert_eq!(
        branches.keys().cloned().collect::<Vec<_>>(),
        [vec![0, 0], vec![0, 1]]
    );
    assert!(
        branches
            .values()
            .all(|list| mesh(&list[0]).triangles == [[0, 1, 3]])
    );
}

#[test]
fn split_disjoint_keeps_separate_indices_even_when_coordinates_match() {
    let mut graph = Graph::default();
    let source = quad(&mut graph);
    let array = add(&mut graph, K::LinearArray, &[("count", 2.), ("x", 0.)]);
    graph.connect(source, array, "geometry").unwrap();
    let joined = connected(&mut graph, K::Merge, array, "geometry");
    let pieces = connected(&mut graph, K::MeshDisjoint, joined, "mesh");
    let e = Evaluator::default().evaluate(&graph, &|| false);
    let original = mesh(&values(&e, source)[0]);
    assert_eq!(mesh(&values(&e, joined)[0]).triangles.len(), 4);
    let pieces = values(&e, pieces);
    assert_eq!(pieces.len(), 2);
    for piece in pieces {
        let piece = mesh(piece);
        assert_eq!(piece.vertices.len(), 4);
        let positions = |m: &Mesh| {
            m.triangles
                .iter()
                .map(|t| t.map(|i| m.vertices[i as usize]))
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(piece), positions(original));
    }
}

#[test]
fn analytic_surface_projection_respects_boundaries_seams_and_axes() {
    let plane =
        Surface::from_plane(PlaneSurface::new([0., 0., 0.], [4., 0., 0.], [2., 3., 0.]).unwrap());
    let p = plane.closest_point([4., 1.5, 9.]).unwrap();
    assert_eq!(p.uv, [0.75, 0.5]);
    assert_eq!(p.point, [4., 1.5, 0.]);
    let edge = plane.closest_point([5., 6., 1e8]).unwrap();
    assert_eq!(edge.uv, [0.75, 1.]);
    assert_eq!(edge.point, [5., 3., 0.]);

    let cylinder = Surface::frustum(2., 2., 4.).unwrap();
    let p = cylinder.closest_point([0., -5., 8.]).unwrap();
    assert_eq!(p.uv, [0.75, 1.]);
    point_near(p.point, [0., -2., 4.]);
    near(p.distance, 5.);
    assert_eq!(cylinder.closest_point([0., 0., 2.]).unwrap().uv, [0., 0.5]);

    let sphere = Surface::sphere(4.).unwrap();
    let p = sphere.closest_point([0., 0., 10.]).unwrap();
    assert_eq!(p.uv, [0., 1.]);
    assert_eq!(p.point, [0., 0., 4.]);
    assert_eq!(p.distance, 6.);
    assert_eq!(sphere.closest_point([0.; 3]).unwrap().point, [4., 0., 0.]);

    let torus = Surface::torus(7., 2.).unwrap();
    let p = torus.closest_point([0., 7., 5.]).unwrap();
    assert_eq!(p.uv, [0.25, 0.25]);
    point_near(p.point, [0., 7., 2.]);
    near(p.distance, 3.);
    point_near(torus.closest_point([0.; 3]).unwrap().point, [5., 0., 0.]);
    let nurbs = Surface::nurbs(NurbsSurface::panel(2., 2., 1.).unwrap());
    assert!(nurbs.closest_point([0.; 3]).is_err());
}

#[test]
fn cone_and_surface_output_adapters_work_with_the_graph() {
    let mut graph = Graph::default();
    let cone = add(
        &mut graph,
        K::ConeSurface,
        &[("radius", 3.), ("height", 4.)],
    );
    let query = point(&mut graph, [4.7, 0., 4.4]);
    let projection = [
        K::SurfaceClosestPoint,
        K::SurfaceClosestUv,
        K::SurfaceDistance,
    ]
    .map(|kind| {
        let id = connected(&mut graph, kind, cone, "surface");
        graph.connect(query, id, "point").unwrap();
        id
    });
    let frames = add(
        &mut graph,
        K::SurfaceFramesUv,
        &[("u_count", 2.), ("v_count", 1.)],
    );
    graph.connect(cone, frames, "surface").unwrap();
    let mesh_id = add(
        &mut graph,
        K::SurfaceMesh,
        &[("u_steps", 8.), ("v_steps", 4.)],
    );
    graph.connect(cone, mesh_id, "surface").unwrap();
    let a = point(&mut graph, [10., 4., -1.]);
    let b = point(&mut graph, [8., 7., -5.]);
    let box_id = connected(&mut graph, K::BrepBoxTwoPoints, a, "a");
    graph.connect(b, box_id, "b").unwrap();

    let e = Evaluator::default().evaluate(&graph, &|| false);
    let [Value::Point(p)] = values(&e, projection[0]) else {
        panic!("closest point expected");
    };
    point_near(*p, [1.5, 0., 2.]);
    let [Value::Point(p)] = values(&e, projection[1]) else {
        panic!("UV point expected");
    };
    point_near(*p, [0., 0.5, 0.]);
    near(values(&e, projection[2])[0].number().unwrap(), 4.);
    assert_eq!(
        values(&e, frames),
        [
            Value::Point([0., 0., 0.]),
            Value::Point([0.5, 0., 0.]),
            Value::Point([1., 0., 0.]),
            Value::Point([0., 1., 0.]),
            Value::Point([0.5, 1., 0.]),
            Value::Point([1., 1., 0.]),
        ]
    );
    let cone_mesh = mesh(&values(&e, mesh_id)[0]);
    assert_eq!(cone_mesh.vertices.len(), 33);
    assert_eq!(cone_mesh.triangles.len(), 56);
    assert_eq!(
        cone_mesh
            .vertices
            .iter()
            .filter(|p| **p == [0., 0., 4.])
            .count(),
        1
    );
    let report = audit(cone_mesh, &|| false).unwrap();
    assert_eq!(report.boundary_edges, 8);
    assert_eq!(report.degenerate_faces, 0);
    assert_eq!(report.orientation_conflicts, 0);

    let [Value::Solid(body)] = values(&e, box_id) else {
        panic!("solid expected");
    };
    assert_eq!(body.volume(), 24.);
    assert_eq!(body.brep().shell().faces().len(), 6);
}
