//! Independent axis-aligned box oracles. The kernel is not its own reference.
use solidcraft_geom::Vec3;
use solidcraft_kernel::boolean_checked::{BooleanStatus, boolean_checked};
use solidcraft_kernel::{Body, BoolOp, box_solid, measure_mass};

fn bx(lo: [f64; 3], hi: [f64; 3]) -> Body {
    box_solid(Vec3::new(lo[0], lo[1], lo[2]), Vec3::new(hi[0], hi[1], hi[2])).unwrap()
}

fn volume(a: &Body, b: &Body, op: BoolOp) -> f64 {
    let r = boolean_checked(a, b, op, &|| false);
    assert_eq!(r.status, BooleanStatus::Success, "{}", r.detail);
    let body = r.body.unwrap();
    assert!(!body.is_mesh());
    assert!(body.validity().is_empty(), "{:?}", body.validity());
    measure_mass(&body).unwrap().volume
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-6 * b.abs().max(1.), "{a} != {b}");
}

#[test]
fn overlapping_boxes_volume_identities() {
    let a = bx([0., 0., 0.], [10., 10., 10.]);
    let b = bx([5., 0., 0.], [15., 10., 10.]);
    let u = volume(&a, &b, BoolOp::Union);
    let i = volume(&a, &b, BoolOp::Intersect);
    let d = volume(&a, &b, BoolOp::Cut);
    close(u, 1500.);
    close(i, 500.);
    close(d, 500.);
    close(u + i, 2000.);
    close(d + i, 1000.);
    close(volume(&b, &a, BoolOp::Union), u);
    close(volume(&b, &a, BoolOp::Intersect), i);
}

#[test]
fn self_difference_empty_is_not_a_certified_success() {
    let a = bx([0., 0., 0.], [10., 10., 10.]);
    close(volume(&a, &a, BoolOp::Union), 1000.);
    close(volume(&a, &a, BoolOp::Intersect), 1000.);
    let r = boolean_checked(&a, &a, BoolOp::Cut, &|| false);
    assert_eq!(r.status, BooleanStatus::AmbiguousIntersection, "{}", r.detail);
    assert!(r.body.is_none());
}

#[test]
fn cancelled_does_not_return_a_body() {
    let a = bx([0., 0., 0.], [10., 10., 10.]);
    let r = boolean_checked(&a, &a, BoolOp::Union, &|| true);
    assert_eq!(r.status, BooleanStatus::Cancelled);
    assert!(r.body.is_none());
}

#[test]
#[ignore = "bounded reproducible stress and timing; run explicitly in release"]
fn seeded_box_stress() {
    let a = bx([0., 0., 0.], [10., 10., 10.]);
    let mut state = 92u64;
    let start = std::time::Instant::now();
    for _ in 0..32 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let x = 1. + ((state >> 32) % 800) as f64 / 100.;
        let b = bx([x, 0., 0.], [x + 10., 10., 10.]);
        let i = volume(&a, &b, BoolOp::Intersect);
        close(i, (10. - x) * 100.);
        close(volume(&a, &b, BoolOp::Union) + i, 2000.);
        close(volume(&a, &b, BoolOp::Cut) + i, 1000.);
    }
    eprintln!("96 checked Booleans: {:?}", start.elapsed());
}
