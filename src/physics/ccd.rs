//! Two-sided swept-sphere CCD against static triangle meshes.
//! A closest-point separating plane gives a conservative advancement bound.
//! This is floating-point geometry, not interval-certified or solid containment.
use crate::mesh_tools::{
    P, Stop, add, closest_on_triangle, cross, dot, len, mul, raw_check, sub, tick,
};
use crate::model::Mesh;

const MAX_FACES: usize = 250_000;
const MAX_ADVANCES: usize = 64;
const LEAF_SIZE: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impact {
    /// Fraction of the supplied linear displacement, in [0, 1].
    pub toi: f64,
    /// Two-sided normal from the nearest triangle point to the sphere centre.
    pub normal: P,
    pub triangle: usize,
}

/// Shared, monotonically consumed budget, including broadphase and narrowphase.
pub struct Budget {
    remaining: usize,
}

impl Budget {
    pub fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }

    fn charge(&mut self, stop: Stop) -> Result<(), String> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or("CCD 計算量の上限を超えました。")?;
        tick(self.remaining, stop)
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    lo: P,
    hi: P,
}

impl Bounds {
    fn of_points(points: &[P]) -> Self {
        let mut b = Self {
            lo: points[0],
            hi: points[0],
        };
        for p in points.iter().skip(1) {
            b.lo = std::array::from_fn(|k| b.lo[k].min(p[k]));
            b.hi = std::array::from_fn(|k| b.hi[k].max(p[k]));
        }
        b
    }

    fn union(self, other: Self) -> Self {
        Self {
            lo: std::array::from_fn(|k| self.lo[k].min(other.lo[k])),
            hi: std::array::from_fn(|k| self.hi[k].max(other.hi[k])),
        }
    }

    fn padded(self, pad: f64) -> Self {
        Self {
            lo: self.lo.map(|x| x - pad),
            hi: self.hi.map(|x| x + pad),
        }
    }

    fn overlaps(self, other: Self) -> bool {
        (0..3).all(|k| self.lo[k] <= other.hi[k] && other.lo[k] <= self.hi[k])
    }
}

#[derive(Clone, Copy)]
struct Triangle {
    points: [P; 3],
    bounds: Bounds,
    id: usize,
}

struct Node {
    bounds: Bounds,
    kind: NodeKind,
}

enum NodeKind {
    Leaf { first: usize, count: usize },
    Branch(Box<Node>, Box<Node>),
}

impl Node {
    fn build(triangles: &mut [Triangle], first: usize, stop: Stop) -> Result<Self, String> {
        if stop() {
            return Err("CCD の構築を中断しました。".into());
        }
        let bounds = triangles
            .iter()
            .skip(1)
            .fold(triangles[0].bounds, |b, t| b.union(t.bounds));
        let kind = if triangles.len() <= LEAF_SIZE {
            NodeKind::Leaf {
                first,
                count: triangles.len(),
            }
        } else {
            let axis = (0..3)
                .max_by(|&a, &b| {
                    (bounds.hi[a] - bounds.lo[a]).total_cmp(&(bounds.hi[b] - bounds.lo[b]))
                })
                .ok_or("CCD の軸が無効です。")?;
            let mid = triangles.len() / 2;
            triangles.select_nth_unstable_by(mid, |a, b| {
                (a.bounds.lo[axis] + a.bounds.hi[axis])
                    .total_cmp(&(b.bounds.lo[axis] + b.bounds.hi[axis]))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let (left, right) = triangles.split_at_mut(mid);
            NodeKind::Branch(
                Box::new(Self::build(left, first, stop)?),
                Box::new(Self::build(right, first + mid, stop)?),
            )
        };
        Ok(Self { bounds, kind })
    }
}

pub struct StaticMesh {
    triangles: Vec<Triangle>,
    root: Node,
}

struct Sweep {
    start: P,
    delta: P,
    radius: f64,
    tolerance: f64,
    bounds: Bounds,
}

fn valid(p: P) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
}

impl StaticMesh {
    /// Builds a reusable deterministic BVH. Degenerate triangles are reported,
    /// not silently removed. Concave and disconnected surfaces are supported.
    pub fn new(mesh: &Mesh, stop: Stop) -> Result<Self, String> {
        if mesh.triangles.is_empty() || mesh.triangles.len() > MAX_FACES {
            return Err("CCD には 1〜25 万面の衝突メッシュが必要です。".into());
        }
        raw_check(mesh, stop)?;
        let mut triangles = Vec::with_capacity(mesh.triangles.len());
        for (id, t) in mesh.triangles.iter().enumerate() {
            tick(id, stop)?;
            let points = t.map(|i| mesh.vertices[i as usize]);
            let [a, b, c] = points;
            let ab = sub(b, a);
            let ac = sub(c, a);
            let scale = len(ab).max(len(ac)).max(len(sub(c, b)));
            if scale < 1e-12 || len(cross(ab, ac)) <= scale * scale * 1e-12 {
                return Err(format!("CCD: 退化または極端に細い三角形 {id}。"));
            }
            triangles.push(Triangle {
                points,
                bounds: Bounds::of_points(&points),
                id,
            });
        }
        let root = Node::build(&mut triangles, 0, stop)?;
        Ok(Self { triangles, root })
    }

