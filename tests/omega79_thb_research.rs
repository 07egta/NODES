//! OMEGA 79: two-level cubic THB-spline research reference.
//! Not a runtime feature. No T-mesh, third-party code or new dependencies.
//! Construction: Giannelli, Juettler, Speleers (2012), doi:10.1016/j.cagd.2012.03.025.
//! Keep out of production until IP review and numerical/architecture gates are met.
#![allow(clippy::needless_range_loop)]

const DEGREE: usize = 3;
const CELLS: usize = 8;
type HPoint = [f64; 4];
type Column = Vec<(usize, f64)>;

fn knots(cells: usize) -> Vec<f64> {
    let mut out = vec![0.0; DEGREE + 1];
    out.extend((1..cells).map(|i| i as f64 / cells as f64));
    out.extend([1.0; DEGREE + 1]);
    out
}

// Cox-de Boor, including the closed right endpoint of a clamped space.
fn basis(t: f64, k: &[f64]) -> Vec<f64> {
    let n = k.len() - DEGREE - 1;
    if t == 1.0 {
        let mut out = vec![0.0; n];
        out[n - 1] = 1.0;
        return out;
    }
    let mut values: Vec<f64> = k
        .windows(2)
        .map(|w| if w[0] <= t && t < w[1] { 1.0 } else { 0.0 })
        .collect();
    for p in 1..=DEGREE {
        for i in 0..k.len() - p - 1 {
            let a = k[i + p] - k[i];
            let b = k[i + p + 1] - k[i + 1];
            let left = if a > 0.0 {
                (t - k[i]) / a * values[i]
            } else {
                0.0
            };
            let right = if b > 0.0 {
                (k[i + p + 1] - t) / b * values[i + 1]
            } else {
                0.0
            };
            values[i] = left + right;
        }
    }
    values.truncate(n);
    values
}

// Coefficient prolongation by repeated ordinary knot insertion. Rows are fine
// coefficients, columns are coarse coefficients. Only simple dyadic interior knots.
fn prolongation() -> Vec<Vec<f64>> {
    let mut k = knots(CELLS);
    let n = k.len() - DEGREE - 1;
    let mut rows = vec![vec![0.0; n]; n];
    for i in 0..n {
        rows[i][i] = 1.0;
    }
    for j in (1..2 * CELLS).step_by(2) {
        let t = j as f64 / (2 * CELLS) as f64;
        let span = k
            .windows(2)
            .position(|w| w[0] <= t && t < w[1])
            .expect("fixed interior knot");
        let mut next = vec![vec![0.0; n]; rows.len() + 1];
        for i in 0..next.len() {
            if i <= span - DEGREE {
                next[i].clone_from(&rows[i]);
            } else if i > span {
                next[i].clone_from(&rows[i - 1]);
            } else {
                let a = (t - k[i]) / (k[i + DEGREE] - k[i]);
                for c in 0..n {
                    next[i][c] = a * rows[i][c] + (1.0 - a) * rows[i - 1][c];
                }
            }
        }
        k.insert(span + 1, t);
        rows = next;
    }
    rows
}

fn in_region(k: &[f64], i: usize, lo: f64, hi: f64) -> bool {
    k[i] >= lo && k[i + DEGREE + 1] <= hi
}

fn tensor_basis(k: &[f64], u: f64, v: f64) -> Vec<f64> {
    let a = basis(u, k);
    let b = basis(v, k);
    a.iter()
        .flat_map(|x| b.iter().map(move |y| x * y))
        .collect()
}

fn homogeneous_sum(weights: &[f64], points: &[HPoint]) -> HPoint {
    let mut out = [0.0; 4];
    for (&w, p) in weights.iter().zip(points) {
        for d in 0..4 {
            out[d] += w * p[d];
        }
    }
    out
}

