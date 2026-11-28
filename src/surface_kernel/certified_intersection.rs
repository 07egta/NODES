//! Conservative exclusion of intersections of two positive-weight NURBS.
//! All boxes are in native knot coordinates, NOT normalized evaluator UV.
//! This is a search cover, not a curve tracer or a proof of existence.
use super::{NurbsSurface, Point3};
use crate::robust_geometry::Interval;

type I = Interval;
type H = [I; 4];

/// Closed parameter box (u_a, v_a, u_b, v_b).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterBox {
    pub axes: [[f64; 2]; 4],
}

impl ParameterBox {
    pub fn midpoint(self) -> [f64; 4] {
        self.axes.map(|[a, b]| a + (b - a) * 0.5)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnresolvedReason {
    ParameterResolution,
    DepthLimit,
    WorkLimit,
    Cancelled,
    FloatingPointResolution,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnresolvedBox {
    pub parameters: ParameterBox,
    pub reason: UnresolvedReason,
}

/// Ordinary floating-point evaluation, NOT an existence certificate.
/// Each point is evaluated on its own surface; residual is their distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericalSample {
    pub uv_a: [f64; 2],
    pub uv_b: [f64; 2],
    pub point_a: Point3,
    pub point_b: Point3,
    pub residual: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchOptions {
    pub max_boxes: usize,
    pub max_depth: usize,
    /// Stop refining when each parameter extent is at most this fraction of
    /// its original native domain. This is NOT a geometric error tolerance.
    pub parameter_fraction: f64,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            max_boxes: 25_000,
            max_depth: 56,
            parameter_fraction: 1.0 / 128.0,
        }
    }
}

#[derive(Debug)]
pub struct SearchReport {
    pub domain: ParameterBox,
    pub visited: usize,
    pub excluded: usize,
    pub subdivided: usize,
    /// The union of these closed boxes contains every possible exact root.
    /// Pending boxes are retained even on cancellation or work exhaustion.
    pub unresolved: Vec<UnresolvedBox>,
    pub best_sample: Option<NumericalSample>,
}

impl SearchReport {
    /// A statement about exact mathematical surfaces represented by the
    /// binary64 inputs, not measurement error or a CAD contact tolerance.
    pub fn proven_disjoint(&self) -> bool {
        self.unresolved.is_empty()
    }
}

fn point(x: f64) -> I {
    I::new(x, x).expect("validated finite control data")
}

fn hull(a: I, b: I) -> I {
    let (al, ah) = a.bounds();
    let (bl, bh) = b.bounds();
    I::new(al.min(bl), ah.max(bh)).expect("finite interval hull")
}

fn clip(a: I, b: I) -> Option<I> {
    let (al, ah) = a.bounds();
    let (bl, bh) = b.bounds();
    I::new(al.max(bl), ah.min(bh))
}

/// Division by a strictly positive enclosure, with outward rounding.
/// Return None on a nonpositive or non-finite denominator or overflow.
fn divide(a: I, b: I) -> Option<I> {
    let (al, ah) = a.bounds();
    let (bl, bh) = b.bounds();
    if bl <= 0.0 {
        return None;
    }
    let q = [al / bl, al / bh, ah / bl, ah / bh];
    if q.iter().any(|x| !x.is_finite()) {
        return None;
    }
    I::new(
        q.iter().copied().fold(f64::INFINITY, f64::min).next_down(),
        q.iter().copied().fold(f64::NEG_INFINITY, f64::max).next_up(),
    )
}

/// Positive B-spline support on a nonzero closed native interval. At its
/// right endpoint use the left limit; the validated knot multiplicities
/// ensure continuity, so interior knots and both outer endpoints are covered.
fn spans(knots: &[f64], n: usize, p: usize, [lo, hi]: [f64; 2]) -> Option<(usize, usize)> {
    let first = (p..n).find(|&i| knots[i] <= lo && lo < knots[i + 1])?;
    let last = (p..n).rev().find(|&i| knots[i] < hi && hi <= knots[i + 1])?;
    (first <= last).then_some((first, last))
}

/// Homogeneous interval de Boor on a single nonempty knot span. No
/// conversion to a rounded control polygon. Convexity permits clipping each
/// interpolation to the hull of its two input enclosures.
fn de_boor(mut d: Vec<H>, knots: &[f64], p: usize, span: usize, t: I) -> Option<H> {
    let unit = I::new(0.0, 1.0)?;
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = span - p + j;
            let den = point(knots[i + p + 1 - r]).checked_sub(point(knots[i]))?;
            let alpha = clip(divide(t.checked_sub(point(knots[i]))?, den)?, unit)?;
            let left = d[j - 1];
            let right = d[j];
            for k in 0..4 {
                let delta = right[k].checked_sub(left[k])?;
                let lerp = left[k].checked_add(alpha.checked_mul(delta)?)?;
                d[j][k] = clip(lerp, hull(left[k], right[k]))?;
            }
        }
    }
    Some(d[p])
}

