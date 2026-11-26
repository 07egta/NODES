//! Exact finite binary64 affine interpolation, with one final rounding.
//!
//! Finite inputs are integers times 2^-1074, so their products are integers
//! times 2^-2148. Three signed terms need fewer than 4198 bits, including carry;
//! two 68-limb magnitudes cover every input with fixed stack storage. No
//! intermediate floating-point product, difference or sum can overflow.

use std::cmp::Ordering;

const LIMBS: usize = 68;
const UNIT_SHIFT: usize = 1074;
const FRACTION: u64 = (1_u64 << 52) - 1;
type Magnitude = [u64; LIMBS];

/// x = (-1)^negative * significand * 2^(shift-1074).
fn parts(x: f64) -> (u64, usize, bool) {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as usize;
    let significand = (bits & FRACTION) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    (significand, exponent.saturating_sub(1), bits >> 63 != 0)
}

fn add_word(words: &mut Magnitude, significand: u64, shift: usize) {
    let mut index = shift / 64;
    let mut carry = (significand as u128) << (shift % 64);
    while carry != 0 {
        let total = words[index] as u128 + (carry as u64) as u128;
        words[index] = total as u64;
        carry = (carry >> 64) + (total >> 64);
        index += 1;
    }
}

fn add_term(words: &mut Magnitude, significand: u128, shift: usize) {
    // A 106-bit product shifted by up to 63 bits would not fit in u128.
    // Insert its two words separately, propagating carry only where needed.
    add_word(words, significand as u64, shift);
    add_word(words, (significand >> 64) as u64, shift + 64);
}

fn subtract(larger: &mut Magnitude, smaller: &Magnitude) {
    let mut borrow = false;
    for (word, other) in larger.iter_mut().zip(smaller) {
        let (value, first) = word.overflowing_sub(*other);
        let (value, second) = value.overflowing_sub(u64::from(borrow));
        *word = value;
        borrow = first || second;
    }
    debug_assert!(!borrow);
}

fn bit(words: &Magnitude, position: usize) -> bool {
    words[position / 64] & (1_u64 << (position % 64)) != 0
}

fn lower_bits(words: &Magnitude, limit: usize) -> bool {
    let whole = limit / 64;
    let partial = limit % 64;
    words[..whole].iter().any(|word| *word != 0)
        || (partial != 0 && words[whole] & ((1_u64 << partial) - 1) != 0)
}

fn rounded(words: &Magnitude, negative: bool) -> f64 {
    let sign = u64::from(negative) << 63;
    let bits = words
        .iter()
        .rposition(|word| *word != 0)
        .map_or(0, |i| i * 64 + 64 - words[i].leading_zeros() as usize);
    let mut shift = UNIT_SHIFT.max(bits.saturating_sub(53));
    let word = shift / 64;
    let offset = shift % 64;
    let mut mantissa = words[word] >> offset;
    if offset != 0 {
        mantissa |= words.get(word + 1).copied().unwrap_or(0) << (64 - offset);
    }
    debug_assert!(mantissa < 1_u64 << 53);
    let guard = bit(words, shift - 1);
    let sticky = lower_bits(words, shift - 1);
    if guard && (sticky || mantissa & 1 != 0) {
        mantissa += 1;
    }

    if shift == UNIT_SHIFT {
        // Subnormals and the first normal binade have the same integer ULP.
        // Here the rounded integer is the binary64 magnitude bit pattern,
        // including a carry to the next binade and signed nonzero underflow.
        return f64::from_bits(sign | mantissa);
    }
    if mantissa == 1_u64 << 53 {
        mantissa >>= 1;
        shift += 1;
    }
    let exponent = shift - UNIT_SHIFT + 1;
    if exponent >= 0x7ff {
        return f64::from_bits(sign | (0x7ff_u64 << 52));
    }
    f64::from_bits(sign | ((exponent as u64) << 52) | (mantissa & FRACTION))
}

/// Round exact a + t*b - t*a to nearest binary64, ties to even.
/// Finite endpoint/constant cases preserve their original bits. Other exact
/// cancellation yields +0; a negative nonzero value can underflow to -0.
/// Nonfinite inputs produce NaN; genuine rounded overflow produces signed Inf.
pub(crate) fn lerp(a: f64, b: f64, t: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || !t.is_finite() {
        return f64::NAN;
    }
    if t == 0. || a.to_bits() == b.to_bits() {
        return a;
    }
    if t == 1. {
        return b;
    }
    let (am, ashift, anegative) = parts(a);
    let (bm, bshift, bnegative) = parts(b);
    let (tm, tshift, tnegative) = parts(t);
    let mut positive = [0; LIMBS];
    let mut negative = [0; LIMBS];
    add_term(
        if anegative {
            &mut negative
        } else {
            &mut positive
        },
        am as u128,
        ashift + UNIT_SHIFT,
    );
    add_term(
        if tnegative ^ bnegative {
            &mut negative
        } else {
            &mut positive
        },
        tm as u128 * bm as u128,
        tshift + bshift,
    );
    add_term(
        if tnegative ^ anegative {
            &mut positive
        } else {
            &mut negative
        },
        tm as u128 * am as u128,
        tshift + ashift,
    );
    match positive.iter().rev().cmp(negative.iter().rev()) {
        Ordering::Equal => 0.,
        Ordering::Greater => {
            subtract(&mut positive, &negative);
            rounded(&positive, false)
        }
        Ordering::Less => {
            subtract(&mut negative, &positive);
            rounded(&negative, true)
        }
    }
}
