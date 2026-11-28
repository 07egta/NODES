//! Bounded column-pivoted Householder QR for the existing LM solver.
//! The rank profile is an estimate of the linearization, not a certificate of rigidity.
use super::Mat;

pub const QR_MAX_COLS: usize = 64;
pub const QR_MAX_ROWS: usize = 2048;

/// A threshold-sensitivity check on the Jacobian; larger thresholds can only reduce rank.
/// The nullity is infinitesimal, not the dimension of the nonlinear solution set.
#[derive(Clone, Debug, PartialEq)]
pub struct RankProfile {
    pub variables: usize,
    pub equations: usize,
    pub rank: usize,
    pub rank_high_threshold: usize,
    pub rank_low_threshold: usize,
    pub pivot_ratios: Vec<f64>,
}

impl RankProfile {
    pub fn infinitesimal_nullity(&self) -> usize {
        self.variables.saturating_sub(self.rank)
    }
    pub fn threshold_sensitive(&self) -> bool {
        self.rank_high_threshold != self.rank_low_threshold
    }
}

fn valid(a: &Mat) -> bool {
    a.rows.checked_mul(a.cols) == Some(a.data.len()) && a.data.iter().all(|x| x.is_finite())
}

/// Factor a globally scaled matrix in place, using freshly computed column norms.
/// If supplied, apply the same reflectors to the right hand side.
fn factor(a: &mut Mat, mut rhs: Option<&mut [f64]>) -> Option<(Vec<usize>, Vec<f64>)> {
    if !valid(a) || rhs.as_ref().is_some_and(|b| b.len() != a.rows || b.iter().any(|x| !x.is_finite())) {
        return None;
    }
    let mut permutation: Vec<usize> = (0..a.cols).collect();
    let mut diagonal = Vec::new();
    let scale = a.data.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    if scale == 0.0 {
        return Some((permutation, diagonal));
    }
    for v in &mut a.data {
        *v /= scale;
    }
    if let Some(b) = rhs.as_deref_mut() {
        for v in b {
            *v /= scale;
            if !v.is_finite() {
                return None;
            }
        }
    }
    for k in 0..a.rows.min(a.cols) {
        let norm = |c| (k..a.rows).fold(0.0_f64, |n, r| n.hypot(a.get(r, c)));
        let mut pivot = k;
        let mut length = norm(k);
        for c in k + 1..a.cols {
            let nc = norm(c);
            if nc > length {
                pivot = c;
                length = nc;
            }
        }
        if length == 0.0 {
            break;
        }
        if pivot != k {
            for r in 0..a.rows {
                a.data.swap(r * a.cols + k, r * a.cols + pivot);
            }
            permutation.swap(k, pivot);
        }
        let sign = if a.get(k, k) >= 0.0 { 1.0 } else { -1.0 };
        let mut v: Vec<f64> = (k..a.rows).map(|r| a.get(r, k) / length).collect();
        v[0] += sign;
        let vn = v.iter().fold(0.0_f64, |n, x| n.hypot(*x));
        for x in &mut v {
            *x /= vn;
        }
        for c in k + 1..a.cols {
            let dot = v.iter().enumerate().map(|(i, x)| x * a.get(k + i, c)).sum::<f64>();
            for (i, x) in v.iter().enumerate() {
                a.add(k + i, c, -2.0 * dot * x);
            }
        }
        if let Some(b) = rhs.as_deref_mut() {
            let dot = v.iter().enumerate().map(|(i, x)| x * b[k + i]).sum::<f64>();
            for (i, x) in v.iter().enumerate() {
                b[k + i] -= 2.0 * dot * x;
            }
        }
        a.set(k, k, -sign * length);
        for r in k + 1..a.rows {
            a.set(r, k, 0.0);
        }
        diagonal.push(length);
    }
    if !valid(a) || rhs.as_ref().is_some_and(|b| b.iter().any(|x| !x.is_finite())) {
        return None;
    }
    Some((permutation, diagonal))
}

/// Rank estimated from pivot magnitudes relative to the largest pivot, not singular values.
/// None means outside the bounded diagnostic scope, invalid tolerance, or non-finite input.
pub fn rank_profile(j: &Mat, tolerance: f64) -> Option<RankProfile> {
    if j.cols > QR_MAX_COLS || j.rows > QR_MAX_ROWS || !(0.0 < tolerance && tolerance < 0.1) {
        return None;
    }
    let (_, diagonal) = factor(&mut j.clone(), None)?;
    let pivot_ratios = diagonal.first().map_or_else(Vec::new, |first| diagonal.iter().map(|d| d / first).collect());
    let rank_at = |t| pivot_ratios.iter().take_while(|d| **d > t).count();
    Some(RankProfile {
        variables: j.cols,
        equations: j.rows,
        rank: rank_at(tolerance),
        rank_high_threshold: rank_at(10.0 * tolerance),
        rank_low_threshold: rank_at(0.1 * tolerance),
        pivot_ratios,
    })
}

