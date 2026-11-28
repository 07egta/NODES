//! Differential geometry from an exact surface's first and second derivatives.
//! Uses the orientation Su × Sv and II=(n·Suu, n·Suv, n·Svv).
//! Thus an outward-oriented sphere has negative mean curvature (-1/r).
use crate::surface_kernel::{Point3, Sample};

type R<T> = Result<T, String>;
fn dot(a: Point3, b: Point3) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn norm(v: Point3) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}
fn cross(a: Point3, b: Point3) -> Point3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// Half-sum without overflow, and without prematurely halving subnormals.
fn half_sum(a: f64, b: f64) -> f64 {
    let sum = a + b;
    if sum.is_finite() {
        sum / 2.
    } else {
        a / 2. + b / 2.
    }
}

// Compensate the rounding of the second product (the same identity used
// for cross products in surface_kernel). Call with a finite second product.
fn difference_of_products(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let product = c * d;
    a.mul_add(b, -product) + (-c).mul_add(d, product)
}

// Eigenvalues of the symmetric shape operator [[a,b],[b,d]].
// The smaller-magnitude eigenvalue must not be obtained by subtracting two
// nearly equal numbers: lambda_small * lambda_large = determinant.
fn shape_eigenvalues(a: f64, b: f64, d: f64) -> (f64, f64, [f64; 2]) {
    let mean = half_sum(a, d);
    if b == 0. {
        return (a * d, mean, [a.min(d), a.max(d)]);
    }

    // Use exact binary scaling when b*b would overflow, or the whole matrix
    // is very small. This retains a useful determinant for the eigenvalue
    // calculation even when the unscaled Gaussian curvature underflows.
    let max_abs = a.abs().max(b.abs()).max(d.abs());
    let scale = if (b * b).is_infinite() {
        2_f64.powi(-512)
    } else if max_abs < 2_f64.powi(-256) {
        2_f64.powi(512)
    } else {
        1.
    };
    let (x, y, z) = (a * scale, b * scale, d * scale);
    let det = difference_of_products(x, z, y, y);
    let gaussian = det / scale / scale;

    let spread = half_sum(a, -d).hypot(b);
    let dominant = mean + spread.copysign(mean);
    // With binary64 products of up to 106 significant bits, cancellation
    // of a square this far from underflow cannot round a nonzero exact
    // determinant to zero. Preserve exact rank-one matrices in that case.
    let square = y * y;
    let small = if det.is_normal()
        || (det == 0. && square >= f64::MIN_POSITIVE * 2_f64.powi(106))
    {
        (det / (dominant * scale)) / scale
    } else {
        // When even the scaled determinant is subnormal, divide before
        // multiplying (DLAE2-style). A representable principal curvature
        // must not disappear just because their product underflows.
        let (hi, lo) = if a.abs() >= d.abs() { (a, d) } else { (d, a) };
        difference_of_products(hi / dominant, lo, b / dominant, b)
    };

    let principal = if dominant < small {
        [dominant, small]
    } else {
        [small, dominant]
    };
    (gaussian, mean, principal)
}

/// Derivatives are with respect to the same normalized UV parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceJet2 {
    pub first: Sample,
    pub duu: Point3,
    pub duv: Point3,
    pub dvv: Point3,
}
impl SurfaceJet2 {
    pub(crate) fn checked(self) -> R<Self> {
        if [
            self.first.point,
            self.first.du,
            self.first.dv,
            self.duu,
            self.duv,
            self.dvv,
        ]
        .iter()
        .flatten()
        .any(|x| !x.is_finite())
        {
            Err("曲面の二階微分が有限値になりません。".into())
        } else {
            Ok(self)
        }
    }