fn project(p: HPoint) -> [f64; 3] {
    [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
}

struct ResearchSurface {
    fine_knots: Vec<f64>,
    columns: Vec<Column>,
    points: Vec<HPoint>,
    coarse_count: usize,
    fine_ids: Vec<usize>,
}

impl ResearchSurface {
    // Refinement domain: one square aligned to the coarse grid. Fixed-size
    // clamped uniform spaces, two levels only. Positive bounded weights.
    fn new(points: &[HPoint], lo: usize, hi: usize) -> Option<Self> {
        let k = knots(CELLS);
        let f = knots(2 * CELLS);
        let n = k.len() - DEGREE - 1;
        let m = f.len() - DEGREE - 1;
        if lo >= hi
            || hi > CELLS
            || points.len() != n * n
            || points.iter().any(|p| {
                p.iter().any(|x| !x.is_finite() || x.abs() > 1e100)
                    || !(1e-12..=1e12).contains(&p[3])
            })
        {
            return None;
        }
        let lo = lo as f64 / CELLS as f64;
        let hi = hi as f64 / CELLS as f64;
        let fine_ids: Vec<usize> = (0..m * m)
            .filter(|&i| in_region(&f, i / m, lo, hi) && in_region(&f, i % m, lo, hi))
            .collect();
        let mut active_fine = vec![false; m * m];
        for &i in &fine_ids {
            active_fine[i] = true;
        }
        let r = prolongation();
        let mut columns = Vec::new();
        let mut coefficients = Vec::new();

        // Retain coarse basis functions not wholly contained in the region,
        // and truncate their fine expansion by deleting active fine terms.
        for i in 0..n * n {
            if in_region(&k, i / n, lo, hi) && in_region(&k, i % n, lo, hi) {
                continue;
            }
            let column = (0..m * m)
                .filter_map(|j| {
                    let w = r[j / m][i / n] * r[j % m][i % n];
                    (!active_fine[j] && w > 0.0).then_some((j, w))
                })
                .collect();
            columns.push(column);
            coefficients.push(points[i]);
        }
        let coarse_count = columns.len();

        // Active fine coefficients are the full homogeneous prolongation,
        // including contributions from retained and removed coarse functions.
        for &j in &fine_ids {
            let mut point = [0.0; 4];
            for i in 0..n * n {
                let w = r[j / m][i / n] * r[j % m][i % n];
                for d in 0..4 {
                    point[d] += w * points[i][d];
                }
            }
            columns.push(vec![(j, 1.0)]);
            coefficients.push(point);
        }
        Some(Self {
            fine_knots: f,
            columns,
            points: coefficients,
            coarse_count,
            fine_ids,
        })
    }

    fn weights(&self, u: f64, v: f64) -> Option<Vec<f64>> {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let b = tensor_basis(&self.fine_knots, u, v);
        Some(
            self.columns
                .iter()
                .map(|col| col.iter().map(|&(i, w)| b[i] * w).sum())
                .collect(),
        )
    }

    fn evaluate(&self, u: f64, v: f64) -> Option<[f64; 3]> {
        let w = self.weights(u, v)?;
        Some(project(homogeneous_sum(&w, &self.points)))
    }

    // Fine control point edit in Euclidean z; weight unchanged. Transactional
    // on failure. This is not a topology/crease or direct surface-point edit.
    fn edit_fine_z(&mut self, index: usize, dz: f64) -> bool {
        if index >= self.fine_ids.len() || !dz.is_finite() || dz.abs() > 1e80 {
            return false;
        }
        let p = &mut self.points[self.coarse_count + index];
        let new_z = p[2] + dz * p[3];
        if !new_z.is_finite() || new_z.abs() > 1e100 {
            return false;
        }
        p[2] = new_z;
        true
    }
}

fn control_net() -> Vec<HPoint> {
    let n = CELLS + DEGREE;
    (0..n * n)
        .map(|i| {
            let x = (i / n) as f64 / (n - 1) as f64;
            let y = (i % n) as f64 / (n - 1) as f64;
            let w = 1.0 + 0.25 * (3.0 * x + 2.0 * y).cos().powi(2);
            [x * w, y * w, (4.0 * x).sin() * (3.0 * y).cos() * w, w]
        })
        .collect()
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[test]
fn thb_partition_shape_and_local_edit() {
    let net = control_net();
    let k = knots(CELLS);
    let mut s = ResearchSurface::new(&net, 2, 6).unwrap();
    assert_eq!(s.coarse_count, 120);
    assert_eq!(s.fine_ids.len(), 25);
    assert_eq!(s.points.len(), 145);
    for i in 0..=64 {
        for j in 0..=64 {
            let (u, v) = (i as f64 / 64.0, j as f64 / 64.0);
            let weights = s.weights(u, v).unwrap();
            assert!(weights.iter().all(|&x| x >= 0.0));
            assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            let original = project(homogeneous_sum(&tensor_basis(&k, u, v), &net));
            assert!(distance(original, s.evaluate(u, v).unwrap()) < 1e-12);
        }
    }
    let before = s.evaluate(0.5, 0.5).unwrap();
    let outside = s.evaluate(0.1, 0.1).unwrap();
    assert!(s.edit_fine_z(12, 0.2));
    assert!(distance(before, s.evaluate(0.5, 0.5).unwrap()) > 0.01);
    assert_eq!(outside, s.evaluate(0.1, 0.1).unwrap());
    let m = 2 * CELLS + DEGREE;
    let id = s.fine_ids[12];
    let (a, b) = (id / m, id % m);
    for i in 0..=64 {
        for j in 0..=64 {
            let (u, v) = (i as f64 / 64.0, j as f64 / 64.0);
            if u < s.fine_knots[a]
                || u > s.fine_knots[a + DEGREE + 1]
                || v < s.fine_knots[b]
                || v > s.fine_knots[b + DEGREE + 1]
            {
                let original = project(homogeneous_sum(&tensor_basis(&k, u, v), &net));
                assert!(distance(original, s.evaluate(u, v).unwrap()) < 1e-12);
            }
        }
    }
}

#[test]
fn thb_affine_invariance_and_invalid_inputs() {
    let net = control_net();
    let s = ResearchSurface::new(&net, 2, 6).unwrap();
    let transformed: Vec<_> = net
        .iter()
        .map(|p| {
            [
                2.0 * p[0] + p[3],
                2.0 * p[1] - 2.0 * p[3],
                2.0 * p[2] + 0.5 * p[3],
                p[3],
            ]
        })
        .collect();
    let t = ResearchSurface::new(&transformed, 2, 6).unwrap();
    for i in 0..=16 {
        for j in 0..=16 {
            let (u, v) = (i as f64 / 16.0, j as f64 / 16.0);
            let p = s.evaluate(u, v).unwrap();
            let expected = [2.0 * p[0] + 1.0, 2.0 * p[1] - 2.0, 2.0 * p[2] + 0.5];
            assert!(distance(expected, t.evaluate(u, v).unwrap()) < 1e-12);
        }
    }
    assert!(s.evaluate(f64::NAN, 0.0).is_none());
    assert!(s.evaluate(1.1, 0.0).is_none());
    assert!(ResearchSurface::new(&net, 7, 9).is_none());
    let mut invalid = net.clone();
    invalid[0][3] = 0.0;
    assert!(ResearchSurface::new(&invalid, 2, 6).is_none());
    let mut s = s;
    let before = s.evaluate(0.5, 0.5);
    assert!(!s.edit_fine_z(99, 1.0));
    assert!(!s.edit_fine_z(12, f64::INFINITY));
    assert_eq!(before, s.evaluate(0.5, 0.5));
}

#[test]
fn thb_full_domain_and_empty_refinement() {
    let net = control_net();
    let k = knots(CELLS);
    let full = ResearchSurface::new(&net, 0, CELLS).unwrap();
    assert_eq!(full.coarse_count, 0);
    assert_eq!(full.fine_ids.len(), 361);
    let small = ResearchSurface::new(&net, 3, 4).unwrap();
    assert!(small.fine_ids.is_empty());
    assert_eq!(small.points.len(), 121);
    for (u, v) in [(0.0, 0.0), (0.33, 0.51), (1.0, 1.0)] {
        let original = project(homogeneous_sum(&tensor_basis(&k, u, v), &net));
        assert!(distance(original, full.evaluate(u, v).unwrap()) < 1e-12);
        assert!(distance(original, small.evaluate(u, v).unwrap()) < 1e-12);
    }
}
