//! Bounded, opt-in geometry timings with independent numerical checks.
//!
//! NODES_SOURCE_SHA="$(git rev-parse HEAD)" \
//! NODES_RUSTC_VERSION="$(rustc --version)" \
//! cargo run --release --locked --example geometry_numeric_benchmark
//!
//! Prints one JSON document. Any operation error or wrong result sets FAILED
//! and exit code 1; timings for an incorrect result are never labeled MEASURED.
//! Fixtures, validation, printing and compilation are outside operation timers.
//! Timed batches include black_box and storage in a preallocated result buffer.
//! The fixed workload is headless; it does not measure GUI, startup or memory.
use morphlet::curve_geometry::{CurveGeometry, NurbsCurve};
use morphlet::surface_kernel::{NurbsSurface, PlaneSurface, Surface};
use serde_json::{Value, json};
use std::hint::black_box;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const SAMPLES: usize = 11;

#[derive(Clone, Copy, Debug)]
enum Observed {
    Scalar(f64),
    Point([f64; 3]),
    Flag(bool),
}

impl Observed {
    fn json(self) -> Value {
        match self {
            Self::Scalar(x) => json!(x),
            Self::Point(p) => json!(p),
            Self::Flag(b) => json!(b),
        }
    }

    fn error(self, expected: Self) -> Option<f64> {
        match (self, expected) {
            (Self::Scalar(a), Self::Scalar(b)) if a.is_finite() => Some((a - b).abs()),
            (Self::Point(a), Self::Point(b)) if a.iter().all(|v| v.is_finite()) => Some(
                a.into_iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0., f64::max),
            ),
            (Self::Flag(a), Self::Flag(b)) => Some(f64::from(a != b)),
            _ => None,
        }
    }
}

struct Fixture {
    name: &'static str,
    input: Value,
    oracle: &'static str,
    expected: Observed,
    absolute_tolerance: f64,
    batch_operations: usize,
    warmup_operations: usize,
    operation: Box<dyn Fn() -> Result<Observed, String>>,
}

fn measure(fixture: Fixture) -> Value {
    let mut failure: Option<String> = None;
    let mut observed = None;
    let mut maximum_error: f64 = 0.;
    let mut check = |result: &Result<Observed, String>| match result {
        Ok(value) => {
            observed = Some(*value);
            match value.error(fixture.expected) {
                Some(error) if error <= fixture.absolute_tolerance => {
                    maximum_error = maximum_error.max(error);
                }
                Some(error) => {
                    maximum_error = maximum_error.max(error);
                    failure.get_or_insert_with(|| {
                        format!(
                            "absolute error {error:e} exceeds {:e}",
                            fixture.absolute_tolerance
                        )
                    });
                }
                None => {
                    failure.get_or_insert_with(|| "nonfinite value or wrong output type".into());
                }
            }
        }
        Err(error) => {
            failure.get_or_insert_with(|| format!("operation error: {error}"));
        }
    };
    for _ in 0..fixture.warmup_operations {
        check(&black_box((fixture.operation)()));
    }
    let mut results = Vec::with_capacity(fixture.batch_operations);
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        results.clear();
        let started = Instant::now();
        for _ in 0..fixture.batch_operations {
            results.push(black_box((fixture.operation)()));
        }
        samples.push(started.elapsed().as_secs_f64() * 1e6 / fixture.batch_operations as f64);
        for result in &results {
            check(result);
        }
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / SAMPLES as f64;
    json!({
        "name": fixture.name,
        "status": if failure.is_none() { "MEASURED" } else { "INVALID_RESULT" },
        "input": fixture.input,
        "oracle": fixture.oracle,
        "expected": fixture.expected.json(),
        "observed": observed.map(Observed::json),
        "absolute_tolerance": fixture.absolute_tolerance,
        "maximum_absolute_error": maximum_error,
        "failure": failure,
        "warmup_operations": fixture.warmup_operations,
        "sample_count": SAMPLES,
        "operations_per_sample": fixture.batch_operations,
        "timed_operations": SAMPLES * fixture.batch_operations,
        "samples_us_per_operation": samples,
        "min_us": sorted[0],
        "p50_us": sorted[(SAMPLES * 50).div_ceil(100) - 1],
        "p95_us": sorted[(SAMPLES * 95).div_ceil(100) - 1],
        "max_us": sorted[SAMPLES - 1],
        "mean_us": mean,
    })
}

fn uniform_knots(count: usize, degree: usize) -> Vec<f64> {
    let mut knots = vec![0.; degree + 1];
    knots.extend((1..count - degree).map(|i| i as f64 / (count - degree) as f64));
    knots.extend(std::iter::repeat_n(1., degree + 1));
    knots
}

