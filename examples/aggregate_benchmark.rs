//! Reproducible numerical and timing comparison; JSON on stdout.
//!
//! env PATH=/home/agent/.cargo/bin:$PATH CARGO_INCREMENTAL=0 \
//!   CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
//!   cargo run --locked --example aggregate_benchmark \
//!   > docs/development/OMEGA_AGGREGATE_PERFORMANCE.json
//!
//! No release build is required. This is a development-profile comparison,
//! not a claim about Windows, GUI responsiveness, startup, or release speed.
//! The legacy adapter reproduces the relevant a1ffa400 math_nodes branch;
//! it is not a separately built historical application or graph evaluator.

// Measure the actual private implementation without widening its public API.
#[path = "../src/number_aggregates.rs"]
mod exact_arithmetic;

use nodes::{
    math_nodes,
    model::{MAX_ITEMS, NodeKind, Value as ModelValue},
};
use serde_json::{Value, json};
use std::hint::black_box;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const LEGACY_REVISION: &str = "a1ffa40071c0c3508b890065bd12220c5abb80d9";
const WARMUP_BATCHES: usize = 3;
const MEASURED_BATCHES: usize = 21;
const ITEMS_PER_BATCH: usize = 65_536;

fn never_stop() -> bool {
    false
}

// The old algorithm's stable_sum and scaling operations, in their original
// order. The input scratch buffer is prepared outside the timed interval.
#[inline(never)]
fn legacy_kernel(data: &mut [f64], average: bool) -> Result<f64, String> {
    let len = data.len() as f64;
    let m = data.iter().fold(0.0_f64, |x, y| x.max(y.abs()));
    if m == 0.0 {
        return Ok(0.0);
    }
    for x in &mut *data {
        *x /= m;
    }
    let (mut sum, mut correction) = (0.0_f64, 0.0_f64);
    for &x in data.iter() {
        let next = sum + x;
        correction += if sum.abs() >= x.abs() {
            (sum - next) + x
        } else {
            (x - next) + sum
        };
        sum = next;
    }
    let sum = sum + correction;
    let result = if average { (sum / len) * m } else { sum * m };
    if result.is_finite() {
        Ok(result)
    } else {
        Err("計算結果が有限ではありません。".into())
    }
}

#[inline(never)]
fn exact_kernel(data: &[f64], average: bool, stop: &dyn Fn() -> bool) -> Result<f64, String> {
    exact_arithmetic::sum_divided(data, if average { data.len() } else { 1 }, stop)
}

