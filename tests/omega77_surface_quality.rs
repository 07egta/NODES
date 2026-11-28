use nodes::{
    engine::Evaluator,
    model::{Graph, NodeKind as K, Value},
    surface_curvature::{boundary_report, uv_condition},
    surface_kernel::{NurbsSurface, PlaneSurface, Surface},
};

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8 * (1. + b.abs()), "{a} != {b}");
}

#[test]
fn cylinder_principal_lines_normal_curvature_and_sphere_umbilic() {
    let cylinder = Surface::frustum(3., 3., 7.).unwrap();
    let jet = cylinder.derivatives2(0., 0.4, &|| false).unwrap();
    let [low, high] = jet.principal_directions().unwrap();
    near(low[1].abs(), 1.);
    near(high[2].abs(), 1.);
    near(jet.normal_curvature(0.).unwrap(), -1. / 3.);
    near(jet.normal_curvature(90.).unwrap(), 0.);
    near(jet.normal_curvature(45.).unwrap(), -1. / 6.);
    let sphere = Surface::sphere(2.).unwrap();
    assert!(sphere.derivatives2(0.2, 0.4, &|| false).unwrap().principal_directions().is_err());
    assert!(sphere.derivatives2(0.2, 0., &|| false).is_err());
}

#[test]
fn saddle_principal_directions_and_uv_condition() {
    let saddle = Surface::nurbs(
        NurbsSurface::bilinear([-1., -1., 1.], [-1., 1., -1.], [1., -1., -1.], [1., 1., 1.])
            .unwrap(),
    );
    let jet = saddle.derivatives2(0.5, 0.5, &|| false).unwrap();
    let [low, high] = jet.principal_directions().unwrap();
    near(low[0] * low[1], -0.5);
    near(high[0] * high[1], 0.5);
    near(jet.normal_curvature(45.).unwrap(), 1.);
    near(uv_condition(jet.first).unwrap(), 1.);
    let plane = Surface::plane(4., 2.).unwrap();
    near(uv_condition(plane.evaluate(0.5, 0.5).unwrap()).unwrap(), 2.);
}

fn plane(origin: [f64; 3], u: [f64; 3], v: [f64; 3]) -> Surface {
    Surface::from_plane(PlaneSurface::new(origin, u, v).unwrap())
}

#[test]
fn seam_distinguishes_parametric_and_geometric_continuity() {
    let a = plane([0.; 3], [1., 0., 0.], [0., 1., 0.]);
    let b = plane([1., 0., 0.], [2., 0., 0.], [0., 1., 0.]);
    let r = boundary_report((&a, 1), (&b, 0), false, 17, &|| false).unwrap();
    for value in &r.errors[..3] {
        near(*value, 0.);
    }
    near(r.errors[3], 1.);
    near(r.errors[4], 0.);
    let b = plane([1., 1., 0.], [1., 0., 0.], [0., -1., 0.]);
    let reversed = boundary_report((&a, 1), (&b, 0), true, 17, &|| false).unwrap();
    for value in reversed.errors {
        near(value, 0.);
    }
    let wrong = boundary_report((&a, 1), (&b, 0), false, 17, &|| false).unwrap();
    near(wrong.errors[0], 1.);
    assert!(boundary_report((&a, 1), (&b, 0), false, 17, &|| true).is_err());
    assert!(boundary_report((&a, 4), (&b, 0), false, 17, &|| false).is_err());
}

#[test]
fn seam_detects_crease_and_curvature_jump() {
    let a = plane([0.; 3], [1., 0., 0.], [0., 1., 0.]);
    let b = plane([1., 0., 0.], [0., 0., 1.], [0., 1., 0.]);
    let r = boundary_report((&a, 1), (&b, 0), false, 9, &|| false).unwrap();
    near(r.errors[0], 0.);
    near(r.errors[1], 90.);
    // B(u,v) = (1+u, v, u²) has the same first derivatives but different S_uu.
    let points = (0..3)
        .flat_map(|i| (0..2).map(move |j| [1. + i as f64 / 2., j as f64, f64::from(i == 2)]))
        .collect();
    let b = Surface::nurbs(NurbsSurface::new(
        3, 2, 2, 1, vec![0., 0., 0., 1., 1., 1.], vec![0., 0., 1., 1.], points, vec![1.; 6],
    ).unwrap());
    let r = boundary_report((&a, 1), (&b, 0), false, 9, &|| false).unwrap();
    near(r.errors[0], 0.);
    near(r.errors[1], 0.);
    near(r.errors[2], 2.);
    near(r.errors[3], 0.);
    near(r.errors[4], 2.);
}

#[test]
fn fairing_graph_reports_energy_and_preserves_boundary() {
    let mut graph = Graph::default();
    let panel = graph.add(K::NurbsBicubicPanel, [0.; 2]).unwrap();
    let fair = graph.add(K::SurfaceFair, [0.; 2]).unwrap();
    let report = graph.add(K::SurfaceFairReport, [0.; 2]).unwrap();
    for id in [fair, report] {
        graph.connect(panel, id, "surface").unwrap();
    }
    let mut evaluator = Evaluator::default();
    let e = evaluator.evaluate(&graph, &|| false);
    let values = |id| {
        e.outputs[&id].as_ref().unwrap().branches.values().flatten()
    };
    let surface = |id| match values(id).next().unwrap() {
        Value::Surface(s) => s.geometry(),
        _ => panic!("surface expected"),
    };
    let numbers: Vec<_> = values(report).map(|v| v.number().unwrap()).collect();
    assert_eq!(numbers.len(), 4);
    assert!(numbers[1] < numbers[0]);
    assert!(numbers[3] <= 2.);
    for i in 0..=8 {
        let t = i as f64 / 8.;
        for (u, v) in [(0., t), (1., t), (t, 0.), (t, 1.)] {
            let a = surface(panel).evaluate(u, v).unwrap().point;
            let b = surface(fair).evaluate(u, v).unwrap().point;
            for (x, y) in a.into_iter().zip(b) {
                near(x, y);
            }
        }
    }
    assert!(surface(panel).fair_polynomial(
        &solidcraft_kernel::FairOptions::new(1., 8, true, 2.).unwrap(),
        &|| false,
    ).is_err());
}
