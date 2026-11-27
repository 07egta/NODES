//! Local-support evaluation must retain endpoint and one-sided derivatives.
use morphlet::surface_kernel::NurbsSurface;
use std::cell::Cell;

fn point_near(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
    for (a, b) in actual.into_iter().zip(expected) {
        assert!(
            a.is_finite() && (a - b).abs() <= tolerance,
            "actual {actual:?}, expected {expected:?}, tolerance {tolerance:e}"
        );
    }
}

fn cubic_domains() -> (Vec<f64>, Vec<f64>) {
    (
        vec![-4., -4., -4., -4., -3., -2.5, 2., 3., 6., 6., 6., 6.],
        vec![10., 10., 10., 10., 10.25, 10.5, 11.5, 12., 12., 12., 12.],
    )
}

// Polynomial blossom coefficients: for cubic B-splines, t, t² and t³ have
// coefficients e1/3, e2/3 and e3 of the three normalized interior knots.
fn cubic_polynomial_coefficients(knots: &[f64]) -> Vec<[f64; 3]> {
    let count = knots.len() - 4;
    let (a, b) = (knots[3], knots[count]);
    (0..count)
        .map(|i| {
            let [x, y, z] = std::array::from_fn(|j| (knots[i + j + 1] - a) / (b - a));
            [(x + y + z) / 3., (x * y + x * z + y * z) / 3., x * y * z]
        })
        .collect()
}

fn polynomial_surface() -> NurbsSurface {
    let (ku, kv) = cubic_domains();
    let us = cubic_polynomial_coefficients(&ku);
    let vs = cubic_polynomial_coefficients(&kv);
    let points = us
        .iter()
        .flat_map(|u| {
            vs.iter().map(move |v| {
                [
                    2. * u[0] + 3. * v[0],
                    4. * u[1] - v[0],
                    5. * u[0] * v[0] + 2. * v[2],
                ]
            })
        })
        .collect();
    NurbsSurface::new(8, 7, 3, 3, ku, kv, points, vec![1.; 56]).unwrap()
}

#[test]
fn cubic_local_support_keeps_endpoint_and_simple_knot_jets() {
    // Independent polynomial image in normalized UV:
    // S=(2u+3v, 4u²-v, 5uv+2v³), with unrelated nonunit native domains.
    let surface = polynomial_surface();
    let us = [0., 0.1 - 1e-7, 0.1, 0.1 + 1e-7, 0.15, 0.6, 0.7, 1.];
    let vs = [0., 0.125, 0.25, 0.75, 1.];
    for u in us {
        for v in vs {
            let sample = surface.evaluate(u, v).unwrap();
            let jet = surface.derivatives2(u, v, &|| false).unwrap();
            let point = [2. * u + 3. * v, 4. * u * u - v, 5. * u * v + 2. * v * v * v];
            for first in [sample, jet.first] {
                point_near(first.point, point, 2e-12);
                point_near(first.du, [2., 8. * u, 5. * v], 2e-11);
                point_near(first.dv, [3., -1., 5. * u + 6. * v * v], 2e-11);
            }
            point_near(jet.duu, [0., 8., 0.], 2e-10);
            point_near(jet.duv, [0., 0., 5.], 2e-10);
            point_near(jet.dvv, [0., 0., 12. * v], 2e-10);
        }
    }
}

#[test]
fn rational_local_support_keeps_normalized_quotient_derivatives() {
    let (ku, kv) = cubic_domains();
    let us = cubic_polynomial_coefficients(&ku);
    let vs = cubic_polynomial_coefficients(&kv);
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for u in &us {
        for v in &vs {
            weights.push(1. + u[0]);
            points.push([2. * u[0] / (1. + u[0]), v[0], 0.]);
        }
    }
    let surface = NurbsSurface::new(8, 7, 3, 3, ku, kv, points, weights).unwrap();
    // Homogeneous coordinates reproduce (2u, (1+u)v, 0, 1+u).
    for u in [0., 0.1, 0.15, 0.6, 0.7, 1.] {
        for v in [0., 0.125, 0.25, 0.75, 1.] {
            let den = 1. + u;
            let jet = surface.derivatives2(u, v, &|| false).unwrap();
            for first in [surface.evaluate(u, v).unwrap(), jet.first] {
                point_near(first.point, [2. * u / den, v, 0.], 2e-12);
                point_near(first.du, [2. / (den * den), 0., 0.], 2e-11);
                point_near(first.dv, [0., 1., 0.], 2e-11);
            }
            point_near(jet.duu, [-4. / (den * den * den), 0., 0.], 2e-10);
            point_near(jet.duv, [0.; 3], 2e-10);
            point_near(jet.dvv, [0.; 3], 2e-10);
        }
    }
}