    /// Fundamental forms expressed in an orthonormal tangent basis. Scaling the
    /// parameter directions first avoids overflow in E*G and its cancellation
    /// against F*F. The symmetric shape operator has real eigenvalues by construction.
    pub fn curvature(self) -> R<Curvature> {
        self.checked()?;
        let a = norm(self.first.du);
        let b = norm(self.first.dv);
        if a == 0. || b == 0. || !a.is_finite() || !b.is_finite() {
            return Err("曲面の特異点では曲率を定義できません。".into());
        }
        let u = self.first.du.map(|x| x / a);
        let v = self.first.dv.map(|x| x / b);
        let cross = cross(u, v);
        let sine = norm(cross);
        if !sine.is_finite() || sine <= 1e-10 {
            return Err("曲面の接線が平行または悪条件のため曲率を評価できません。".into());
        }
        let cosine = dot(u, v).clamp(-1., 1.);
        let normal = cross.map(|x| x / sine);
        let e = dot(normal, self.duu) / a / a;
        let f = dot(normal, self.duv) / a / b;
        let g = dot(normal, self.dvv) / b / b;
        let q11 = e;
        let q12 = (f - cosine * e) / sine;
        let q22 = (g - 2. * cosine * f + cosine * cosine * e) / sine / sine;
        if [q11, q12, q22].iter().any(|x| !x.is_finite()) {
            return Err("曲率が有限値になりません。寸法または接線を確認してください。".into());
        }
        let (gaussian, mean, principal) = shape_eigenvalues(q11, q12, q22);
        if [gaussian, mean, principal[0], principal[1]]
            .iter()
            .any(|x| !x.is_finite())
        {
            return Err("曲率が有限値になりません。寸法または接線を確認してください。".into());
        }
        Ok(Curvature {
            normal,
            gaussian,
            mean,
            principal,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curvature {
    pub normal: Point3,
    pub gaussian: f64,
    pub mean: f64,
    /// Signed principal curvatures, sorted in ascending order.
    pub principal: [f64; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    // At the origin of z=(a*u²+2*b*u*v+d*v²)/2, the first fundamental
    // form is identity and the shape operator is the Hessian [[a,b],[b,d]].
    // Expected values below are analytic or independently derived.
    fn quadratic(a: f64, b: f64, d: f64) -> Curvature {
        SurfaceJet2 {
            first: Sample {
                point: [0.; 3],
                du: [1., 0., 0.],
                dv: [0., 1., 0.],
            },
            duu: [0., 0., a],
            duv: [0., 0., b],
            dvv: [0., 0., d],
        }
        .curvature()
        .unwrap()
    }

    fn relative(actual: f64, expected: f64) {
        assert!(
            (actual / expected - 1.).abs() <= 8. * f64::EPSILON,
            "{actual:e} != {expected:e}"
        );
    }

    #[test]
    fn principal_curvatures_preserve_disparate_diagonal_entries() {
        for (a, expected) in [(1e16, [1., 1e16]), (-1e16, [-1e16, 1.])] {
            let c = quadratic(a, 0., 1.);
            assert_eq!(c.principal, expected);
            assert_eq!(c.gaussian, a);
        }
        let c = quadratic(1e16, 1., 1.);
        // The smaller root lies between 1-2e-16 and 1.
        relative(c.principal[0], 1.);
        relative(c.principal[1], 1e16);
    }

    #[test]
    fn exact_rank_one_hessian_has_zero_gaussian_curvature() {
        for a in [1.1, -1.1, 1e200, -1e200, 1e-160] {
            let c = quadratic(a, a, a);
            assert_eq!(c.gaussian, 0.);
            assert_eq!(c.principal, [0_f64.min(2. * a), 0_f64.max(2. * a)]);
            assert_eq!(c.mean, a);
        }
    }

    #[test]
    fn underflowing_gaussian_does_not_erase_principal_curvatures() {
        let c = quadratic(1e-100, 1e-180, 1e-250);
        assert_eq!(c.gaussian, 0.);
        // Small root = 1e-250 - 1e-260 to the precision tested;
        // the correction from the exact larger root is below one ULP.
        relative(c.principal[0], 9.999999999e-251);
        relative(c.principal[1], 1e-100);

        let c = quadratic(1e-200, 1e-200, 2e-200);
        let root5 = 5_f64.sqrt();
        assert_eq!(c.gaussian, 0.);
        relative(c.principal[0], (3. - root5) / 2. * 1e-200);
        relative(c.principal[1], (3. + root5) / 2. * 1e-200);
    }

    #[test]
    fn mixed_sign_hessian_matches_analytic_eigenvalues() {
        let c = quadratic(3., 2., 1.);
        assert_eq!(c.gaussian, -1.);
        assert_eq!(c.mean, 2.);
        relative(c.principal[0], 2. - 5_f64.sqrt());
        relative(c.principal[1], 2. + 5_f64.sqrt());
    }
}
