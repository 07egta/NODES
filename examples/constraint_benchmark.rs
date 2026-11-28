//! Run: cargo run --release --locked --example constraint_benchmark
//! Measures parse and full solve separately, not GUI frame time.
use nodes::sketch_nodes::session::SketchSession;
use solidcraft_geom::Vec2;
use std::fmt::Write as _;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("points,parse_ms,solve_ms,dof,max_residual,iterations");
    for n in [10, 100, 1000] {
        let mut text = String::new();
        for i in 0..n {
            writeln!(text, "point p{i} 3 4\ndistance origin p{i} 10")?;
        }
        let start = Instant::now();
        let mut session = SketchSession::from_text(&text)?;
        let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let attempt = session.solve();
        let solve_ms = start.elapsed().as_secs_f64() * 1000.0;
        if attempt.dof() != Some(n) {
            return Err(format!("unexpected result for {n}: {:?}", attempt.report).into());
        }
        for p in session.sketch().points.iter().skip(1) {
            if (p.pos.dist(Vec2::new(0.0, 0.0)) - 10.0).abs() > 1e-6 {
                return Err(format!("distance oracle failed for {}", p.id).into());
            }
        }
        println!(
            "{n},{parse_ms:.3},{solve_ms:.3},{},{:.3e},{}",
            attempt.report.dof, attempt.report.max_residual, attempt.report.iterations
        );
    }
    Ok(())
}
