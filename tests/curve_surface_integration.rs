//! Independent analytic checks at the curve/surface kernel boundary.
use nodes::{
    curve_geometry::{CurveGeometry, NurbsCurve},
    engine::Evaluator,
    model::{Curve, Graph, NodeKind as K, Value},
    persistence,
    surface_kernel::{NurbsSurface, Surface},
};
use std::f64::consts::{PI, TAU};

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b}, tolerance {tol}");
}
fn point_near(a: [f64; 3], b: [f64; 3], tol: f64) {
    for i in 0..3 {
        near(a[i], b[i], tol);
    }
}
fn curve(g: CurveGeometry, steps: usize) -> Curve {
    Curve {
        points: g.preview(steps, &|| false).unwrap(),
        closed: g.closed().unwrap(),
        geometry: Some(g),
    }
}
fn first(e: &nodes::engine::Evaluation, id: u64) -> &Value {
    e.outputs[&id]
        .as_ref()
        .unwrap()
        .branches
        .values()
        .flatten()
        .next()
        .unwrap()
}

#[test]
fn circle_extrusion_uses_exact_controls_not_preview_samples() {
    for segments in [8., 16., 96., 256.] {
        let mut graph = Graph::default();
        let c = graph.add(K::Circle, [0.; 2]).unwrap();
        graph
            .node_mut(c)
            .unwrap()
            .params
            .extend([("radius".into(), 3.), ("segments".into(), segments)]);
        let s = graph.add(K::SurfaceExtrude, [0.; 2]).unwrap();
        graph
            .node_mut(s)
            .unwrap()
            .params
            .insert("height".into(), 7.);
        graph.connect(c, s, "curve").unwrap();
        let iso = graph.add(K::SurfaceIsoCurve, [0.; 2]).unwrap();
        graph.node_mut(iso).unwrap().params.extend([
            ("axis".into(), 1.),
            ("t".into(), 0.4),
            ("steps".into(), 8.),
        ]);
        graph.connect(s, iso, "surface").unwrap();
        let length = graph.add(K::CurveLength, [0.; 2]).unwrap();
        graph.connect(iso, length, "curve").unwrap();
        let e = Evaluator::default().evaluate(&graph, &|| false);
        let Value::Surface(surface) = first(&e, s) else {
            panic!("surface expected")
        };
        for i in 0..=32 {
            let p = surface
                .geometry()
                .evaluate(i as f64 / 32., 0.4)
                .unwrap()
                .point;
            near(p[0].hypot(p[1]), 3., 2e-13);
            near(p[2], 2.8, 2e-13);
        }
        let Value::Curve(iso) = first(&e, iso) else {
            panic!("curve expected")
        };
        assert!(iso.geometry.is_some());
        assert_eq!(iso.points.len(), 8);
        near(first(&e, length).number().unwrap(), 3. * TAU, 1e-8);
    }
}

#[test]
fn rational_tensor_isocurves_preserve_native_knots_and_derivatives() {
    let w = 0.5_f64.sqrt();
    let n = NurbsSurface::new(
        3,
        2,
        2,
        1,
        vec![2., 2., 2., 5., 5., 5.],
        vec![-2., -2., 3., 3.],
        vec![
            [1., 0., 0.],
            [1., 0., 4.],
            [1., 1., 0.],
            [1., 1., 4.],
            [0., 1., 0.],
            [0., 1., 4.],
        ],
        vec![1., 1., w, w, 1., 1.],
    )
    .unwrap();
    for fixed_u in [false, true] {
        for fixed in [0., 0.21, 1.] {
            let iso = n.iso_curve(fixed_u, fixed).unwrap();
            for i in 0..=20 {
                let t = i as f64 / 20.;
                let sample = if fixed_u {
                    n.evaluate(fixed, t)
                } else {
                    n.evaluate(t, fixed)
                }
                .unwrap();
                let derivatives = iso.derivatives(t).unwrap();
                point_near(derivatives[0], sample.point, 2e-13);
                point_near(
                    derivatives[1],
                    if fixed_u { sample.dv } else { sample.du },
                    2e-12,
                );
            }
        }
    }
}

#[test]
fn analytic_isocurve_loci_lengths_and_poles_are_explicit() {
    let cases = [
        (Surface::sphere(3.).unwrap(), true, 0.3, 3. * PI),
        (Surface::sphere(3.).unwrap(), false, 0.5, 3. * TAU),
        (Surface::torus(7., 2.).unwrap(), true, 0.4, 2. * TAU),
        (Surface::torus(7., 2.).unwrap(), false, 0., 9. * TAU),
        (Surface::frustum(3., 5., 4.).unwrap(), false, 0.5, 4. * TAU),
        (
            Surface::frustum(3., 5., 4.).unwrap(),
            true,
            0.4,
            20_f64.sqrt(),
        ),
    ];
    for (s, constant_u, t, expected) in cases {
        let g = s.iso_geometry(constant_u, t).unwrap();
        near(g.length_table(&|| false).unwrap().total(), expected, 1e-7);
        let a = if constant_u {
            s.evaluate(t, 0.)
        } else {
            s.evaluate(0., t)
        }
        .unwrap()
        .point;
        let b = if constant_u {
            s.evaluate(t, 1.)
        } else {
            s.evaluate(1., t)
        }
        .unwrap()
        .point;
        point_near(g.point(0.).unwrap(), a, 1e-12);
        point_near(g.point(1.).unwrap(), b, 1e-12);
    }
    let sphere = Surface::sphere(3.).unwrap();
    assert!(sphere.iso_geometry(false, 0.).is_err());
    assert!(sphere.iso_geometry(false, 1.).is_err());
    assert!(sphere.iso_curve(true, 0.2, 8, &|| true).is_err());
}

