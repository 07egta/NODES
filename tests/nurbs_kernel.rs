//! Exact NURBS / B-rep / T-spline checks on the vendored SolidCraft kernel (`kernel_bridge`,
//! `subd`, `nurbs_nodes`).
use nodes::curve_geometry::CurveGeometry;
use nodes::kernel_bridge::{self as kb, Bool, BrepValue};
use nodes::model::{Curve, Mesh, Tree, Value};
use nodes::subd::Cage;
use std::f64::consts::PI;
use std::sync::Arc;

fn go() -> bool {
    false
}

fn circle(center: [f64; 3], r: f64) -> Curve {
    let g = CurveGeometry::circle(center, [0., 0., 1.], r).unwrap();
    kb::curve_value(g, &go).unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-300)
}

/// Largest angle (radians) between the normals of triangles sharing an edge.
fn max_crease(m: &Mesh) -> f64 {
    let normal = |t: &[u32; 3]| {
        let p = |i: u32| m.vertices[i as usize];
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        [n[0] / l, n[1] / l, n[2] / l]
    };
    let mut by_edge: std::collections::HashMap<(u32, u32), Vec<usize>> = Default::default();
    for (i, t) in m.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            by_edge.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let mut worst = 0f64;
    for faces in by_edge.values() {
        if let [a, b] = faces.as_slice() {
            let (na, nb) = (normal(&m.triangles[*a]), normal(&m.triangles[*b]));
            let d = (na[0] * nb[0] + na[1] * nb[1] + na[2] * nb[2]).clamp(-1., 1.);
            worst = worst.max(d.acos());
        }
    }
    worst
}

#[test]
fn exact_circle_pieces_have_no_radius_error() {
    let r = 17.25;
    let c = circle([3., -2., 5.], r);
    let (pieces, closed) = kb::curve_pieces(&c, &go).unwrap();
    assert!(closed);
    assert!(pieces.len() >= 4);
    let pts = kb::sample_curve(&c, 64, &go).unwrap();
    let worst = pts
        .iter()
        .map(|p| {
            (((p[0] - 3.).powi(2) + (p[1] + 2.).powi(2)).sqrt() - r)
                .abs()
                .max((p[2] - 5.).abs())
        })
        .fold(0f64, f64::max);
    assert!(worst < 1e-9, "radius error {worst}");
    // Extruded, the side is an exact cylinder: the volume matches πr²h.
    let solid = kb::extrude(&c, &[], 10., &go).unwrap();
    assert!(solid.is_closed());
    let v = solid.volume().unwrap();
    assert!(rel(v, PI * r * r * 10.) < 1e-3, "volume {v}");
}

#[test]
fn loft_of_three_sections_is_watertight() {
    let a = circle([0., 0., 0.], 20.);
    let b = circle([0., 0., 40.], 30.);
    let c = circle([0., 0., 80.], 15.);
    let solid = kb::loft(&[&a, &b, &c], true, &go).unwrap();
    assert!(solid.is_closed(), "loft is not closed");
    assert!(kb::is_watertight(solid.preview()));
    assert!(solid.volume().unwrap() > 0.);
    let stats = nodes::persistence::mesh_stats(&[nodes::model::Tree::single(
        nodes::model::Value::Brep(std::sync::Arc::new(solid)),
    )])
    .unwrap();
    assert!(stats.is_closed(), "{stats:?}");
}

#[test]
fn one_rail_sweep_requires_a_path_that_leaves_the_profile_plane() {
    let profile = circle([0., 0., 0.], 5.);
    let planar_rail = circle([20., 0., 0.], 15.);
    assert!(
        kb::sweep1(&profile, &planar_rail, &go)
            .unwrap_err()
            .contains("立体を作れません")
    );

    let rising_rail = Curve {
        points: vec![[0., 0., 0.], [0., 0., 20.]],
        closed: false,
        geometry: None,
    };
    let swept = kb::sweep1(&profile, &rising_rail, &go).unwrap();
    assert!(swept.is_closed());
    assert!(rel(swept.volume().unwrap(), 500. * PI) < 1e-3);
}

