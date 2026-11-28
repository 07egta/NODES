//! Polynomial tensor-product volume deformation, independent of SubD surface cages.
//! A certificate is sufficient, not necessary. Failure to certify is not a foldover proof.

use std::array::from_fn;

pub type Point3 = [f64; 3];
pub type Matrix3 = [[f64; 3]; 3];
const MAX_DEGREE: usize = 5;
const MAX_ITERATIONS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphError {
    InvalidDegree,
    InvalidControls,
    InvalidParameter,
    InvalidTolerance,
    NumericalRange,
    Singular,
    Uncertified,
    OutsideBounds,
    NoConvergence,
    IncompatibleLattice,
    Cancelled,
    InvalidMesh,
}

#[derive(Clone, Debug)]
pub struct Volume {
    degree: [usize; 3],
    // u varies fastest, then v, then w.
    controls: Vec<Point3>,
}

#[derive(Clone, Copy, Debug)]
pub struct Certificate {
    preconditioner: Matrix3,
    /// Outward-rounded upper bound on ||I - A J||_infinity over the entire cube.
    pub contraction_upper: f64,
    /// Conservative lower bound on det(J); zero can mean underflow, not a singularity.
    pub determinant_lower: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Inverse {
    pub parameter: Point3,
    /// Numerical world-space infinity-norm residual, not an interval certificate.
    pub residual: f64,
    pub iterations: usize,
    pub near_boundary: bool,
}

#[derive(Clone, Debug)]
pub struct VolumeMap {
    source: Volume,
    target: Volume,
    source_certificate: Certificate,
    target_certificate: Certificate,
}

fn finite(p: Point3) -> bool {
    p.iter().all(|x| x.is_finite())
}
fn add(a: Point3, b: Point3) -> Point3 {
    from_fn(|i| a[i] + b[i])
}
fn sub(a: Point3, b: Point3) -> Point3 {
    from_fn(|i| a[i] - b[i])
}
fn scale(a: Point3, s: f64) -> Point3 {
    a.map(|x| x * s)
}
fn norm(a: Point3) -> f64 {
    a.iter().fold(0.0_f64, |n, x| n.max(x.abs()))
}
fn dot(a: Point3, b: Point3) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: Point3, b: Point3) -> Point3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn mv(m: Matrix3, x: Point3) -> Point3 {
    m.map(|row| dot(row, x))
}
fn transpose(m: Matrix3) -> Matrix3 {
    from_fn(|i| from_fn(|j| m[j][i]))
}
fn det(m: Matrix3) -> f64 {
    dot(m[0], cross(m[1], m[2]))
}
fn inverse_matrix(m: Matrix3) -> Option<Matrix3> {
    let s = m.iter().map(|r| norm(*r)).fold(0.0_f64, f64::max);
    if !s.is_finite() || s == 0.0 {
        return None;
    }
    let m = m.map(|r| r.map(|x| x / s));
    let d = det(m);
    if !d.is_finite() || d.abs() <= 1e-14 {
        return None;
    }
    let c = [cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1])];
    let inv = transpose(c).map(|r| r.map(|x| (x / d) / s));
    inv.iter().all(|r| finite(*r)).then_some(inv)
}
fn inside(p: Point3) -> bool {
    p.iter().all(|x| (0.0..=1.0).contains(x))
}
fn project(p: Point3) -> Point3 {
    p.map(|x| x.clamp(0.0, 1.0))
}