    /// Earliest conservative contact for a sphere of positive radius moving
    /// linearly from start to end. Initial penetration and unresolved numeric
    /// cases are errors, never successful misses. An already touching sphere
    /// may move tangentially or away from an individual triangle.
    pub fn sweep(
        &self,
        start: P,
        end: P,
        radius: f64,
        budget: &mut Budget,
        stop: Stop,
    ) -> Result<Option<Impact>, String> {
        if stop() {
            return Err("CCD を中断しました。".into());
        }
        if !valid(start) || !valid(end) || !(0.0 < radius && radius <= 1e9) {
            return Err("CCD の座標または半径が範囲外です。".into());
        }
        let scale = start
            .into_iter()
            .chain(end)
            .chain(self.root.bounds.lo)
            .chain(self.root.bounds.hi)
            .map(f64::abs)
            .fold(1.0, f64::max);
        let tolerance = (64.0 * f64::EPSILON * scale).max(radius * 1e-8);
        if tolerance >= radius * 0.1 {
            return Err("CCD: 座標のスケールに対して半径が小さすぎます。".into());
        }
        let sweep = Sweep {
            start,
            delta: sub(end, start),
            radius,
            tolerance,
            bounds: Bounds::of_points(&[start, end]).padded(radius + 2.0 * tolerance),
        };
        let mut best = None;
        self.visit(&self.root, &sweep, &mut best, budget, stop)?;
        Ok(best)
    }

