//! Differential geometry from an exact surface's first and second derivatives.
//! Uses the orientation Su × Sv and II=(n·Suu, n·Suv, n·Svv).
//! Thus an outward-oriented sphere has negative mean curvature (-1/r).
use crate::surface_kernel::{Point3, Sample, Surface};

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
    fn operator(self) -> R<ShapeOperator> {
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
            return Err("曲率の演算が有限値になりません。".into());
        }
        Ok(ShapeOperator {
            x: u,
            y: cross(normal, u),
            normal,
            q11,
            q12,
            q22,
        })
    }

    pub fn curvature(self) -> R<Curvature> {
        let ShapeOperator {
            normal,
            q11,
            q12,
            q22,
            ..
        } = self.operator()?;
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

/// Symmetric shape operator in a right-handed, orthonormal tangent frame.
#[derive(Clone, Copy)]
struct ShapeOperator {
    x: Point3,
    y: Point3,
    normal: Point3,
    q11: f64,
    q12: f64,
    q22: f64,
}

fn distance(a: Point3, b: Point3) -> f64 {
    norm(std::array::from_fn(|i| a[i] - b[i]))
}

impl ShapeOperator {
    /// The operator embedded in world coordinates. It annihilates the normal.
    fn tensor(self, i: usize, j: usize) -> f64 {
        self.q11 * self.x[i] * self.x[j]
            + self.q12 * (self.x[i] * self.y[j] + self.y[i] * self.x[j])
            + self.q22 * self.y[i] * self.y[j]
    }
}

impl SurfaceJet2 {
    /// Unit principal directions, in ascending signed-curvature order. Signs
    /// of eigenvectors are arbitrary. At umbilics there is no unique direction.
    pub fn principal_directions(self) -> R<[Point3; 2]> {
        let q = self.operator()?;
        let delta = q.q11 / 2. - q.q22 / 2.;
        let spread = delta.hypot(q.q12);
        let scale = q.q11.abs().max(q.q22.abs()).max(q.q12.abs());
        if spread <= scale * 1e-10 || spread == 0. {
            return Err("臍点付近では主曲率方向を一意に選べません。".into());
        }
        let (s, c) = (0.5 * q.q12.atan2(delta)).sin_cos();
        Ok([
            std::array::from_fn(|i| -s * q.x[i] + c * q.y[i]),
            std::array::from_fn(|i| c * q.x[i] + s * q.y[i]),
        ])
    }

    /// Signed normal curvature at an angle in the orthonormal tangent frame
    /// (zero is the increasing-U direction).
    pub fn normal_curvature(self, angle_deg: f64) -> R<f64> {
        if !angle_deg.is_finite() {
            return Err("角度が有限値ではありません。".into());
        }
        let q = self.operator()?;
        let (s, c) = angle_deg.rem_euclid(360.).to_radians().sin_cos();
        let k = q.q11 * c * c + q.q12 * (2. * c * s) + q.q22 * s * s;
        if !k.is_finite() {
            return Err("法曲率が有限値になりません。".into());
        }
        Ok(k)
    }
}

/// Condition number of the normalized-UV Jacobian (sigma_max/sigma_min).
/// One means isotropic local stretch, not equal-area parameterization.
pub fn uv_condition(sample: Sample) -> R<f64> {
    let a = norm(sample.du);
    let b = norm(sample.dv);
    if a == 0. || b == 0. || !a.is_finite() || !b.is_finite() {
        return Err("UVの接線が特異です。".into());
    }
    let u = sample.du.map(|x| x / a);
    let v = sample.dv.map(|x| x / b);
    let sine = norm(cross(u, v));
    if sine <= 1e-10 || !sine.is_finite() {
        return Err("UVの接線が平行または悪条件です。".into());
    }
    let scale = a.max(b);
    let (a, b) = (a / scale, b / scale);
    let (aa, bb) = (a * a, b * b);
    let ab = a * b * dot(u, v).clamp(-1., 1.);
    let largest = aa / 2. + bb / 2. + (aa / 2. - bb / 2.).hypot(ab);
    let ratio = largest / (a * b * sine);
    if !ratio.is_finite() {
        return Err("UVの歪みが有限値になりません。".into());
    }
    Ok(ratio.max(1.))
}

/// Edge IDs: U=0, U=1, V=0, V=1. The parameter t runs along the edge.
fn edge_uv(edge: usize, t: f64) -> [f64; 2] {
    match edge {
        0 => [0., t],
        1 => [1., t],
        2 => [t, 0.],
        _ => [t, 1.],
    }
}

/// Canonical derivative frame (s along the seam, r from A towards B).
/// Changing edge or reversing correspondence applies the chain rule.
fn edge_derivatives(jet: SurfaceJet2, edge: usize, reverse: bool, side_a: bool) -> [Point3; 5] {
    let s = if reverse { -1. } else { 1. };
    let inward = if edge.is_multiple_of(2) { 1. } else { -1. };
    let r = if side_a { -inward } else { inward };
    let (ds, dr, dss, drr) = if edge < 2 {
        (jet.first.dv, jet.first.du, jet.dvv, jet.duu)
    } else {
        (jet.first.du, jet.first.dv, jet.duu, jet.dvv)
    };
    [
        ds.map(|x| x * s),
        dr.map(|x| x * r),
        dss,
        jet.duv.map(|x| x * s * r),
        drr,
    ]
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundaryReport {
    /// Maximum position distance, tangent-plane angle (degrees), embedded
    /// shape-operator Frobenius difference, first and second derivative jumps.
    pub errors: [f64; 5],
    /// Edge parameter on A where each maximum occurred (ties: first).
    pub worst_t: [f64; 5],
}

/// Sampled correspondence diagnostic for two untrimmed mathematical surfaces.
/// Not a certificate over a continuous interval; no projection or topology
/// inference is performed. A singular or non-C2 station returns an error.
pub fn boundary_report(
    a: (&Surface, usize),
    b: (&Surface, usize),
    reverse_b: bool,
    samples: usize,
    stop: &dyn Fn() -> bool,
) -> R<BoundaryReport> {
    if a.1 > 3 || b.1 > 3 || !(3..=257).contains(&samples) {
        return Err("境界番号は0〜3、測定点数は3〜257です。".into());
    }
    let mut report = BoundaryReport::default();
    for k in 0..samples {
        if stop() {
            return Err("境界診断がキャンセルされました。".into());
        }
        let t = k as f64 / (samples - 1) as f64;
        let [u, v] = edge_uv(a.1, t);
        let [p, q] = edge_uv(b.1, if reverse_b { 1. - t } else { t });
        let ja = a.0.derivatives2(u, v, stop)?;
        let jb = b.0.derivatives2(p, q, stop)?;
        let (oa, ob) = (ja.operator()?, jb.operator()?);
        let cosine = dot(oa.normal, ob.normal);
        let sign = if cosine < 0. { -1. } else { 1. };
        let angle = norm(cross(oa.normal, ob.normal))
            .atan2(cosine.abs())
            .to_degrees();
        let mut tensor_error = 0.0_f64;
        for i in 0..3 {
            for j in 0..3 {
                tensor_error = tensor_error.hypot(oa.tensor(i, j) - sign * ob.tensor(i, j));
            }
        }
        let da = edge_derivatives(ja, a.1, false, true);
        let db = edge_derivatives(jb, b.1, reverse_b, false);
        let first = distance(da[0], db[0]).max(distance(da[1], db[1]));
        let second = distance(da[2], db[2])
            .max(distance(da[3], db[3]))
            .max(distance(da[4], db[4]));
        let errors = [
            distance(ja.first.point, jb.first.point),
            angle,
            tensor_error,
            first,
            second,
        ];
        for (i, error) in errors.into_iter().enumerate() {
            if !error.is_finite() {
                return Err("境界診断の残差が有限値になりません。".into());
            }
            if error > report.errors[i] {
                report.errors[i] = error;
                report.worst_t[i] = t;
            }
        }
    }
    Ok(report)
}