// Each basic IEEE-754 operation is widened by one representable number.
// Non-finite intermediates fail closed. No fast-math or FMA contraction is assumed.
#[derive(Clone, Copy, Debug)]
struct Interval {
    lo: f64,
    hi: f64,
}
impl Interval {
    fn point(x: f64) -> Self {
        Self { lo: x, hi: x }
    }
    fn whole() -> Self {
        Self {
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
        }
    }
    fn rounded(lo: f64, hi: f64) -> Self {
        if !lo.is_finite() || !hi.is_finite() {
            return Self::whole();
        }
        Self {
            lo: lo.next_down(),
            hi: hi.next_up(),
        }
    }
    fn add(self, b: Self) -> Self {
        Self::rounded(self.lo + b.lo, self.hi + b.hi)
    }
    fn sub(self, b: Self) -> Self {
        Self::rounded(self.lo - b.hi, self.hi - b.lo)
    }
    fn mul(self, b: Self) -> Self {
        let p = [
            self.lo * b.lo,
            self.lo * b.hi,
            self.hi * b.lo,
            self.hi * b.hi,
        ];
        if p.iter().any(|x| !x.is_finite()) {
            return Self::whole();
        }
        Self::rounded(
            p.iter().copied().fold(f64::INFINITY, f64::min),
            p.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    }
    fn hull(self, b: Self) -> Self {
        Self {
            lo: self.lo.min(b.lo),
            hi: self.hi.max(b.hi),
        }
    }
    fn abs_upper(self) -> f64 {
        self.lo.abs().max(self.hi.abs())
    }
}

fn interval_det(m: Matrix3) -> Interval {
    let [a, b, c] = m.map(|r| r.map(Interval::point));
    a[0].mul(b[1].mul(c[2]).sub(b[2].mul(c[1])))
        .sub(a[1].mul(b[0].mul(c[2]).sub(b[2].mul(c[0]))))
        .add(a[2].mul(b[0].mul(c[1]).sub(b[1].mul(c[0]))))
}

fn basis(n: usize, t: f64) -> [f64; MAX_DEGREE + 1] {
    // Stable Bernstein recursion, including t=0 and t=1.
    let mut b = [0.0; MAX_DEGREE + 1];
    b[0] = 1.0;
    for k in 1..=n {
        for i in (1..=k).rev() {
            b[i] = (1.0 - t) * b[i] + t * b[i - 1];
        }
        b[0] *= 1.0 - t;
    }
    b
}

impl Volume {
    pub fn new(degree: [usize; 3], controls: Vec<Point3>) -> Result<Self, MorphError> {
        if degree.iter().any(|&n| !(1..=MAX_DEGREE).contains(&n)) {
            return Err(MorphError::InvalidDegree);
        }
        if controls.len() != degree.iter().map(|n| n + 1).product::<usize>()
            || controls.iter().any(|p| !finite(*p))
        {
            return Err(MorphError::InvalidControls);
        }
        Ok(Self { degree, controls })
    }

    /// NODES box order: bottom 00,10,11,01 then corresponding top vertices.
    pub fn from_box_corners(c: [Point3; 8]) -> Result<Self, MorphError> {
        Self::new([1; 3], vec![c[0], c[1], c[3], c[2], c[4], c[5], c[7], c[6]])
    }

    pub fn controls(&self) -> &[Point3] {
        &self.controls
    }

    pub fn degree(&self) -> [usize; 3] {
        self.degree
    }

    fn index(&self, p: [usize; 3]) -> usize {
        p[0] + (self.degree[0] + 1) * (p[1] + (self.degree[1] + 1) * p[2])
    }

    fn lattice_index(&self, i: usize) -> [usize; 3] {
        let (u, v) = (self.degree[0] + 1, self.degree[1] + 1);
        [i % u, (i / u) % v, i / (u * v)]
    }

    fn difference_pair(&self, p: [usize; 3], axis: usize) -> Option<(Point3, Point3)> {
        if p[axis] == self.degree[axis] {
            return None;
        }
        let mut next = p;
        next[axis] += 1;
        Some((self.controls[self.index(p)], self.controls[self.index(next)]))
    }

