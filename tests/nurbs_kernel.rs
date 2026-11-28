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

// ---- Exact intersections, general cages, cage edits, curve tools ----

#[test]
fn exact_intersection_curves_lie_on_both_cylinders() {
    let (r, r2) = (10., 6.);
    let z = kb::cylinder([0., 0., -30.], [0., 0., 1.], r, 60., &go).unwrap();
    let x = kb::cylinder([-30., 0., 0.], [1., 0., 0.], r2, 60., &go).unwrap();
    let curves = nodes::kernel_ops::exact_intersection(&z, &x, &go).unwrap();
    assert!(!curves.is_empty());
    let mut worst = 0f64;
    for c in &curves {
        assert!(
            c.geometry.is_some(),
            "intersection curve is not exact NURBS"
        );
        for p in kb::sample_curve(c, 8, &go).unwrap() {
            let on_z = ((p[0] * p[0] + p[1] * p[1]).sqrt() - r).abs();
            let on_x = ((p[1] * p[1] + p[2] * p[2]).sqrt() - r2).abs();
            worst = worst.max(on_z).max(on_x);
        }
    }
    assert!(
        worst < 1e-4,
        "intersection curve leaves the cylinders by {worst}"
    );
}

#[test]
fn box_cage_becomes_a_closed_nurbs_brep() {
    let cage = Cage::box_cage(40., 40., 40., &go)
        .unwrap()
        .subdivide(&go)
        .unwrap();
    let solid = cage.to_brep(true, &go).unwrap();
    assert!(solid.is_closed(), "ACC B-rep is not closed");
    let v = solid.volume().unwrap();
    // The patches follow the Catmull-Clark limit surface: same volume as its fine mesh.
    let mesh_v = kb::mesh_volume(cage.preview()).abs();
    assert!(
        rel(v, mesh_v) < 2e-2,
        "ACC volume {v} vs subdivision mesh {mesh_v}"
    );
}

#[test]
fn cage_edit_list_replays_moves_with_symmetry() {
    use nodes::subd::{EditOp, edits_text};
    let cage = Cage::box_cage(40., 40., 40., &go).unwrap();
    // Vertex 1 is (20, -20, 0); its X mirror is vertex 0.
    let ops = vec![
        EditOp::Move {
            verts: vec![1],
            d: [5., 0., 0.],
            sym: Some(0),
        },
        EditOp::Extrude {
            faces: vec![1],
            d: 10.,
        },
        EditOp::Crease { edges: vec![0] },
    ];
    let edited = cage.apply_edits(&edits_text(&ops), &go).unwrap();
    assert!((edited.vertices()[1][0] - 25.).abs() < 1e-12);
    assert!((edited.vertices()[0][0] + 25.).abs() < 1e-12);
    assert_eq!(edited.vertices().len(), 12);
    assert!(!edited.creases().is_empty());
    assert!(cage.apply_edits("not json", &go).is_err());
}

#[test]
fn match_and_blend_curves_reach_g2() {
    let pts = |v: &[[f64; 3]]| {
        let n = nodes::curve_geometry::NurbsCurve::uniform(v.to_vec(), 3, false).unwrap();
        kb::curve_value(CurveGeometry::nurbs(n), &go).unwrap()
    };
    let a = pts(&[
        [0., 0., 0.],
        [10., 5., 0.],
        [20., -3., 0.],
        [30., 4., 0.],
        [40., 0., 0.],
    ]);
    let b = pts(&[[45., 3., 0.], [55., 10., 2.], [65., 0., 4.], [75., 6., 0.]]);
    let matched = nodes::kernel_ops::match_curve(&a, &b, 2, &go).unwrap();
    let c = nodes::kernel_ops::continuity(&matched, &b).unwrap();
    assert_eq!(c[0], 2., "match: {c:?}");
    let blend = nodes::kernel_ops::blend_curve(&a, &b, true, 1., &go).unwrap();
    let start = nodes::kernel_ops::continuity(&a, &blend).unwrap();
    let end = nodes::kernel_ops::continuity(&blend, &b).unwrap();
    assert_eq!(start[0], 2., "blend start: {start:?}");
    assert_eq!(end[0], 2., "blend end: {end:?}");
}

#[test]
fn curved_cup_is_shelled() {
    let cyl = kb::cylinder([0., 0., 0.], [0., 0., 1.], 20., 40., &go).unwrap();
    let cup = kb::shell(&cyl, 2., Some(usize::MAX), &go).unwrap();
    assert!(cup.is_closed());
    let v = cup.volume().unwrap();
    let expected = PI * 400. * 40. - PI * 18. * 18. * 38.;
    assert!(rel(v, expected) < 2e-2, "cup volume {v} vs {expected}");
}

// ---- Constrained sketch, analysis, diagnostics, IGES ----

