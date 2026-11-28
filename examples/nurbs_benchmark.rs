//! Timings of the exact NURBS / B-rep / SubD operations (`cargo run --example nurbs_benchmark`).
//! Results are recorded by hand in docs/NURBS_PERFORMANCE.md with the machine and profile used.
use nodes::kernel_bridge as kb;
use nodes::kernel_ops as ko;
use nodes::subd::Cage;
use std::time::Instant;

fn time<T>(name: &str, runs: usize, f: impl Fn() -> Result<T, String>) {
    let mut best = f64::INFINITY;
    for _ in 0..runs.max(1) {
        let t = Instant::now();
        match f() {
            Ok(_) => best = best.min(t.elapsed().as_secs_f64() * 1000.),
            Err(e) => {
                println!("{name}: error {e}");
                return;
            }
        }
    }
    println!("{name}: {best:.1} ms");
}

fn main() {
    let go = || false;
    let go = &go;
    let Ok(cyl) = kb::cylinder([0., 0., -30.], [0., 0., 1.], 10., 60., go) else {
        return;
    };
    let Ok(cross) = kb::cylinder([-30., 0., 0.], [1., 0., 0.], 6., 60., go) else {
        return;
    };
    let Ok(boxed) = kb::box_solid(40., 30., 20., go) else {
        return;
    };
    time("box 40x30x20 (build + mesh)", 5, || {
        kb::box_solid(40., 30., 20., go)
    });
    time("fillet all 12 edges r=3", 3, || {
        kb::fillet(&boxed, 3., &[], go)
    });
    time("boolean union of crossing cylinders", 3, || {
        kb::boolean(&cyl, &cross, kb::Bool::Union, go)
    });
    time("exact intersection curves (cylinders)", 3, || {
        ko::exact_intersection(&cyl, &cross, go)
    });
    time("mesh-approximate intersection curves", 3, || {
        kb::brep_brep_mesh(&cyl, &cross, go)
    });
    time("shell of a cylinder cup t=2", 3, || {
        kb::shell(&cyl, 2., Some(usize::MAX), go)
    });
    let Ok(cage) = Cage::box_cage(40., 40., 40., go) else {
        return;
    };
    let Ok(cage2) = cage.subdivide(go).and_then(|c| c.subdivide(go)) else {
        return;
    };
    time(
        "SubD box cage, 2 subdivisions (96 quads) -> B-rep",
        1,
        || cage2.to_brep(true, go),
    );
    time("T-spline vase cage -> B-rep", 1, || {
        Cage::cylinder_cage(25., 80., 8, 6, 0.35, go).and_then(|c| c.to_brep(true, go))
    });
    time("constrained sketch (plate with hole) solve", 5, || {
        nodes::sketch_nodes::run(
            "長方形 r 0 0 50 30\n固定 r.p0\n長さ r.b 60\n長さ r.l 40\n円 c 30 20 5\n半径 c 8\n",
            0,
            go,
        )
    });
    time("polar array of the filleted box x12 (parallel)", 1, || {
        kb::fillet(&boxed, 3., &[], go).and_then(|f| ko::polar_array(&f, 12, 360., go))
    });
    time(
        "polar array x12, one copy at a time (serial reference)",
        1,
        || {
            kb::fillet(&boxed, 3., &[], go).and_then(|f| {
                (1..12)
                    .map(|i| {
                        let (sn, cs) = (std::f64::consts::TAU * i as f64 / 12.).sin_cos();
                        let m = [
                            [cs, sn, 0., 0.],
                            [-sn, cs, 0., 0.],
                            [0., 0., 1., 0.],
                            [0., 0., 0., 1.],
                        ];
                        ko::transformed_copies(&f, &[m], "serial", go)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        },
    );
    time("zebra on the filleted box", 3, || {
        kb::fillet(&boxed, 3., &[], go).and_then(|f| ko::zebra(&f, 12., 0, go))
    });
    println!(
        "threads available: {}",
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
}