    // Return the position relative to control zero, reducing translation cancellation.
    fn sample_offset(&self, t: Point3) -> Result<(Point3, Matrix3), MorphError> {
        if !inside(t) {
            return Err(MorphError::InvalidParameter);
        }
        let b: [_; 3] = from_fn(|i| basis(self.degree[i], t[i]));
        let lower: [_; 3] = from_fn(|i| basis(self.degree[i] - 1, t[i]));
        let mut p = [0.0; 3];
        let mut columns = [[0.0; 3]; 3];
        for (i, &control) in self.controls.iter().enumerate() {
            let k = self.lattice_index(i);
            let w = b[0][k[0]] * b[1][k[1]] * b[2][k[2]];
            p = add(p, scale(sub(control, self.controls[0]), w));
            for (axis, column) in columns.iter_mut().enumerate() {
                if let Some((a, c)) = self.difference_pair(k, axis) {
                    let weight: f64 = (0..3)
                        .map(|j| {
                            if j == axis {
                                lower[j][k[j]]
                            } else {
                                b[j][k[j]]
                            }
                        })
                        .product();
                    *column = add(*column, scale(sub(c, a), weight * self.degree[axis] as f64));
                }
            }
        }
        if !finite(p) || columns.iter().any(|c| !finite(*c)) {
            return Err(MorphError::NumericalRange);
        }
        Ok((p, transpose(columns)))
    }

    pub fn evaluate(&self, t: Point3) -> Result<Point3, MorphError> {
        let p = add(self.controls[0], self.sample_offset(t)?.0);
        if !finite(p) {
            return Err(MorphError::NumericalRange);
        }
        Ok(p)
    }

    pub fn jacobian(&self, t: Point3) -> Result<Matrix3, MorphError> {
        Ok(self.sample_offset(t)?.1)
    }

    pub fn jacobian_determinant(&self, t: Point3) -> Result<f64, MorphError> {
        let d = det(self.jacobian(t)?);
        if !d.is_finite() {
            return Err(MorphError::NumericalRange);
        }
        Ok(d)
    }

    /// A finite sufficient condition for global orientation-preserving injectivity.
    /// Uses the convex hull of all Bernstein derivative controls, not samples.
    pub fn certify(&self) -> Result<Certificate, MorphError> {
        let a = inverse_matrix(self.jacobian([0.5; 3])?).ok_or(MorphError::Singular)?;
        self.certify_with(a)
    }

    fn certify_with(&self, a: Matrix3) -> Result<Certificate, MorphError> {
        let da = interval_det(a);
        if !(da.lo > 0.0 && da.hi.is_finite()) {
            return Err(MorphError::Uncertified);
        }
        let mut bounds = [[None::<Interval>; 3]; 3];
        for i in 0..self.controls.len() {
            let k = self.lattice_index(i);
            for axis in 0..3 {
                let Some((p, q)) = self.difference_pair(k, axis) else {
                    continue;
                };
                let d: [Interval; 3] = from_fn(|j| {
                    Interval::point(q[j])
                        .sub(Interval::point(p[j]))
                        .mul(Interval::point(self.degree[axis] as f64))
                });
                for (row, row_bounds) in bounds.iter_mut().enumerate() {
                    let aj = a[row]
                        .iter()
                        .zip(d)
                        .fold(Interval::point(0.0), |sum, (&x, y)| {
                            sum.add(Interval::point(x).mul(y))
                        });
                    let e = Interval::point(if row == axis { 1.0 } else { 0.0 }).sub(aj);
                    row_bounds[axis] = Some(row_bounds[axis].map_or(e, |old| old.hull(e)));
                }
            }
        }
        let q = bounds.iter().fold(0.0_f64, |upper, row| {
            let row_sum = row.iter().fold(0.0_f64, |sum, v| {
                (sum + v.map_or(f64::INFINITY, Interval::abs_upper)).next_up()
            });
            upper.max(row_sum)
        });
        if !q.is_finite() || q >= 1.0 {
            return Err(MorphError::Uncertified);
        }
        let gap = Interval::point(1.0).sub(Interval::point(q));
        let cube = gap.mul(gap).mul(gap);
        let determinant_lower = (cube.lo / da.hi).next_down().max(0.0);
        Ok(Certificate {
            preconditioner: a,
            contraction_upper: q,
            determinant_lower,
        })
    }

    pub fn inverse(&self, world: Point3, tolerance: f64) -> Result<Inverse, MorphError> {
        self.inverse_with(self.certify()?, world, tolerance)
    }

