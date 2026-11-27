//! New implementation after the runtime outage; not a byte-for-byte recovery
//! of local commit 997183b. CI must validate these regressions.
use nodes::surface_kernel::NurbsSurface;

fn crease(v_axis: bool) -> NurbsSurface {
    let creased = vec![0., 0., 0., 0.7, 0.7, 10., 10., 10.];
    let linear = vec![0., 0., 1., 1.];
    let (nu, nv, pu, pv, ku, kv) = if v_axis {
        (2, 5, 1, 2, linear, creased)
    } else {
        (5, 2, 2, 1, creased, linear)
    };
    let points = (0..nu)
        .flat_map(|i| {
            (0..nv).map(move |j| {
                let along = if v_axis { j } else { i };
                [i as f64, j as f64, (along * along) as f64]
            })
        })
        .collect();
    NurbsSurface::new(nu, nv, pu, pv, ku, kv, points, vec![1.; nu * nv]).unwrap()
}

#[test]
fn normalized_uv_one_ulp_from_non_c2_knot_is_rejected_in_both_axes() {
    let native = 10.0_f64 * 0.07_f64;
    let stored = 0.7_f64;
    assert_ne!(native, stored);
    assert_eq!(native.to_bits(), stored.to_bits() + 1);
    let u_surface = crease(false);
    assert!(u_surface.derivatives2(0.07, 0.4, &|| false).is_err());
    let v_surface = crease(true);
    assert!(v_surface.derivatives2(0.4, 0.07, &|| false).is_err());
}

#[test]
fn clear_spans_and_exact_endpoints_remain_available() {
    let surface = crease(false);
    for u in [0., 0.06, 0.08, 1.] {
        assert!(
            surface
                .derivatives2(u, 0.4, &|| false)
                .unwrap()
                .curvature()
                .is_ok(),
            "regular parameter {u}"
        );
    }
    assert!(
        surface
            .derivatives2(0.06, 0.4, &|| true)
            .unwrap_err()
            .contains("停止")
    );
}

#[test]
fn nearby_distinct_simple_cubic_knots_keep_their_true_multiplicity() {
    let a = 0.7_f64;
    let b = f64::from_bits(a.to_bits() + 1);
    let surface = NurbsSurface::new(
        6,
        2,
        3,
        1,
        vec![0., 0., 0., 0., a, b, 10., 10., 10., 10.],
        vec![0., 0., 1., 1.],
        (0..6)
            .flat_map(|i| [[i as f64, 0., 0.], [i as f64, 1., 0.]])
            .collect(),
        vec![1.; 12],
    )
    .unwrap();
    // Both native knots are simple (C2), even though both lie inside the
    // proximity band. Summing nearby entries would incorrectly make C1.
    assert!(surface.derivatives2(0.07, 0.4, &|| false).is_ok());
}
