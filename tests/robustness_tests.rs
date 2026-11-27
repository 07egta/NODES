//! Every node must survive hostile parameters and typical upstream data: evaluation never
//! panics, and every result is either an `Err` message or a tree of finite, in-range values.
use nodes::engine::Evaluator;
use nodes::model::{DataType, Graph, Mesh, NodeId, NodeKind, Tree, Value};
use std::time::Instant;

/// Small deterministic xorshift generator (no dependencies).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn finite3(p: &[f64; 3]) -> bool {
    p.iter().all(|v| v.is_finite())
}
fn check_mesh(m: &Mesh, what: &str) {
    assert!(m.vertices.iter().all(finite3), "{what}: non-finite vertex");
    let n = m.vertices.len() as u32;
    assert!(
        m.triangles.iter().all(|t| t.iter().all(|&i| i < n)),
        "{what}: triangle index out of range"
    );
}
fn check_tree(t: &Tree, what: &str) {
    for v in t.branches.values().flatten() {
        match v {
            Value::Number(x) => assert!(x.is_finite(), "{what}: non-finite number {x}"),
            Value::Point(p) => assert!(finite3(p), "{what}: non-finite point"),
            Value::Curve(c) => assert!(c.points.iter().all(finite3), "{what}: non-finite curve"),
            Value::Mesh(m) => check_mesh(m, what),
            Value::Surface(_) | Value::Solid(_) | Value::Gcode(_) => {}
            Value::Brep(b) => check_mesh(b.preview(), what),
            Value::Cage(c) => check_mesh(c.preview(), what),
        }
    }
}