    fn inverse_with(
        &self,
        certificate: Certificate,
        world: Point3,
        tolerance: f64,
    ) -> Result<Inverse, MorphError> {
        if !finite(world) {
            return Err(MorphError::InvalidParameter);
        }
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err(MorphError::InvalidTolerance);
        }
        for (axis, &x) in world.iter().enumerate() {
            let lo = self.controls.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
            let hi = self.controls.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
            if x < (lo - tolerance).next_down() || x > (hi + tolerance).next_up() {
                return Err(MorphError::OutsideBounds);
            }
        }
        let target = sub(world, self.controls[0]);
        if !finite(target) {
            return Err(MorphError::NumericalRange);
        }
        let a = certificate.preconditioner;
        let mut t = [0.5; 3];
        for iteration in 0..=MAX_ITERATIONS {
            let (p, j) = self.sample_offset(t)?;
            let residual = sub(p, target);
            let error = norm(residual);
            let preconditioned = norm(mv(a, residual));
            if finite(residual)
                && preconditioned.is_finite()
                && error <= tolerance
                && preconditioned <= 1e-10 * (1.0 - certificate.contraction_upper)
            {
                return Ok(Inverse {
                    parameter: t,
                    residual: error,
                    iterations: iteration,
                    near_boundary: t.iter().any(|x| *x <= 1e-9 || *x >= 1.0 - 1e-9),
                });
            }
            if !finite(residual) || !preconditioned.is_finite() {
                return Err(MorphError::NumericalRange);
            }
            if iteration == MAX_ITERATIONS {
                break;
            }
            // Newton is an accelerator. Always retain the projected contraction fallback.
            let fixed = project(sub(t, mv(a, residual)));
            if !finite(fixed) {
                return Err(MorphError::NumericalRange);
            }
            let mut next = fixed;
            if let Some(inv) = inverse_matrix(j) {
                let step = mv(inv, residual);
                if finite(step) {
                    let mut factor = 1.0;
                    for _ in 0..12 {
                        let trial = sub(t, scale(step, factor));
                        if inside(trial) {
                            let f = sub(self.sample_offset(trial)?.0, target);
                            // Constrain the Newton update to the contraction basin/progress.
                            let fixed_distance = norm(sub(fixed, t));
                            if norm(f) < error
                                && norm(mv(a, f)) < preconditioned
                                && norm(sub(trial, t)) <= 4.0 * fixed_distance
                            {
                                next = trial;
                                break;
                            }
                        }
                        factor *= 0.5;
                    }
                }
            }
            if next == t {
                break;
            }
            t = next;
        }
        // Could be outside the curved image, too ill-conditioned, or exhausted.
        // Never report a proven nonexistence or a unique solution from this failure.
        Err(MorphError::NoConvergence)
    }

    /// Backtrack a proposed edit with one common certificate for the whole
    /// straight-line control-net path. Does not promise the largest safe step.
    pub fn safe_step(&self, proposed: &Self) -> Result<(Self, f64), MorphError> {
        if self.degree != proposed.degree {
            return Err(MorphError::IncompatibleLattice);
        }
        let a = self.certify()?.preconditioner;
        let mut alpha = 1.0;
        for _ in 0..32 {
            let candidate = if alpha == 1.0 {
                proposed.clone()
            } else {
                let controls = self
                    .controls
                    .iter()
                    .zip(&proposed.controls)
                    .map(|(&p, &q)| add(scale(p, 1.0 - alpha), scale(q, alpha)))
                    .collect();
                Self::new(self.degree, controls)?
            };
            if candidate.certify_with(a).is_ok() {
                return Ok((candidate, alpha));
            }
            alpha *= 0.5;
        }
        Err(MorphError::Uncertified)
    }

