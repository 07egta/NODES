//! Independent line/arc expectations for endpoint projection and geometric closure.
use morphlet::curve_geometry::{CurveGeometry, NurbsCurve};
use std::{cell::Cell, f64::consts::FRAC_PI_2};

fn line() -> CurveGeometry {
    CurveGeometry::nurbs(NurbsCurve::polyline(vec![[0.; 3], [1., 0., 0.]], false).unwrap())
}

fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}; tolerance {tolerance}"
    );
}

#[test]
fn closest_line_projects_near_both_ends() {
    let curve = line();
    for t in [0.000001, 0.001, 0.002, 0.998, 0.999, 0.999999] {
        // Orthogonal projection on the x axis has the query's x coordinate.
        let actual = curve.closest_parameter([t, 0., 0.], &|| false).unwrap();
        near(actual, t, 1e-8);
    }
}

#[test]
fn closest_obeys_trimmed_and_reversed_domains() {
    let curve = line().subcurve(0.2, 0.8).unwrap().reversed();
    for (query_x, expected_t) in [(0.7994, 0.001), (0.2006, 0.999)] {
        let actual = curve
            .closest_parameter([query_x, 0., 0.], &|| false)
            .unwrap();
        near(actual, expected_t, 1e-8);
        near(curve.point(actual).unwrap()[0], query_x, 1e-8);
    }
}

#[test]
fn closest_rational_line_respects_nonuniform_parameter_speed() {
    let curve = CurveGeometry::nurbs(
        NurbsCurve::new(
            1,
            vec![[0.; 3], [1., 0., 0.]],
            vec![1., 4.],
            vec![0., 0., 1., 1.],
        )
        .unwrap(),
    );
    for x in [0.001, 0.004, 0.9995] {
        // Rational linear curve: x(t)=4t/(1+3t), hence t=x/(4-3x).
        let expected = x / (4. - 3. * x);
        let actual = curve.closest_parameter([x, 0., 0.], &|| false).unwrap();
        near(actual, expected, 1e-8);
    }
}

#[test]
fn closest_arc_preserves_near_endpoint_minima() {
    let curve = CurveGeometry::arc([0.; 3], [2., 0., 0.], [0., 2., 0.], 0., FRAC_PI_2).unwrap();
    for angle in [0.001_f64, FRAC_PI_2 - 0.001] {
        let query = [2. * angle.cos(), 2. * angle.sin(), 0.];
        let parameter = curve.closest_parameter(query, &|| false).unwrap();
        let point = curve.point(parameter).unwrap();
        for axis in 0..3 {
            near(point[axis], query[axis], 5e-8);
        }
    }
}

#[test]
fn closest_outside_curve_and_invalid_queries() {
    let curve = line();
    for (x, expected) in [(-1., 0.), (0., 0.), (1., 1.), (2., 1.)] {
        assert_eq!(
            curve.closest_parameter([x, 0., 0.], &|| false).unwrap(),
            expected
        );
    }
    assert!(
        curve
            .closest_parameter([f64::NAN, 0., 0.], &|| false)
            .is_err()
    );
    assert!(
        curve
            .closest_parameter([f64::INFINITY, 0., 0.], &|| false)
            .is_err()
    );
    assert!(curve.closest_parameter([0.001, 0., 0.], &|| true).is_err());
}

#[test]
fn closest_refinement_can_be_cancelled() {
    let curve = line();
    let calls = Cell::new(0);
    let stop = || {
        let count = calls.get() + 1;
        calls.set(count);
        // The 129 initial sample points complete; cancellation then reaches
        // the newly required endpoint refinement instead of returning early.
        count > 129
    };
    assert!(curve.closest_parameter([0.001, 0., 0.], &stop).is_err());
}

fn nonuniform_open_curve(first_span: f64) -> CurveGeometry {
    CurveGeometry::nurbs(
        NurbsCurve::new(
            1,
            vec![[0.; 3], [1., 0., 0.], [2., 0., 0.]],
            vec![1.; 3],
            vec![0., 0., first_span, 1., 1.],
        )
        .unwrap(),
    )
}

#[test]
fn closure_uses_geometry_instead_of_extreme_knot_speed() {
    for first_span in [0.5, 1e-15, 1e-100] {
        let curve = nonuniform_open_curve(first_span);
        assert!(!curve.closed().unwrap(), "first span {first_span}");
        assert!(
            !curve.reversed().closed().unwrap(),
            "reversed span {first_span}"
        );
    }
}

#[test]
fn open_curve_preview_keeps_final_endpoint() {
    let curve = nonuniform_open_curve(1e-15);
    let preview = curve.preview(16, &|| false).unwrap();
    assert_eq!(preview.first(), Some(&[0.; 3]));
    assert_eq!(preview.last(), Some(&[2., 0., 0.]));
}

#[test]
fn closure_supports_periodic_curves_and_small_open_trims() {
    let curve =
        CurveGeometry::helix([0.; 3], [1., 0., 0.], [0., 1., 0.], [0.; 3], 100., 0.).unwrap();
    assert!(curve.closed().unwrap());
    assert!(curve.subcurve(0.01, 0.02).unwrap().closed().unwrap());
    assert!(!line().subcurve(0.5, 0.5 + 1e-14).unwrap().closed().unwrap());
    let periodic = CurveGeometry::nurbs(
        NurbsCurve::uniform(
            vec![[0.; 3], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
            3,
            true,
        )
        .unwrap(),
    );
    assert!(periodic.closed().unwrap());
}