fn control_hull(s: &NurbsSurface, us: (usize, usize), vs: (usize, usize)) -> [I; 3] {
    let mut bounds = s.points[us.0 * s.nv + vs.0].map(point);
    for i in us.0..=us.1 {
        for j in vs.0..=vs.1 {
            for (k, x) in s.points[i * s.nv + j].iter().enumerate() {
                bounds[k] = hull(bounds[k], point(*x));
            }
        }
    }
    bounds
}

fn refine(
    s: &NurbsSurface,
    u: [f64; 2],
    v: [f64; 2],
    su: usize,
    sv: usize,
) -> Option<[I; 3]> {
    let (u, v) = (I::new(u[0], u[1])?, I::new(v[0], v[1])?);
    let mut rows = Vec::with_capacity(s.pu + 1);
    for i in su - s.pu..=su {
        let mut row = Vec::with_capacity(s.pv + 1);
        for j in sv - s.pv..=sv {
            let k = i * s.nv + j;
            let w = point(s.weights[k]);
            let [x, y, z] = s.points[k].map(point);
            row.push([x.checked_mul(w)?, y.checked_mul(w)?, z.checked_mul(w)?, w]);
        }
        rows.push(de_boor(row, &s.kv, s.pv, sv, v)?);
    }
    let h = de_boor(rows, &s.ku, s.pu, su, u)?;
    Some([divide(h[0], h[3])?, divide(h[1], h[3])?, divide(h[2], h[3])?])
}

/// Exact-real enclosure for the represented surface on a closed box.
/// If interval evaluation fails or spans several knots, retain the
/// conservative positive-weight control-point hull instead.
fn surface_bounds(s: &NurbsSurface, u: [f64; 2], v: [f64; 2]) -> [I; 3] {
    let Some((u0, u1)) = spans(&s.ku, s.nu, s.pu, u) else {
        return control_hull(s, (0, s.nu - 1), (0, s.nv - 1));
    };
    let Some((v0, v1)) = spans(&s.kv, s.nv, s.pv, v) else {
        return control_hull(s, (0, s.nu - 1), (0, s.nv - 1));
    };
    let coarse = control_hull(s, (u0 - s.pu, u1), (v0 - s.pv, v1));
    if u0 == u1 && v0 == v1
        && let Some(tight) = refine(s, u, v, u0, v0)
    {
        return std::array::from_fn(|i| clip(coarse[i], tight[i]).unwrap_or(coarse[i]));
    }
    coarse
}

fn strictly_separated(a: [I; 3], b: [I; 3]) -> bool {
    a.iter().zip(b).any(|(x, y)| {
        let (xl, xh) = x.bounds();
        let (yl, yh) = y.bounds();
        xh < yl || yh < xl
    })
}

fn record_sample(a: &NurbsSurface, b: &NurbsSurface, cell: ParameterBox, report: &mut SearchReport) {
    let [u, v, s, t] = cell.midpoint();
    let (Ok(pa), Ok(pb)) = (a.evaluate_native(u, v), b.evaluate_native(s, t)) else {
        return;
    };
    let delta = super::sub(pa.point, pb.point);
    let residual = delta[0].hypot(delta[1]).hypot(delta[2]);
    if residual.is_finite() && report.best_sample.is_none_or(|p| residual < p.residual) {
        report.best_sample = Some(NumericalSample {
            uv_a: [u, v],
            uv_b: [s, t],
            point_a: pa.point,
            point_b: pb.point,
            residual,
        });
    }
}

/// Split by largest relative extent; prefer an interior knot on that axis.
/// Children share the *same* represented split value, leaving no gap.
fn split(
    cell: ParameterBox,
    domain: ParameterBox,
    knots: [&[f64]; 4],
) -> Option<(ParameterBox, ParameterBox)> {
    let mut best = None;
    for (i, k) in knots.iter().enumerate() {
        let [lo, hi] = cell.axes[i];
        let mid = lo + (hi - lo) * 0.5;
        let at = k.iter().copied().filter(|&x| lo < x && x < hi)
            .min_by(|x, y| (x - mid).abs().total_cmp(&(y - mid).abs()))
            .unwrap_or(mid);
        if lo < at && at < hi {
            let width = (hi - lo) / (domain.axes[i][1] - domain.axes[i][0]);
            if best.is_none_or(|(_, _, w)| width > w) {
                best = Some((i, at, width));
            }
        }
    }
    let (i, at, _) = best?;
    let (mut left, mut right) = (cell, cell);
    left.axes[i][1] = at;
    right.axes[i][0] = at;
    Some((left, right))
}