#[test]
fn repeated_cubic_knot_uses_the_right_hand_first_derivative() {
    // Two straight cubic Bezier pieces meet at u=.5. Their normalized speeds
    // are 2 on the left and 4 on the right; curvature is undefined at the join.
    let xs = [0., 1. / 3., 2. / 3., 1., 5. / 3., 7. / 3., 3.];
    let points = xs
        .iter()
        .flat_map(|&x| [[x, 0., 0.], [x, 1., 0.]])
        .collect();
    let surface = NurbsSurface::new(
        7,
        2,
        3,
        1,
        vec![0., 0., 0., 0., 0.5, 0.5, 0.5, 1., 1., 1., 1.],
        vec![0., 0., 1., 1.],
        points,
        vec![1.; 14],
    )
    .unwrap();
    for u in [0., 0.5 - 1e-7, 0.5, 0.5 + 1e-7, 1.] {
        let (x, dx) = if u < 0.5 {
            (2. * u, 2.)
        } else {
            (4. * u - 1., 4.)
        };
        let sample = surface.evaluate(u, 0.25).unwrap();
        point_near(sample.point, [x, 0.25, 0.], 2e-12);
        point_near(sample.du, [dx, 0., 0.], 2e-12);
        point_near(sample.dv, [0., 1., 0.], 2e-12);
    }
    assert!(surface.derivatives2(0.5, 0.25, &|| false).is_err());
}

fn uniform_knots(count: usize, degree: usize) -> Vec<f64> {
    let mut knots = vec![0.; degree + 1];
    knots.extend((1..count - degree).map(|i| i as f64 / (count - degree) as f64));
    knots.extend(std::iter::repeat_n(1., degree + 1));
    knots
}

#[test]
fn maximum_degree_keeps_all_nine_support_controls_per_direction() {
    let (nu, nv, degree) = (13, 11, 8);
    let ku = uniform_knots(nu, degree);
    let kv = uniform_knots(nv, degree);
    let greville = |knots: &[f64], count: usize| {
        (0..count)
            .map(|i| knots[i + 1..=i + degree].iter().sum::<f64>() / degree as f64)
            .collect::<Vec<_>>()
    };
    let us = greville(&ku, nu);
    let vs = greville(&kv, nv);
    let points = us
        .iter()
        .flat_map(|&u| vs.iter().map(move |&v| [u, v, u * v]))
        .collect();
    let surface =
        NurbsSurface::new(nu, nv, degree, degree, ku, kv, points, vec![1.; nu * nv]).unwrap();
    for u in [0., 0.2, 0.4, 0.7, 1.] {
        for v in [0., 1. / 3., 0.5, 2. / 3., 1.] {
            let jet = surface.derivatives2(u, v, &|| false).unwrap();
            for first in [surface.evaluate(u, v).unwrap(), jet.first] {
                point_near(first.point, [u, v, u * v], 2e-12);
                point_near(first.du, [1., 0., v], 2e-11);
                point_near(first.dv, [0., 1., u], 2e-11);
            }
            point_near(jet.duu, [0.; 3], 2e-10);
            point_near(jet.duv, [0., 0., 1.], 2e-10);
            point_near(jet.dvv, [0.; 3], 2e-10);
        }
    }
}

#[test]
fn local_support_preserves_cancellation_on_inactive_rows() {
    let surface = polynomial_surface();
    // u=0 has support only in the first four of eight rows. A stop request
    // arriving at a later existing checkpoint must still be observed.
    for limit in [1, 2, 5, 9] {
        let calls = Cell::new(0usize);
        let result = surface.derivatives2(0., 0.25, &|| {
            calls.set(calls.get() + 1);
            calls.get() >= limit
        });
        assert!(result.unwrap_err().contains("停止"));
        assert_eq!(calls.get(), limit);
    }
}
