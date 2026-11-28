//! Regression for rational linear arc length: no quadrature of singularly concentrated speed.
use nodes::curve_geometry::{CurveGeometry, NurbsCurve};

fn near(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance, "{a} != {b} (tol {tolerance})");
}

fn rational_line(weights: [f64; 2]) -> CurveGeometry {
    CurveGeometry::nurbs(
        NurbsCurve::new(
            1,
            vec![[0.; 3], [1., 0., 0.]],
            weights.to_vec(),
            vec![0., 0., 1., 1.],
        )
        .unwrap(),
    )
}

#[test]
fn extreme_positive_weights_do_not_hide_line_length() {
    for weights in [[1., 1e-12], [1e-12, 1.], [1., 1.]] {
        let g = rational_line(weights);
        let table = g.length_table(&|| false).unwrap();
        near(table.total(), 1., 1e-14);
        for f in [0.125, 0.5, 0.875] {
            let expected = f * weights[0] / ((1. - f) * weights[1] + f * weights[0]);
            let t = table.parameter(&g, f, &|| false).unwrap();
            near(t, expected, 2e-15);
        }
        assert!(table.parameter(&g, 0.5, &|| true).is_err());
    }
}

#[test]
fn linear_lengths_follow_trimmed_and_reversed_rational_spans() {
    let g = rational_line([0.25, 1.]);
    let g = g.subcurve(0.2, 0.8).unwrap().reversed();
    let a = g.point(0.).unwrap()[0];
    let b = g.point(1.).unwrap()[0];
    let table = g.length_table(&|| false).unwrap();
    near(table.total(), a - b, 1e-14);
    for f in [0., 0.25, 0.5, 0.75, 1.] {
        let t = table.parameter(&g, f * table.total(), &|| false).unwrap();
        near(g.point(t).unwrap()[0], a + f * (b - a), 1e-13);
    }
}

#[test]
fn polyline_length_and_inverse_follow_native_knot_spans() {
    let g = CurveGeometry::nurbs(
        NurbsCurve::new(
            1,
            vec![[0.; 3], [3., 0., 0.], [3., 4., 0.]],
            vec![1., 2., 4.],
            vec![0., 0., 0.125, 1., 1.],
        )
        .unwrap(),
    );
    let table = g.length_table(&|| false).unwrap();
    near(table.total(), 7., 1e-14);
    let t = table.parameter(&g, 3., &|| false).unwrap();
    near(t, 0.125, 1e-14);
    let t = table.parameter(&g, 5., &|| false).unwrap();
    near(g.point(t).unwrap()[1], 2., 1e-13);
    let reversed = g.reversed();
    let table = reversed.length_table(&|| false).unwrap();
    near(table.total(), 7., 1e-14);
    let t = table.parameter(&reversed, 2., &|| false).unwrap();
    near(reversed.point(t).unwrap()[1], 2., 1e-13);
}