// Historical Summation/Average branch including its input conversion and
// cancellation checks. The full historical run_list dispatch is not reproduced.
#[inline(never)]
fn legacy_node_adapter(
    kind: NodeKind,
    values: &[ModelValue],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<ModelValue>, String> {
    if stop() {
        return Err("計算をキャンセルしました。".into());
    }
    let mut data = Vec::with_capacity(values.len());
    for value in values {
        if stop() {
            return Err("計算をキャンセルしました。".into());
        }
        data.push(value.number()?);
    }
    if data.is_empty() {
        return Ok(if kind == NodeKind::Summation {
            vec![ModelValue::Number(0.0)]
        } else {
            vec![]
        });
    }
    Ok(vec![ModelValue::Number(legacy_kernel(
        &mut data,
        kind == NodeKind::Average,
    )?)])
}

struct Fixture {
    name: &'static str,
    description: &'static str,
    data: Vec<f64>,
    sum: Result<f64, &'static str>,
    mean: Result<f64, &'static str>,
    oracle: &'static str,
}

fn fixtures(n: usize) -> Vec<Fixture> {
    assert!(n >= 4 && n.is_power_of_two() && n <= MAX_ITEMS);
    let ordinary: Vec<_> = (0..n)
        .map(|i| ((i * 37 % 511) as i64 - 255) as f64 / 16.0)
        .collect();
    let integer_sum: i64 = (0..n).map(|i| (i * 37 % 511) as i64 - 255).sum();
    let tiny = f64::from_bits(1);
    let small = 2.0_f64.powi(-1000);
    let large = 2.0_f64.powi(1000);
    let blocks = n / 4;
    let sticky_base = 3.0 * blocks as f64;
    let repeated = |block: &[f64]| (0..n).map(|i| block[i % block.len()]).collect();
    vec![
        Fixture {
            name: "ordinary_dyadic",
            description: "Deterministic mixed signs: ((37*i mod 511)-255)/16.",
            data: ordinary,
            sum: Ok(integer_sum as f64 / 16.0),
            mean: Ok(integer_sum as f64 / (16 * n) as f64),
            oracle: "Exact i64 numerator; all divisors and sizes are powers of two, within exact binary64 range.",
        },
        Fixture {
            name: "ordinary_cancellation",
            description: "Repeated [1e16, 1, -1e16, 1].",
            data: repeated(&[1e16, 1.0, -1e16, 1.0]),
            sum: Ok((n / 2) as f64),
            mean: Ok(0.5),
            oracle: "Each exactly represented +1e16/-1e16 pair cancels; each block contributes exactly two.",
        },
        Fixture {
            name: "wide_exponent_cancellation",
            description: "Repeated [2^1000, 2^-1000, -2^1000, 2^-1000].",
            data: repeated(&[large, small, -large, small]),
            sum: Ok((n / 2) as f64 * small),
            mean: Ok(small * 0.5),
            oracle: "Large powers cancel exactly; the sum is (n/2)*2^-1000 and the mean is 2^-1001.",
        },
        Fixture {
            name: "maximum_finite",
            description: "All values are f64::MAX; sum overflows, mean is MAX.",
            data: vec![f64::MAX; n],
            sum: Err("correctly_rounded_overflow"),
            mean: Ok(f64::MAX),
            oracle: "For n>=2 the correctly rounded sum is infinite; the exact mean of identical finite values is that value.",
        },
        Fixture {
            name: "subnormal_tie",
            description: "Alternating minimum subnormal and twice that value.",
            data: repeated(&[tiny, f64::from_bits(2)]),
            sum: Ok(f64::from_bits((3 * n / 2) as u64)),
            mean: Ok(f64::from_bits(2)),
            oracle: "Integer multiples of 2^-1074 sum exactly; 1.5 units rounds to the even value two.",
        },
        Fixture {
            name: "sticky_rounding",
            description: "Repeated [1, 1, nextafter(1,+inf), minimum subnormal].",
            data: repeated(&[1.0, 1.0, f64::from_bits(1.0_f64.to_bits() + 1), tiny]),
            sum: Ok(f64::from_bits(sticky_base.to_bits() + 1)),
            mean: Ok(f64::from_bits(0.75_f64.to_bits() + 1)),
            oracle: "Each power-of-two count has an exact midpoint plus a positive subnormal tail; round upward by one ULP.",
        },
    ]
}

fn outcome(actual: Result<f64, String>, expected: &Result<f64, &str>) -> Value {
    match actual {
        Ok(number) => {
            let ordered_bits = |value: f64| {
                if value.is_sign_negative() {
                    !value.to_bits()
                } else {
                    value.to_bits() | (1_u64 << 63)
                }
            };
            json!({
                "status": "VALUE",
                "value": number,
                "bits_hex": format!("{:016x}", number.to_bits()),
                "correct": expected.as_ref().is_ok_and(|x| x.to_bits() == number.to_bits()),
                "absolute_error": expected.as_ref().ok().map(|x| (number - x).abs()),
                "ulp_distance_from_expected": expected.as_ref().ok().map(|x| ordered_bits(number).abs_diff(ordered_bits(*x))),
            })
        }
        Err(message) => json!({
            "status": "ERROR", "message": message, "correct": expected.is_err()
        }),
    }
}

fn single_number(output: Result<Vec<ModelValue>, String>) -> Result<f64, String> {
    let values = output?;
    if values.len() != 1 {
        return Err("benchmark expected exactly one aggregate value".into());
    }
    values[0].number()
}

fn statistics(mut samples: Vec<f64>, items: usize) -> Value {
    let recorded = samples.clone();
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];
    json!({
        "unit": "nanoseconds_per_call",
        "median": median,
        "minimum": samples[0],
        "p90": samples[(samples.len() * 9).div_ceil(10) - 1],
        "maximum": samples[samples.len() - 1],
        "median_ns_per_item": median / items as f64,
        "samples": recorded
    })
}

