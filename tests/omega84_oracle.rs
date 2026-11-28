//! OMEGA 84: independent references, not Rhino/Grasshopper observations.
//! The fixed generator is independent of nodes::gh_nodes::Rng.
//! Run: cargo test --locked --test omega84_oracle -- --nocapture
use nodes::{
    curve_geometry::{CurveGeometry, NurbsCurve},
    data_tree::{self as dt, ItemMatches, MatchMode},
    engine::Evaluator,
    gh_nodes, math_nodes,
    model::{Graph, MAX_ITEMS, NodeId, NodeKind as K, Tree, Value},
    surface_kernel::{PlaneSurface, Surface},
};

const CASES: usize = 1024;
const SEED: u64 = 0x84_65_2026_1128_0001;

struct Generator(u64);
impl Generator {
    fn pick(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) % n
    }
    fn signed(&mut self) -> i64 {
        self.pick(2001) as i64 - 1000
    }
}

fn nums(xs: &[f64]) -> Vec<Value> {
    xs.iter().copied().map(Value::Number).collect()
}

fn actual_numbers(values: &[Value]) -> Vec<f64> {
    values.iter().map(|v| v.number().unwrap()).collect()
}

fn item(k: K, input: &[f64]) -> Result<Vec<Value>, String> {
    let values = nums(input);
    let refs: Vec<_> = values.iter().collect();
    math_nodes::run_item(k, &refs, &|| false)
}

fn node(g: &mut Graph, k: K, params: &[(&str, f64)]) -> NodeId {
    let id = g.add(k, [0., 0.]).unwrap();
    for &(key, value) in params {
        g.node_mut(id).unwrap().params.insert(key.into(), value);
    }
    id
}

fn tree_numbers(tree: &Tree, expected: &[f64], label: &str) {
    assert_eq!(tree.branches.len(), 1, "{label}: branch count");
    assert_eq!(
        actual_numbers(&tree.branches[&vec![0]]),
        expected,
        "{label}: ordered values or count"
    );
}

fn close(actual: f64, expected: f64, abs: f64, rel: f64, label: &str) {
    assert!(actual.is_finite() && expected.is_finite(), "{label}: nonfinite");
    let error = (actual - expected).abs();
    let allowed = abs.max(rel * expected.abs());
    assert!(
        error <= allowed,
        "{label}: actual={actual:e}, expected={expected:e}, error={error:e}, allowed={allowed:e}"
    );
}

// Paired coordinate metrics preserve parameterization and orientation;
// unlike a point-cloud distance, they cannot hide a reversed curve.
fn point_metrics(actual: &[[f64; 3]], expected: &[[f64; 3]], scale: f64, label: &str) {
    assert!(!actual.is_empty() && actual.len() == expected.len());
    assert!(scale.is_finite() && scale > 0.);
    let mut max = 0.0_f64;
    let mut sum_sq = 0.;
    for (a, e) in actual.iter().zip(expected) {
        assert!(a.iter().chain(e).all(|x| x.is_finite()), "{label}: nonfinite");
        let d = ((a[0] - e[0]) / scale)
            .hypot((a[1] - e[1]) / scale)
            .hypot((a[2] - e[2]) / scale);
        max = max.max(d);
        sum_sq += d * d;
    }
    let rms = (sum_sq / actual.len() as f64).sqrt();
    assert!(max <= 2e-11 && rms <= 2e-11, "{label}: normalized max={max:e}, rms={rms:e}");
}