    /// Signed integral of det(J). Tensor Gauss-Legendre order 8 integrates
    /// these polynomial determinants exactly in real arithmetic (roundoff remains).
    /// Only an injective, orientation-preserving volume gives physical volume.
    pub fn signed_volume(&self) -> Result<f64, MorphError> {
        let nodes = [
            -0.960_289_856_497_536_3,
            -0.796_666_477_413_626_7,
            -0.525_532_409_916_329,
            -0.183_434_642_495_649_8,
            0.183_434_642_495_649_8,
            0.525_532_409_916_329,
            0.796_666_477_413_626_7,
            0.960_289_856_497_536_3,
        ];
        let weights = [
            0.101_228_536_290_376_3,
            0.222_381_034_453_374_5,
            0.313_706_645_877_887_3,
            0.362_683_783_378_362,
            0.362_683_783_378_362,
            0.313_706_645_877_887_3,
            0.222_381_034_453_374_5,
            0.101_228_536_290_376_3,
        ];
        let mut volume = 0.0;
        for (&x, &wx) in nodes.iter().zip(&weights) {
            for (&y, &wy) in nodes.iter().zip(&weights) {
                for (&z, &wz) in nodes.iter().zip(&weights) {
                    let t = [(x + 1.0) * 0.5, (y + 1.0) * 0.5, (z + 1.0) * 0.5];
                    volume += wx * wy * wz * self.jacobian_determinant(t)? / 8.0;
                }
            }
        }
        if !volume.is_finite() {
            return Err(MorphError::NumericalRange);
        }
        Ok(volume)
    }
}

impl VolumeMap {
    /// Different degrees are permitted: the common parameter cube defines the correspondence.
    pub fn new(source: Volume, target: Volume) -> Result<Self, MorphError> {
        let source_certificate = source.certify()?;
        let target_certificate = target.certify()?;
        Ok(Self {
            source,
            target,
            source_certificate,
            target_certificate,
        })
    }

    pub fn certificates(&self) -> (Certificate, Certificate) {
        (self.source_certificate, self.target_certificate)
    }

    pub fn map_point(&self, p: Point3, tolerance: f64) -> Result<(Point3, Inverse), MorphError> {
        let inverse = self.source.inverse_with(self.source_certificate, p, tolerance)?;
        Ok((self.target.evaluate(inverse.parameter)?, inverse))
    }

    pub fn inverse_point(&self, p: Point3, tolerance: f64) -> Result<(Point3, Inverse), MorphError> {
        let inverse = self.target.inverse_with(self.target_certificate, p, tolerance)?;
        Ok((self.source.evaluate(inverse.parameter)?, inverse))
    }

    /// Inverse-transpose normal mapping at a known source parameter.
    pub fn map_normal(&self, t: Point3, normal: Point3) -> Result<Point3, MorphError> {
        if !finite(normal) || norm(normal) == 0.0 {
            return Err(MorphError::InvalidParameter);
        }
        let source_j = self.source.jacobian(t)?;
        let target_j = self.target.jacobian(t)?;
        let target_inv = inverse_matrix(target_j).ok_or(MorphError::Singular)?;
        let n = mv(transpose(target_inv), mv(transpose(source_j), normal));
        let size = norm(n);
        if !finite(n) || size == 0.0 {
            return Err(MorphError::NumericalRange);
        }
        let n = n.map(|x| x / size);
        Ok(scale(n, 1.0 / dot(n, n).sqrt()))
    }

    /// Reuse existing NODES mesh topology; vertex mapping only.
    /// Chordal faces are NOT a certified image of curved faces.
    /// No partial output on error/cancellation.
    pub fn map_mesh(
        &self,
        mesh: &crate::model::Mesh,
        tolerance: f64,
        cancelled: impl Fn() -> bool,
    ) -> Result<crate::model::Mesh, MorphError> {
        if mesh.triangles.iter().flatten().any(|&i| i as usize >= mesh.vertices.len()) {
            return Err(MorphError::InvalidMesh);
        }
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err(MorphError::InvalidTolerance);
        }
        let mut vertices = Vec::with_capacity(mesh.vertices.len());
        for &p in &mesh.vertices {
            if cancelled() {
                return Err(MorphError::Cancelled);
            }
            vertices.push(self.map_point(p, tolerance)?.0);
        }
        if cancelled() {
            return Err(MorphError::Cancelled);
        }
        Ok(crate::model::Mesh {
            vertices,
            triangles: mesh.triangles.clone(),
        })
    }
}
