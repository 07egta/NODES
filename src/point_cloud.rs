//! Bounded point-cloud preprocessing, independent of the graph representation.
//! SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::BTreeMap;

pub type Point = [f64; 3];
pub const MAX_POINTS: usize = 250_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;

fn check(p: Point) -> Result<(), String> {
    if p.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
        Ok(())
    } else {
        Err("点群の座標が不正です。".into())
    }
}

fn tick(i: usize, stop: &dyn Fn() -> bool) -> Result<(), String> {
    if i.is_multiple_of(256) && stop() {
        return Err("点群処理を中断しました。".into());
    }
    Ok(())
}

/// Strict whitespace-delimited XYZ, with optional blank lines and # comments.
/// Coordinates are preserved; units are supplied by the caller.
pub fn parse_xyz(bytes: &[u8], stop: &dyn Fn() -> bool) -> Result<Vec<Point>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("点群ファイルが64 MiBを超えています。".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "XYZはUTF-8で保存してください。")?;
    let mut result = Vec::new();
    for (i, line) in text.lines().enumerate() {
        tick(i, stop)?;
        let mut fields = line
            .split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace();
        let Some(x) = fields.next() else {
            continue;
        };
        let parse = |s: &str| {
            s.parse::<f64>()
                .map_err(|_| format!("XYZ {} 行目の数値が不正です。", i + 1))
        };
        let p = [
            parse(x)?,
            parse(fields.next().ok_or("Y座標がありません。")?)?,
            parse(fields.next().ok_or("Z座標がありません。")?)?,
        ];
        if fields.next().is_some() {
            return Err(format!("XYZ {} 行目は3列にしてください。", i + 1));
        }
        check(p)?;
        if result.len() == MAX_POINTS {
            return Err("点群の点数が上限を超えています。".into());
        }
        result.push(p);
    }
    Ok(result)
}

/// Deterministic voxel centroid sampling, with a world-origin anchored grid.
pub fn voxel_centroids(
    points: &[Point],
    size: f64,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Point>, String> {
    if points.len() > MAX_POINTS || !size.is_finite() || !(1e-9..=1e9).contains(&size) {
        return Err("点群の点数またはボクセル寸法が不正です。".into());
    }
    let mut cells = BTreeMap::<[i64; 3], (Point, usize)>::new();
    for (i, &p) in points.iter().enumerate() {
        tick(i, stop)?;
        check(p)?;
        let key = p.map(|v| (v / size).floor() as i64);
        let (sum, count) = cells.entry(key).or_insert(([0.; 3], 0));
        for (s, v) in sum.iter_mut().zip(p) {
            *s += v;
        }
        *count += 1;
    }
    let mut result = Vec::with_capacity(cells.len());
    for (i, (sum, count)) in cells.into_values().enumerate() {
        tick(i, stop)?;
        result.push(sum.map(|v| v / count as f64));
    }
    Ok(result)
}

fn distance2(a: Point, b: Point) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}

/// Balanced implicit KD tree. Index ties are resolved by original input order.
pub struct KdTree {
    points: Vec<(Point, usize)>,
}

impl KdTree {
    pub fn new(points: &[Point], stop: &dyn Fn() -> bool) -> Result<Self, String> {
        if points.len() > MAX_POINTS {
            return Err("点群の点数が上限を超えています。".into());
        }
        let mut entries = Vec::with_capacity(points.len());
        for (i, &p) in points.iter().enumerate() {
            tick(i, stop)?;
            check(p)?;
            entries.push((p, i));
        }
        Self::build(&mut entries, 0, stop)?;
        Ok(Self { points: entries })
    }

    fn build(
        entries: &mut [(Point, usize)],
        axis: usize,
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if entries.len() <= 1 {
            return Ok(());
        }
        tick(0, stop)?;
        let mid = entries.len() / 2;
        let (left, _, right) = entries.select_nth_unstable_by(mid, |a, b| {
            a.0[axis].total_cmp(&b.0[axis]).then(a.1.cmp(&b.1))
        });
        Self::build(left, (axis + 1) % 3, stop)?;
        Self::build(right, (axis + 1) % 3, stop)
    }

