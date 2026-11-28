//! Bounded, lazy BCC lattice queries. Model units are millimetres.
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy)]
pub struct GradedBcc {
    extent: [f64; 3],
    cells: [NonZeroU32; 3],
    grading: f64,
    radii: [f64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LatticeError {
    InvalidParameters,
    InvalidPoint,
    BudgetExceeded,
}

#[derive(Debug, Clone, Copy)]
pub struct DensityEstimate {
    pub relative_density: f64,
    pub material_volume_mm3: f64,
    pub samples: u64,
}

impl GradedBcc {
    /// X cell widths vary linearly by index; grading must lie in [-0.8, 0.8].
    /// Radii vary linearly in physical X. Radius is capped at half the
    /// smallest cell dimension to bound the neighbourhood of a query.
    pub fn new(
        extent: [f64; 3],
        cells: [NonZeroU32; 3],
        grading: f64,
        radii: [f64; 2],
        minimum_diameter: f64,
    ) -> Result<Self, LatticeError> {
        if extent.iter().any(|v| !v.is_finite() || !(1e-6..=1e6).contains(v))
            || cells.iter().any(|n| n.get() > 1_000_000)
            || !grading.is_finite()
            || grading.abs() > 0.8
            || !minimum_diameter.is_finite()
            || minimum_diameter <= 0.0
        {
            return Err(LatticeError::InvalidParameters);
        }
        let min_pitch = (extent[0] / f64::from(cells[0].get())
            * (1.0 - grading.abs()))
            .min(extent[1] / f64::from(cells[1].get()))
            .min(extent[2] / f64::from(cells[2].get()));
        if radii.iter().any(|r| {
            !r.is_finite() || *r < minimum_diameter * 0.5 || *r > min_pitch * 0.5
        }) {
            return Err(LatticeError::InvalidParameters);
        }
        Ok(Self {
            extent,
            cells,
            grading,
            radii,
        })
    }

    fn boundary(&self, axis: usize, index: u32) -> f64 {
        let t = f64::from(index) / f64::from(self.cells[axis].get());
        let warped = if axis == 0 {
            t + self.grading * t * (t - 1.0)
        } else {
            t
        };
        self.extent[axis] * warped
    }

    fn containing_cell(&self, axis: usize, coordinate: f64) -> u32 {
        let mut lo = 0;
        let mut hi = self.cells[axis].get();
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if self.boundary(axis, mid) <= coordinate {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    fn radius(&self, x: f64) -> f64 {
        let t = (x / self.extent[0]).clamp(0.0, 1.0);
        self.radii[0] * (1.0 - t) + self.radii[1] * t
    }

    /// A sign field, not a certified signed distance. Each beam is a
    /// capsule with query-position-dependent radius; shared corners join
    /// without duplicated meshes. The box clips all beams.
    pub fn field(&self, p: [f64; 3]) -> Result<f64, LatticeError> {
        if p.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Err(LatticeError::InvalidPoint);
        }
        let box_field = (0..3)
            .map(|a| (-p[a]).max(p[a] - self.extent[a]))
            .fold(f64::NEG_INFINITY, f64::max);
        if box_field > 0.0 {
            return Ok(box_field);
        }
        let indices: [u32; 3] = std::array::from_fn(|a| self.containing_cell(a, p[a]));
        let ranges = std::array::from_fn::<_, 3, _>(|a| {
            indices[a].saturating_sub(1)
                ..=(indices[a] + 1).min(self.cells[a].get() - 1)
        });
        let radius = self.radius(p[0]);
        let mut field = f64::INFINITY;
        for x in ranges[0].clone() {
            for y in ranges[1].clone() {
                for z in ranges[2].clone() {
                    let cell = [x, y, z];
                    let low: [f64; 3] =
                        std::array::from_fn(|a| self.boundary(a, cell[a]));
                    let high: [f64; 3] =
                        std::array::from_fn(|a| self.boundary(a, cell[a] + 1));
                    let center: [f64; 3] =
                        std::array::from_fn(|a| (low[a] + high[a]) * 0.5);
                    for bits in 0..8 {
                        let corner = std::array::from_fn(|a| {
                            if bits & (1 << a) == 0 {
                                low[a]
                            } else {
                                high[a]
                            }
                        });
                        field = field.min(segment_distance(p, center, corner) - radius);
                    }
                }
            }
        }
        Ok(field.max(box_field))
    }

    /// Deterministic midpoint quadrature. Not a certified error bound.
    /// The caller chooses sampling resolution independently of cell count.
    pub fn estimate_density(
        &self,
        resolution: NonZeroU32,
        budget: u64,
    ) -> Result<DensityEstimate, LatticeError> {
        let n = u64::from(resolution.get());
        let samples = n.checked_pow(3).ok_or(LatticeError::BudgetExceeded)?;
        if samples > budget.min(1_000_000) {
            return Err(LatticeError::BudgetExceeded);
        }
        let mut inside = 0_u64;
        for x in 0..n {
            for y in 0..n {
                for z in 0..n {
                    let ijk = [x, y, z];
                    let p = std::array::from_fn(|a| {
                        self.extent[a] * ((ijk[a] as f64 + 0.5) / n as f64)
                    });
                    if self.field(p)? <= 0.0 {
                        inside += 1;
                    }
                }
            }
        }
        let relative_density = inside as f64 / samples as f64;
        Ok(DensityEstimate {
            relative_density,
            material_volume_mm3: relative_density * self.extent.iter().product::<f64>(),
            samples,
        })
    }

    /// Sample a Z section at pixel centres. The result is occupancy, not
    /// a contour, toolpath, or printable mesh. Caller selects preview or
    /// final sampling resolution explicitly.
    pub fn section(
        &self,
        z: f64,
        resolution: [NonZeroU32; 2],
        budget: usize,
    ) -> Result<Vec<bool>, LatticeError> {
        if !z.is_finite() || z < 0.0 || z > self.extent[2] {
            return Err(LatticeError::InvalidPoint);
        }
        let [nx, ny] = resolution.map(|n| n.get() as usize);
        let len = nx.checked_mul(ny).ok_or(LatticeError::BudgetExceeded)?;
        if len > budget.min(1_000_000) {
            return Err(LatticeError::BudgetExceeded);
        }
        let mut result = Vec::with_capacity(len);
        for y in 0..ny {
            for x in 0..nx {
                let p = [
                    self.extent[0] * (x as f64 + 0.5) / nx as f64,
                    self.extent[1] * (y as f64 + 0.5) / ny as f64,
                    z,
                ];
                result.push(self.field(p)? <= 0.0);
            }
        }
        Ok(result)
    }
}

fn segment_distance(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let d: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
    let v: [f64; 3] = std::array::from_fn(|i| p[i] - a[i]);
    let length_squared: f64 = d.iter().map(|x| x * x).sum();
    let t = (d.iter().zip(v).map(|(x, y)| x * y).sum::<f64>() / length_squared)
        .clamp(0.0, 1.0);
    d.iter()
        .zip(v)
        .map(|(x, y)| (y - t * x).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    fn sample(radii: [f64; 2]) -> GradedBcc {
        GradedBcc::new([20.0; 3], [nz(4); 3], 0.4, radii, 0.4).unwrap()
    }

    #[test]
    fn boundaries_and_centres() {
        let lattice = sample([0.3, 0.8]);
        assert_eq!(lattice.boundary(0, 0), 0.0);
        assert_eq!(lattice.boundary(0, 4), 20.0);
        for x in 0..4 {
            let centre = (lattice.boundary(0, x) + lattice.boundary(0, x + 1)) * 0.5;
            assert!(lattice.field([centre, 2.5, 2.5]).unwrap() < 0.0);
            assert!(lattice.field([lattice.boundary(0, x), 5.0, 5.0]).unwrap() <= 0.0);
        }
    }

    #[test]
    fn thicker_beams_never_reduce_sampled_volume() {
        let thin = sample([0.3, 0.3]).estimate_density(nz(20), 8000).unwrap();
        let thick = sample([0.8, 0.8]).estimate_density(nz(20), 8000).unwrap();
        assert!(thick.relative_density > thin.relative_density);
        assert!((0.0..=1.0).contains(&thick.relative_density));
    }

    #[test]
    fn invalid_and_budgeted() {
        let lattice = sample([0.3, 0.8]);
        assert_eq!(lattice.field([f64::NAN, 0.0, 0.0]), Err(LatticeError::InvalidPoint));
        assert!(lattice.field([-1.0, 0.0, 0.0]).unwrap() > 0.0);
        assert!(matches!(
            lattice.estimate_density(nz(u32::MAX), u64::MAX),
            Err(LatticeError::BudgetExceeded)
        ));
        assert_eq!(lattice.section(2.5, [nz(10); 2], 99), Err(LatticeError::BudgetExceeded));
        assert_eq!(lattice.section(2.5, [nz(10); 2], 100).unwrap().len(), 100);
    }
}
