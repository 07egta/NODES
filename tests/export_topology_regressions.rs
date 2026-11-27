//! Closed edges alone do not establish a closed manifold surface.
use nodes::mesh_fabrication::{audit, signed_volume};
use nodes::model::{Graph, MAX_VERTICES, Mesh, NodeKind, Tree, Value};
use nodes::persistence::{self, Project};
use nodes::stl_export::{Level, check_bodies};
use std::fs;
use std::process::Command;
use std::sync::Arc;

fn tetrahedron() -> Mesh {
    Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    }
}

fn opposite_tetrahedron(offset: [f64; 3]) -> Mesh {
    let mut mesh = tetrahedron();
    for vertex in &mut mesh.vertices {
        for axis in 0..3 {
            vertex[axis] = offset[axis] - vertex[axis];
        }
    }
    for face in &mut mesh.triangles {
        face.swap(1, 2);
    }
    mesh
}

fn bowtie() -> Mesh {
    let mut mesh = tetrahedron();
    mesh.vertices
        .extend([[0., 0., -1.], [-1., 0., 0.], [0., -1., 0.]]);
    mesh.triangles
        .extend([[0, 4, 5], [0, 6, 4], [4, 6, 5], [5, 6, 0]]);
    mesh
}

fn trees(meshes: &[Mesh]) -> Vec<Tree> {
    meshes
        .iter()
        .cloned()
        .map(|mesh| Tree::single(Value::Mesh(Arc::new(mesh))))
        .collect()
}

#[test]
fn a_shared_vertex_with_two_face_fans_is_not_a_closed_manifold() {
    let mesh = bowtie();
    assert_eq!(audit(&mesh, &|| false).unwrap().non_manifold_vertices, 1);
    assert!(signed_volume(&mesh, &|| false).is_err());
    let report = persistence::mesh_stats(&trees(&[mesh])).unwrap();
    assert_eq!(report.non_manifold_vertices, 1);
    assert_eq!(
        report.open_edges + report.non_manifold_edges + report.inconsistent_edges,
        0
    );
    assert!(
        !report.is_closed(),
        "separate face fans must not be labelled a closed manifold"
    );
}

#[test]
fn global_quantized_welding_finds_contacts_across_separate_meshes() {
    for offset in [[0.; 3], [1e-8, 0., 0.]] {
        let bodies = [tetrahedron(), opposite_tetrahedron(offset)];
        for body in &bodies {
            assert!(audit(body, &|| false).unwrap().closed_oriented());
        }
        let report = persistence::mesh_stats(&trees(&bodies)).unwrap();
        assert_eq!(report.non_manifold_vertices, 1);
        assert!(
            !report.is_closed(),
            "quantized contact at {offset:?}: {report:?}"
        );
    }
}

#[test]
fn opposite_duplicate_faces_do_not_form_a_closed_manifold() {
    let mesh = Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        triangles: vec![[0, 1, 2], [2, 1, 0]],
    };
    assert_eq!(audit(&mesh, &|| false).unwrap().duplicate_faces, 1);
    let report = persistence::mesh_stats(&trees(&[mesh])).unwrap();
    assert_eq!(report.duplicate_faces, 1);
    assert_eq!(report.open_edges + report.inconsistent_edges, 0);
    assert!(!report.is_closed());
}

#[test]
fn print_checks_report_nonmanifold_vertices_and_duplicate_faces() {
    let (checks, _) = check_bodies(&[bowtie()], 256.).unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c.level == Level::Error && c.detail.contains("頂点"))
    );
    let double_face = Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        triangles: vec![[0, 1, 2], [2, 1, 0]],
    };
    let (checks, _) = check_bodies(&[double_face], 256.).unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c.level == Level::Error && c.detail.contains("重複"))
    );
}

#[test]
fn disjoint_closed_shells_and_a_triangle_soup_keep_their_existing_contract() {
    let disjoint = [tetrahedron(), opposite_tetrahedron([-2.; 3])];
    assert!(
        persistence::mesh_stats(&trees(&disjoint))
            .unwrap()
            .is_closed()
    );
    let indexed = tetrahedron();
    let mut soup = Mesh::default();
    for face in indexed.triangles {
        let base = soup.vertices.len() as u32;
        soup.vertices
            .extend(face.map(|i| indexed.vertices[i as usize]));
        soup.triangles.push([base, base + 1, base + 2]);
    }
    assert!(
        persistence::mesh_stats(&trees(&[soup]))
            .unwrap()
            .is_closed()
    );
}

#[test]
fn export_topology_keeps_its_larger_aggregate_vertex_budget() {
    let mut mesh = tetrahedron();
    // Isolated vertices do not change the tetrahedron's surface. Export permits
    // 2m aggregate vertices even though an individual engine mesh permits 250k.
    mesh.vertices
        .extend((4..=MAX_VERTICES).map(|i| [i as f64 + 10., 0., 0.]));
    assert!(audit(&mesh, &|| false).is_err());
    let report = persistence::mesh_stats(&trees(&[mesh])).unwrap();
    assert_eq!(report.vertices, MAX_VERTICES + 1);
    assert!(report.is_closed());
}

#[test]
fn cli_reports_a_vertex_contact_without_claiming_printability() {
    let temp =
        std::env::temp_dir().join(format!("nodes-cli-vertex-contact-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let project_path = temp.join("touching.morph");
    let output_path = temp.join("touching.stl");
    let mut graph = Graph::default();
    let shape = graph.add(NodeKind::Box, [0., 0.]).unwrap();
    for key in ["width", "depth", "height"] {
        graph.node_mut(shape).unwrap().params.insert(key.into(), 2.);
    }
    let moved = graph.add(NodeKind::Move, [200., 0.]).unwrap();
    graph.connect(shape, moved, "geometry").unwrap();
    for key in ["x", "y", "z"] {
        graph.node_mut(moved).unwrap().params.insert(key.into(), 2.);
    }
    for (index, source) in [shape, moved].into_iter().enumerate() {
        let output = graph
            .add(NodeKind::Output, [400., index as f32 * 100.])
            .unwrap();
        graph.connect(source, output, "geometry").unwrap();
    }
    persistence::save(&project_path, &Project::new("vertex contact", graph)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nodes-cli"))
        .arg("export-stl")
        .args([&project_path, &output_path])
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(temp);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = &json["result"];
    assert_eq!(result["non_manifold_vertices"], 1);
    assert_eq!(result["duplicate_faces"], 0);
    assert_eq!(result["topologically_closed"], false);
    assert_eq!(result["printability_verified"], false);
}