/// Bounded, deterministic exclusion search. No mesh, seeds, or Newton
/// convergence are used to reject a region. The report does not classify
/// candidate topology or certify that any remaining box contains a root.
pub fn search(
    a: &NurbsSurface,
    b: &NurbsSurface,
    options: SearchOptions,
    stop: &dyn Fn() -> bool,
) -> Result<SearchReport, String> {
    if !(1..=100_000).contains(&options.max_boxes)
        || options.max_depth > 128
        || !(options.parameter_fraction > 0.0 && options.parameter_fraction <= 1.0)
    {
        return Err("Invalid certified intersection search limits".into());
    }
    let [au, av] = a.native_domain();
    let [bu, bv] = b.native_domain();
    let domain = ParameterBox { axes: [au, av, bu, bv] };
    let knots: [&[f64]; 4] = [&a.ku, &a.kv, &b.ku, &b.kv];
    let mut report = SearchReport {
        domain,
        visited: 0,
        excluded: 0,
        subdivided: 0,
        unresolved: Vec::new(),
        best_sample: None,
    };
    let mut pending = vec![(domain, 0)];
    while !pending.is_empty() {
        let interruption = if stop() {
            Some(UnresolvedReason::Cancelled)
        } else if report.visited >= options.max_boxes {
            Some(UnresolvedReason::WorkLimit)
        } else {
            None
        };
        if let Some(reason) = interruption {
            report.unresolved.extend(pending.drain(..).map(|(parameters, _)| {
                UnresolvedBox { parameters, reason }
            }));
            break;
        }
        let (cell, depth) = pending.pop().expect("nonempty frontier");
        report.visited += 1;
        let [u, v, s, t] = cell.axes;
        if strictly_separated(surface_bounds(a, u, v), surface_bounds(b, s, t)) {
            report.excluded += 1;
            continue;
        }
        if depth == 0 {
            record_sample(a, b, cell, &mut report);
        }
        let small = cell.axes.iter().zip(domain.axes).all(|([lo, hi], [a, b])| {
            (hi - lo) / (b - a) <= options.parameter_fraction
        });
        let reason = if small {
            UnresolvedReason::ParameterResolution
        } else if depth >= options.max_depth {
            UnresolvedReason::DepthLimit
        } else if let Some((left, right)) = split(cell, domain, knots) {
            report.subdivided += 1;
            pending.push((right, depth + 1));
            pending.push((left, depth + 1));
            continue;
        } else {
            UnresolvedReason::FloatingPointResolution
        };
        record_sample(a, b, cell, &mut report);
        report.unresolved.push(UnresolvedBox { parameters: cell, reason });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(z: f64) -> NurbsSurface {
        NurbsSurface::bilinear([0., 0., z], [0., 1., z], [1., 0., z], [1., 1., z]).unwrap()
    }

    // Exactly represented dyadic coefficients: z=(u-.5)^2+(v-.5)^2-r^2.
    fn bowl(r: f64) -> NurbsSurface {
        let c = [0.25, -0.25, 0.25];
        let mut p = Vec::new();
        for (i, x) in c.iter().enumerate() {
            for (j, y) in c.iter().enumerate() {
                p.push([i as f64 * 0.5, j as f64 * 0.5, x + y - r * r]);
            }
        }
        let k = vec![0., 0., 0., 1., 1., 1.];
        NurbsSurface::new(3, 3, 2, 2, k.clone(), k, p, vec![1.; 9]).unwrap()
    }

    fn run(a: &NurbsSurface, b: &NurbsSurface) -> SearchReport {
        search(a, b, SearchOptions {
            max_boxes: 12_000,
            parameter_fraction: 1.0 / 32.0,
            ..SearchOptions::default()
        }, &|| false).unwrap()
    }

    fn assert_covered(report: &SearchReport, root: [f64; 4]) {
        assert!(report.unresolved.iter().any(|b| b.parameters.axes.iter()
            .zip(root).all(|([lo, hi], x)| *lo <= x && x <= *hi)));
    }

    #[test]
    fn near_parallel_disjoint_is_proved() {
        let r = run(&plane(0.), &plane(1e-12));
        assert!(r.proven_disjoint());
        assert_eq!(r.excluded, 1);
    }

    #[test]
    fn tangency_is_not_a_false_negative() {
        let r = run(&bowl(0.), &plane(0.));
        assert_covered(&r, [0.5; 4]);
        assert!(!r.proven_disjoint());
        assert!(r.best_sample.unwrap().residual <= 1e-12);
    }

    #[test]
    fn tiny_closed_loop_has_all_four_known_roots_covered() {
        let d = 1.0 / 1024.0;
        let r = run(&bowl(d), &plane(0.));
        for (u, v) in [(0.5 + d, 0.5), (0.5 - d, 0.5), (0.5, 0.5 + d), (0.5, 0.5 - d)] {
            assert_covered(&r, [u, v, u, v]);
        }
    }

    #[test]
    fn branch_and_boundary_roots_are_retained() {
        let saddle = NurbsSurface::bilinear(
            [0., 0., 0.25], [0., 1., -0.25],
            [1., 0., -0.25], [1., 1., 0.25],
        ).unwrap();
        let r = run(&saddle, &plane(0.));
        for (u, v) in [(0.5, 0.5), (0., 0.5), (1., 0.5), (0.5, 0.), (0.5, 1.)] {
            assert_covered(&r, [u, v, u, v]);
        }
    }

    #[test]
    fn overlap_preserves_frontier_when_budget_expires() {
        let a = plane(0.);
        let r = search(&a, &a, SearchOptions { max_boxes: 1, ..Default::default() }, &|| false).unwrap();
        assert_eq!(r.visited, 1);
        assert_eq!(r.subdivided, 1);
        assert_eq!(r.unresolved.len(), 2);
        assert!(r.unresolved.iter().all(|b| b.reason == UnresolvedReason::WorkLimit));
        assert_covered(&r, [0.125, 0.875, 0.125, 0.875]);
        assert_covered(&r, [0.875, 0.125, 0.875, 0.125]);
    }

    #[test]
    fn cancel_before_search_preserves_entire_domain() {
        let a = plane(0.);
        let r = search(&a, &a, Default::default(), &|| true).unwrap();
        assert_eq!(r.visited, 0);
        assert_eq!(r.unresolved, vec![UnresolvedBox {
            parameters: r.domain, reason: UnresolvedReason::Cancelled,
        }]);
        assert!(!r.proven_disjoint());
    }

    #[test]
    fn repeated_knots_and_positive_weights_enclose_sampled_points() {
        // Degree 2 with a double inner knot, non-unit native domains and
        // varying rational weights. Include the knot from both sides.
        let mut p = Vec::new();
        for i in 0..5 {
            for j in 0..2 {
                p.push([i as f64, j as f64, ((i + j) % 2) as f64]);
            }
        }
        let s = NurbsSurface::new(
            5, 2, 2, 1,
            vec![2., 2., 2., 3., 3., 5., 5., 5.],
            vec![-2., -2., 2., 2.],
            p, vec![1., 2., 0.5, 4., 2., 1., 8., 0.25, 1., 3.],
        ).unwrap();
        for u in [[2.75, 3.], [3., 3.25], [2.75, 3.25]] {
            let v = [-0.125, 0.25];
            let bounds = surface_bounds(&s, u, v);
            for i in 0..=4 {
                for j in 0..=4 {
                    let p = s.evaluate_native(
                        u[0] + (u[1] - u[0]) * i as f64 / 4.,
                        v[0] + (v[1] - v[0]) * j as f64 / 4.,
                    ).unwrap().point;
                    for (b, x) in bounds.iter().zip(p) {
                        let (lo, hi) = b.bounds();
                        // Existing evaluator is floating-point, not an oracle.
                        assert!(lo - 1e-12 <= x && x <= hi + 1e-12);
                    }
                }
            }
        }
    }

    #[test]
    fn interval_refinement_is_tighter_than_full_control_hull() {
        let bounds = surface_bounds(&plane(0.), [0.25, 0.375], [0.25, 0.375]);
        let (lo, hi) = bounds[0].bounds();
        assert!(lo > 0.0 && hi < 1.0);
        assert!(lo <= 0.25 && hi >= 0.375);
    }

    #[test]
    #[ignore = "manual timing; no wall-clock assertions"]
    fn stress_benchmark() {
        use std::time::Instant;
        for (name, a, b) in [
            ("near_parallel", plane(0.), plane(1e-12)),
            ("tangent", bowl(0.), plane(0.)),
            ("tiny_loop", bowl(1. / 1024.), plane(0.)),
            ("coincident", plane(0.), plane(0.)),
        ] {
            let now = Instant::now();
            let r = run(&a, &b);
            println!("{name}: elapsed={:?}, visited={}, excluded={}, subdivided={}, unresolved={}, residual={:?}, disjoint={}",
                now.elapsed(), r.visited, r.excluded, r.subdivided, r.unresolved.len(),
                r.best_sample.map(|s| s.residual), r.proven_disjoint());
        }
    }
}