static OK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ERR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn run(graph: &Graph, what: &str) {
    let start = Instant::now();
    let result = Evaluator::default().evaluate(graph, &|| false);
    assert!(!result.cancelled);
    for (id, out) in &result.outputs {
        if let Ok(t) = out {
            OK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            check_tree(t, &format!("{what} (node {id})"));
        } else {
            ERR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    assert!(
        start.elapsed().as_secs() < 60,
        "{what}: evaluation took {:?}",
        start.elapsed()
    );
}

fn node_with(kind: NodeKind, set: impl Fn(&str, f64, f64, f64) -> f64) -> Graph {
    let mut g = Graph::default();
    let id = g.add(kind, [0., 0.]).unwrap();
    let node = g.node_mut(id).unwrap();
    for s in kind.spec().inputs.iter().filter(|s| s.default.is_some()) {
        let v = set(s.key, s.default.unwrap_or(0.), s.min, s.max);
        node.params.insert(s.key.into(), v);
    }
    g
}

#[test]
fn defaults_and_range_extremes_never_panic() {
    for &kind in NodeKind::all() {
        let name = format!("{kind:?}");
        run(&node_with(kind, |_, d, _, _| d), &format!("{name} default"));
        run(&node_with(kind, |_, _, lo, _| lo), &format!("{name} min"));
        run(&node_with(kind, |_, _, _, hi| hi), &format!("{name} max"));
        run(
            &node_with(kind, |_, _, lo, hi| (lo + hi) / 2.),
            &format!("{name} mid"),
        );
    }
}

#[test]
fn hostile_numbers_never_panic() {
    let hostile = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.,
        -1e300,
        1e300,
        1e-300,
        0.,
        -0.,
        0.5,
        1e9,
        f64::MAX,
        f64::MIN_POSITIVE,
        4294967296.,
    ];
    for &kind in NodeKind::all() {
        for (i, &h) in hostile.iter().enumerate() {
            // Whole-node hostile value, and a single hostile slot with the rest at defaults.
            let g = node_with(kind, |_, _, _, _| h);
            run(&g, &format!("{kind:?} all={h}"));
            let slots = kind.spec().inputs.len();
            for slot in 0..slots {
                let mut g = node_with(kind, |_, d, _, _| d);
                let id = g.nodes[0].id;
                let key = kind.spec().inputs[slot].key;
                if kind.spec().inputs[slot].default.is_some() {
                    g.node_mut(id).unwrap().params.insert(key.into(), h);
                }
                run(&g, &format!("{kind:?} {key}={h} (case {i})"));
            }
        }
    }
}

/// Feeds `Number` upstream nodes with hostile values (bypassing nothing: the graph validator
/// rejects non-finite parameters, so extremes here are finite but huge).
#[test]
fn every_input_fed_by_typical_upstream_nodes() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let kinds = NodeKind::all();
    let mut checked = 0usize;
    for &kind in kinds {
        let specs = kind.spec().inputs;
        for spec in specs {
            let wanted: Vec<NodeKind> = kinds
                .iter()
                .copied()
                .filter(|k| {
                    *k != NodeKind::Cluster
                        && *k != NodeKind::Script
                        && *k != NodeKind::ImportMesh
                        && spec.kind.accepts(k.spec().output)
                })
                .collect();
            if wanted.is_empty() {
                continue;
            }
            // A few deterministic picks plus the first of each output type.
            let mut picks: Vec<NodeKind> =
                (0..4).map(|_| wanted[rng.below(wanted.len())]).collect();
            picks.push(wanted[0]);
            picks.push(wanted[wanted.len() - 1]);
            for up in picks {
                let mut g = Graph::default();
                let target = g.add(kind, [0., 0.]).unwrap();
                let source = g.add(up, [1., 1.]).unwrap();
                // Randomise the upstream parameters within and slightly beyond their range.
                for s in up.spec().inputs.iter().filter(|s| s.default.is_some()) {
                    let t = rng.below(5);
                    let v = match t {
                        0 => s.min,
                        1 => s.max,
                        2 => s.default.unwrap_or(0.),
                        3 => (s.min + s.max) / 2.,
                        _ => 0.,
                    };
                    g.node_mut(source).unwrap().params.insert(s.key.into(), v);
                }
                if g.connect(source, target, spec.key).is_err() {
                    continue;
                }
                // Also feed a second upstream into every other connectable input.
                for other in specs.iter().filter(|o| o.key != spec.key) {
                    let compat: Vec<NodeKind> = kinds
                        .iter()
                        .copied()
                        .filter(|k| {
                            matches!(k, NodeKind::Number | NodeKind::Circle | NodeKind::Box)
                                && other.kind.accepts(k.spec().output)
                        })
                        .collect();
                    if compat.is_empty() {
                        continue;
                    }
                    let k = compat[rng.below(compat.len())];
                    if let Ok(id) = g.add(k, [2., 2.]) {
                        let _: NodeId = id;
                        let _ = g.connect(id, target, other.key);
                    }
                }
                run(&g, &format!("{kind:?}.{} <- {up:?}", spec.key));
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} combinations were exercised");
    let _ = DataType::Number;
}

fn random_value(rng: &mut Rng, lo: f64, hi: f64, default: f64, integer: bool) -> f64 {
    let unit = (rng.next() >> 11) as f64 / (1u64 << 53) as f64;
    let v = match rng.below(7) {
        0 => lo,
        1 => hi,
        2 => default,
        3 => lo + (hi - lo) * 1e-9,
        4 => hi - (hi - lo) * 1e-9,
        5 if lo <= 0. && hi >= 0. => 0.,
        _ => lo + (hi - lo) * unit,
    };
    let v = if integer { v.round() } else { v };
    v.clamp(lo, hi)
}
fn randomise(g: &mut Graph, id: NodeId, rng: &mut Rng) {
    let kind = g.node(id).map(|n| n.kind).unwrap();
    for s in kind.spec().inputs.iter().filter(|s| s.default.is_some()) {
        let v = random_value(rng, s.min, s.max, s.default.unwrap_or(0.), s.integer);
        g.node_mut(id).unwrap().params.insert(s.key.into(), v);
    }
}

/// Random in-range parameter mixes (range checks reject out-of-range values early, so the
/// interesting failures are combinations of legal extremes), alone and behind a chain of
/// two random upstream nodes feeding each input.
#[test]
fn random_in_range_mixes_never_panic() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let kinds: Vec<NodeKind> = NodeKind::all()
        .iter()
        .copied()
        .filter(|k| {
            !matches!(
                k,
                NodeKind::Cluster | NodeKind::Script | NodeKind::ImportMesh
            )
        })
        .collect();
    for &kind in &kinds {
        for round in 0..60 {
            let mut g = Graph::default();
            let target = g.add(kind, [0., 0.]).unwrap();
            randomise(&mut g, target, &mut rng);
            if round >= 20 {
                for spec in kind.spec().inputs {
                    let wanted: Vec<NodeKind> = kinds
                        .iter()
                        .copied()
                        .filter(|k| spec.kind.accepts(k.spec().output))
                        .collect();
                    if wanted.is_empty() || rng.below(3) == 0 {
                        continue;
                    }
                    let up = wanted[rng.below(wanted.len())];
                    let a = g.add(up, [1., 1.]).unwrap();
                    randomise(&mut g, a, &mut rng);
                    // Second hop: feed one input of `a` from another random node.
                    if let Some(s2) = up
                        .spec()
                        .inputs
                        .get(rng.below(up.spec().inputs.len().max(1)))
                    {
                        let w2: Vec<NodeKind> = kinds
                            .iter()
                            .copied()
                            .filter(|k| s2.kind.accepts(k.spec().output))
                            .collect();
                        if !w2.is_empty() {
                            let b = g.add(w2[rng.below(w2.len())], [2., 2.]).unwrap();
                            randomise(&mut g, b, &mut rng);
                            let _ = g.connect(b, a, s2.key);
                        }
                    }
                    let _ = g.connect(a, target, spec.key);
                }
            }
            run(&g, &format!("{kind:?} random round {round}"));
        }
    }
    let (ok, err) = (
        OK.load(std::sync::atomic::Ordering::Relaxed),
        ERR.load(std::sync::atomic::Ordering::Relaxed),
    );
    println!("node results so far: {ok} ok, {err} err");
    assert!(
        ok > err / 4,
        "too few successful evaluations ({ok} ok / {err} err)"
    );
}

/// Hostile formula text and file paths must be reported as errors, never panic.
#[test]
fn expression_and_import_nodes_survive_hostile_text() {
    let texts = [
        "",
        "(",
        ")))",
        "1/0",
        "0/0",
        "0^-1",
        "(-8)^0.5",
        "1e308*1e308",
        "x",
        "sin(",
        "abs()",
        "9999999999999999999999",
        "((((((((((((((((((((((((((((((((((((1))))))))))))))))))))))))))))))))))))",
        "--------1",
        "a+b+c+d+e+f",
        "\u{0}\u{1}\u{2}",
        "日本語",
        "min(1,2,3,4,5,6,7,8,9)",
        "1 % 0",
        "2^1024",
        "-2^0.5",
    ];
    let long = "1+".repeat(2000) + "1";
    for kind in [NodeKind::Expression, NodeKind::ImportMesh] {
        for text in texts.iter().copied().chain([
            long.as_str(),
            "/nonexistent/x.stl",
            "../../etc/passwd",
            "con:",
            "\0",
        ]) {
            let mut g = Graph::default();
            let id = g.add(kind, [0., 0.]).unwrap();
            g.node_mut(id).unwrap().script = text.into();
            run(&g, &format!("{kind:?} script {text:?}"));
        }
    }
}

#[test]
fn shipped_examples_load_and_evaluate_finite() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("morph") {
            continue;
        }
        let project = nodes::persistence::load(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let result = Evaluator::default().evaluate(&project.graph, &|| false);
        for (id, out) in &result.outputs {
            let t = out
                .as_ref()
                .unwrap_or_else(|e| panic!("{path:?} node {id}: {e}"));
            check_tree(t, &format!("{path:?} node {id}"));
        }
        count += 1;
    }
    assert!(count >= 10);
}

