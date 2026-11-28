//! Conservative binary64 interval filters and exact planar predicate signs.
//! Certification concerns the represented inputs, not their measurement error.
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    lo: f64,
    hi: f64,
}

impl Interval {
    pub fn new(lo: f64, hi: f64) -> Option<Self> {
        (lo.is_finite() && hi.is_finite() && lo <= hi).then_some(Self { lo, hi })
    }

    pub fn bounds(self) -> (f64, f64) {
        (self.lo, self.hi)
    }

    fn point(x: f64) -> Option<Self> {
        Self::new(x, x)
    }

    fn outward(lo: f64, hi: f64) -> Option<Self> {
        Self::new(lo.next_down(), hi.next_up())
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        Self::outward(self.lo + rhs.lo, self.hi + rhs.hi)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        Self::outward(self.lo - rhs.hi, self.hi - rhs.lo)
    }

    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        let p = [
            self.lo * rhs.lo,
            self.lo * rhs.hi,
            self.hi * rhs.lo,
            self.hi * rhs.hi,
        ];
        if p.iter().any(|x| !x.is_finite()) {
            return None;
        }
        Self::outward(
            p.iter().copied().fold(f64::INFINITY, f64::min),
            p.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    fn strict_sign(self) -> Option<Ordering> {
        if self.lo > 0. {
            Some(Ordering::Greater)
        } else if self.hi < 0. {
            Some(Ordering::Less)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignMethod {
    IntervalFilter,
    ExactInteger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CertifiedSign {
    pub sign: Ordering,
    pub method: SignMethod,
}

// Finite binary64 values are integers times 2^-1074. A product occupies
// fewer than 4196 bits; six terms fit in 68 limbs including carry.
const LIMBS: usize = 68;
type Magnitude = [u64; LIMBS];

fn parts(x: f64) -> (u64, usize, bool) {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as usize;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    (mantissa, exponent.saturating_sub(1), bits >> 63 != 0)
}

fn add_word<const N: usize>(words: &mut [u64; N], word: u64, shift: usize) {
    let mut i = shift / 64;
    let mut carry = (word as u128) << (shift % 64);
    while carry != 0 {
        let sum = words[i] as u128 + (carry as u64) as u128;
        words[i] = sum as u64;
        carry = (carry >> 64) + (sum >> 64);
        i += 1;
    }
}

fn add_product(pos: &mut Magnitude, neg: &mut Magnitude, a: f64, b: f64, minus: bool) {
    let (am, ae, an) = parts(a);
    let (bm, be, bn) = parts(b);
    let p = am as u128 * bm as u128;
    let words = if an ^ bn ^ minus { neg } else { pos };
    add_word(words, p as u64, ae + be);
    add_word(words, (p >> 64) as u64, ae + be + 64);
}

fn orient_filter(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<Ordering> {
    let x = |p: [f64; 2], q: [f64; 2], i: usize| {
        Interval::point(p[i])?.checked_sub(Interval::point(q[i])?)
    };
    let left = x(b, a, 0)?.checked_mul(x(c, a, 1)?)?;
    let right = x(b, a, 1)?.checked_mul(x(c, a, 0)?)?;
    left.checked_sub(right)?.strict_sign()
}

/// Exact sign of the orientation determinant of three finite binary64 points.
/// Positive means counterclockwise. Nonfinite inputs return None.
pub fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<CertifiedSign> {
    if [a, b, c].iter().flatten().any(|x| !x.is_finite()) {
        return None;
    }
    if let Some(sign) = orient_filter(a, b, c) {
        return Some(CertifiedSign {
            sign,
            method: SignMethod::IntervalFilter,
        });
    }
    let mut pos = [0; LIMBS];
    let mut neg = [0; LIMBS];
    for (p, q) in [(a, b), (b, c), (c, a)] {
        add_product(&mut pos, &mut neg, p[0], q[1], false);
        add_product(&mut pos, &mut neg, p[1], q[0], true);
    }
    Some(CertifiedSign {
        sign: pos.iter().rev().cmp(neg.iter().rev()),
        method: SignMethod::ExactInteger,
    })
}

// Four binary64 factors occupy fewer than 8392 bits in units of
// 2^-4296. Forty-eight signed terms need at most six additional bits.
const INCIRCLE_LIMBS: usize = 132;
type CircleMagnitude = [u64; INCIRCLE_LIMBS];

fn add_product4(
    pos: &mut CircleMagnitude,
    neg: &mut CircleMagnitude,
    factors: [f64; 4],
    minus: bool,
) {
    let mut product = [1_u64, 0, 0, 0];
    let mut shift = 0;
    let mut negative = minus;
    for factor in factors {
        let (mantissa, exponent, sign) = parts(factor);
        if mantissa == 0 {
            return;
        }
        shift += exponent;
        negative ^= sign;
        let mut carry = 0_u128;
        for word in &mut product {
            let value = *word as u128 * mantissa as u128 + carry;
            *word = value as u64;
            carry = value >> 64;
        }
        // Four 53-bit mantissas fit into the 256-bit product.
        debug_assert_eq!(carry, 0);
    }
    let words = if negative { neg } else { pos };
    for (i, word) in product.into_iter().enumerate() {
        add_word(words, word, shift + i * 64);
    }
}

fn incircle_filter(a: [f64; 2], b: [f64; 2], c: [f64; 2], p: [f64; 2]) -> Option<Ordering> {
    let diff = |q: [f64; 2]| -> Option<[Interval; 2]> {
        Some([
            Interval::point(q[0])?.checked_sub(Interval::point(p[0])?)?,
            Interval::point(q[1])?.checked_sub(Interval::point(p[1])?)?,
        ])
    };
    let cross = |a: [Interval; 2], b: [Interval; 2]| {
        a[0].checked_mul(b[1])?.checked_sub(a[1].checked_mul(b[0])?)
    };
    let lift = |a: [Interval; 2]| {
        a[0].checked_mul(a[0])?.checked_add(a[1].checked_mul(a[1])?)
    };
    let (a, b, c) = (diff(a)?, diff(b)?, diff(c)?);
    let x = lift(a)?.checked_mul(cross(b, c)?)?;
    let y = lift(b)?.checked_mul(cross(c, a)?)?;
    let z = lift(c)?.checked_mul(cross(a, b)?)?;
    x.checked_add(y)?.checked_add(z)?.strict_sign()
}

/// Exact sign of the incircle determinant on finite binary64 coordinates.
/// For counterclockwise a,b,c, positive means inside and zero means cocircular.
/// Swapping two defining points reverses the sign. Nonfinite inputs return None.
/// The sign certifies the represented inputs, not uncertainty in measured data.
pub fn incircle2d(a: [f64; 2], b: [f64; 2], c: [f64; 2], p: [f64; 2]) -> Option<CertifiedSign> {
    let points = [a, b, c, p];
    if points.iter().flatten().any(|x| !x.is_finite()) {
        return None;
    }
    if let Some(sign) = incircle_filter(a, b, c, p) {
        return Some(CertifiedSign {
            sign,
            method: SignMethod::IntervalFilter,
        });
    }
    // Expand det([x, y, x*x+y*y, 1]) along the last column.
    // Each cyclic rotation of the three remaining rows has even parity.
    let mut pos = [0; INCIRCLE_LIMBS];
    let mut neg = [0; INCIRCLE_LIMBS];
    for omit in 0..4_usize {
        let rows = [
            points[(omit + 1) % 4],
            points[(omit + 2) % 4],
            points[(omit + 3) % 4],
        ];
        let minus = omit.is_multiple_of(2);
        for i in 0..3 {
            let (a, b, c) = (rows[i], rows[(i + 1) % 3], rows[(i + 2) % 3]);
            for coordinate in c {
                add_product4(
                    &mut pos,
                    &mut neg,
                    [a[0], b[1], coordinate, coordinate],
                    minus,
                );
                add_product4(
                    &mut pos,
                    &mut neg,
                    [b[0], a[1], coordinate, coordinate],
                    !minus,
                );
            }
        }
    }
    Some(CertifiedSign {
        sign: pos.iter().rev().cmp(neg.iter().rev()),
        method: SignMethod::ExactInteger,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentContact {
    Disjoint,
    ProperCrossing,
    PointContact,
    CollinearOverlap,
}

fn on_box(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    (0..2).all(|i| a[i].min(b[i]) <= p[i] && p[i] <= a[i].max(b[i]))
}

/// Classifies closed planar segments without a modelling tolerance.
/// Point segments are supported. No intersection coordinates are constructed.
pub fn segment_contact(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    d: [f64; 2],
) -> Option<SegmentContact> {
    use Ordering::{Equal, Greater, Less};
    let o = [
        orient2d(a, b, c)?.sign,
        orient2d(a, b, d)?.sign,
        orient2d(c, d, a)?.sign,
        orient2d(c, d, b)?.sign,
    ];
    let opposite = |x, y| matches!((x, y), (Less, Greater) | (Greater, Less));
    if opposite(o[0], o[1]) && opposite(o[2], o[3]) {
        return Some(SegmentContact::ProperCrossing);
    }
    if o == [Equal; 4] {
        if a == b {
            return Some(if on_box(c, d, a) {
                SegmentContact::PointContact
            } else {
                SegmentContact::Disjoint
            });
        }
        let i = usize::from(a[0] == b[0]);
        let lo = a[i].min(b[i]).max(c[i].min(d[i]));
        let hi = a[i].max(b[i]).min(c[i].max(d[i]));
        return Some(if lo < hi {
            SegmentContact::CollinearOverlap
        } else if lo == hi {
            SegmentContact::PointContact
        } else {
            SegmentContact::Disjoint
        });
    }
    Some(
        if (o[0] == Equal && on_box(a, b, c))
            || (o[1] == Equal && on_box(a, b, d))
            || (o[2] == Equal && on_box(c, d, a))
            || (o[3] == Equal && on_box(c, d, b))
        {
            SegmentContact::PointContact
        } else {
            SegmentContact::Disjoint
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_extreme_exponents() {
        let tiny = f64::from_bits(1);
        for size in [tiny, 1e-150, 1., 1e150, f64::MAX] {
            let a = [0., 0.];
            let b = [size, 0.];
            let c = [0., size];
            assert_eq!(orient2d(a, b, c).unwrap().sign, Ordering::Greater);
            assert_eq!(orient2d(a, c, b).unwrap().sign, Ordering::Less);
            assert_eq!(orient2d(a, b, b).unwrap().sign, Ordering::Equal);
        }
        assert_eq!(
            orient2d([-f64::MAX, 0.], [f64::MAX, 0.], [0., 1.])
                .unwrap()
                .sign,
            Ordering::Greater
        );
        assert!(orient2d([f64::NAN, 0.], [0., 0.], [1., 1.]).is_none());
    }

    #[test]
    fn near_collinear_cancellation() {
        let n = 134_217_728.;
        let a = [0., 0.];
        let b = [n, n - 1.];
        let c = [n + 1., n];
        assert_eq!(b[0] * c[1] - b[1] * c[0], 0.);
        assert_eq!(
            orient2d(a, b, c),
            Some(CertifiedSign {
                sign: Ordering::Greater,
                method: SignMethod::ExactInteger,
            })
        );
        assert_eq!(
            orient2d([0., 0.], [1., 0.], [0., 1.]).unwrap().method,
            SignMethod::IntervalFilter
        );
    }

    #[test]
    fn interval_contains_underflow_and_rejects_overflow() {
        let t = Interval::new(f64::from_bits(1), f64::from_bits(1)).unwrap();
        let (lo, hi) = t.checked_mul(t).unwrap().bounds();
        assert!(lo < 0. && hi > 0.);
        let m = Interval::new(f64::MAX, f64::MAX).unwrap();
        assert!(m.checked_add(m).is_none());
        assert!(Interval::new(2., 1.).is_none());
    }

    #[test]
    fn incircle_exact_cocircular_and_extreme_scales() {
        for size in [f64::from_bits(1), 1e-150, 1., 1e150, f64::MAX] {
            let a = [0., 0.];
            let b = [size, 0.];
            let c = [0., size];
            let p = [size, size];
            assert_eq!(incircle2d(a, b, c, p).unwrap().sign, Ordering::Equal);
            assert_eq!(incircle2d(a, c, b, p).unwrap().sign, Ordering::Equal);
        }
        let a = [0., 0.];
        let b = [1., 0.];
        let c = [1., 1.];
        let inside = [0., 1_f64.next_down()];
        let outside = [0., 1_f64.next_up()];
        assert_eq!(
            incircle2d(a, b, c, inside).unwrap().sign,
            Ordering::Greater
        );
        assert_eq!(incircle2d(a, b, c, outside).unwrap().sign, Ordering::Less);
        assert_eq!(incircle2d(a, c, b, inside).unwrap().sign, Ordering::Less);
        assert_eq!(
            incircle2d(a, b, c, [0.5, 0.5]).unwrap().sign,
            Ordering::Greater
        );
        assert!(incircle2d(a, b, c, [f64::NAN, 0.]).is_none());
        let m = f64::MAX;
        assert_eq!(
            incircle2d([0., 0.], [m, 0.], [0., m], [m / 2., m / 2.])
                .unwrap()
                .sign,
            Ordering::Greater
        );
    }

    #[test]
    fn segment_classification() {
        use SegmentContact::*;
        assert_eq!(
            segment_contact([0., 0.], [2., 2.], [0., 2.], [2., 0.]),
            Some(ProperCrossing)
        );
        assert_eq!(
            segment_contact([0., 0.], [2., 0.], [1., 0.], [3., 0.]),
            Some(CollinearOverlap)
        );
        assert_eq!(
            segment_contact([0., 0.], [2., 0.], [2., 0.], [3., 0.]),
            Some(PointContact)
        );
        assert_eq!(
            segment_contact([0., 0.], [0., 0.], [1., 0.], [1., 0.]),
            Some(Disjoint)
        );
        assert_eq!(
            segment_contact([0., 0.], [2., 0.], [1., 0.], [1., 0.]),
            Some(PointContact)
        );
    }
}