#[test]
fn constrained_sketch_solves_to_exact_profiles() {
    let text = "長方形 r 0 0 50 30\n固定 r.p0\n長さ r.b 60\n長さ r.l 40\n円 c 30 20 5\n半径 c 8\n";
    let tree = nodes::sketch_nodes::run(text, 0, &go).unwrap();
    let curves: Vec<_> = tree.branches.values().flatten().collect();
    assert!(
        curves.len() >= 2,
        "outer and hole expected, got {}",
        curves.len()
    );
    let nodes::model::Value::Curve(outer) = curves[0] else {
        panic!("not a curve")
    };
    let solid = kb::extrude(outer, &[], 10., &go).unwrap();
    let v = solid.volume().unwrap();
    assert!(
        rel(v, 60. * 40. * 10.) < 1e-6 || rel(v, PI * 64. * 10.) < 1e-6,
        "volume {v}"
    );
    // Unknown commands and conflicts give Japanese errors, not panics.
    assert!(nodes::sketch_nodes::run("ふしぎ x 1", 0, &go).is_err());
    let conflict = "線 l 0 0 10 0\n長さ l 10\n距離 l.start l.end 20\n";
    assert!(nodes::sketch_nodes::run(conflict, 1, &go).is_err());
}

#[test]
fn zebra_and_diagnostics_on_a_filleted_box() {
    let b = kb::fillet(&kb::box_solid(40., 30., 20., &go).unwrap(), 4., &[], &go).unwrap();
    let z = nodes::kernel_ops::zebra(&b, 10., 0, &go).unwrap();
    assert!(!z.triangles.is_empty());
    let d = nodes::kernel_ops::diagnose(&b, &go).unwrap();
    assert_eq!(d[0], 1.);
    assert!(d[1] > 6.);
    let draft = nodes::kernel_ops::draft_analysis(&b, 2., &go).unwrap();
    assert!(!draft.triangles.is_empty());
}

#[test]
fn iges_round_trip_keeps_a_cylinder() {
    let c = kb::cylinder([0., 0., 0.], [0., 0., 1.], 10., 20., &go).unwrap();
    let text = kb::iges_export(&[&c], &go).unwrap();
    let back = kb::iges_import(&text, &go).unwrap();
    assert_eq!(back.len(), 1);
    assert!(back[0].is_closed());
    assert!(rel(back[0].volume().unwrap(), PI * 100. * 20.) < 1e-3);
}

// ---- T-spline patches with T-junctions ----

#[test]
fn tspline_patch_local_refinement_keeps_the_surface_and_converts() {
    use nodes::subd::{EditOp, edits_text};
    let patch = Cage::tspline_patch(80., 60., 6, 5, 12., &go).unwrap();
    let before = patch.tspline().unwrap().clone();
    // Refine one inner face (face 7 of the 6 × 5 net spans s 1..2, t 1..2): points are added
    // (T-junctions), the surface stays the same.
    let inner = 7;
    let refined = patch
        .apply_edits(&edits_text(&[EditOp::Refine { faces: vec![inner] }]), &go)
        .unwrap();
    let after = refined.tspline().unwrap();
    assert!(refined.vertices().len() > patch.vertices().len());
    assert!(
        !after.mesh().is_regular(),
        "a local refinement leaves T-junctions"
    );
    let [[s0, s1], [t0, t1]] = before.domain();
    let mut worst = 0f64;
    for i in 0..=10 {
        for j in 0..=10 {
            let (s, t) = (
                s0 + (s1 - s0) * i as f64 / 10.,
                t0 + (t1 - t0) * j as f64 / 10.,
            );
            worst = worst.max(before.eval(s, t).unwrap().dist(after.eval(s, t).unwrap()));
        }
    }
    assert!(worst < 1e-9, "refinement moved the surface by {worst}");
    // Moving a new point changes the surface locally; the T-spline converts to NURBS faces.
    let moved = refined
        .apply_edits(
            &edits_text(&[EditOp::Move {
                verts: vec![refined.vertices().len() - 1],
                d: [0., 0., 5.],
                sym: None,
            }]),
            &go,
        )
        .unwrap();
    let sheet = moved.to_brep(false, &go).unwrap();
    assert!(!sheet.is_closed());
    assert!(sheet.face_count() >= 1);
    let solid = nodes::kernel_ops::thicken(&sheet, 2., false, &go).unwrap();
    assert!(solid.is_closed(), "thickened patch is not closed");
}

#[test]
fn closed_mesh_volume_is_translation_stable() {
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    let corners = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
        [1., 0., 1.],
        [1., 1., 1.],
        [0., 1., 1.],
    ];
    for side in [1.0_f64, 0.125, 0.015625, 0.001953125] {
        for offset in [0., 500_000., -500_000., 999_999.] {
            let mut vertices = vec![[1_000_000., -1_000_000., 1_000_000.]];
            vertices.extend(corners.map(|p| p.map(|v| offset + side * v)));
            let mut mesh = Mesh {
                vertices,
                triangles: triangles.iter().map(|t| t.map(|i| i + 1)).collect(),
            };
            let expected = side.powi(3);
            assert!(kb::is_watertight(&mesh));
            assert!(
                rel(kb::mesh_volume(&mesh), expected) < 1e-12,
                "side={side}, offset={offset}"
            );
            for t in &mut mesh.triangles {
                t.swap(1, 2);
            }
            assert!(rel(kb::mesh_volume(&mesh), -expected) < 1e-12);
        }
    }
}