/// Solve min ||J step + r||² + sum((lambda*w + 1e-12)*step²).
/// QR on the augmented least-squares matrix avoids squaring J's condition number.
pub(crate) fn damped_step(j: &Mat, r: &[f64], weight: &[f64], lambda: f64) -> Option<Vec<f64>> {
    let n = j.cols;
    if n > QR_MAX_COLS || j.rows > QR_MAX_ROWS || !valid(j) || r.len() != j.rows || weight.len() != n || !lambda.is_finite() || lambda < 0.0 {
        return None;
    }
    if n == 0 {
        return Some(Vec::new());
    }
    let mut a = Mat::zeros(j.rows + n, n);
    a.data[..j.data.len()].copy_from_slice(&j.data);
    let mut rhs = vec![0.0; a.rows];
    for (b, ri) in rhs.iter_mut().zip(r) {
        *b = -ri;
    }
    for (i, w) in weight.iter().enumerate() {
        if !w.is_finite() || *w <= 0.0 {
            return None;
        }
        a.set(j.rows + i, i, (lambda * w + 1e-12).sqrt());
    }
    let (permutation, diagonal) = factor(&mut a, Some(&mut rhs))?;
    if diagonal.len() != n {
        return None;
    }
    let cutoff = 16.0 * f64::EPSILON * a.rows.max(n) as f64 * diagonal[0];
    if diagonal.iter().any(|d| *d <= cutoff) {
        return None;
    }
    let mut pivoted = vec![0.0; n];
    for k in (0..n).rev() {
        let tail = (k + 1..n).map(|c| a.get(k, c) * pivoted[c]).sum::<f64>();
        pivoted[k] = (rhs[k] - tail) / a.get(k, k);
    }
    let mut step = vec![0.0; n];
    for (i, p) in permutation.into_iter().enumerate() {
        step[p] = pivoted[i];
    }
    step.iter().all(|v| v.is_finite()).then_some(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_is_scale_invariant_and_flags_near_threshold() {
        for scale in [1e-120, 1.0, 1e120] {
            let j = Mat { rows: 2, cols: 2, data: vec![scale, 0.0, 0.0, scale * 2e-7] };
            let Some(p) = rank_profile(&j, 1e-7) else {
                assert!(false, "missing rank profile");
                return;
            };
            assert_eq!((p.rank_high_threshold, p.rank, p.rank_low_threshold), (1, 2, 2));
            assert!(p.threshold_sensitive());
        }
    }

    #[test]
    fn underdetermined_minimum_displacement() {
        let j = Mat { rows: 1, cols: 2, data: vec![1.0, 1.0] };
        let Some(x) = damped_step(&j, &[-2.0], &[1.0, 1.0], 1e-6) else {
            assert!(false, "missing step");
            return;
        };
        let expected = 2.0 / (2.0 + 1e-6 + 1e-12);
        assert!(x.iter().all(|v| (v - expected).abs() < 1e-9));
    }

    #[test]
    fn column_pivoting_restores_original_variable_order() {
        let j = Mat { rows: 2, cols: 2, data: vec![0.01, 0.0, 0.0, 100.0] };
        let Some(x) = damped_step(&j, &[-0.02, 300.0], &[1.0, 1.0], 1e-12) else {
            assert!(false, "missing step");
            return;
        };
        assert!((x[0] - 2.0).abs() < 1e-6);
        assert!((x[1] + 3.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_invalid_and_handles_empty() {
        let bad = Mat { rows: 1, cols: 1, data: vec![f64::NAN] };
        assert!(rank_profile(&bad, 1e-7).is_none());
        assert!(damped_step(&bad, &[1.0], &[1.0], 1e-3).is_none());
        let zero = Mat::zeros(0, 3);
        assert_eq!(rank_profile(&zero, 1e-7).map(|p| p.infinitesimal_nullity()), Some(3));
        assert!(rank_profile(&Mat::zeros(1, QR_MAX_COLS + 1), 1e-7).is_none());
    }

    #[test]
    fn tangent_circles_have_infinitesimal_but_not_finite_motion() {
        // Constraints: distance from (-1,0) and (1,0) equals 1.
        // Unique intersection (0,0); Jacobian rows [1,0], [-1,0].
        let j = Mat { rows: 2, cols: 2, data: vec![1.0, 0.0, -1.0, 0.0] };
        assert_eq!(rank_profile(&j, 1e-7).map(|p| p.infinitesimal_nullity()), Some(1));
        let y = 1e-3_f64;
        assert!(1.0_f64.hypot(y) - 1.0 > 0.0);
    }
}