fn measure(fixture: &Fixture, average: bool) -> Value {
    let n = fixture.data.len();
    let iterations = (ITEMS_PER_BATCH / n).max(4);
    let kind = if average {
        NodeKind::Average
    } else {
        NodeKind::Summation
    };
    let expected = if average { &fixture.mean } else { &fixture.sum };
    let values: Vec<_> = fixture
        .data
        .iter()
        .copied()
        .map(ModelValue::Number)
        .collect();
    let callback: fn() -> bool = black_box(never_stop);
    let stop: &dyn Fn() -> bool = &callback;
    let correctness = json!({
        "legacy_kernel": outcome(legacy_kernel(&mut fixture.data.clone(), average), expected),
        "exact_kernel": outcome(exact_kernel(&fixture.data, average, stop), expected),
        "legacy_node_adapter": outcome(single_number(legacy_node_adapter(kind, &values, stop)), expected),
        "current_math_nodes_run_list": outcome(single_number(math_nodes::run_list(kind, &[&values], stop)), expected),
    });
    assert_eq!(correctness["exact_kernel"]["correct"], true);
    assert_eq!(correctness["current_math_nodes_run_list"]["correct"], true);

    let mut scratch = vec![fixture.data.clone(); iterations];
    let mut timings: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::new());
    for round in 0..WARMUP_BATCHES + MEASURED_BATCHES {
        // Alternating order reduces consistent first/last and warm-cache bias.
        let order = if round % 2 == 0 {
            [0, 1, 2, 3]
        } else {
            [3, 2, 1, 0]
        };
        for mode in order {
            // Clone/copy and fixture creation are excluded for both kernels.
            // Each kernel sees the same batch of distinct, equally warmed slices.
            if mode < 2 {
                for buffer in &mut scratch {
                    buffer.copy_from_slice(&fixture.data);
                }
            }
            let started = Instant::now();
            match mode {
                0 => {
                    for buffer in &mut scratch {
                        let _ = black_box(legacy_kernel(black_box(buffer), black_box(average)));
                    }
                }
                1 => {
                    for buffer in &scratch {
                        let _ =
                            black_box(exact_kernel(black_box(buffer), black_box(average), stop));
                    }
                }
                2 => {
                    for _ in 0..iterations {
                        let _ = black_box(legacy_node_adapter(
                            black_box(kind),
                            black_box(&values),
                            stop,
                        ));
                    }
                }
                3 => {
                    for _ in 0..iterations {
                        let _ = black_box(math_nodes::run_list(
                            black_box(kind),
                            &[black_box(&values)],
                            stop,
                        ));
                    }
                }
                _ => unreachable!(),
            }
            let elapsed = started.elapsed().as_secs_f64() * 1e9 / iterations as f64;
            if round >= WARMUP_BATCHES {
                timings[mode].push(elapsed);
            }
        }
    }
    let [legacy, exact, old_node, new_node] = timings.map(|samples| statistics(samples, n));
    let kernel_ratio = exact["median"].as_f64().unwrap() / legacy["median"].as_f64().unwrap();
    let node_ratio = new_node["median"].as_f64().unwrap() / old_node["median"].as_f64().unwrap();
    json!({
        "fixture": fixture.name,
        "description": fixture.description,
        "items": n,
        "operation": if average { "Average" } else { "Summation" },
        "oracle": fixture.oracle,
        "expected": match expected {
            Ok(value) => json!({"status": "VALUE", "value": value, "bits_hex": format!("{:016x}", value.to_bits())}),
            Err(reason) => json!({"status": "ERROR", "reason": reason})
        },
        "correctness": correctness,
        "calls_per_batch_per_mode": iterations,
        "kernel_timings": {"legacy_scaled_neumaier": legacy, "current_exact": exact, "median_ratio_exact_over_legacy": kernel_ratio},
        "node_entry_timings": {"historical_branch_adapter": old_node, "current_math_nodes_run_list": new_node, "median_ratio_current_over_adapter": node_ratio}
    })
}