#[test]
fn crossing_cylinders_boolean_is_watertight_with_exact_volume() {
    let r = 10.;
    let z = kb::cylinder([0., 0., -30.], [0., 0., 1.], r, 60., &go).unwrap();
    let x = kb::cylinder([-30., 0., 0.], [1., 0., 0.], r, 60., &go).unwrap();
    let both = kb::boolean(&z, &x, Bool::Intersection, &go).unwrap();
    assert!(both.is_closed());
    // Steinmetz bicylinder: 16 r³ / 3.
    let v = both.volume().unwrap();
    assert!(
        rel(v, 16. * r * r * r / 3.) < 1e-3,
        "intersection volume {v}"
    );
    let union = kb::boolean(&z, &x, Bool::Union, &go).unwrap();
    assert!(union.is_closed());
    let expected = 2. * PI * r * r * 60. - 16. * r * r * r / 3.;
    let v = union.volume().unwrap();
    assert!(rel(v, expected) < 1e-3, "union volume {v} vs {expected}");
    for solid in [both, union] {
        // The repaired preview passes the same topology checks as the STL exporter,
        // while the underlying geometry still uses cylindrical faces.
        let body = solid.body();
        assert!((0..body.face_count()).any(|face| matches!(
            solidcraft_kernel::face_surface(body, face),
            Some(solidcraft_kernel::FaceSurface::Cylinder { .. })
        )));
        let tree = Tree::single(Value::Brep(Arc::new(solid)));
        let report = nodes::persistence::mesh_stats(&[tree]).unwrap();
        assert!(report.is_closed(), "{report:?}");
    }
}

#[test]
fn filleted_box_is_watertight_and_smaller() {
    let b = kb::box_solid(40., 40., 30., &go).unwrap();
    let f = kb::fillet(&b, 3., &[], &go).unwrap();
    assert!(f.is_closed());
    assert!(f.face_count() > 6);
    let v = f.volume().unwrap();
    assert!(
        v < 40. * 40. * 30. && v > 40. * 40. * 30. * 0.9,
        "volume {v}"
    );
}

#[test]
fn step_round_trip_keeps_a_box() {
    let b = kb::box_solid(20., 30., 40., &go).unwrap();
    let text = kb::step_export(&[&b], &go).unwrap();
    assert!(text.starts_with("ISO-10303-21"));
    let back: Vec<BrepValue> = kb::step_import(&text, &go).unwrap();
    assert_eq!(back.len(), 1);
    assert!(back[0].is_closed());
    assert!(rel(back[0].volume().unwrap(), 24_000.) < 1e-6);
}

#[test]
fn tspline_box_cage_refines_to_a_smooth_closed_surface() {
    let cage = Cage::box_cage(40., 40., 40., &go).unwrap();
    let coarse = cage.preview();
    assert!(kb::is_watertight(coarse));
    let fine = cage.subdivide(&go).unwrap().subdivide(&go).unwrap();
    assert!(kb::is_watertight(fine.preview()));
    // Smooth: neighbouring facets turn by a few degrees only, unlike the 90° cage.
    let crease = max_crease(fine.preview());
    assert!(crease < 15f64.to_radians(), "max facet turn {crease}");
    // A crease keeps an edge sharp.
    let sharp = cage.crease(&[0], &go).unwrap();
    assert!(max_crease(sharp.preview()) > crease);
}

#[test]
fn cylinder_cage_refinement_keeps_the_exact_surface() {
    let cage = Cage::cylinder_cage(20., 60., 8, 6, 0.3, &go).unwrap();
    let fine = cage.subdivide(&go).unwrap();
    let (s0, s1) = (cage.grid_surface().unwrap(), fine.grid_surface().unwrap());
    let cols = 8.;
    let mut worst = 0f64;
    for i in 0..16 {
        for j in 0..=6 {
            let u = cols * i as f64 / 16.;
            let v = 3. * j as f64 / 6.;
            let mut u2 = 2. * u + 1.;
            if u2 >= 2. * cols {
                u2 -= 2. * cols;
            }
            let (p, q) = (s0.eval(u, v).unwrap(), s1.eval(u2, 2. * v).unwrap());
            worst = worst.max(p.dist(q));
        }
    }
    assert!(worst < 1e-9, "refined surface moved by {worst}");
}

#[test]
fn tspline_vase_becomes_a_closed_brep() {
    let cage = Cage::cylinder_cage(25., 80., 8, 6, 0.35, &go).unwrap();
    let solid = cage.to_brep(true, &go).unwrap();
    assert!(solid.is_closed(), "T-spline vase is not closed");
    let v = solid.volume().unwrap();
    let mesh_v = kb::mesh_volume(cage.preview()).abs();
    assert!(v > 0. && mesh_v >= 0.);
    let step = kb::step_export(&[&solid], &go).unwrap();
    assert!(step.contains("B_SPLINE_SURFACE"));
}