/// Benchmark: `cargo test --release --test robustness_tests -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_examples_evaluation_time() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("morph"))
        .collect();
    files.sort();
    for path in files {
        let project = nodes::persistence::load(&path).unwrap();
        let cold = (0..5)
            .map(|_| {
                let t = Instant::now();
                let r = Evaluator::default().evaluate(&project.graph, &|| false);
                std::hint::black_box(&r);
                t.elapsed().as_secs_f64() * 1000.
            })
            .fold(f64::MAX, f64::min);
        let mut ev = Evaluator::default();
        ev.evaluate(&project.graph, &|| false);
        let t = Instant::now();
        let r = ev.evaluate(&project.graph, &|| false);
        let warm = t.elapsed().as_secs_f64() * 1000.;
        println!(
            "{:?}: nodes {} cold(min of 5) {cold:.2} ms, cached {warm:.3} ms (cached nodes {})",
            path.file_name().unwrap(),
            project.graph.nodes.len(),
            r.cached
        );
    }
    // Synthetic wide graph: many independent heavy nodes on one level.
    let mut g = Graph::default();
    for i in 0..32 {
        let id = g.add(NodeKind::Sphere, [i as f32, 0.]).unwrap();
        for sp in NodeKind::Sphere.spec().inputs {
            if sp.key == "segments" || sp.key == "layers" {
                g.node_mut(id)
                    .unwrap()
                    .params
                    .insert(sp.key.into(), sp.max.min(160.));
            }
        }
    }
    let cold = (0..5)
        .map(|_| {
            let t = Instant::now();
            let r = Evaluator::default().evaluate(&g, &|| false);
            std::hint::black_box(&r);
            t.elapsed().as_secs_f64() * 1000.
        })
        .fold(f64::MAX, f64::min);
    let sequential = (0..5)
        .map(|_| {
            let mut ev = Evaluator::default();
            ev.set_max_threads(1);
            let t = Instant::now();
            let r = ev.evaluate(&g, &|| false);
            std::hint::black_box(&r);
            t.elapsed().as_secs_f64() * 1000.
        })
        .fold(f64::MAX, f64::min);
    println!(
        "wide: 32 independent spheres cold(min of 5): parallel {cold:.2} ms, 1 thread {sequential:.2} ms"
    );
}
