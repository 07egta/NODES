//! OMEGA 87: direct intersections of planar B-rep faces with straight trim edges.
//! No display tessellation is used. Curved trims, curved supports and coincident
//! planes are deliberately unsupported by this restricted solver.
use crate::{Body, KernelError, Result, guard};
use solidcraft_geom::{Plane, Vec3};
use truck_modeling::{self as mt, BoundedCurve, ParametricCurve};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanarIntersectionSegment {
    pub face_a: usize,
    pub face_b: usize,
    pub start: Vec3,
    pub end: Vec3,
    pub max_plane_residual: f64,
}

struct Patch {
    plane: Plane,
    loops: Vec<Vec<Vec3>>,
}

fn patches(body: &Body, tol: f64, stop: &dyn Fn() -> bool) -> Result<Option<Vec<Patch>>> {
    if body.is_mesh() || body.face_count() > 128 {
        return Ok(None);
    }
    let mut patches = Vec::new();
    let mut budget = 4096usize;
    for (i, face) in body.solid.face_iter().enumerate() {
        if stop() {
            return Err(KernelError::Failed("intersection cancelled".into()));
        }
        let Some(plane) = body.planar_face(i) else { return Ok(None) };
        let mut loops = Vec::new();
        for wire in face.absolute_boundaries() {
            let mut points = Vec::new();
            let mut last: Option<Vec3> = None;
            for edge in wire.edge_iter() {
                if budget == 0 {
                    return Ok(None);
                }
                budget -= 1;
                let c = edge.oriented_curve();
                if !matches!(&c, mt::Curve::Line(_)) {
                    return Ok(None);
                }
                let (a, b) = c.range_tuple();
                let p = crate::body::from_p3(c.subs(a));
                let q = crate::body::from_p3(c.subs(b));
                if !p.is_finite() || !q.is_finite() || p.dist(q) <= tol
                    || plane.height(p).abs() > tol || plane.height(q).abs() > tol
                    || last.is_some_and(|v| v.dist(p) > tol)
                {
                    return Ok(None);
                }
                points.push(p);
                last = Some(q);
            }
            if points.len() < 3 || last.zip(points.first()).is_none_or(|(q, p)| q.dist(*p) > tol) {
                return Ok(None);
            }
            loops.push(points);
        }
        if loops.is_empty() {
            return Ok(None);
        }
        patches.push(Patch { plane, loops });
    }
    Ok(Some(patches))
}

// Coordinates (t,h) in an orthonormal frame with the intersection line at h=0.
fn intervals(patch: &Patch, origin: Vec3, dir: Vec3, tol: f64) -> Vec<(f64, f64)> {
    let side = patch.plane.normal().cross(dir);
    let loops: Vec<Vec<[f64; 2]>> = patch.loops.iter().map(|lp| {
        lp.iter().map(|p| {
            let q = *p - origin;
            [q.dot(dir), q.dot(side)]
        }).collect()
    }).collect();
    let mut events = Vec::new();
    for lp in &loops {
        for i in 0..lp.len() {
            let [t, h] = lp[i];
            let [s, k] = lp[(i + 1) % lp.len()];
            if h.abs() <= tol {
                events.push(t);
            }
            if (h > tol && k < -tol) || (h < -tol && k > tol) {
                events.push(t + (s - t) * (h / (h - k)));
            }
        }
    }
    events.sort_by(f64::total_cmp);
    events.dedup_by(|a, b| (*a - *b).abs() <= tol);
    let mut result = Vec::new();
    for w in events.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a <= tol {
            continue;
        }
        let t = a + (b - a) * 0.5;
        // Even-odd over every valid trim loop; orientation and hole order do
        // not matter. A segment lying on a straight trim edge is included.
        let mut inside = false;
        let mut boundary = false;
        for lp in &loops {
            for i in 0..lp.len() {
                let [x, y] = lp[i];
                let [u, v] = lp[(i + 1) % lp.len()];
                if y.abs() <= tol && v.abs() <= tol
                    && t >= x.min(u) - tol && t <= x.max(u) + tol
                {
                    boundary = true;
                }
                if (y > 0.0) != (v > 0.0) && t < x + (u - x) * y / (y - v) {
                    inside = !inside;
                }
            }
        }
        if inside || boundary {
            result.push((a, b));
        }
    }
    result
}

