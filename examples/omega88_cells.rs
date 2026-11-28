//! Reproducible bounded-cell benchmark; run with cargo run --release --example omega88_cells.
use nodes::spatial_cells::{Bounds, Site, bounded_power_cells};
use std::time::Instant;

fn main() {
    let bounds = Bounds {
        min: [0.; 3],
        max: [100.; 3],
    };
    let mut state = 42u64;
    let mut random = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    for n in [100, 1000, 10_000] {
        let sites: Vec<_> = (0..n)
            .map(|_| Site {
                point: [100. * random(), 100. * random(), 100. * random()],
                weight: 0.,
            })
            .collect();
        let start = Instant::now();
        match bounded_power_cells(&sites, bounds, &|| false) {
            Ok(cells) => {
                let volume: f64 = cells.iter().map(|c| c.volume).sum();
                let triangles: usize = cells.iter().map(|c| c.mesh.triangles.len()).sum();
                println!(
                    "{n}: {:?}, cells={}, triangles={triangles}, relative volume error={:.3e}",
                    start.elapsed(),
                    cells.len(),
                    (volume - 1e6).abs() / 1e6
                );
            }
            Err(e) => println!("{n}: {:?}, rejected: {e}", start.elapsed()),
        }
    }
}
