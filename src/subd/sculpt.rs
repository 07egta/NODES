//! Connected control-cage brushes. No display-mesh mutation or T-spline refinement.
//!
//! Distances are shortest paths on the control-cage edges, not exact surface geodesics.
use super::{Cage, P3, R, Stop, add, kb, kb_stop, norm, scale};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

/// Radial response, truncated to zero at the radius.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Falloff {
    Linear,
    #[default]
    Smooth,
    Gaussian,
}

impl Falloff {
    fn weight(self, distance: f64, radius: f64) -> f64 {
        if distance >= radius {
            return 0.;
        }
        let t = (distance / radius).clamp(0., 1.);
        match self {
            Self::Linear => 1. - t,
            Self::Smooth => 1. - t * t * (3. - 2. * t),
            Self::Gaussian => {
                let end = (-4.5_f64).exp();
                ((-4.5 * t * t).exp() - end) / (1. - end)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrushAction {
    Move {
        delta: P3,
    },
    /// One simultaneous, weighted Laplacian step; boundaries and creases stay fixed.
    Relax {
        strength: f64,
    },
}

/// A single reproducible edit; all distances are measured in mm on the input cage.
/// Pinned points remain fixed but do not block graph traversal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrushEdit {
    pub topology: u64,
    pub seeds: Vec<usize>,
    #[serde(default)]
    pub pinned: Vec<usize>,
    pub radius: f64,
    pub falloff: Falloff,
    pub action: BrushAction,
}

#[derive(Clone, Copy)]
struct Visit {
    distance: f64,
    vertex: usize,
}

impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Visit {}
impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

type Neighbors = Vec<Vec<(usize, f64)>>;

fn distances(neighbors: &Neighbors, seeds: &[usize], radius: f64, stop: Stop) -> R<Vec<f64>> {
    let mut result = vec![f64::INFINITY; neighbors.len()];
    let mut queue = BinaryHeap::new();
    for &vertex in seeds {
        if result[vertex] != 0. {
            result[vertex] = 0.;
            queue.push(Visit {
                distance: 0.,
                vertex,
            });
        }
    }
    while let Some(Visit { distance, vertex }) = queue.pop() {
        kb_stop(stop)?;
        if distance != result[vertex] {
            continue;
        }
        for &(next, length) in &neighbors[vertex] {
            let candidate = distance + length;
            if candidate < radius && candidate < result[next] {
                result[next] = candidate;
                queue.push(Visit {
                    distance: candidate,
                    vertex: next,
                });
            }
        }
    }
    Ok(result)
}

impl Cage {
    /// Stable connectivity fingerprint, independent of point positions. A guard against
    /// common upstream topology changes, NOT a persistent semantic vertex ID.
    pub fn brush_topology_key(&self) -> u64 {
        // FNV-1a over fixed-width, little-endian fields; no randomized HashMap ordering.
        let mut hash = 0xcbf29ce484222325_u64;
        let mut feed = |value: u64| {
            for b in value.to_le_bytes() {
                hash ^= u64::from(b);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        };
        feed(1); // fingerprint version
        feed(self.vertices.len() as u64);
        match self.grid {
            Some(g) => {
                feed(1);
                feed(g.cols as u64);
                feed(g.rows as u64);
            }
            None => feed(if self.ts.is_some() { 2 } else { 0 }),
        }
        feed(self.faces.len() as u64);
        for face in &self.faces {
            feed(face.len() as u64);
            for &v in face {
                feed(u64::from(v));
            }
        }
        feed(self.creases.len() as u64);
        for &(a, b) in &self.creases {
            feed(u64::from(a));
            feed(u64::from(b));
        }
        hash
    }

    pub(super) fn apply_brush(&self, brush: &BrushEdit, stop: Stop) -> R<Self> {
        kb_stop(stop)?;
        if self.ts.is_some() {
            return Err("このブラシは通常の SubD・格子ケージ専用です。".into());
        }
        if brush.topology != self.brush_topology_key() {
            return Err(
                "上流のケージ位相が変わりました。ブラシの編集を戻して選び直してください。".into(),
            );
        }
        let n = self.vertices.len();
        if brush.seeds.is_empty()
            || brush.seeds.len() > n
            || brush.pinned.len() > n
            || brush.seeds.iter().chain(&brush.pinned).any(|&v| v >= n)
        {
            return Err("ブラシの選択点または固定点が無効です。".into());
        }
        if !(brush.radius.is_finite()
            && brush.radius > 0.
            && brush.radius <= kb::limits::MAX_COORDINATE)
        {
            return Err("ブラシ半径は有限の正の値にしてください。".into());
        }
        match brush.action {
            BrushAction::Move { delta }
                if delta
                    .iter()
                    .any(|x| !x.is_finite() || x.abs() > kb::limits::MAX_COORDINATE) =>
            {
                return Err("ブラシの移動量が無効です。".into());
            }
            BrushAction::Relax { strength }
                if !(strength.is_finite() && (0. ..=1.).contains(&strength)) =>
            {
                return Err("緩和の強度は 0〜1 にしてください。".into());
            }
            _ => {}
        }

        // Bound work before building adjacency; reject malformed or non-manifold edges.
        let mut corners = 0usize;
        let mut edges = BTreeMap::<(usize, usize), usize>::new();
        for face in &self.faces {
            kb_stop(stop)?;
            corners = corners.saturating_add(face.len());
            if corners > 480_000 || face.len() < 3 {
                return Err("ブラシのケージ面数が上限を超えたか無効です。".into());
            }
            for (&a, &b) in face.iter().zip(face.iter().cycle().skip(1)) {
                let (a, b) = (a as usize, b as usize);
                if a == b || a >= n || b >= n {
                    return Err("ケージの辺が無効です。".into());
                }
                let count = edges.entry((a.min(b), a.max(b))).or_default();
                *count += 1;
                if *count > 2 {
                    return Err("非多様体辺のあるケージはブラシで編集できません。".into());
                }
            }
        }
        let mut neighbors = vec![Vec::new(); n];
        let mut fixed = vec![false; n];
        for &v in &brush.pinned {
            fixed[v] = true;
        }
        for ((a, b), count) in edges {
            kb_stop(stop)?;
            let p = self.vertices[a];
            let q = self.vertices[b];
            let length = norm([p[0] - q[0], p[1] - q[1], p[2] - q[2]]);
            if !length.is_finite() {
                return Err("ケージの辺長が無効です。".into());
            }
            neighbors[a].push((b, length));
            neighbors[b].push((a, length));
            if matches!(brush.action, BrushAction::Relax { .. })
                && (count == 1 || self.creases.contains(&(a as u32, b as u32)))
            {
                fixed[a] = true;
                fixed[b] = true;
            }
        }

        let d = distances(&neighbors, &brush.seeds, brush.radius, stop)?;
        let mut next = self.vertices.clone();
        for (i, out) in next.iter_mut().enumerate() {
            kb_stop(stop)?;
            let w = brush.falloff.weight(d[i], brush.radius);
            if fixed[i] || w == 0. {
                continue;
            }
            let p = self.vertices[i];
            *out = match brush.action {
                BrushAction::Move { delta } => add(p, scale(delta, w)),
                BrushAction::Relax { strength } => {
                    let adj = &neighbors[i];
                    if adj.is_empty() {
                        continue;
                    }
                    let avg = scale(
                        adj.iter()
                            .fold([0.; 3], |sum, &(j, _)| add(sum, self.vertices[j])),
                        1. / adj.len() as f64,
                    );
                    add(
                        p,
                        scale([avg[0] - p[0], avg[1] - p[1], avg[2] - p[2]], w * strength),
                    )
                }
            };
        }
        kb_stop(stop)?;
        // Keep the same control topology, representation and crease set.
        self.with_vertices(next, stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subd::{EditOp, edits_text, parse_edits};

    fn move_brush(cage: &Cage) -> BrushEdit {
        BrushEdit {
            topology: cage.brush_topology_key(),
            seeds: vec![0],
            pinned: Vec::new(),
            radius: 20.,
            falloff: Falloff::Linear,
            action: BrushAction::Move {
                delta: [0., 0., 4.],
            },
        }
    }

    #[test]
    fn connected_distance_does_not_cross_islands() {
        let neighbors = vec![vec![(1, 2.)], vec![(0, 2.), (2, 3.)], vec![(1, 3.)], vec![]];
        let d = distances(&neighbors, &[0, 0], 6., &|| false).unwrap();
        assert_eq!(d, vec![0., 2., 5., f64::INFINITY]);
        assert!(distances(&neighbors, &[0], 6., &|| true).is_err());
    }

    #[test]
    fn all_falloffs_are_bounded_monotone_and_compact() {
        for f in [Falloff::Linear, Falloff::Smooth, Falloff::Gaussian] {
            assert!((f.weight(0., 10.) - 1.).abs() < 1e-12);
            assert_eq!(f.weight(10., 10.), 0.);
            assert_eq!(f.weight(f64::INFINITY, 10.), 0.);
            let mut previous = 1. + 1e-12;
            for i in 0..=100 {
                let w = f.weight(i as f64 / 10., 10.);
                assert!((0. ..=1. + 1e-12).contains(&w) && w <= previous);
                previous = w;
            }
        }
    }

    #[test]
    fn move_replays_and_preserves_cage_topology() {
        let cage = Cage::box_cage(10., 10., 10., &|| false).unwrap();
        let brush = move_brush(&cage);
        let op = EditOp::Brush { brush };
        let history = edits_text(std::slice::from_ref(&op));
        assert_eq!(parse_edits(&history).unwrap(), vec![op]);
        let result = cage.apply_edits(&history, &|| false).unwrap();
        assert_eq!(result.vertices()[0][2], 4.);
        assert_eq!(result.vertices()[1][2], 2.);
        assert_eq!(result.vertices()[2][2], 0.);
        assert_eq!(result.brush_topology_key(), cage.brush_topology_key());
        assert_eq!(result, cage.apply_edits(&history, &|| false).unwrap());
        assert!(cage.apply_edits(&history, &|| true).is_err());
        let changed = cage.subdivide(&|| false).unwrap();
        assert!(changed.apply_edits(&history, &|| false).is_err());
    }

    #[test]
    fn mask_and_relax_respect_pins() {
        let cage = Cage::box_cage(10., 10., 10., &|| false).unwrap();
        let mut brush = move_brush(&cage);
        brush.pinned = vec![1];
        let moved = cage.apply_brush(&brush, &|| false).unwrap();
        assert_eq!(moved.vertices()[1], cage.vertices()[1]);
        brush.action = BrushAction::Relax { strength: 1. };
        let relaxed = cage.apply_brush(&brush, &|| false).unwrap();
        let expected = [-5. + 10. / 3., -5. + 10. / 3., 10. / 3.];
        for (a, b) in relaxed.vertices()[0].iter().zip(expected) {
            assert!((*a - b).abs() < 1e-12);
        }
        assert_eq!(relaxed.vertices()[1], cage.vertices()[1]);
        let creased = cage.crease(&[0], &|| false).unwrap();
        brush.topology = creased.brush_topology_key();
        let relaxed = creased.apply_brush(&brush, &|| false).unwrap();
        let (a, b) = creased.edges()[0];
        assert_eq!(
            relaxed.vertices()[a as usize],
            creased.vertices()[a as usize]
        );
        assert_eq!(
            relaxed.vertices()[b as usize],
            creased.vertices()[b as usize]
        );
    }

    #[test]
    fn grid_keeps_its_spline_and_bad_input_fails() {
        let cage = Cage::cylinder_cage(10., 20., 8, 5, 0., &|| false).unwrap();
        let mut brush = move_brush(&cage);
        let moved = cage.apply_brush(&brush, &|| false).unwrap();
        assert_eq!(moved.grid(), cage.grid());
        assert!(moved.grid_surface().is_ok());
        brush.radius = f64::NAN;
        assert!(cage.apply_brush(&brush, &|| false).is_err());
        brush.radius = 20.;
        brush.seeds = vec![usize::MAX];
        assert!(cage.apply_brush(&brush, &|| false).is_err());
    }
}
