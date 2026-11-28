//! Run with cargo run --locked --release --example mesh_topology_benchmark.
//! Reports wall times; no historical speedup claim.
use nodes::mesh_fabrication::{boundary_loops, topology_summary};
use nodes::model::Mesh;
use std::time::Instant;

fn main() {
    for side in [64usize, 256, 500] {
        let mut mesh = Mesh::default();
        for y in 0..side {
            for x in 0..side {
                mesh.vertices.push([x as f64, y as f64, 0.]);
            }
        }
        for y in 0..side - 1 {
            for x in 0..side - 1 {
                let a = (y * side + x) as u32;
                let b = a + 1;
                let d = a + side as u32;
                let c = d + 1;
                mesh.triangles.extend([[a, b, c], [a, c, d]]);
            }
        }
        let start = Instant::now();
        let stats = topology_summary(&mesh, &|| false).unwrap();
        let summary_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let rings = boundary_loops(&mesh, &|| false).unwrap();
        let rings_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(stats.euler_surface, 1);
        assert_eq!(stats.edge_components, 1);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 4 * (side - 1));
        println!(
            "vertices={},faces={},summary_ms={:.3},loops_ms={:.3}",
            stats.vertices, stats.faces, summary_ms, rings_ms
        );
    }
}
