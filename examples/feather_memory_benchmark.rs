//! Reproducible unflatten workload for before/after process-memory measurements.
use nodes::data_tree::unflatten;
use nodes::model::{Tree, Value};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let data = Tree {
        branches: (0..64)
            .map(|branch| {
                let items = (0..256)
                    .map(|i| Value::Number(f64::from(branch * 256 + i)))
                    .collect();
                (vec![branch], items)
            })
            .collect(),
    };
    let guide = Tree {
        branches: (0..128)
            .map(|branch| (vec![branch], vec![Value::Number(0.); 128]))
            .collect(),
    };
    let start = Instant::now();
    let mut checksum = 0.;
    for _ in 0..1000 {
        let result = unflatten(black_box(&data), black_box(&guide)).expect("valid trees");
        checksum += result.branches[&vec![127]][127]
            .number()
            .expect("number");
        black_box(result);
    }
    println!(
        "iterations=1000 items=16384 checksum={checksum} elapsed_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
}
