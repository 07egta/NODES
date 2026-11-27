//! NURBS/B-rep 幾何カーネル共通の許容誤差ポリシー（単一モジュール）。
//!
//! 方針は `docs/NURBS_ROADMAP.md` の「許容誤差ポリシー」を参照。
//! 長さは mm、角度は rad。幾何カーネルの比較はすべてこの定数・関数を通し、
//! `1e-6` などのリテラルを各所に散らさない。

/// 距離の絶対許容誤差（mm）。これ以下の距離は「同一点」とみなす。
pub const ABS_TOL: f64 = 1.0e-6;

/// 角度の許容誤差（rad）。これ以下の角度は「平行・接線連続」とみなす。
pub const ANGLE_TOL: f64 = 1.0e-9;

/// パラメータ空間の許容誤差。ノット・トリム曲線の端点一致に使う。
pub const PARAM_TOL: f64 = 1.0e-9;

/// 浮動小数点で「0 とみなす」下限。除算の分母保護に使う。
pub const ZERO_EPS: f64 = 1.0e-12;

/// 2 つの長さが許容誤差内で等しいか。
pub fn lengths_equal(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= ABS_TOL
}

/// 2 点が許容誤差内で一致するか。
pub fn points_coincide(a: [f64; 3], b: [f64; 3]) -> bool {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    if !d.iter().all(|v| v.is_finite()) {
        return false;
    }
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= ABS_TOL
}

/// 2 つのパラメータが許容誤差内で等しいか。
pub fn params_equal(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= PARAM_TOL
}

/// 2 つの単位方向ベクトルが許容誤差内で平行（同向）か。
///
/// 外積の大きさ（= sin θ）と内積の符号で判定する。零ベクトルは平行とみなさない。
pub fn directions_parallel(a: [f64; 3], b: [f64; 3]) -> bool {
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let na = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let nb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if !(na.is_finite() && nb.is_finite()) || na < ZERO_EPS || nb < ZERO_EPS {
        return false;
    }
    let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() / (na * nb);
    sin <= ANGLE_TOL && dot > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_follow_the_policy() {
        assert_eq!(ABS_TOL, 1.0e-6);
        assert_eq!(ANGLE_TOL, 1.0e-9);
        let ordered = [ABS_TOL, ANGLE_TOL, PARAM_TOL, ZERO_EPS];
        assert!(ordered.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn lengths_use_the_absolute_tolerance() {
        assert!(lengths_equal(10.0, 10.0 + 0.9e-6));
        assert!(!lengths_equal(10.0, 10.0 + 2.0e-6));
        assert!(!lengths_equal(f64::NAN, 0.0));
        assert!(!lengths_equal(f64::INFINITY, f64::INFINITY));
    }

    #[test]
    fn points_coincide_within_tolerance_only() {
        assert!(points_coincide([1.0, 2.0, 3.0], [1.0, 2.0, 3.0 + 5.0e-7]));
        assert!(!points_coincide([1.0, 2.0, 3.0], [1.0, 2.0, 3.0 + 5.0e-6]));
        assert!(!points_coincide([f64::NAN, 0.0, 0.0], [0.0, 0.0, 0.0]));
    }

    #[test]
    fn parameters_compare_with_param_tolerance() {
        assert!(params_equal(0.5, 0.5 + 5.0e-10));
        assert!(!params_equal(0.5, 0.5 + 1.0e-8));
    }

    #[test]
    fn parallel_directions_respect_the_angle_tolerance() {
        assert!(directions_parallel([1.0, 0.0, 0.0], [2.0, 0.0, 0.0]));
        assert!(directions_parallel([1.0, 0.0, 0.0], [1.0, 5.0e-10, 0.0]));
        assert!(!directions_parallel([1.0, 0.0, 0.0], [1.0, 1.0e-8, 0.0]));
        assert!(!directions_parallel([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]));
        assert!(!directions_parallel([0.0; 3], [1.0, 0.0, 0.0]));
    }
}