fn greville_points(knots: &[f64], count: usize, degree: usize) -> Vec<f64> {
    (0..count)
        .map(|i| knots[i + 1..=i + degree].iter().sum::<f64>() / degree as f64)
        .collect()
}

fn straight_curve(count: usize) -> Result<CurveGeometry, String> {
    let degree = 3;
    let knots = uniform_knots(count, degree);
    // Linear reproduction of the B-spline basis gives x(t)=t, y=z=0.
    // The expected parameter is analytic, not obtained by calling evaluate.
    let points = greville_points(&knots, count, degree)
        .into_iter()
        .map(|x| [x, 0., 0.])
        .collect();
    Ok(CurveGeometry::nurbs(NurbsCurve::new(
        degree,
        points,
        vec![1.; count],
        knots,
    )?))
}

fn rectangular_surface(count: usize) -> Result<Surface, String> {
    let degree = if count == 2 { 1 } else { 3 };
    let knots = uniform_knots(count, degree);
    let coords = greville_points(&knots, count, degree);
    let points = coords
        .iter()
        .flat_map(|&u| coords.iter().map(move |&v| [31. * u, 31. * v, 0.]))
        .collect();
    Ok(Surface::nurbs(NurbsSurface::new(
        count,
        count,
        degree,
        degree,
        knots.clone(),
        knots,
        points,
        vec![1.; count * count],
    )?))
}

fn fixtures() -> Result<Vec<Fixture>, String> {
    let mut fixtures = Vec::new();
    let short_span = Surface::nurbs(NurbsSurface::new(
        3,
        2,
        1,
        1,
        vec![0., 0., 0.001, 1., 1.],
        vec![0., 0., 1., 1.],
        vec![
            [0., 0., 0.],
            [0., 1., 0.],
            [100., 0., 0.],
            [100., 1., 0.],
            [101., 0., 0.],
            [101., 1., 0.],
        ],
        vec![1.; 6],
    )?);
    for (name, surface, input, expected, oracle, repetitions) in [
        (
            "surface_area_bilinear_4_controls",
            rectangular_surface(2)?,
            json!({"control_grid": [2, 2], "degree": [1, 1], "steps": 32}),
            961.,
            "31 by 31 planar rectangle; area=31*31",
            16,
        ),
        (
            "surface_area_short_knot_span_6_controls",
            short_span,
            json!({"control_grid": [3, 2], "degree": [1, 1], "u_knots": [0., 0., 0.001, 1., 1.], "steps": 32}),
            101.,
            "101 by 1 planar rectangle, independent of parameter speed",
            8,
        ),
        (
            "surface_area_rational_cylinder_27_controls",
            Surface::nurbs(NurbsSurface::vase(5., 5., 5., 7.)?),
            json!({"control_grid": [9, 3], "radius": 5., "height": 7., "steps": 32}),
            70. * std::f64::consts::PI,
            "exact rational cylinder without caps; area=2*pi*5*7",
            4,
        ),
        (
            "surface_area_planar_1024_controls",
            rectangular_surface(32)?,
            json!({"control_grid": [32, 32], "degree": [3, 3], "steps": 32}),
            961.,
            "Greville control coordinates reproduce a 31 by 31 planar rectangle",
            1,
        ),
    ] {
        fixtures.push(Fixture {
            name,
            input,
            oracle,
            expected: Observed::Scalar(expected),
            absolute_tolerance: expected * 1e-6,
            batch_operations: repetitions,
            warmup_operations: 1,
            operation: Box::new(move || {
                black_box(&surface)
                    .area(black_box(32), &|| false)
                    .map(Observed::Scalar)
            }),
        });
    }
    for (name, u, v, query, expected, tolerance) in [
        (
            "plane_closest_orthogonal",
            [4., 0., 0.],
            [0., 6., 0.],
            [2., 2., 7.],
            [2., 2., 0.],
            1e-12,
        ),
        (
            "plane_closest_skew_1e8",
            [1e8, 0., 0.],
            [1e8, 1., 0.],
            [1e8, 0.5, 7.],
            [1e8, 0.5, 0.],
            1e-8,
        ),
        (
            "plane_closest_rotated_skew",
            [100000003., 100000007., 100000011.],
            [100000004., 100000006., 100000011.5],
            [175000010.75, 125000011.25, 6.25],
            [100000003.5, 100000006.5, 100000011.25],
            6e-8,
        ),
    ] {
        let plane = PlaneSurface::new([0.; 3], u, v)?;
        fixtures.push(Fixture {
            name,
            input: json!({"origin": [0, 0, 0], "u": u, "v": v, "query": query}),
            oracle: "query is target plus a perpendicular normal displacement",
            expected: Observed::Point(expected),
            absolute_tolerance: tolerance,
            batch_operations: 4096,
            warmup_operations: 128,
            operation: Box::new(move || {
                black_box(&plane)
                    .closest(black_box(query))
                    .map(Observed::Point)
            }),
        });
    }
    for (name, count, query, repetitions) in [
        ("curve_closest_interior_4_controls", 4, 0.314159, 16),
        ("curve_closest_endpoint_4_controls", 4, 0.001, 16),
        ("curve_closest_1024_controls", 1024, 0.314159, 2),
    ] {
        let curve = straight_curve(count)?;
        fixtures.push(Fixture {
            name,
            input: json!({"controls": count, "degree": 3, "query": [query, 0.25, 0.]}),
            oracle: "Greville control coordinates give x(t)=t; orthogonal projection has t=query.x",
            expected: Observed::Scalar(query),
            absolute_tolerance: 3e-8,
            batch_operations: repetitions,
            warmup_operations: 2,
            operation: Box::new(move || {
                black_box(&curve)
                    .closest_parameter(black_box([query, 0.25, 0.]), &|| false)
                    .map(Observed::Scalar)
            }),
        });
    }
    for (name, count) in [
        ("curve_closed_4_controls", 4),
        ("curve_closed_64_controls", 64),
        ("curve_closed_1024_controls", 1024),
        ("curve_closed_8192_controls", 8192),
    ] {
        let curve = straight_curve(count)?;
        fixtures.push(Fixture {
            name,
            input: json!({"controls": count, "degree": 3}),
            oracle: "straight open curve with endpoints x=0 and x=1",
            expected: Observed::Flag(false),
            absolute_tolerance: 0.,
            batch_operations: 128,
            warmup_operations: 32,
            operation: Box::new(move || black_box(&curve).closed().map(Observed::Flag)),
        });
    }
    let curve = CurveGeometry::nurbs(NurbsCurve::new(
        1,
        vec![[0., 0., 0.], [1., 0., 0.], [2., 0., 0.]],
        vec![1.; 3],
        vec![0., 0., 1e-15, 1., 1.],
    )?);
    fixtures.push(Fixture {
        name: "curve_closed_short_knot_span",
        input: json!({"controls": 3, "degree": 1, "knots": [0., 0., 1e-15, 1., 1.]}),
        oracle: "open line segment [0,2]; knot speed does not change endpoint separation",
        expected: Observed::Flag(false),
        absolute_tolerance: 0.,
        batch_operations: 128,
        warmup_operations: 32,
        operation: Box::new(move || black_box(&curve).closed().map(Observed::Flag)),
    });
    Ok(fixtures)
}

