//! OMEGA 95: diagnostics of the classical uniform Catmull-Clark interior one-ring.
//! Research only. No modified subdivision rules, T-junctions, eigen-polyhedron
//! adjustments, runtime code, or claim of characteristic-map regularity.
//! Independent derivation from Catmull & Clark (1978), see OMEGA95_CONSTELLATION.md.

use std::f64::consts::TAU;

// Ring ordering: center, n adjacent edge vertices, n opposite quad vertices.
// Face i contains center, edge i, opposite i, edge (i+1)%n.
fn step(p: &[f64], n: usize) -> Vec<f64> {
    assert!(n >= 3 && p.len() == 2 * n + 1);
    let c = p[0];
    let e = &p[1..=n];
    let d = &p[n + 1..];
    let faces: Vec<_> = (0..n)
        .map(|i| (c + e[i] + d[i] + e[(i + 1) % n]) / 4.0)
        .collect();
    let nf = n as f64;
    let f_avg = faces.iter().sum::<f64>() / nf;
    let e_avg = e.iter().sum::<f64>() / nf;
    let mut next = Vec::with_capacity(p.len());
    next.push((f_avg + e_avg + (nf - 2.0) * c) / nf);
    next.extend((0..n).map(|i| (c + e[i] + faces[(i + n - 1) % n] + faces[i]) / 4.0));
    next.extend(faces);
    next
}

fn fourier(n: usize, k: usize) -> (f64, f64) {
    let c = (TAU * k as f64 / n as f64).cos();
    let root = ((9.0 + c) * (1.0 + c)).max(0.0).sqrt();
    ((5.0 + c + root) / 16.0, (5.0 + c - root) / 16.0)
}

fn radial(n: usize) -> (f64, f64) {
    let nf = n as f64;
    let sum = (12.0 * nf - 28.0) / (16.0 * nf);
    let product = (nf - 3.0) / (16.0 * nf);
    let root = (sum * sum - 4.0 * product).max(0.0).sqrt();
    ((sum + root) / 2.0, (sum - root) / 2.0)
}

// Largest absolute local eigenvalue after constant and two tangential modes.
fn remainder(n: usize) -> f64 {
    let (a, b) = radial(n);
    let mut mu = a.abs().max(b.abs());
    for k in 1..n {
        let (hi, lo) = fourier(n, k);
        mu = mu.max(lo.abs());
        if k != 1 && k != n - 1 {
            mu = mu.max(hi.abs());
        }
    }
    mu
}

fn mode(n: usize, k: usize, sine: bool) -> Vec<f64> {
    let theta = TAU * k as f64 / n as f64;
    let lambda = fourier(n, k).0;
    let factor = if (lambda - 0.25).abs() < 1e-12 {
        // Nyquist mode: the edge/diagonal coupling vanishes.
        0.0
    } else {
        (theta / 2.0).cos() / (2.0 * (lambda - 0.25))
    };
    let trig = |x: f64| if sine { x.sin() } else { x.cos() };
    let mut p = vec![0.0];
    p.extend((0..n).map(|i| trig(theta * i as f64)));
    p.extend((0..n).map(|i| factor * trig(theta * (i as f64 + 0.5))));
    p
}

fn limit(p: &[f64], n: usize) -> f64 {
    let nf = n as f64;
    (nf * p[0] + (4.0 * p[1..=n].iter().sum::<f64>() + p[n + 1..].iter().sum::<f64>()) / nf)
        / (nf + 5.0)
}

#[test]
fn classical_ring_partition_positivity_and_affine_invariance() {
    for n in 3..=32 {
        let size = 2 * n + 1;
        // Assemble columns by applying the mask to each coordinate unit vector.
        let mut sums = vec![0.0; size];
        for j in 0..size {
            let mut unit = vec![0.0; size];
            unit[j] = 1.0;
            for (sum, weight) in sums.iter_mut().zip(step(&unit, n)) {
                assert!(weight >= 0.0 && weight.is_finite());
                *sum += weight;
            }
        }
        assert!(sums.iter().all(|s| (s - 1.0).abs() < 1e-12));
        let p: Vec<_> = (0..size).map(|i| (i as f64 * 0.73).sin()).collect();
        let transformed: Vec<_> = p.iter().map(|x| -2.0 * x + 7.0).collect();
        for (a, b) in step(&p, n).iter().zip(step(&transformed, n)) {
            assert!((b - (-2.0 * a + 7.0)).abs() < 1e-12);
        }
    }
}

#[test]
fn classical_ring_limit_and_fourier_modes() {
    for n in 3..=32 {
        let mut p: Vec<_> = (0..=2 * n).map(|i| (i as f64 * 0.31).cos()).collect();
        let target = limit(&p, n);
        for _ in 0..80 {
            p = step(&p, n);
            assert!((limit(&p, n) - target).abs() < 1e-12);
        }
        assert!(p.iter().all(|v| (v - target).abs() < 1e-10));
        for k in 1..=n / 2 {
            let lambda = fourier(n, k).0;
            for sine in [false, true] {
                let p = mode(n, k, sine);
                for (value, refined) in p.iter().zip(step(&p, n)) {
                    assert!((refined - lambda * value).abs() < 1e-11);
                }
            }
        }
        let lambda = fourier(n, 1).0;
        assert!(lambda > 0.0 && lambda < 1.0);
        assert!(remainder(n) < lambda);
    }
}

#[test]
fn higher_order_mode_scaling_is_not_a_g2_certificate() {
    let cases = [
        (3, 0.410097050801, 0.166666666667),
        (4, 0.5, 0.25),
        (5, 0.549988354518, 0.340107388174),
        (6, 0.579682326102, 0.410097050801),
        (8, 0.611116526695, 0.5),
        (12, 0.634796412354, 0.579682326102),
    ];
    for (n, expected_lambda, expected_mu) in cases {
        let lambda = fourier(n, 1).0;
        let mu = remainder(n);
        assert!((lambda - expected_lambda).abs() < 1e-11);
        assert!((mu - expected_mu).abs() < 1e-11);
        let ratio = mu / (lambda * lambda);
        match n {
            3 => assert!(ratio < 1.0),
            4 => assert!((ratio - 1.0).abs() < 1e-12),
            _ => {
                assert!(ratio > 1.0);
                assert!(mu / lambda < 1.0);
            }
        }
    }
}