fn command(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    let started = Instant::now();
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|s| s.split_once(':'))
                    .map(|(_, s)| s.trim().to_owned())
            })
        })
        .or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok());
    let mut measurements = Vec::new();
    for n in [32, 1024, MAX_ITEMS] {
        for fixture in fixtures(n) {
            for average in [false, true] {
                measurements.push(measure(&fixture, average));
            }
        }
    }
    let count_correct = |key: &str| {
        measurements
            .iter()
            .filter(|m| m["correctness"][key]["correct"] == true)
            .count()
    };
    let report = json!({
        "schema_version": 1,
        "status": "MEASURED",
        "timestamp_utc": command("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]),
        "unix_time_seconds": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        "platform": {"os": std::env::consts::OS, "architecture": std::env::consts::ARCH, "cpu": cpu, "available_parallelism": std::thread::available_parallelism().ok().map(usize::from), "compiler": command("rustc", &["-vV"])},
        "source": {"head_revision": command("git", &["rev-parse", "HEAD"]), "historical_revision": LEGACY_REVISION, "exact_source_git_blob": command("git", &["hash-object", "src/number_aggregates.rs"]), "math_nodes_git_blob": command("git", &["hash-object", "src/math_nodes.rs"]), "benchmark_git_blob": command("git", &["hash-object", "examples/aggregate_benchmark.rs"]), "working_tree_changes": command("git", &["status", "--porcelain"]), "comparison_note": "Both algorithms are compiled in the same example/profile; current node uses the real library. Legacy node is a faithful relevant-branch adapter, not an old full binary."},
        "profile": {"intended": "dev opt-level=1, debug=0, incremental=false; dependency opt-level=2", "debug_assertions_observed": cfg!(debug_assertions), "cargo_incremental": std::env::var("CARGO_INCREMENTAL").ok(), "cargo_profile_dev_debug": std::env::var("CARGO_PROFILE_DEV_DEBUG").ok(), "cargo_profile_test_debug": std::env::var("CARGO_PROFILE_TEST_DEBUG").ok()},
        "protocol": {"warmup_batches_per_case": WARMUP_BATCHES, "measured_batches_per_case": MEASURED_BATCHES, "target_items_per_batch": ITEMS_PER_BATCH, "timer": "std::time::Instant around a batch; divide duration by calls; report all samples and median/min/p90/max", "mode_order": "forward/reverse alternating", "callback": "opaque false callback for cancellation; checks already present in each implementation are included", "kernel_scope": "Actual arithmetic entry functions, including their existing validation/cancellation checks. Input/scratch allocation and copy excluded; legacy mutates scratch as its historical implementation did. Exact implementation included by path without source duplication.", "node_scope": "Current math_nodes::run_list includes dispatch, Value-to-f64 conversion, cancellation, internal allocations and scalar output; historical adapter reproduces input handling plus old arithmetic but not the full match dispatch. Fixture Value creation excluded. No graph engine/cache/UI timings.", "system_limit": MAX_ITEMS, "limit_note": "16384 is the real MAX_ITEMS, so the largest workload reaches the supported per-list limit.", "correctness_scope": "Selected deterministic and adversarial cases, not a representative failure rate for user models or a complete correctness proof. Correctness compares the complete binary64 encoding; some old mismatches are one ULP while wide-exponent cancellation loses the whole residual.", "limitations": "Single shared Linux virtual machine, no CPU pinning or isolation, turbo/frequency and concurrent system work uncontrolled. Same profile and data support local comparisons; absolute release/Windows speed and end-to-end graph latency are not established."},
        "reproduce": "env PATH=/home/agent/.cargo/bin:$PATH CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo run --locked --example aggregate_benchmark > docs/development/OMEGA_AGGREGATE_PERFORMANCE.json",
        "summary": {"cases": measurements.len(), "legacy_kernel_correct": count_correct("legacy_kernel"), "current_kernel_correct": count_correct("exact_kernel"), "legacy_node_adapter_correct": count_correct("legacy_node_adapter"), "current_math_node_correct": count_correct("current_math_nodes_run_list")},
        "elapsed_workload_seconds": started.elapsed().as_secs_f64(),
        "measurements": measurements
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
