//! Certified 3D predicates for represented finite binary64 inputs.
//! The interval filter is optional; exact dyadic integer sums settle every
//! ambiguous sign, including zero. No constructed coordinate is certified here.
use super::{CertifiedSign, Interval, SignMethod, add_word, parts};
use std::cmp::Ordering;

pub type Point3 = [f64; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightedSite {
    pub point: Point3,
    /// Power distance is squared Euclidean distance minus weight.
    /// Weight has units of length squared, not length.
    pub weight: f64,
}

impl WeightedSite {
    pub fn unweighted(point: Point3) -> Self {
        Self { point, weight: 0. }
    }

    fn finite(self) -> bool {
        self.weight.is_finite() && self.point.iter().all(|v| v.is_finite())
    }
}

// A finite f64 is an integer with fewer than 2098 bits times 2^-1074.
// Five factors need fewer than 10490 bits; at most 480 terms add <9 bits.
// 165 limbs (10560 bits) therefore cover all signs, even subnormal/max inputs.
const LIMBS_3D: usize = 165;
type Words = [u64; LIMBS_3D];

struct ExactSum {
    pos: Words,
    neg: Words,
}

impl ExactSum {
    fn new() -> Self {
        Self {
            pos: [0; LIMBS_3D],
            neg: [0; LIMBS_3D],
        }
    }

    // All products in one sum share a common dyadic denominator. For a
    // lower-degree term (weight), shift the numerator by the missing powers.
    fn product(&mut self, factors: &[f64], mut minus: bool, extra_shift: usize) {
        let mut mantissa = [0_u64; 5];
        mantissa[0] = 1;
        let mut shift = extra_shift;
        for &factor in factors {
            let (m, e, negative) = parts(factor);
            if m == 0 {
                return;
            }
            shift += e;
            minus ^= negative;
            let mut carry = 0_u128;
            for word in &mut mantissa {
                let product = u128::from(*word) * u128::from(m) + carry;
                *word = product as u64;
                carry = product >> 64;
            }
            // Five 53-bit mantissas fit in 320 bits.
            debug_assert_eq!(carry, 0);
        }
        let words = if minus { &mut self.neg } else { &mut self.pos };
        for (i, word) in mantissa.into_iter().enumerate() {
            if word != 0 {
                add_word(words, word, shift + 64 * i);
            }
        }
    }

    fn sign(&self) -> CertifiedSign {
        CertifiedSign {
            sign: self.pos.iter().rev().cmp(self.neg.iter().rev()),
            method: SignMethod::ExactInteger,
        }
    }
}

fn visit<const N: usize>(
    row: usize,
    used: usize,
    odd: bool,
    cols: &mut [usize; N],
    f: &mut impl FnMut([usize; N], bool) -> Option<()>,
) -> Option<()> {
    if row == N {
        return f(*cols, odd);
    }
    for col in 0..N {
        let bit = 1 << col;
        if used & bit == 0 {
            cols[row] = col;
            let inversions = (used >> (col + 1)).count_ones();
            visit(row + 1, used | bit, odd ^ (inversions % 2 != 0), cols, f)?;
        }
    }
    Some(())
}

fn permutations<const N: usize>(
    mut f: impl FnMut([usize; N], bool) -> Option<()>,
) -> Option<()> {
    visit(0, 0, false, &mut [0; N], &mut f)
}

fn interval_det<const N: usize>(rows: [[Interval; N]; N]) -> Option<Interval> {
    let mut sum = Interval::point(0.)?;
    permutations::<N>(|cols, odd| {
        let mut term = Interval::point(1.)?;
        for (row, col) in rows.iter().zip(cols) {
            term = term.checked_mul(row[col])?;
        }
        sum = if odd {
            sum.checked_sub(term)?
        } else {
            sum.checked_add(term)?
        };
        Some(())
    })?;
    Some(sum)
}

fn difference(p: Point3, q: Point3) -> Option<[Interval; 3]> {
    let mut result = [Interval::point(0.)?; 3];
    for (i, entry) in result.iter_mut().enumerate() {
        *entry = Interval::point(p[i])?.checked_sub(Interval::point(q[i])?)?;
    }
    Some(result)
}

fn squared_norm(p: [Interval; 3]) -> Option<Interval> {
    let mut norm = Interval::point(0.)?;
    for x in p {
        norm = norm.checked_add(x.checked_mul(x)?)?;
    }
    Some(norm)
}

fn filtered(sign: Ordering) -> CertifiedSign {
    CertifiedSign {
        sign,
        method: SignMethod::IntervalFilter,
    }
}

/// Sign of det(b-a,c-a,d-a). Positive for (origin, +X, +Y, +Z).
/// Exact zero denotes true coplanarity (including duplicates).
pub fn orient3d(a: Point3, b: Point3, c: Point3, d: Point3) -> Option<CertifiedSign> {
    let points = [a, b, c, d];
    if points.iter().flatten().any(|x| !x.is_finite()) {
        return None;
    }
    let fast = || {
        interval_det([difference(b, a)?, difference(c, a)?, difference(d, a)?])?
            .strict_sign()
    };
    if let Some(sign) = fast() {
        return Some(filtered(sign));
    }
    // Negative of homogeneous det([x,y,z,1]); no rounded subtraction.
    let mut exact = ExactSum::new();
    permutations::<4>(|cols, odd| {
        let mut terms = [0.; 3];
        let mut n = 0;
        for (p, col) in points.iter().zip(cols) {
            if col < 3 {
                terms[n] = p[col];
                n += 1;
            }
        }
        exact.product(&terms, !odd, 0);
        Some(())
    })?;
    Some(exact.sign())
}

/// Orientation-dependent in-sphere predicate: for a positively oriented
/// tetrahedron, Greater = query strictly inside; Equal = on the sphere.
/// Reverse the interpretation for negative orientation; a coplanar tet
/// does not define a unique circumsphere.
pub fn insphere(
    a: Point3,
    b: Point3,
    c: Point3,
    d: Point3,
    query: Point3,
) -> Option<CertifiedSign> {
    weighted_insphere([a, b, c, d, query].map(WeightedSite::unweighted))
}

/// Same sign convention as insphere, with lift x²+y²+z²-weight.
/// In a regular tetrahedralization, a positive query sign for a positive
/// tet violates the empty power-sphere condition. Zero is not perturbed.
pub fn weighted_insphere(sites: [WeightedSite; 5]) -> Option<CertifiedSign> {
    if sites.iter().any(|s| !s.finite()) {
        return None;
    }
    let fast = || {
        let query = sites[4];
        let mut rows = [[Interval::point(0.)?; 4]; 4];
        for (row, site) in rows.iter_mut().zip(sites) {
            let p = difference(site.point, query.point)?;
            let w = Interval::point(site.weight)?.checked_sub(Interval::point(query.weight)?)?;
            *row = [p[0], p[1], p[2], squared_norm(p)?.checked_sub(w)?];
        }
        Some(interval_det(rows)?.strict_sign()?.reverse())
    };
    if let Some(sign) = fast() {
        return Some(filtered(sign));
    }
    // Expand homogeneous det([x,y,z,x²+y²+z²-w,1]) without ever
    // computing a rounded lift. Common denominator 2^5370.
    let mut exact = ExactSum::new();
    permutations::<5>(|cols, odd| {
        let mut terms = [0.; 5];
        let mut n = 0;
        let mut lifted = sites[0];
        for (site, col) in sites.iter().zip(cols) {
            match col {
                0..=2 => {
                    terms[n] = site.point[col];
                    n += 1;
                }
                3 => lifted = *site,
                _ => {}
            }
        }
        for x in lifted.point {
            terms[3] = x;
            terms[4] = x;
            exact.product(&terms, !odd, 0);
        }
        terms[3] = lifted.weight;
        exact.product(&terms[..4], odd, 1074);
        Some(())
    })?;
    Some(exact.sign())
}

/// Compare power distances at p exactly. Less means a is closer than b.
/// Works for coincident sites, negative weights and extreme binary64 inputs.
/// A bisector construction can use this to audit a represented vertex; it
/// does not imply that a rounded intersection lies exactly on the bisector.
pub fn power_side(p: Point3, a: WeightedSite, b: WeightedSite) -> Option<CertifiedSign> {
    if !a.finite() || !b.finite() || p.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let fast = || {
        let da = squared_norm(difference(p, a.point)?)?
            .checked_sub(Interval::point(a.weight)?)?;
        let db = squared_norm(difference(p, b.point)?)?
            .checked_sub(Interval::point(b.weight)?)?;
        da.checked_sub(db)?.strict_sign()
    };
    if let Some(sign) = fast() {
        return Some(filtered(sign));
    }
    let mut exact = ExactSum::new();
    for (x, (u, v)) in p.into_iter().zip(a.point.into_iter().zip(b.point)) {
        exact.product(&[u, u], false, 0);
        exact.product(&[v, v], true, 0);
        for _ in 0..2 {
            exact.product(&[x, u], true, 0);
            exact.product(&[x, v], false, 0);
        }
    }
    exact.product(&[a.weight], true, 1074);
    exact.product(&[b.weight], false, 1074);
    Some(exact.sign())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_and_sphere() {
        let a = [0.; 3];
        let b = [1., 0., 0.];
        let c = [0., 1., 0.];
        let d = [0., 0., 1.];
        assert_eq!(orient3d(a, b, c, d).unwrap().sign, Ordering::Greater);
        assert_eq!(orient3d(a, c, b, d).unwrap().sign, Ordering::Less);
        assert_eq!(orient3d(a, b, c, b).unwrap().sign, Ordering::Equal);
        assert_eq!(insphere(a, b, c, d, [0.5; 3]).unwrap().sign, Ordering::Greater);
        assert_eq!(insphere(a, b, c, d, [1.; 3]).unwrap().sign, Ordering::Equal);
        assert_eq!(insphere(a, b, c, d, [2.; 3]).unwrap().sign, Ordering::Less);
        assert_eq!(insphere(a, c, b, d, [0.5; 3]).unwrap().sign, Ordering::Less);
    }

    #[test]
    fn extreme_scales() {
        for s in [f64::from_bits(1), 1e-150, 1e150, f64::MAX] {
            let a = [0.; 3];
            let b = [s, 0., 0.];
            let c = [0., s, 0.];
            let d = [0., 0., s];
            assert_eq!(orient3d(a, b, c, d).unwrap().sign, Ordering::Greater);
            assert_eq!(insphere(a, b, c, d, [s; 3]).unwrap().sign, Ordering::Equal);
        }
    }

    #[test]
    fn weighted_and_invalid() {
        let a = WeightedSite::unweighted([0.; 3]);
        let b = WeightedSite { point: [2., 0., 0.], weight: 4. };
        assert_eq!(power_side([0.; 3], a, b).unwrap().sign, Ordering::Equal);
        assert_eq!(power_side([1., 0., 0.], a, b).unwrap().sign, Ordering::Greater);
        let mut sites = [[0.; 3], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [1.; 3]]
            .map(WeightedSite::unweighted);
        sites[4].weight = 1.;
        assert_eq!(weighted_insphere(sites).unwrap().sign, Ordering::Greater);
        sites[4].weight = -1.;
        assert_eq!(weighted_insphere(sites).unwrap().sign, Ordering::Less);
        sites[4].weight = f64::NAN;
        assert!(weighted_insphere(sites).is_none());
    }
}