    fn visit(
        &self,
        node: &Node,
        sweep: &Sweep,
        best: &mut Option<Impact>,
        budget: &mut Budget,
        stop: Stop,
    ) -> Result<(), String> {
        budget.charge(stop)?;
        if !node.bounds.overlaps(sweep.bounds) {
            return Ok(());
        }
        match &node.kind {
            NodeKind::Leaf { first, count } => {
                for tri in &self.triangles[*first..*first + *count] {
                    budget.charge(stop)?;
                    if !tri.bounds.overlaps(sweep.bounds) {
                        continue;
                    }
                    if let Some(hit) = triangle_impact(tri, sweep, budget, stop)?
                        && best.as_ref().is_none_or(|old| {
                            hit.toi < old.toi || (hit.toi == old.toi && hit.triangle < old.triangle)
                        })
                    {
                        *best = Some(hit);
                    }
                }
            }
            NodeKind::Branch(a, b) => {
                self.visit(a, sweep, best, budget, stop)?;
                self.visit(b, sweep, best, budget, stop)?;
            }
        }
        Ok(())
    }
}

fn triangle_impact(
    tri: &Triangle,
    sweep: &Sweep,
    budget: &mut Budget,
    stop: Stop,
) -> Result<Option<Impact>, String> {
    let [a, b, c] = tri.points;
    let mut t = 0.0;
    for _ in 0..MAX_ADVANCES {
        budget.charge(stop)?;
        let p = add(sweep.start, mul(sweep.delta, t));
        let offset = sub(p, closest_on_triangle(p, a, b, c));
        let distance = len(offset);
        let gap = distance - sweep.radius;
        if !distance.is_finite() || gap < -sweep.tolerance || distance <= 0.0 {
            return Err(format!("CCD: 初期貫通または数値誤差 (面 {})。", tri.id));
        }
        let normal = mul(offset, 1.0 / distance);
        let closing = -dot(sweep.delta, normal);
        if closing <= 0.0 {
            // Convex triangle + sphere has a supporting plane with this normal.
            // Moving away from that plane cannot enter it.
            return Ok(None);
        }
        if gap <= 2.0 * sweep.tolerance {
            return Ok(Some(Impact {
                toi: t,
                normal,
                triangle: tri.id,
            }));
        }
        // Distance to the current supporting plane bounds the earliest possible
        // hit even when the nearest feature switches between face/edge/vertex.
        let dt = (gap - sweep.tolerance) / closing;
        if dt > 1.0 - t {
            return Ok(None);
        }
        let next = t + dt;
        if next <= t || !next.is_finite() {
            return Err("CCD: 時刻の更新が収束しません。".into());
        }
        t = next.min(1.0);
    }
    Err("CCD: 反復上限に達し、衝突の有無を判定できません。".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GO: Stop = &|| false;

    fn triangle() -> Mesh {
        Mesh {
            vertices: vec![[0., 0., 0.], [4., 0., 0.], [0., 4., 0.]],
            triangles: vec![[0, 1, 2]],
        }
    }

    fn hit(a: P, b: P, r: f64) -> Option<Impact> {
        StaticMesh::new(&triangle(), GO)
            .unwrap()
            .sweep(a, b, r, &mut Budget::new(10_000), GO)
            .unwrap()
    }

    #[test]
    fn thin_wall_crossed_at_high_speed_from_both_sides() {
        for side in [-1., 1.] {
            let h = hit([1., 1., side * 1000.], [1., 1., -side * 1000.], 0.25).unwrap();
            let expected = (1000. - 0.25) / 2000.;
            assert!((h.toi - expected).abs() < 1e-9);
            assert!(h.toi <= expected + 1e-12);
            assert!((h.normal[2] - side).abs() < 1e-9);
        }
    }

    #[test]
    fn edge_and_vertex_contacts_match_analytic_sphere_distance() {
        for (x, y, lateral) in [(2., -0.3, 0.3), (-0.3, -0.4, 0.5)] {
            let r: f64 = 0.6;
            let h = hit([x, y, 2.], [x, y, -2.], r).unwrap();
            let expected = (2. - (r * r - lateral * lateral).sqrt()) / 4.;
            assert!((h.toi - expected).abs() < 1e-7, "{h:?}");
        }
    }

    #[test]
    fn separating_touch_and_clear_miss() {
        assert!(hit([1., 1., 0.25], [2., 1., 0.25], 0.25).is_none());
        assert!(hit([1., 1., 0.25], [1., 1., 2.], 0.25).is_none());
        assert!(hit([10., 10., 2.], [10., 10., -2.], 0.25).is_none());
        assert_eq!(hit([1., 1., 0.25], [1., 1., -2.], 0.25).unwrap().toi, 0.0);
    }

    #[test]
    fn initial_overlap_invalid_geometry_budget_and_cancel_are_errors() {
        let mut m = triangle();
        let collider = StaticMesh::new(&m, GO).unwrap();
        assert!(
            collider
                .sweep([1., 1., 0.1], [1., 1., 2.], 0.25, &mut Budget::new(100), GO)
                .is_err()
        );
        assert!(
            collider
                .sweep([1., 1., 2.], [1., 1., -2.], 0.25, &mut Budget::new(0), GO)
                .is_err()
        );
        assert!(
            collider
                .sweep(
                    [1., 1., 2.],
                    [1., 1., -2.],
                    0.25,
                    &mut Budget::new(100),
                    &|| true
                )
                .is_err()
        );
        m.triangles[0] = [0, 1, 1];
        assert!(StaticMesh::new(&m, GO).is_err());
    }

    #[test]
    fn bvh_selects_earliest_hit_independent_of_face_order() {
        let mut m = Mesh {
            vertices: Vec::new(),
            triangles: Vec::new(),
        };
        for i in (0..20).rev() {
            let base = m.vertices.len() as u32;
            m.vertices.extend(
                triangle()
                    .vertices
                    .into_iter()
                    .map(|p| add(p, [0., 0., i as f64])),
            );
            m.triangles.push([base, base + 1, base + 2]);
        }
        let run = |mesh: &Mesh| {
            StaticMesh::new(mesh, GO)
                .unwrap()
                .sweep(
                    [1., 1., 25.],
                    [1., 1., -5.],
                    0.5,
                    &mut Budget::new(10_000),
                    GO,
                )
                .unwrap()
                .unwrap()
        };
        let h = run(&m);
        assert!((h.toi - 5.5 / 30.).abs() < 1e-8);
        assert_eq!(h, run(&m));
        m.triangles.reverse();
        assert_eq!(h.toi, run(&m).toi);
    }

    #[test]
    #[ignore = "manual release-mode timing, not a correctness threshold"]
    fn benchmark_sweeps() {
        let collider = StaticMesh::new(&triangle(), GO).unwrap();
        let timer = std::time::Instant::now();
        let mut budget = Budget::new(1_000_000);
        for _ in 0..10_000 {
            std::hint::black_box(
                collider
                    .sweep([1., 1., 100.], [1., 1., -100.], 0.25, &mut budget, GO)
                    .unwrap()
                    .unwrap(),
            );
        }
        eprintln!("10000 swept-sphere queries: {:?}", timer.elapsed());
    }
}
