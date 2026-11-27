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
        let mean = q11 / 2. + q22 / 2.;
        let spread = (q11 / 2. - q22 / 2.).hypot(q12);
        let gaussian = q11.mul_add(q22, -q12 * q12);
        let principal = [mean - spread, mean + spread];
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
