//! Independent numerical regressions for exact planar geometry and NURBS area.
use nodes::surface_kernel::{NurbsSurface, PlaneSurface, Surface};
use std::cell::Cell;
use std::f64::consts::PI;

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17e}, expected {expected:.17e}, tolerance {tolerance:.3e}"
    );
}

fn point_near(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
    for (a, b) in actual.into_iter().zip(expected) {
        near(a, b, tolerance);
    }
}

fn rectangular_nurbs(xs: &[f64], ys: &[f64], ku: Vec<f64>, kv: Vec<f64>) -> Surface {
    let points = xs
        .iter()
        .flat_map(|&x| ys.iter().map(move |&y| [x, y, 0.]))
        .collect();
    Surface::nurbs(
        NurbsSurface::new(
            xs.len(),
            ys.len(),
            1,
            1,
            ku,
            kv,
            points,
            vec![1.; xs.len() * ys.len()],
        )
        .unwrap(),
    )
}

#[test]
fn area_does_not_skip_a_short_knot_span() {
    // The image is the 101-by-1 rectangle, independently of the speed of U.
    // Its first 100 units occupy only 0.001 of the parameter domain.
    let surface = rectangular_nurbs(
        &[0., 100., 101.],
        &[0., 1.],
        vec![0., 0., 0.001, 1., 1.],
        vec![0., 0., 1., 1.],
    );
    for steps in [4, 16, 128] {
        near(surface.area(steps, &|| false).unwrap(), 101., 1e-9);
    }
}

#[test]
fn area_respects_nonunit_domains_and_short_spans_in_both_directions() {
    // A 101-by-51 planar rectangle with unrelated U/V native domains.
    let surface = rectangular_nurbs(
        &[0., 100., 101.],
        &[0., 1., 51.],
        vec![2., 2., 2.000001, 1002., 1002.],
        vec![-30., -30., 9.99999, 10., 10.],
    );
    near(surface.area(32, &|| false).unwrap(), 101. * 51., 1e-6);
}

#[test]
fn area_of_a_rational_cylinder_agrees_with_its_analytic_surface() {
    // Rational quarter-circle pieces, extruded in Z: exact geometry is a
    // radius-5, height-7 cylinder without caps, hence area 2*pi*5*7.
    let surface = Surface::nurbs(NurbsSurface::vase(5., 5., 5., 7.).unwrap());
    let expected = 70. * PI;
    for steps in [4, 16, 64] {
        near(
            surface.area(steps, &|| false).unwrap(),
            expected,
            expected * 1e-6,
        );
    }
}

#[test]
fn area_retains_cancellation_and_input_limits() {
    let surface = Surface::nurbs(NurbsSurface::panel(4., 5., 3.).unwrap());
    for steps in [0, 3, 129, usize::MAX] {
        assert!(surface.area(steps, &|| false).is_err());
    }
    assert!(surface.area(32, &|| true).unwrap_err().contains("停止"));
    let calls = Cell::new(0usize);
    let result = surface.area(128, &|| {
        calls.set(calls.get() + 1);
        calls.get() >= 12
    });
    assert!(result.unwrap_err().contains("停止"));
    assert_eq!(calls.get(), 12);
}

#[test]
fn unresolved_rational_area_must_not_return_a_tiny_false_success() {
    // Every U still covers [0,1], but the extreme permitted weights concentrate
    // almost all area in a very thin parameter layer near U=0. The known image
    // is the unit square. An explicit bounded-convergence error is acceptable;
    // a finite-looking answer many orders below one is not.
    let surface = Surface::nurbs(
        NurbsSurface::new(
            2,
            2,
            1,
            1,
            vec![0., 0., 1., 1.],
            vec![0., 0., 1., 1.],
            vec![[0., 0., 0.], [0., 1., 0.], [1., 0., 0.], [1., 1., 0.]],
            vec![1e-6, 1e-6, 1e6, 1e6],
        )
        .unwrap(),
    );
    match surface.area(128, &|| false) {
        Ok(area) => near(area, 1., 1e-6),
        Err(message) => assert!(message.contains("収束") || message.contains("上限")),
    }
}

#[test]
fn translated_rational_area_must_not_silently_drop_a_whole_cell() {
    // The exact image is a translated 2-by-1 rectangle. The first knot cell
    // has an extreme rational parameterization; a large world offset used to
    // cancel all its sampled derivatives to zero and silently remove its area.
    let p = 16777216.; // 2^24: all coordinates and weights are exact binary values.
    let surface = Surface::nurbs(
        NurbsSurface::new(
            3,
            2,
            1,
            1,
            vec![0., 0., 0.5, 1., 1.],
            vec![0., 0., 1., 1.],
            vec![
                [p, 0., 0.],
                [p, 1., 0.],
                [p + 1., 0., 0.],
                [p + 1., 1., 0.],
                [p + 2., 0., 0.],
                [p + 2., 1., 0.],
            ],
            vec![
                524288.,
                524288.,
                1. / 524288.,
                1. / 524288.,
                1. / 524288.,
                1. / 524288.,
            ],
        )
        .unwrap(),
    );
    match surface.area(128, &|| false) {
        Ok(area) => near(area, 2., 2e-6),
        Err(message) => assert!(message.contains("収束") || message.contains("上限")),
    }
}

