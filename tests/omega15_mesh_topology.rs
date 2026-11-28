use nodes::engine::Evaluator;
use nodes::mesh_fabrication::{
    audit, boundary_loops, signed_volume, split_islands, topology_summary,
};
use nodes::mesh_tools::fill_holes;
use nodes::model::{MAX_VERTICES, Mesh, Value};
use nodes::persistence::Project;
use std::f64::consts::TAU;

fn go() -> bool {
    false
}

fn tetra() -> Mesh {
    Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
    }
}

fn torus(n: usize, k: usize) -> Mesh {
    let mut m = Mesh::default();
    for i in 0..n {
        let u = TAU * i as f64 / n as f64;
        for j in 0..k {
            let v = TAU * j as f64 / k as f64;
            m.vertices
                .push([(3. + v.cos()) * u.cos(), (3. + v.cos()) * u.sin(), v.sin()]);
        }
    }
    let idx = |i: usize, j: usize| ((i % n) * k + j % k) as u32;
    for i in 0..n {
        for j in 0..k {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            m.triangles.extend([[a, b, c], [a, c, d]]);
        }
    }
    m
}

#[test]
fn sphere_topology_and_torus_have_different_euler_characteristics() {
    let s = topology_summary(&tetra(), &go).unwrap();
    assert_eq!(
        (s.vertices, s.edges, s.faces, s.euler_surface),
        (4, 6, 4, 2)
    );
    assert_eq!((s.edge_components, s.boundary_edges), (1, 0));
    assert!(boundary_loops(&tetra(), &go).unwrap().is_empty());
    let t = torus(16, 12);
    let s = topology_summary(&t, &go).unwrap();
    assert_eq!((s.vertices, s.edges, s.faces), (192, 576, 384));
    assert_eq!(
        (s.euler_all, s.edge_components, s.boundary_edges),
        (0, 1, 0)
    );
    assert!(audit(&t, &go).unwrap().closed_oriented());
    assert!(boundary_loops(&t, &go).unwrap().is_empty());
}

#[test]
fn oriented_open_ring_and_isolated_vertex() {
    let mut m = tetra();
    m.triangles.remove(0);
    m.vertices.push([8., 8., 8.]);
    let s = topology_summary(&m, &go).unwrap();
    assert_eq!(
        (s.isolated_vertices, s.euler_all, s.euler_surface),
        (1, 2, 1)
    );
    assert_eq!(s.boundary_edges, 3);
    assert_eq!(boundary_loops(&m, &go).unwrap(), vec![vec![0, 1, 2]]);
    let cap = fill_holes(&m, 3, &go).unwrap();
    assert!(audit(&cap, &go).unwrap().closed_oriented());
    assert!((signed_volume(&cap, &go).unwrap() - 1. / 6.).abs() < 1e-12);
}

#[test]
fn separate_shells_preserve_winding_and_geometry() {
    let mut m = tetra();
    let other = tetra();
    m.vertices
        .extend(other.vertices.iter().map(|p| [p[0] + 5., p[1], p[2]]));
    m.triangles
        .extend(other.triangles.iter().map(|t| t.map(|v| v + 4)));
    let s = topology_summary(&m, &go).unwrap();
    assert_eq!((s.edge_components, s.euler_surface), (2, 4));
    let out = split_islands(&m, &go).unwrap();
    assert_eq!(out.len(), 2);
    for island in out {
        assert_eq!((island.vertices.len(), island.triangles.len()), (4, 4));
        assert!((signed_volume(&island, &go).unwrap() - 1. / 6.).abs() < 1e-12);
    }
}

#[test]
fn vertex_contact_is_not_edge_connectivity() {
    let m = Mesh {
        vertices: vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [-1., 0., 0.],
            [0., -1., 0.],
        ],
        triangles: vec![[0, 1, 2], [0, 3, 4]],
    };
    assert_eq!(topology_summary(&m, &go).unwrap().edge_components, 2);
    assert!(boundary_loops(&m, &go).is_err());
    let out = split_islands(&m, &go).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].triangles, vec![[0, 1, 2]]);
    assert_eq!(out[1].vertices[0], [0.; 3]);
}

#[test]
fn non_manifold_edge_and_bad_faces_are_not_silently_repaired() {
    let mut m = tetra();
    m.vertices.push([0., -1., 0.]);
    m.triangles.push([0, 1, 4]);
    assert_eq!(topology_summary(&m, &go).unwrap().edge_components, 1);
    assert!(boundary_loops(&m, &go).is_err());
    assert_eq!(split_islands(&m, &go).unwrap()[0].triangles.len(), 5);
    for bad in [[0, 2, 1], [0, 0, 1], [0, 1, 99]] {
        let mut m = tetra();
        m.triangles.push(bad);
        assert!(topology_summary(&m, &go).is_err());
        assert!(boundary_loops(&m, &go).is_err());
        assert!(split_islands(&m, &go).is_err());
    }
    let mut reversed = tetra();
    reversed.triangles[1].swap(1, 2);
    assert!(boundary_loops(&reversed, &go).is_err());
}

#[test]
fn nonplanar_boundary_is_not_fan_filled() {
    let m = Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.4], [0., 1., 0.]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    };
    assert_eq!(boundary_loops(&m, &go).unwrap()[0].len(), 4);
    assert!(fill_holes(&m, 4, &go).is_err());
}

#[test]
fn cancel_and_size_limit() {
    let m = tetra();
    assert!(topology_summary(&m, &|| true).is_err());
    assert!(boundary_loops(&m, &|| true).is_err());
    assert!(split_islands(&m, &|| true).is_err());
    let m = Mesh {
        vertices: vec![[0.; 3]; MAX_VERTICES + 1],
        triangles: Vec::new(),
    };
    assert!(topology_summary(&m, &go).is_err());
}

#[test]
fn editable_graph_roundtrip_and_expected_counts() {
    let project: Project = serde_json::from_str(include_str!(
        "../examples/mesh-fabrication/topology-laboratory.morph"
    ))
    .unwrap();
    project.validate().unwrap();
    let json = serde_json::to_string(&project).unwrap();
    let restored: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(project, restored);
    let evaluation = Evaluator::default().evaluate(&restored.graph, &go);
    let stats = evaluation.outputs[&2].as_ref().unwrap();
    let numbers: Vec<f64> = stats
        .branches
        .values()
        .flatten()
        .map(|v| v.number().unwrap())
        .collect();
    assert_eq!(numbers, vec![8., 18., 11., 0., 1., 3., 1., 1.]);
    let curves = evaluation.outputs[&3].as_ref().unwrap();
    let values: Vec<_> = curves.branches.values().flatten().collect();
    assert_eq!(values.len(), 1);
    assert!(matches!(values[0], Value::Curve(c) if c.closed && c.points.len() == 3));
    assert_eq!(evaluation.outputs[&4].as_ref().unwrap().item_count(), 1);
}