fn supplied(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn cpu_model() -> Option<String> {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim() == "model name").then(|| value.trim().to_owned())
            })
        })
        .or_else(|| supplied("PROCESSOR_IDENTIFIER"))
}

fn main() {
    let started = Instant::now();
    let built = fixtures();
    let fixture_ms = started.elapsed().as_secs_f64() * 1e3;
    let (results, fixture_error): (Vec<Value>, _) = match built {
        Ok(fixtures) => (fixtures.into_iter().map(measure).collect(), None),
        Err(error) => (vec![], Some(error)),
    };
    let passed =
        fixture_error.is_none() && results.iter().all(|result| result["status"] == "MEASURED");
    let report = json!({
        "schema_version": 1,
        "status": if passed { "MEASURED" } else { "FAILED" },
        "label": supplied("NODES_BENCH_LABEL"),
        "unix_timestamp_seconds": SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()),
        "source_revision": supplied("NODES_SOURCE_SHA"),
        "environment": {
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "available_parallelism": std::thread::available_parallelism().ok().map(usize::from),
            "cpu_affinity_if_supplied": supplied("NODES_BENCH_AFFINITY"),
            "cpu_model": cpu_model(),
            "compiler_version_if_supplied": supplied("NODES_RUSTC_VERSION"),
            "build_flags_if_supplied": supplied("NODES_BUILD_FLAGS"),
            "build_scope_if_supplied": supplied("NODES_BENCH_BUILD_SCOPE"),
            "debug_assertions": cfg!(debug_assertions),
        },
        "method": {
            "sample_count_per_fixture": SAMPLES,
            "clock": "std::time::Instant wall time",
            "distribution": "11 sequential batch means; nearest-rank p50 and p95, not independent process runs",
            "timed_scope": "kernel call, black_box and preallocated Result storage; validation is outside timer",
            "excluded_scope": "fixture construction, result checks, printing, compilation, GUI, startup and memory",
            "comparison_warning": "incorrect outputs invalidate performance claims; this is not an application benchmark",
        },
        "fixture_ms": fixture_ms,
        "fixture_error": fixture_error,
        "total_workload_ms": started.elapsed().as_secs_f64() * 1e3,
        "results": results,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("JSON report")
    );
    if !passed {
        std::process::exit(1);
    }
}
