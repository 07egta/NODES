//! Independent analytic expectations, without reference to tessellated meshes.
use morphlet::{
    engine::Evaluator,
    model::{Graph, NodeKind as K, Value},
    persistence::{self, Project},
    surface_kernel::{NurbsSurface, Surface},
};
use std::f64::consts::TAU;
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 2e-10 * (1. + b.abs()), "{a} != {b}");
}
fn xyz(a: [f64; 3], b: [f64; 3]) {
    for i in 0..3 {
        near(a[i], b[i]);
    }
}
fn bowl(weight: f64, swap: bool) -> NurbsSurface {
    let mut points = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            let (x, y) = if swap { (j, i) } else { (i, j) };
            points.push([
                x as f64 / 2.,
                y as f64 / 2.,
                f64::from(i == 2) + f64::from(j == 2),
            ]);
        }
    }
    NurbsSurface::new(
        3,
        3,
        2,
        2,
        vec![2., 2., 2., 7., 7., 7.],
        vec![-4., -4., -4., 3., 3., 3.],
        points,
        vec![weight; 9],
    )
    .unwrap()
}
fn quarter_cylinder(weight: f64) -> NurbsSurface {
    let w = 0.5_f64.sqrt();
    NurbsSurface::new(
        3,
        2,
        2,
        1,
        vec![2., 2., 2., 7., 7., 7.],
        vec![-3., -3., 2., 2.],
        vec![
            [3., 0., 0.],
            [3., 0., 7.],
            [3., 3., 0.],
            [3., 3., 7.],
            [0., 3., 0.],
            [0., 3., 7.],
        ],
        [1., 1., w, w, 1., 1.]
            .into_iter()
            .map(|x| x * weight)
            .collect(),
    )
    .unwrap()
}
#[test]
fn plane_sphere_cylinder_and_frustum_match_analytic_curvatures() {
    let plane = Surface::plane(3., 5.).unwrap();
    let c = plane.curvature(0.2, 0.7, &|| false).unwrap();
    near(c.gaussian, 0.);
    near(c.mean, 0.);
    assert_eq!(c.principal, [0., 0.]);
    let r = 2.5;
    for u in [0., 0.33, 1.] {
        for v in [0.1, 0.3, 0.5, 0.9] {
            let s = Surface::sphere(r).unwrap();
            let c = s.curvature(u, v, &|| false).unwrap();
            near(c.gaussian, 1. / (r * r));
            near(c.mean, -1. / r);
            for k in c.principal {
                near(k, -1. / r);
            }
            xyz(c.normal, s.evaluate(u, v).unwrap().point.map(|x| x / r));
        }
    }
    let cylinder = Surface::frustum(3., 3., 7.).unwrap();
    let c = cylinder.curvature(0.2, 0.3, &|| false).unwrap();
    near(c.gaussian, 0.);
    near(c.mean, -1. / 6.);
    near(c.principal[0], -1. / 3.);
    near(c.principal[1], 0.);
    let frustum = Surface::frustum(2., 5., 7.).unwrap();
    let v = 0.4;
    let c = frustum.curvature(0.3, v, &|| false).unwrap();
    near(c.gaussian, 0.);
    near(c.mean, -7. / (2. * (2. + 3. * v) * 58_f64.sqrt()));
}
#[test]
fn torus_outer_inner_and_parabolic_rings_match_analytic_solution() {
    let major = 4.;
    let minor = 1.2;
    let torus = Surface::torus(major, minor).unwrap();
    for v in [0., 0.25, 0.37, 0.5, 0.75, 1.] {
        let cosine = (TAU * v).cos();
        let radial = major + minor * cosine;
        let c = torus.curvature(0.41, v, &|| false).unwrap();
        near(c.gaussian, cosine / (minor * radial));
        near(c.mean, -0.5 * (1. / minor + cosine / radial));
        let mut principal = [-1. / minor, -cosine / radial];
        principal.sort_by(f64::total_cmp);
        xyz(
            [c.principal[0], c.principal[1], 0.],
            [principal[0], principal[1], 0.],
        );
    }
}
#[test]
fn nurbs_polynomial_jet_and_curvature_match_paraboloid() {
    let n = bowl(1., false);
    for (u, v) in [(0., 0.), (0.2, 0.3), (0.5, 0.8), (1., 1.)] {
        let j = n.derivatives2(u, v, &|| false).unwrap();
        xyz(j.first.point, [u, v, u * u + v * v]);
        xyz(j.first.du, [1., 0., 2. * u]);
        xyz(j.first.dv, [0., 1., 2. * v]);
        xyz(j.duu, [0., 0., 2.]);
        xyz(j.duv, [0.; 3]);
        xyz(j.dvv, [0., 0., 2.]);
        let c = j.curvature().unwrap();
        let d = 1. + 4. * u * u + 4. * v * v;
        near(c.gaussian, 4. / (d * d));
        near(c.mean, 2. * (1. + 2. * u * u + 2. * v * v) / d.powf(1.5));
    }
}
#[test]
fn bilinear_saddle_has_nonzero_mixed_derivative() {
    let n = NurbsSurface::bilinear([-1., -1., 1.], [-1., 1., -1.], [1., -1., -1.], [1., 1., 1.])
        .unwrap();
    let j = n.derivatives2(0.5, 0.5, &|| false).unwrap();
    xyz(j.duu, [0.; 3]);
    xyz(j.duv, [0., 0., 4.]);
    xyz(j.dvv, [0.; 3]);
    let c = j.curvature().unwrap();
    near(c.gaussian, -1.);
    near(c.mean, 0.);
    near(c.principal[0], -1.);
    near(c.principal[1], 1.);
}
#[test]
fn rational_derivatives_and_curvature_are_weight_scale_invariant() {
    let a = quarter_cylinder(1.);
    let b = quarter_cylinder(23.);
    for u in [0., 0.17, 0.5, 0.83, 1.] {
        let x = a.derivatives2(u, 0.37, &|| false).unwrap();
        let y = b.derivatives2(u, 0.37, &|| false).unwrap();
        xyz(x.first.point, y.first.point);
        xyz(x.first.du, y.first.du);
        xyz(x.first.dv, y.first.dv);
        xyz(x.duu, y.duu);
        xyz(x.duv, y.duv);
        xyz(x.dvv, y.dvv);
        let c = x.curvature().unwrap();
        near(c.gaussian, 0.);
        near(c.mean, -1. / 6.);
        near(c.principal[0], -1. / 3.);
        near(c.principal[1], 0.);
        xyz(c.normal, [x.first.point[0] / 3., x.first.point[1] / 3., 0.]);
    }
}
#[test]
fn normal_reversal_preserves_gaussian_and_reverses_mean() {
    let a = bowl(1., false)
        .derivatives2(0.2, 0.3, &|| false)
        .unwrap()
        .curvature()
        .unwrap();
    let b = bowl(1., true)
        .derivatives2(0.3, 0.2, &|| false)
        .unwrap()
        .curvature()
        .unwrap();
    near(a.gaussian, b.gaussian);
    near(a.mean, -b.mean);
    xyz(a.normal, b.normal.map(|x| -x));
    near(a.principal[0], -b.principal[1]);
    near(a.principal[1], -b.principal[0]);
}
#[test]
fn invalid_singular_discontinuous_and_cancelled_evaluations_are_errors() {
    let sphere = Surface::sphere(2.).unwrap();
    for (u, v) in [
        (0., 0.),
        (0., 1.),
        (f64::NAN, 0.3),
        (0.2, f64::INFINITY),
        (-0.1, 0.5),
        (0.5, 1.1),
    ] {
        assert!(sphere.curvature(u, v, &|| false).is_err());
    }
    assert!(sphere.curvature(0.2, 0.3, &|| true).is_err());
    let singular = NurbsSurface::bilinear([0.; 3], [0.; 3], [0.; 3], [0.; 3]).unwrap();
    assert!(
        singular
            .derivatives2(0.3, 0.4, &|| false)
            .unwrap()
            .curvature()
            .is_err()
    );
    let n = NurbsSurface::new(
        5,
        2,
        2,
        1,
        vec![0., 0., 0., 0.5, 0.5, 1., 1., 1.],
        vec![0., 0., 1., 1.],
        (0..5)
            .flat_map(|i| {
                [
                    [i as f64, 0., (i * i) as f64],
                    [i as f64, 1., (i * i) as f64],
                ]
            })
            .collect(),
        vec![1.; 10],
    )
    .unwrap();
    assert!(n.derivatives2(0.5, 0.4, &|| false).is_err());
    assert!(
        n.derivatives2(0.49, 0.4, &|| false)
            .unwrap()
            .curvature()
            .is_ok()
    );
    assert!(n.derivatives2(0.4, 0.4, &|| true).is_err());
    let mut j = bowl(1., false).derivatives2(0.2, 0.3, &|| false).unwrap();
    j.duu[0] = f64::NAN;
    assert!(j.curvature().is_err());
}
#[test]
fn simple_cubic_interior_knot_is_c2_and_allowed() {
    let n = NurbsSurface::new(
        5,
        2,
        3,
        1,
        vec![0., 0., 0., 0., 0.5, 1., 1., 1., 1.],
        vec![0., 0., 1., 1.],
        (0..5)
            .flat_map(|i| {
                [
                    [i as f64, 0., (i * i) as f64],
                    [i as f64, 1., (i * i) as f64],
                ]
            })
            .collect(),
        vec![1.; 10],
    )
    .unwrap();
    assert!(
        n.derivatives2(0.5, 0.4, &|| false)
            .unwrap()
            .curvature()
            .is_ok()
    );
}
fn numbers(e: &morphlet::engine::Evaluation, id: u64) -> Vec<f64> {
    e.outputs[&id]
        .as_ref()
        .unwrap()
        .branches
        .values()
        .flatten()
        .map(|v| v.number().unwrap())
        .collect()
}
#[test]
fn curvature_nodes_roundtrip_recompute_and_propagate_errors() {
    let mut graph = Graph::default();
    let sphere = graph.add(K::SphereSurface, [0.; 2]).unwrap();
    graph
        .node_mut(sphere)
        .unwrap()
        .params
        .insert("radius".into(), 2.);
    let kinds = [
        K::SurfaceGaussianCurvature,
        K::SurfaceMeanCurvature,
        K::SurfacePrincipalCurvatures,
    ];
    let mut ids = Vec::new();
    for k in kinds {
        assert!(K::all().contains(&k));
        let id = graph.add(k, [0.; 2]).unwrap();
        graph.connect(sphere, id, "surface").unwrap();
        ids.push(id);
    }
    let mut evaluator = Evaluator::default();
    let e = evaluator.evaluate(&graph, &|| false);
    near(numbers(&e, ids[0])[0], 0.25);
    near(numbers(&e, ids[1])[0], -0.5);
    assert_eq!(numbers(&e, ids[2]).len(), 2);
    for k in numbers(&e, ids[2]) {
        near(k, -0.5);
    }
    graph
        .node_mut(sphere)
        .unwrap()
        .params
        .insert("radius".into(), 4.);
    let e = evaluator.evaluate(&graph, &|| false);
    near(numbers(&e, ids[0])[0], 1. / 16.);
    assert_eq!(evaluator.evaluate(&graph, &|| false).computed, 0);
    let project = Project::new("Curvature", graph);
    let path = std::env::temp_dir().join(format!("nodes-curvature-{}.morph", std::process::id()));
    persistence::save(&path, &project).unwrap();
    let mut loaded = persistence::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, project);
    let e = evaluator.evaluate(&loaded.graph, &|| false);
    near(numbers(&e, ids[1])[0], -0.25);
    loaded
        .graph
        .node_mut(ids[0])
        .unwrap()
        .params
        .insert("v".into(), 0.);
    let e = evaluator.evaluate(&loaded.graph, &|| false);
    assert!(e.outputs[&ids[0]].is_err());
    assert!(e.outputs[&sphere].is_ok());
    assert!(matches!(
        e.outputs[&sphere]
            .as_ref()
            .unwrap()
            .branches
            .values()
            .flatten()
            .next(),
        Some(Value::Surface(_))
    ));
}