// Discrete symmetric Hausdorff: suitable only for already sampled sets.
// It is NOT a certified continuous curve/surface distance.
fn sampled_hausdorff(a: &[[f64; 3]], b: &[[f64; 3]]) -> f64 {
    assert!(!a.is_empty() && !b.is_empty());
    assert!(a.iter().chain(b).flatten().all(|v| v.is_finite()));
    let directed = |from: &[[f64; 3]], to: &[[f64; 3]]| {
        from.iter()
            .map(|p| {
                to.iter()
                    .map(|q| (p[0] - q[0]).hypot(p[1] - q[1]).hypot(p[2] - q[2]))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0_f64, f64::max)
    };
    directed(a, b).max(directed(b, a))
}

#[test]
fn graph_arithmetic_and_series_exact_integer_oracles() {
    let mut rng = Generator(SEED);
    for case in 0..CASES {
        let a = rng.signed();
        let b = rng.signed();
        let divisor = (1_i64 << rng.pick(8)) * if case % 2 == 0 { 1 } else { -1 };
        let mut graph = Graph::default();
        let mut expected = Vec::new();
        for (kind, rhs, answer) in [
            (K::Add, b, (a + b) as f64),
            (K::Subtract, b, (a - b) as f64),
            (K::Multiply, b, (a * b) as f64),
            (K::Divide, divisor, a as f64 / divisor as f64),
        ] {
            let id = node(&mut graph, kind, &[("a", a as f64), ("b", rhs as f64)]);
            expected.push((kind, id, vec![answer]));
        }
        let count = 1 + rng.pick(16) as usize;
        let series = node(
            &mut graph,
            K::Series,
            &[("start", a as f64), ("step", b as f64), ("count", count as f64)],
        );
        expected.push((
            K::Series,
            series,
            (0..count).map(|i| (a + i as i64 * b) as f64).collect(),
        ));
        graph.validate().unwrap();
        let result = Evaluator::default().evaluate(&graph, &|| false);
        for (kind, id, values) in expected {
            let label = format!("seed={SEED:x} case={case} {kind:?}");
            tree_numbers(result.outputs[&id].as_ref().unwrap(), &values, &label);
        }
    }
}

fn remap(data: &[f64], source: [f64; 2], target: [f64; 2], clamp: bool) -> Vec<f64> {
    let data = nums(data);
    let controls = [
        target[0], target[1], 1., source[0], source[1], if clamp { 1. } else { 0. },
    ].map(|x| [Value::Number(x)]);
    let mut lists: Vec<&[Value]> = vec![&data];
    lists.extend(controls.iter().map(|v| v.as_slice()));
    actual_numbers(&gh_nodes::run_list(K::Remap, &lists, &|| false).unwrap())
}

#[test]
fn range_lerp_bounds_and_remap_exact_dyadic_oracles() {
    let mut rng = Generator(SEED ^ 1);
    for case in 0..CASES {
        let a = rng.signed();
        let b = rng.signed();
        let n = 1_i64 << rng.pick(5);
        let label = format!("seed={:x} case={case}", SEED ^ 1);
        // Exact integer numerator, binary denominator: no reference to NODES lerp.
        let expected: Vec<_> = (0..=n)
            .map(|i| (a * (n - i) + b * i) as f64 / n as f64)
            .collect();
        assert_eq!(
            actual_numbers(&item(K::GhRange, &[a as f64, b as f64, n as f64]).unwrap()),
            expected,
            "{label}: GhRange"
        );
        let k = rng.pick(97) as i64 - 32;
        let expected = (a * (32 - k) + b * k) as f64 / 32.;
        assert_eq!(
            actual_numbers(&item(K::Lerp, &[a as f64, b as f64, k as f64 / 32.]).unwrap()),
            [expected],
            "{label}: Lerp"
        );
        let c = rng.signed();
        let data = nums(&[a as f64, b as f64, c as f64, a as f64]);
        assert_eq!(
            actual_numbers(&gh_nodes::run_list(K::Bounds, &[&data], &|| false).unwrap()),
            [a.min(b).min(c) as f64, a.max(b).max(c) as f64],
            "{label}: Bounds"
        );
        // Positive and descending source intervals over a wide scale range.
        let step = 2.0_f64.powi([-60, -40, -10, 0, 20, 40][case % 6]);
        let k = rng.pick(17) as i64;
        let direction = if case % 2 == 0 { 1. } else { -1. };
        let source = [0., direction * 16. * step];
        let input = [source[0], direction * k as f64 * step, source[1]];
        let expected = (a * (16 - k) + b * k) as f64 / 16.;
        assert_eq!(
            remap(&input, source, [a as f64, b as f64], false),
            [a as f64, expected, b as f64],
            "{label}: Remap"
        );
    }
}

#[test]
fn remap_regression_nonzero_width_extreme_span_and_clamping() {
    let tiny = 2.0_f64.powi(-60);
    assert_eq!(remap(&[0., tiny, 2. * tiny], [0., 2. * tiny], [0., 10.], false), [0., 5., 10.]);
    let subnormal = f64::from_bits(1);
    assert_eq!(
        remap(&[0., subnormal, 2. * subnormal], [0., 2. * subnormal], [0., 10.], false),
        [0., 5., 10.]
    );
    let m = f64::MAX;
    assert_eq!(remap(&[-m, 0., m], [-m, m], [0., 10.], false), [0., 5., 10.]);
    assert_eq!(remap(&[0., 1., 2.], [0., 2.], [-m, m], false), [-m, 0., m]);
    assert_eq!(remap(&[-1., 0., 1., 2., 3.], [0., 2.], [0., 10.], true), [0., 0., 5., 10., 10.]);
    // Zero-width policy is NODES-specific, not a GH result.
    assert_eq!(remap(&[3.], [3., 3.], [0., 10.], false), [5.]);
}

#[test]
fn public_spec_list_matching_and_mixed_tree_structure() {
    let mut rng = Generator(SEED ^ 2);
    for case in 0..CASES {
        let na = rng.pick(9) as usize;
        let nb = rng.pick(9) as usize;
        let a: Vec<_> = (0..na).map(|_| Value::Number(rng.signed() as f64)).collect();
        let b: Vec<_> = (0..nb).map(|_| Value::Point([rng.signed() as f64, 2., 3.])).collect();
        // Independent references: ordered pairs, including full Value variants.
        let mut cross = Vec::new();
        for x in &a {
            for y in &b {
                cross.push(vec![x.clone(), y.clone()]);
            }
        }
        let shortest: Vec<_> = a.iter().zip(&b).map(|(x, y)| vec![x.clone(), y.clone()]).collect();
        let mut longest = Vec::new();
        if !a.is_empty() && !b.is_empty() {
            for i in 0..na.max(nb) {
                longest.push(vec![a[i.min(na - 1)].clone(), b[i.min(nb - 1)].clone()]);
            }
        }
        for (mode, expected) in [
            (MatchMode::Shortest, shortest),
            (MatchMode::Longest, longest),
            (MatchMode::Cross, cross),
        ] {
            let actual: Vec<Vec<Value>> = ItemMatches::new(&[&a, &b], mode, MAX_ITEMS)
                .unwrap()
                .map(|row| row.into_iter().cloned().collect())
                .collect();
            assert_eq!(actual, expected, "case={case} mode={mode:?}");
        }

        // Explicit branch paths and empty branch are meaningful, not just counts.
        // Empty branch policy is currently a NODES contract.
        let tree = Tree {
            branches: [(vec![4, 0], a.clone()), (vec![4, 1], vec![]), (vec![4, 2], b.clone())].into(),
        };
        let flattened = Tree {
            branches: [(vec![0], a.iter().chain(&b).cloned().collect())].into(),
        };
        assert_eq!(dt::flatten(&tree).unwrap(), flattened, "case={case} flatten");
        assert_eq!(dt::unflatten(&flattened, &tree).unwrap(), tree, "case={case} unflatten");
        let simplified = Tree {
            branches: [(vec![0], a.clone()), (vec![1], vec![]), (vec![2], b.clone())].into(),
        };
        assert_eq!(dt::simplify(&tree).unwrap(), simplified, "case={case} simplify");
        let swapped = Tree {
            branches: [(vec![0, 4], a), (vec![1, 4], vec![]), (vec![2, 4], b)].into(),
        };
        assert_eq!(dt::path_mapper(&tree, "{A;B}->{B;A}").unwrap(), swapped, "case={case} path mapper");
    }
}

#[test]
fn independent_bernstein_rational_and_plane_references() {
    let mut rng = Generator(SEED ^ 3);
    for case in 0..CASES {
        let scale = [1e-6, 1e-3, 1., 1e3, 1e6][case % 5];
        let t = rng.pick(257) as f64 / 256.;
        let s = 1. - t;
        let p: [[f64; 3]; 4] = std::array::from_fn(|_| {
            std::array::from_fn(|_| rng.signed() as f64 * scale / 1024.)
        });
        // Direct cubic Bernstein polynomial, not the kernel's de Boor evaluator.
        let coeff = [s * s * s, 3. * s * s * t, 3. * s * t * t, t * t * t];
        let expected: [f64; 3] = std::array::from_fn(|j| (0..4).map(|i| coeff[i] * p[i][j]).sum());
        let expected_derivative: [f64; 3] = std::array::from_fn(|j| {
            3. * (s * s * (p[1][j] - p[0][j])
                + 2. * s * t * (p[2][j] - p[1][j])
                + t * t * (p[3][j] - p[2][j]))
        });
        let bezier = NurbsCurve::bezier(p.to_vec()).unwrap();
        let label = format!("case={case} cubic Bezier t={t} scale={scale}");
        point_metrics(&[bezier.point(t).unwrap()], &[expected], scale, &label);
        point_metrics(&[bezier.derivatives(t).unwrap()[1]], &[expected_derivative], scale, &label);

        // Rational quadratic with nonunit weights; independent rational Bernstein.
        let w = [1., 2., 4.];
        let coeff = [s * s * w[0], 2. * s * t * w[1], t * t * w[2]];
        let denominator: f64 = coeff.iter().sum();
        let expected: [f64; 3] = std::array::from_fn(|j| {
            (0..3).map(|i| coeff[i] * p[i][j]).sum::<f64>() / denominator
        });
        let rational = NurbsCurve::new(
            2, p[..3].to_vec(), w.to_vec(), vec![0., 0., 0., 1., 1., 1.],
        ).unwrap();
        point_metrics(
            &[rational.point(t).unwrap()], &[expected], scale,
            &format!("case={case} rational NURBS"),
        );

        // A plane with a nonorthogonal basis. Oriented normal is +Z.
        let size = scale.max(0.01);
        let v = rng.pick(257) as f64 / 256.;
        let origin = [size, -size, size];
        let plane = Surface::from_plane(PlaneSurface::new(origin, [size, 0., 0.], [size, size, 0.]).unwrap());
        let sample = plane.evaluate(t, v).unwrap();
        let expected = [size * (1. + t + v), size * (-1. + v), size];
        point_metrics(&[sample.point], &[expected], size, &format!("case={case} plane"));
        point_metrics(&[sample.du, sample.dv], &[[size, 0., 0.], [size, size, 0.]], size, "plane tangents");
        point_metrics(&[sample.normal().unwrap()], &[[0., 0., 1.]], 1., "plane orientation");
    }
}

#[test]
fn sampled_locus_and_parameter_orientation_are_separate() {
    let curve = CurveGeometry::nurbs(NurbsCurve::bezier(vec![[0., 0., 0.], [2., 0., 0.]]).unwrap());
    let actual: Vec<_> = (0..=32).map(|i| curve.point(i as f64 / 32.).unwrap()).collect();
    let reference: Vec<_> = (0..=32).map(|i| [i as f64 / 16., 0., 0.]).collect();
    point_metrics(&actual, &reference, 2., "line ordered parameters");
    let reversed: Vec<_> = reference.iter().rev().copied().collect();
    close(sampled_hausdorff(&actual, &reversed), 0., 1e-12, 0., "line locus");
    assert_ne!(actual[0], reversed[0], "reversal must remain detectable");
}

#[test]
fn fixed_reference_data_and_invalid_numeric_inputs() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/omega84_golden.json")).unwrap();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["rhino_grasshopper_executed"], false);
    for case in fixture["numeric_cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let kind: K = serde_json::from_value(case["node"].clone()).unwrap();
        let input: Vec<_> = case["input"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let result = item(kind, &input);
        if case["expected"].is_null() {
            assert!(result.is_err(), "{id}: expected rejection, got {result:?}");
        } else {
            let actual = actual_numbers(&result.unwrap());
            let expected = case["expected"].as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{id}: output count");
            for (a, e) in actual.into_iter().zip(expected) {
                close(a, e.as_f64().unwrap(), case["abs_tol"].as_f64().unwrap(), case["rel_tol"].as_f64().unwrap(), id);
            }
        }
    }
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(item(K::SinRadians, &[v]).is_err());
        assert!(item(K::Lerp, &[0., 1., v]).is_err());
    }
}

#[test]
fn random_seed_repeatability_not_grasshopper_rng_equivalence() {
    for seed in [0., 1., 42., 65535.] {
        let values = nums(&[64., -2., 5., seed]);
        let refs: Vec<_> = values.iter().collect();
        let a = gh_nodes::run_item(K::Random, &refs, &|| false).unwrap();
        let b = gh_nodes::run_item(K::Random, &refs, &|| false).unwrap();
        assert_eq!(a, b, "NODES SplitMix64 repeatability");
        assert_eq!(a.len(), 64);
        assert!(actual_numbers(&a).iter().all(|&x| (-2. ..5.).contains(&x)));
    }
}