#[test]
fn rational_first_and_second_derivatives_are_translation_invariant() {
    // x=P+2u/(1+u), y=P+v, z=P. Analytic normalized derivatives at u=1/2
    // are dx/du=8/9 and d2x/du2=-32/27, independent of the world offset.
    let p = 16777216.;
    let n = NurbsSurface::new(
        2,
        2,
        1,
        1,
        vec![0., 0., 1., 1.],
        vec![0., 0., 1., 1.],
        vec![
            [p, p, p],
            [p, p + 1., p],
            [p + 1., p, p],
            [p + 1., p + 1., p],
        ],
        vec![1., 1., 2., 2.],
    )
    .unwrap();
    let sample = n.evaluate(0.5, 0.25).unwrap();
    let jet = n.derivatives2(0.5, 0.25, &|| false).unwrap();
    for first in [sample, jet.first] {
        point_near(first.point, [p + 2. / 3., p + 0.25, p], 4e-9);
        point_near(first.du, [8. / 9., 0., 0.], 2e-14);
        point_near(first.dv, [0., 1., 0.], 2e-14);
    }
    point_near(jet.duu, [-32. / 27., 0., 0.], 2e-14);
    point_near(jet.duv, [0.; 3], 2e-14);
    point_near(jet.dvv, [0.; 3], 2e-14);
}

#[test]
fn closest_point_keeps_an_interior_point_on_a_skew_plane() {
    let plane = PlaneSurface::new([0.; 3], [1e8, 0., 0.], [1e8, 1., 0.]).unwrap();
    point_near(plane.closest([1e8, 0.5, 0.]).unwrap(), [1e8, 0.5, 0.], 1e-8);
    point_near(plane.closest([1e8, 0.5, 7.]).unwrap(), [1e8, 0.5, 0.], 1e-8);
    // A large normal distance must not hide the smaller in-plane differences
    // between an interior projection and an edge in squared-distance rounding.
    point_near(
        plane.closest([1e8, 0.5, 1e8]).unwrap(),
        [1e8, 0.5, 0.],
        1e-8,
    );
}

#[test]
fn closest_point_keeps_the_four_constrained_edge_candidates() {
    let plane = PlaneSurface::new([0.; 3], [1e8, 0., 0.], [1e8, 1., 0.]).unwrap();
    for (query, expected) in [
        ([5e7, -0.25, 3.], [5e7, 0., 0.]),
        ([1.5e8, 1.25, -2.], [1.5e8, 1., 0.]),
        ([1.5e8, 1.25, 1e8], [1.5e8, 1., 0.]),
        ([-1., 0., 0.], [0., 0., 0.]),
        ([2e8 + 1., 1., 0.], [2e8, 1., 0.]),
    ] {
        point_near(plane.closest(query).unwrap(), expected, 1e-8);
    }
}

#[test]
fn closest_point_handles_opposite_axes_translation_and_rotated_frames() {
    let plane = PlaneSurface::new([100., 200., 300.], [1e8, 0., 0.], [-1e8, 1., 0.]).unwrap();
    point_near(
        plane.closest([100., 200.5, 305.]).unwrap(),
        [100., 200.5, 300.],
        1e-8,
    );
    let rotated = PlaneSurface::new([0.; 3], [6e7, 8e7, 0.], [6e7, 8e7, 1.]).unwrap();
    point_near(
        rotated.closest([6e7 + 4., 8e7 - 3., 0.75]).unwrap(),
        [6e7, 8e7, 0.75],
        3e-8,
    );
    assert!(rotated.closest([f64::NAN, 0., 0.]).is_err());
    assert!(rotated.closest([f64::INFINITY, 0., 0.]).is_err());
}

#[test]
fn closest_point_preserves_small_cross_products_of_large_skew_axes() {
    // All fixture values are exactly representable in f64. In exact rational
    // arithmetic u x v = [150000014.5, 50000009.5, -200000010]. The query is
    // target + (u x v)/2, so its independent orthogonal projection is target.
    let plane = PlaneSurface::new(
        [0.; 3],
        [100000003., 100000007., 100000011.],
        [100000004., 100000006., 100000011.5],
    )
    .unwrap();
    point_near(
        plane.closest([175000010.75, 125000011.25, 6.25]).unwrap(),
        [100000003.5, 100000006.5, 100000011.25],
        6e-8,
    );
    let cross: [f64; 3] = [150000014.5, 50000009.5, -200000010.];
    let area = cross.iter().map(|x| x * x).sum::<f64>().sqrt();
    near(plane.area(), area, area * 2e-15);
    point_near(
        plane.evaluate(0.5, 0.5).unwrap().normal().unwrap(),
        cross.map(|x| x / area),
        2e-15,
    );
}
