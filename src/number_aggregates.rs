//! Bounded exact accumulation of finite binary64 values, rounded once.
//!
//! Every finite f64 is an integer multiple of 2^-1074. Keep positive and
//! negative magnitudes separately in little-endian limbs, then subtract them.
//! The largest input needs 2098 bits; even usize::MAX inputs on a 64-bit target
//! need fewer than 2163. Two 34-limb arrays therefore cover every valid slice
//! with fixed stack storage and no data-dependent precision limit.
//!
//! This is an independent fixed-point implementation of the superaccumulator
//! principle (Neal, https://arxiv.org/abs/1505.05571), not imported xsum code.
//! Binary layout follows https://doc.rust-lang.org/std/primitive.f64.html.

use std::cmp::Ordering;

const LIMBS: usize = 34;
const FRACTION: u64 = (1_u64 << 52) - 1;
type Magnitude = [u64; LIMBS];

fn add_shifted(words: &mut Magnitude, significand: u64, shift: usize) -> Result<(), String> {
    let mut index = shift / 64;
    let mut carry = (significand as u128) << (shift % 64);
    while carry != 0 {
        let word = words.get_mut(index).ok_or("合計の内部容量を超えました。")?;
        let total = *word as u128 + (carry as u64) as u128;
        *word = total as u64;
        carry = (carry >> 64) + (total >> 64);
        index += 1;
    }
    Ok(())
}

/// Subtract a smaller magnitude without overflowing at a borrowed u64::MAX.
fn subtract(a: &mut Magnitude, b: &Magnitude) {
    let mut borrow = false;
    for (word, other) in a.iter_mut().zip(b) {
        let (value, first) = word.overflowing_sub(*other);
        let (value, second) = value.overflowing_sub(u64::from(borrow));
        *word = value;
        borrow = first || second;
    }
    debug_assert!(!borrow);
}

/// Long division keeps the remainder, which can decide a later rounding tie.
fn divide(words: &mut Magnitude, divisor: u64) -> u64 {
    let mut remainder = 0_u128;
    for word in words.iter_mut().rev() {
        let value = (remainder << 64) | *word as u128;
        *word = (value / divisor as u128) as u64;
        remainder = value % divisor as u128;
    }
    remainder as u64
}

fn bit(words: &Magnitude, position: usize) -> bool {
    words[position / 64] & (1_u64 << (position % 64)) != 0
}

/// True if any bit strictly below `limit` is set; handles 0 and limb boundaries.
fn lower_bits(words: &Magnitude, limit: usize) -> bool {
    let whole = limit / 64;
    let partial = limit % 64;
    words[..whole].iter().any(|word| *word != 0)
        || (partial != 0 && words[whole] & ((1_u64 << partial) - 1) != 0)
}

fn rounded(words: &Magnitude, remainder: u64, divisor: u64, negative: bool) -> Result<f64, String> {
    let sign = u64::from(negative) << 63;
    let bits = words
        .iter()
        .rposition(|word| *word != 0)
        .map_or(0, |i| i * 64 + 64 - words[i].leading_zeros() as usize);

    if bits <= 53 {
        // Subnormals and the first normal binade have a one-unit ULP here.
        // The resulting integer is also its f64 magnitude bit representation,
        // including a carry into the next binade. Retain negative underflow.
        let twice = 2 * remainder as u128;
        let round_up = twice > divisor as u128 || (twice == divisor as u128 && words[0] & 1 != 0);
        return Ok(f64::from_bits(sign | (words[0] + u64::from(round_up))));
    }

    let mut shift = bits - 53;
    let word = shift / 64;
    let offset = shift % 64;
    let mut mantissa = words[word] >> offset;
    if offset != 0 {
        mantissa |= words.get(word + 1).copied().unwrap_or(0) << (64 - offset);
    }
    mantissa &= (1_u64 << 53) - 1;
    let guard = bit(words, shift - 1);
    let sticky = lower_bits(words, shift - 1) || remainder != 0;
    if guard && (sticky || mantissa & 1 != 0) {
        mantissa += 1;
        if mantissa == 1_u64 << 53 {
            mantissa >>= 1;
            shift += 1;
        }
    }
    // A mathematical total slightly above MAX can still round to MAX. Reject
    // only after rounding, when its binary64 representation would be infinite.
    let exponent = shift + 1;
    if exponent >= 0x7ff {
        return Err("合計が浮動小数点数の範囲を超えました。".into());
    }
    Ok(f64::from_bits(
        sign | ((exponent as u64) << 52) | (mantissa & FRACTION),
    ))
}

/// Return exact_sum(data) / divisor, rounded once to nearest, ties to even.
/// Use divisor=1 for a sum and divisor=data.len() for a nonempty mean.
/// Exact cancellation returns +0; a negative nonzero mean may round to -0.
pub(crate) fn sum_divided(
    data: &[f64],
    divisor: usize,
    stop: &dyn Fn() -> bool,
) -> Result<f64, String> {
    if stop() {
        return Err("計算をキャンセルしました。".into());
    }
    let divisor = u64::try_from(divisor)
        .ok()
        .filter(|n| *n != 0)
        .ok_or("集計の除数は正の整数が必要です。")?;
    let mut positive = [0; LIMBS];
    let mut negative = [0; LIMBS];
    for value in data {
        if stop() {
            return Err("計算をキャンセルしました。".into());
        }
        if !value.is_finite() {
            return Err("有限の数値だけを集計できます。".into());
        }
        let bits = value.to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as usize;
        let significand = (bits & FRACTION) | if exponent == 0 { 0 } else { 1_u64 << 52 };
        let shift = exponent.saturating_sub(1);
        add_shifted(
            if value.is_sign_negative() {
                &mut negative
            } else {
                &mut positive
            },
            significand,
            shift,
        )?;
    }
    if stop() {
        return Err("計算をキャンセルしました。".into());
    }
    let (mut magnitude, is_negative) = match positive.iter().rev().cmp(negative.iter().rev()) {
        Ordering::Equal => return Ok(0.0),
        Ordering::Greater => {
            subtract(&mut positive, &negative);
            (positive, false)
        }
        Ordering::Less => {
            subtract(&mut negative, &positive);
            (negative, true)
        }
    };
    let remainder = if divisor == 1 {
        0
    } else {
        divide(&mut magnitude, divisor)
    };
    rounded(&magnitude, remainder, divisor, is_negative)
}
