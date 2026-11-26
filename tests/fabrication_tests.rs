use morphlet::engine::Evaluator;
use morphlet::import::{parse_obj, parse_stl};
use morphlet::mesh_fabrication::{audit, signed_volume, subdivide};
use morphlet::model::{Graph, Mesh, NodeKind, Tree, Value};
use morphlet::persistence::{self, Project};
use morphlet::stl_export::encode_3mf;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
fn no() -> bool {
    false
}
const SAMPLES: [&str; 5] = [
    include_str!("../examples/mesh-fabrication/rounded-box.morph"),
    include_str!("../examples/mesh-fabrication/twisted-vase-check.morph"),
    include_str!("../examples/mesh-fabrication/multi-part-check.morph"),
    include_str!("../examples/mesh-fabrication/wave-panel-check.morph"),
    include_str!("../examples/mesh-fabrication/hollow-vessel-check.morph"),
];
fn evaluate(project: &Project) -> Vec<Tree> {
    project.validate().unwrap();
    let evaluation = Evaluator::default().evaluate(&project.graph, &no);
    for (id, result) in &evaluation.outputs {
        assert!(
            result.is_ok(),
            "{} node {id}: {:?}",
            project.name,
            result.as_ref().err()
        );
    }
    project
        .graph
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Output)
        .map(|n| evaluation.outputs[&n.id].as_ref().unwrap().clone())
        .collect()
}
fn meshes(trees: &[Tree]) -> Vec<Arc<Mesh>> {
    trees
        .iter()
        .flat_map(|t| t.branches.values().flatten())
        .filter_map(|v| {
            if let Value::Mesh(m) = v {
                Some(m.clone())
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn samples_evaluate_and_serialize() {
    for s in SAMPLES {
        let mut p: Project = serde_json::from_str(s).unwrap();
        let serialized = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Project>(&serialized).unwrap(), p);
        let trees = evaluate(&p);
        let outputs = meshes(&trees);
        assert!(!outputs.is_empty());
        for m in &outputs {
            assert!(audit(m, &no).unwrap().closed_oriented(), "{}", p.name);
        }
        // Change a geometry dimension with an explicit numeric input.
        // These projects deliberately use bounded, editable dimensions.
        if let Some(n) = p
            .graph
            .nodes
            .iter_mut()
            .find(|n| n.kind == NodeKind::Box || n.kind == NodeKind::Vase)
        {
            let key = if n.kind == NodeKind::Box {
                "height"
            } else {
                "radius"
            };
            *n.params.get_mut(key).unwrap() *= 1.1;
        } else if let Some(n) = p.graph.nodes.iter_mut().find(|n| n.kind == NodeKind::Shell) {
            *n.params.get_mut("thickness").unwrap() *= 0.95;
        }
        for m in meshes(&evaluate(&p)) {
            assert!(audit(&m, &no).unwrap().closed_oriented(), "{}", p.name);
        }
    }
}
#[test]
fn old_examples_still_deserialize() {
    for s in [
        include_str!("../examples/01.morph"),
        include_str!("../examples/02.morph"),
        include_str!("../examples/03.morph"),
        include_str!("../examples/04.morph"),
        include_str!("../examples/05.morph"),
        include_str!("../examples/06.morph"),
        include_str!("../examples/07.morph"),
        include_str!("../examples/08.morph"),
        include_str!("../examples/09.morph"),
        include_str!("../examples/10.morph"),
    ] {
        let p: Project = serde_json::from_str(s).unwrap();
        p.validate().unwrap();
    }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nodes-fabrication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn rounded_box_stl_obj_roundtrip() {
    let p: Project = serde_json::from_str(SAMPLES[0]).unwrap();
    let trees = evaluate(&p);
    let m = meshes(&trees).remove(0);
    assert_eq!(m.triangles.len(), 288);
    let expected = signed_volume(&m, &no).unwrap();
    assert!(expected > 0.);
    let dir = Temp::new();
    let stl = dir.0.join("rounded.stl");
    let obj = dir.0.join("rounded.obj");
    persistence::export_stl(&stl, &trees).unwrap();
    persistence::export_obj(&obj, &trees).unwrap();
    for imported in [
        parse_stl(&std::fs::read(stl).unwrap(), &no).unwrap(),
        parse_obj(&std::fs::read(obj).unwrap(), &no).unwrap(),
    ] {
        assert_eq!(imported.triangles.len(), m.triangles.len());
        assert!(audit(&imported, &no).unwrap().closed_oriented());
        let actual = signed_volume(&imported, &no).unwrap();
        assert!((actual - expected).abs() / expected < 1e-5);
    }
}
#[test]
fn threemf_preserves_separate_input_objects() {
    let p: Project = serde_json::from_str(SAMPLES[0]).unwrap();
    let m = meshes(&evaluate(&p)).remove(0);
    let mut moved = (*m).clone();
    for v in &mut moved.vertices {
        v[0] += 100.;
    }
    let bytes = encode_3mf(&[(*m).clone(), moved]).unwrap();
    // The existing writer uses uncompressed ZIP entries. The existing
    // stl_export_tests separately verify the local headers and CRC.
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("<object id=\"1\" type=\"model\">"));
    assert!(text.contains("<object id=\"2\" type=\"model\">"));
    assert!(text.contains("<item objectid=\"1\"/>"));
    assert!(text.contains("<item objectid=\"2\"/>"));
    let mut open = (*m).clone();
    open.triangles.pop();
    assert!(encode_3mf(&[open]).is_err());
    let mut reverse = (*m).clone();
    for t in &mut reverse.triangles {
        t.swap(1, 2);
    }
    assert!(encode_3mf(&[reverse]).is_err());
}
#[test]
fn graph_rejects_open_volume() {
    let mut g = Graph::default();
    let p = g.add(NodeKind::Plane, [0., 0.]).unwrap();
    let v = g.add(NodeKind::MeshVolume, [200., 0.]).unwrap();
    g.connect(p, v, "mesh").unwrap();
    let e = Evaluator::default().evaluate(&g, &no);
    assert!(e.outputs[&p].is_ok());
    assert!(e.outputs[&v].is_err());
}
#[test]
fn construct_from_lists_and_tree_matching() {
    let mut g = Graph::default();
    let mesh = g.add(NodeKind::Box, [0., 0.]).unwrap();
    let vertices = g.add(NodeKind::MeshVertices, [200., 0.]).unwrap();
    let faces = g.add(NodeKind::MeshFaces, [200., 200.]).unwrap();
    let build = g.add(NodeKind::MeshConstruct, [450., 0.]).unwrap();
    g.connect(mesh, vertices, "mesh").unwrap();
    g.connect(mesh, faces, "mesh").unwrap();
    g.connect(vertices, build, "vertices").unwrap();
    g.connect(faces, build, "faces").unwrap();
    let e = Evaluator::default().evaluate(&g, &no);
    assert_eq!(e.outputs[&mesh], e.outputs[&build]);
}
fn torus(u: usize, v: usize) -> Mesh {
    use std::f64::consts::TAU;
    let mut m = Mesh::default();
    for i in 0..u {
        for j in 0..v {
            let a = i as f64 * TAU / u as f64;
            let b = j as f64 * TAU / v as f64;
            let r = 50. + 10. * b.cos();
            m.vertices.push([r * a.cos(), r * a.sin(), 10. * b.sin()]);
        }
    }
    let id = |i: usize, j: usize| ((i % u) * v + j % v) as u32;
    for i in 0..u {
        for j in 0..v {
            let (a, b, c, d) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
            m.triangles.extend([[a, b, c], [a, c, d]]);
        }
    }
    m
}
#[test]
fn oversized_input_fails_and_subdivision_budget_is_bounded() {
    let m = Mesh {
        vertices: vec![[0.; 3]],
        triangles: vec![[0; 3]; 1_000_000],
    };
    assert!(audit(&m, &no).is_err());
    let m = torus(128, 128);
    assert!(subdivide(&m, 3, &no).is_err());
}
#[test]
#[ignore = "Opt-in 100k and 500k triangle stress benchmark; run in release mode"]
fn audit_stress() {
    for (u, v) in [(200, 250), (500, 500)] {
        let m = torus(u, v);
        let now = std::time::Instant::now();
        let r = audit(&m, &no).unwrap();
        assert!(r.closed_oriented());
        eprintln!(
            "{} triangles, audit {:?}, indexed arrays {} bytes (not peak memory)",
            m.triangles.len(),
            now.elapsed(),
            m.vertices.len() * 24 + m.triangles.len() * 12
        );
    }
}