fn pair(a: &Patch, b: &Patch, ia: usize, ib: usize, tol: f64) -> Result<Option<Vec<PlanarIntersectionSegment>>> {
    let na = a.plane.normal();
    let nb = b.plane.normal();
    let cross = na.cross(nb);
    let sin = cross.len();
    let d = (b.plane.origin - a.plane.origin).dot(nb);
    if sin == 0.0 {
        return if d.abs() > tol { Ok(Some(Vec::new())) } else { Ok(None) };
    }
    // Do not call almost parallel planes disjoint (or intersecting) using an
    // ill-conditioned division. Report unsupported instead.
    if sin < 1e-8 {
        return Ok(None);
    }
    let dir = cross / sin;
    let in_a = dir.cross(na);
    let den = in_a.dot(nb);
    let origin = a.plane.origin + in_a * (d / den);
    if !origin.is_finite() {
        return Ok(None);
    }
    let aa = intervals(a, origin, dir, tol);
    let bb = intervals(b, origin, dir, tol);
    let mut out = Vec::new();
    for &(l, r) in &aa {
        for &(s, t) in &bb {
            let lo = l.max(s);
            let hi = r.min(t);
            if hi - lo <= tol {
                continue; // This curve-only API does not report isolated contacts.
            }
            let start = origin + dir * lo;
            let end = origin + dir * hi;
            let residual = [start, end].into_iter().map(|p| a.plane.height(p).abs().max(b.plane.height(p).abs())).fold(0.0, f64::max);
            if !start.is_finite() || !end.is_finite() || residual > tol {
                return Err(KernelError::Failed("planar intersection residual exceeds tolerance".into()));
            }
            out.push(PlanarIntersectionSegment { face_a: ia, face_b: ib, start, end, max_plane_residual: residual });
        }
    }
    Ok(Some(out))
}

/// Direct line-segment intersections for planar faces with valid polygonal trims.
/// Some(empty) means no positive-length segment in this supported domain, not
/// absence of isolated point contacts. None means unsupported/indeterminate;
/// callers must not interpret it as disjoint. Face IDs are snapshot indices.
/// Floating point, tolerance-based; not a certified exact-arithmetic predicate.
pub fn planar_intersection_segments(
    a: &Body, b: &Body, tol: f64, stop: &dyn Fn() -> bool,
) -> Result<Option<Vec<PlanarIntersectionSegment>>> {
    if !tol.is_finite() || tol <= 0.0 {
        return Err(KernelError::Invalid("invalid intersection tolerance".into()));
    }
    guard("planar intersection", || {
        let (Some(a), Some(b)) = (patches(a, tol, stop)?, patches(b, tol, stop)?) else {
            return Ok(None);
        };
        let mut out = Vec::new();
        for (ia, pa) in a.iter().enumerate() {
            for (ib, pb) in b.iter().enumerate() {
                if stop() {
                    return Err(KernelError::Failed("intersection cancelled".into()));
                }
                let Some(segments) = pair(pa, pb, ia, ib, tol)? else {
                    return Ok(None);
                };
                out.extend(segments);
                if out.len() > 20_000 {
                    return Err(KernelError::TooLarge("too many intersection segments".into()));
                }
            }
        }
        Ok(Some(out))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xy_rect(x: f64, y: f64) -> Vec<Vec3> {
        vec![Vec3::new(-x, -y, 0.0), Vec3::new(x, -y, 0.0),
             Vec3::new(x, y, 0.0), Vec3::new(-x, y, 0.0)]
    }

    #[test]
    fn finite_open_planes_and_hole_yield_two_segments() -> Result<()> {
        let a = Patch { plane: Plane::XY, loops: vec![xy_rect(4.0, 3.0), xy_rect(1.0, 1.0)] };
        let b = Patch { plane: Plane::XZ, loops: vec![vec![
            Vec3::new(-2.0, 0.0, -2.0), Vec3::new(3.0, 0.0, -2.0),
            Vec3::new(3.0, 0.0, 2.0), Vec3::new(-2.0, 0.0, 2.0),
        ]] };
        let out = pair(&a, &b, 0, 0, 1e-8)?.ok_or_else(|| KernelError::Failed("expected supported pair".into()))?;
        assert_eq!(out.len(), 2);
        let mut lengths: Vec<f64> = out.iter().map(|s| s.start.dist(s.end)).collect();
        lengths.sort_by(f64::total_cmp);
        assert!((lengths[0] - 1.0).abs() < 1e-8);
        assert!((lengths[1] - 2.0).abs() < 1e-8);
        assert!(out.iter().all(|s| s.max_plane_residual < 1e-8));
        Ok(())
    }

    #[test]
    fn coincident_is_not_empty_and_parallel_separate_is_empty() -> Result<()> {
        let a = Patch { plane: Plane::XY, loops: vec![xy_rect(2.0, 2.0)] };
        assert!(pair(&a, &a, 0, 0, 1e-8)?.is_none());
        let b = Patch { plane: Plane::XY.offset(1.0), loops: vec![xy_rect(2.0, 2.0).into_iter().map(|p| p + Vec3::Z).collect()] };
        assert_eq!(pair(&a, &b, 0, 0, 1e-8)?, Some(vec![]));
        Ok(())
    }
}