    /// Returns original index and Euclidean distance. Empty input returns None.
    pub fn nearest(
        &self,
        query: Point,
        stop: &dyn Fn() -> bool,
    ) -> Result<Option<(usize, f64)>, String> {
        check(query)?;
        tick(0, stop)?;
        let mut best = None;
        let mut visits = 0;
        Self::search(&self.points, query, 0, &mut best, &mut visits, stop)?;
        Ok(best.map(|(i, d): (usize, f64)| (i, d.sqrt())))
    }

    fn search(
        entries: &[(Point, usize)],
        query: Point,
        axis: usize,
        best: &mut Option<(usize, f64)>,
        visits: &mut usize,
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }
        tick(*visits, stop)?;
        *visits += 1;
        let mid = entries.len() / 2;
        let (p, i) = entries[mid];
        let d = distance2(query, p);
        if best.is_none_or(|(bi, bd)| d < bd || (d == bd && i < bi)) {
            *best = Some((i, d));
        }
        let delta = query[axis] - p[axis];
        let (near, far) = if delta < 0. {
            (&entries[..mid], &entries[mid + 1..])
        } else {
            (&entries[mid + 1..], &entries[..mid])
        };
        let next = (axis + 1) % 3;
        Self::search(near, query, next, best, visits, stop)?;
        if best.is_none_or(|(_, bd)| delta * delta <= bd) {
            Self::search(far, query, next, best, visits, stop)?;
        }
        Ok(())
    }
}

/// Directed cloud-to-cloud distances, not a symmetric Hausdorff metric.
/// Reject an excessive worst-case pair count to bound degenerate KD searches.
pub fn directed_distances(
    source: &[Point],
    target: &[Point],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<f64>, String> {
    if source.len() > MAX_POINTS
        || target.is_empty()
        || source.len().saturating_mul(target.len()) > 100_000_000
    {
        return Err("点群距離の対象または計算量が不正です。間引いてください。".into());
    }
    let tree = KdTree::new(target, stop)?;
    let mut distances = Vec::with_capacity(source.len());
    for &p in source {
        let (_, d) = tree.nearest(p, stop)?.ok_or("比較点群が空です。")?;
        distances.push(d);
    }
    Ok(distances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xyz_rejects_bad_rows_and_nonfinite() {
        assert_eq!(
            parse_xyz(b"# scan\n1 2 3\n-1 0 4", &|| false).unwrap(),
            vec![[1., 2., 3.], [-1., 0., 4.]]
        );
        for bytes in [b"1 2".as_slice(), b"1 2 3 4", b"NaN 0 0", b"inf 0 0"] {
            assert!(parse_xyz(bytes, &|| false).is_err());
        }
        assert!(parse_xyz(b"1 2 3", &|| true).is_err());
    }

    #[test]
    fn voxel_negative_and_positive_cells() {
        let points = [[-0.2, 0., 0.], [0.2, 0., 0.], [0.4, 0., 0.]];
        let result = voxel_centroids(&points, 1., &|| false).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], points[0]);
        assert!((result[1][0] - 0.3).abs() < 1e-14);
        assert!(voxel_centroids(&points, 0., &|| false).is_err());
    }

    #[test]
    fn kd_matches_brute_force_and_resolves_ties() {
        let points: Vec<_> = (0..1000)
            .map(|i| [(i % 31) as f64, (i % 17) as f64, (i % 7) as f64])
            .collect();
        let tree = KdTree::new(&points, &|| false).unwrap();
        for i in 0..50 {
            let q = [i as f64 / 2., 3.2, 2.1];
            let (expected, p) = points
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    distance2(*a.1, q)
                        .total_cmp(&distance2(*b.1, q))
                        .then(a.0.cmp(&b.0))
                })
                .unwrap();
            let (actual, d) = tree.nearest(q, &|| false).unwrap().unwrap();
            assert_eq!(actual, expected);
            assert_eq!(d, distance2(*p, q).sqrt());
        }
        let tied = KdTree::new(&[[-1., 0., 0.], [1., 0., 0.]], &|| false).unwrap();
        assert_eq!(tied.nearest([0.; 3], &|| false).unwrap(), Some((0, 1.)));
        assert!(tree.nearest([0.; 3], &|| true).is_err());
    }

    #[test]
    fn directed_distances_known_translation() {
        let source = [[0., 0., 3.], [10., 0., 4.]];
        let target = [[0., 0., 0.], [10., 0., 0.]];
        assert_eq!(
            directed_distances(&source, &target, &|| false).unwrap(),
            vec![3., 4.]
        );
        assert!(directed_distances(&source, &[], &|| false).is_err());
    }
}