#[test]
fn ruled_rational_surface_is_euclidean_linear_in_v() {
    let g = CurveGeometry::circle([0.; 3], [0., 0., 1.], 2.).unwrap();
    let h = g.affine(|p| [2. * p[0] + 1., 2. * p[1] - 3., 5.]).unwrap();
    let a = curve(g.clone(), 4);
    let b = curve(h.clone(), 128);
    let surface = NurbsSurface::ruled(&a, &b, false).unwrap();
    for i in 0..=24 {
        let u = i as f64 / 24.;
        for v in [0., 0.2, 0.7, 1.] {
            let p = g.point(u).unwrap();
            let q = h.point(u).unwrap();
            point_near(
                surface.evaluate(u, v).unwrap().point,
                std::array::from_fn(|k| (1. - v) * p[k] + v * q[k]),
                2e-12,
            );
        }
    }
    let backwards = curve(h.reversed(), 16);
    let aligned = NurbsSurface::ruled(&a, &backwards, true).unwrap();
    point_near(
        aligned.evaluate(0.32, 0.6).unwrap().point,
        surface.evaluate(0.32, 0.6).unwrap().point,
        2e-12,
    );
    let trimmed = curve(g.subcurve(0.1, 0.8).unwrap(), 16);
    assert!(NurbsSurface::ruled(&trimmed, &b, false).is_err());
    let unequal = CurveGeometry::nurbs(
        NurbsCurve::new(
            2,
            vec![[0., 0., 0.], [1., 1., 0.], [2., 0., 0.]],
            vec![1., 2., 1.],
            vec![0., 0., 0., 1., 1., 1.],
        )
        .unwrap(),
    );
    let equal = CurveGeometry::nurbs(
        NurbsCurve::bezier(vec![[0., 0., 2.], [1., 1., 2.], [2., 0., 2.]]).unwrap(),
    );
    assert!(NurbsSurface::ruled(&curve(unequal, 16), &curve(equal, 16), false).is_err());
}

#[test]
fn exact_curve_surface_workflow_survives_save_and_recompute() {
    let mut project = persistence::Project::new("Exact curve and surface", Graph::default());
    let c = project.graph.add(K::Circle, [0.; 2]).unwrap();
    let s = project.graph.add(K::SurfaceExtrude, [0.; 2]).unwrap();
    project.graph.connect(c, s, "curve").unwrap();
    let i = project.graph.add(K::SurfaceIsoCurve, [0.; 2]).unwrap();
    project
        .graph
        .node_mut(i)
        .unwrap()
        .params
        .insert("axis".into(), 1.);
    project.graph.connect(s, i, "surface").unwrap();
    let path =
        std::env::temp_dir().join(format!("nodes-exact-surface-{}.morph", std::process::id()));
    persistence::save(&path, &project).unwrap();
    let mut loaded = persistence::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut evaluator = Evaluator::default();
    let before = evaluator.evaluate(&loaded.graph, &|| false);
    loaded
        .graph
        .node_mut(c)
        .unwrap()
        .params
        .insert("radius".into(), 11.);
    let after = evaluator.evaluate(&loaded.graph, &|| false);
    assert_ne!(first(&before, i), first(&after, i));
    let Value::Curve(c) = first(&after, i) else {
        panic!("curve expected")
    };
    near(
        c.geometry
            .as_ref()
            .unwrap()
            .length_table(&|| false)
            .unwrap()
            .total(),
        11. * TAU,
        1e-7,
    );
}

#[test]
fn editable_exact_cylinder_example_loads_and_evaluates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/surface-exact-cylinder.morph");
    let project = persistence::load(&path).unwrap();
    let e = Evaluator::default().evaluate(&project.graph, &|| false);
    for (id, result) in &e.outputs {
        assert!(result.is_ok(), "example node {id}: {result:?}");
    }
    near(first(&e, 4).number().unwrap(), 24. * TAU, 1e-7);
    assert_eq!(e.outputs[&3].as_ref().unwrap().item_count(), 16);
}

#[test]
fn surface_tessellation_preserves_nonuniform_polyline_corners() {
    use nodes::curve_geometry::{cross, norm, sub};
    let g = CurveGeometry::nurbs(
        NurbsCurve::new(
            1,
            vec![[0., 0., 0.], [4., 0., 0.], [4., 2., 0.]],
            vec![1.; 3],
            vec![0., 0., 0.23, 1., 1.],
        )
        .unwrap(),
    );
    let h = g.affine(|p| [p[0], p[1], 3.]).unwrap();
    let surface = Surface::nurbs(NurbsSurface::ruled(&curve(g, 4), &curve(h, 16), false).unwrap());
    let mesh = surface.tessellate(4, 4, &|| false).unwrap();
    assert!(mesh.vertices.contains(&[4., 0., 0.]));
    let area: f64 = mesh
        .triangles
        .iter()
        .map(|&[i, j, k]| {
            norm(cross(
                sub(mesh.vertices[j as usize], mesh.vertices[i as usize]),
                sub(mesh.vertices[k as usize], mesh.vertices[i as usize]),
            )) / 2.
        })
        .sum();
    near(area, 18., 1e-11);
}
