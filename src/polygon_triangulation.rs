//! Bounded triangulation of one simple planar polygon, retaining every input
//! vertex and its winding. Holes, self-touching rings and nonplanar polygons are
//! rejected rather than approximated by a fan. This is a floating-point kernel:
//! ambiguous geometry within the documented tolerances is rejected.

pub const MAX_POLYGON_VERTICES: usize = 2048;
const MAX_PREDICATES: usize = 8_000_000;
const MAX_COORDINATE: f64 = 1.0e9;
// Predicates use local coordinates divided by the polygon's largest extent.
const DISTANCE_EPS: f64 = 1.0e-12;
const AREA_EPS: f64 = 1.0e-14;
const PLANAR_EPS: f64 = 1.0e-9;
// Matches the minimum physical cross-product magnitude accepted by the mesh.
const MIN_CROSS: f64 = 1.0e-12;

type Point = [f64; 3];
type Point2 = [f64; 2];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn within(a: Point2, b: Point2, p: Point2) -> bool {
    (0..2).all(|i| p[i] >= a[i].min(b[i]) - DISTANCE_EPS
        && p[i] <= a[i].max(b[i]) + DISTANCE_EPS)
}
fn intersects(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let o = [orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b)];
    let opposite = |x: f64, y: f64| (x > AREA_EPS && y < -AREA_EPS)
        || (y > AREA_EPS && x < -AREA_EPS);
    (opposite(o[0], o[1]) && opposite(o[2], o[3]))
        || (o[0].abs() <= AREA_EPS && within(a, b, c))
        || (o[1].abs() <= AREA_EPS && within(a, b, d))
        || (o[2].abs() <= AREA_EPS && within(c, d, a))
        || (o[3].abs() <= AREA_EPS && within(c, d, b))
}
fn checkpoint(predicates: &mut usize, stop: &dyn Fn() -> bool) -> Result<(), String> {
    if *predicates % 512 == 0 && stop() {
        return Err("計算を停止しました。".into());
    }
    *predicates += 1;
    if *predicates > MAX_PREDICATES {
        return Err("多角形の三角形分割が計算量上限を超えました。面を分割してください。".into());
    }
    Ok(())
}

/// Triangles reference the original `points` indices. Exactly `n - 2` nonzero
/// triangles are returned, or an explicit error; no partial result is returned.
/// The final point must not repeat the first. The planarity tolerance is 1e-9
/// times the largest local coordinate extent; original coordinates are retained.
pub fn triangulate(points: &[Point], stop: &dyn Fn() -> bool) -> Result<Vec<[usize; 3]>, String> {
    if !(3..=MAX_POLYGON_VERTICES).contains(&points.len()) {
        return Err("多角形の頂点数は 3 〜 2048 にしてください。".into());
    }
    let mut work = 0;
    checkpoint(&mut work, stop)?;
    if points.iter().flatten().any(|x| !x.is_finite() || x.abs() > MAX_COORDINATE) {
        return Err("多角形の座標が無効です。有限な ±1e9 以内の値が必要です。".into());
    }
    let origin = points[0];
    let scale = points.iter().flat_map(|&p| sub(p, origin)).map(f64::abs).fold(0., f64::max);
    if scale == 0. {
        return Err("多角形の頂点が重複しています。".into());
    }
    let local: Vec<Point> = points.iter().map(|&p| sub(p, origin).map(|x| x / scale)).collect();
    let n = local.len();
    let mut normal = [0.; 3];
    for i in 0..n {
        let area = cross(local[i], local[(i + 1) % n]);
        for axis in 0..3 { normal[axis] += area[axis]; }
    }
    let norm = dot(normal, normal).sqrt();
    if norm <= AREA_EPS {
        return Err("多角形の面積がゼロ、または境界が自己交差しています。".into());
    }
    let unit = normal.map(|x| x / norm);
    if local.iter().any(|&p| dot(unit, p).abs() > PLANAR_EPS) {
        return Err("多角形の頂点が同一平面上にありません。".into());
    }
    let axis = (0..3).max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs())).unwrap_or(2);
    let projected: Vec<Point2> = local.iter().map(|p| match axis {
        0 => [p[1], p[2]], 1 => [p[2], p[0]], _ => [p[0], p[1]],
    }).collect();
    let winding = normal[axis].signum();
    for i in 0..n {
        let next = (i + 1) % n;
        let p = projected[i];
        let q = projected[next];
        let edge = [q[0] - p[0], q[1] - p[1]];
        if edge[0].hypot(edge[1]) <= DISTANCE_EPS {
            return Err("多角形に重複する頂点または短すぎる辺があります。".into());
        }
        // Adjacent edges may meet, but must not reverse over the same segment.
        let before = projected[(i + n - 1) % n];
        if orient(before, p, q).abs() <= AREA_EPS
            && (p[0] - before[0]) * edge[0] + (p[1] - before[1]) * edge[1] < 0.
        {
            return Err("多角形の隣り合う辺が重なっています。".into());
        }
        for j in i + 1..n {
            checkpoint(&mut work, stop)?;
            let jnext = (j + 1) % n;
            if next != j && jnext != i
                && intersects(p, q, projected[j], projected[jnext])
            {
                return Err("多角形の境界が交差または接触しています。穴付き境界は未対応です。".into());
            }
        }
    }
    let mut ring: Vec<usize> = (0..n).collect();
    let mut triangles = Vec::with_capacity(n - 2);
    while ring.len() > 3 {
        let mut ear = None;
        for k in 0..ring.len() {
            checkpoint(&mut work, stop)?;
            let a = ring[(k + ring.len() - 1) % ring.len()];
            let b = ring[k];
            let c = ring[(k + 1) % ring.len()];
            if winding * orient(projected[a], projected[b], projected[c]) <= AREA_EPS {
                continue;
            }
            let mut contains = false;
            for &v in &ring {
                checkpoint(&mut work, stop)?;
                if v == a || v == b || v == c { continue; }
                let p = projected[v];
                if winding * orient(projected[a], projected[b], p) >= -AREA_EPS
                    && winding * orient(projected[b], projected[c], p) >= -AREA_EPS
                    && winding * orient(projected[c], projected[a], p) >= -AREA_EPS
                {
                    contains = true;
                    break;
                }
            }
            if !contains { ear = Some((k, [a, b, c])); break; }
        }
        let Some((k, tri)) = ear else {
            return Err("多角形を安全に三角形分割できません。退化した頂点を確認してください。".into());
        };
        triangles.push(tri);
        ring.remove(k);
    }
    triangles.push([ring[0], ring[1], ring[2]]);
    for &[a, b, c] in &triangles {
        checkpoint(&mut work, stop)?;
        let physical_cross = cross(sub(points[b], points[a]), sub(points[c], points[a]));
        if winding * orient(projected[a], projected[b], projected[c]) <= AREA_EPS
            || dot(physical_cross, physical_cross).sqrt() < MIN_CROSS
        {
            return Err("三角形分割に退化または微小な面が生じます。".into());
        }
    }
    if stop() { return Err("計算を停止しました。".into()); }
    Ok(triangles)
}

/// Indexed variant used by hole repair. Every boundary index is checked before
/// accessing a vertex. Output triangles use the original vertex numbering.
pub fn triangulate_indexed(
    vertices: &[Point], boundary: &[usize], stop: &dyn Fn() -> bool,
) -> Result<Vec<[usize; 3]>, String> {
    if !(3..=MAX_POLYGON_VERTICES).contains(&boundary.len()) {
        return Err("多角形の頂点数は 3 〜 2048 にしてください。".into());
    }
    let points: Vec<Point> = boundary.iter().map(|&v| vertices.get(v).copied()
        .ok_or_else(|| "多角形が存在しない頂点を参照しています。".to_string())).collect::<Result<_, _>>()?;
    Ok(triangulate(&points, stop)?.into_iter().map(|t| t.map(|v| boundary[v])).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    const L: [Point; 6] = [[0.,0.,0.],[3.,0.,0.],[3.,1.,0.],[1.,1.,0.],[1.,3.,0.],[0.,3.,0.]];
    const U: [Point; 8] = [[0.,0.,0.],[3.,0.,0.],[3.,3.,0.],[2.,3.,0.],[2.,1.,0.],[1.,1.,0.],[1.,3.,0.],[0.,3.,0.]];
    fn check(points: &[Point], area: f64, sign: f64) {
        let triangles = triangulate(points, &||false).unwrap();
        assert_eq!(triangles.len(), points.len()-2);
        let mut seen = vec![false;points.len()];
        let mut total = 0.;
        for t in triangles {
            for v in t { seen[v] = true; }
            let [a,b,c] = t.map(|v| points[v]);
            let z = cross(sub(b,a),sub(c,a))[2];
            assert!(sign*z>0.);
            total += z/2.;
        }
        assert!((total-sign*area).abs()<1e-10);
        assert!(seen.into_iter().all(|v| v));
    }
    #[test] fn concave_area_winding_and_vertices() {
        for (points,area) in [(&L[..],5.),(&U[..],7.)] {
            check(points,area,1.);
            let rev:Vec<_> = points.iter().rev().copied().collect();
            check(&rev,area,-1.);
        }
    }
    #[test] fn large_translation_and_collinear_boundary_vertex() {
        let shifted:Vec<_> = U.iter().map(|p| [p[0]-1e8,p[1]+1e8,p[2]+2e8]).collect();
        check(&shifted,7.,1.);
        check(&[[0.,0.,0.],[1.,0.,0.],[2.,0.,0.],[2.,2.,0.],[0.,2.,0.]],4.,1.);
    }
    #[test] fn invalid_nonplanar_and_crossing_rings_are_rejected() {
        assert!(triangulate(&[[0.,0.,0.],[2.,2.,0.],[0.,2.,0.],[2.,0.,0.]],&||false).is_err());
        assert!(triangulate(&[[0.,0.,0.],[2.,0.,0.],[2.,2.,1.],[0.,2.,0.]],&||false).is_err());
        assert!(triangulate(&[[0.,0.,0.],[1.,0.,0.],[2.,0.,0.]],&||false).is_err());
        let mut duplicate = L.to_vec(); duplicate.push(L[0]);
        assert!(triangulate(&duplicate,&||false).is_err());
        let mut nan = L; nan[0][0] = f64::NAN;
        assert!(triangulate(&nan,&||false).is_err());
    }
    #[test]
    fn cyclic_projection_axes_and_cancellation_during_work() {
        for axis in 0..3 {
            let points: Vec<_> = L.iter().map(|p| match axis {
                0 => [7., p[0], p[1]],
                1 => [p[1], 7., p[0]],
                _ => [p[0], p[1], 7.],
            }).collect();
            let triangles = triangulate(&points, &|| false).unwrap();
            let area: f64 = triangles.iter().map(|t| {
                let [a,b,c] = t.map(|i| points[i]);
                let n = cross(sub(b,a),sub(c,a));
                assert!(n[axis] > 0.);
                dot(n,n).sqrt()/2.
            }).sum();
            assert!((area-5.).abs()<1e-10);
        }
        let circle: Vec<_> = (0..256).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 256.;
            [t.cos(),t.sin(),0.]
        }).collect();
        let calls = std::cell::Cell::new(0);
        let stop = || { calls.set(calls.get()+1); calls.get() >= 2 };
        assert!(triangulate(&circle,&stop).unwrap_err().contains("停止"));
        assert_eq!(calls.get(),2);
    }

    #[test] fn indexed_errors_limits_and_cancellation() {
        assert!(triangulate_indexed(&L,&[0,1,usize::MAX],&||false).is_err());
        assert!(triangulate_indexed(&L,&[0,1,1],&||false).is_err());
        assert!(triangulate(&vec![[0.;3]; MAX_POLYGON_VERTICES+1],&||false).is_err());
        assert!(triangulate(&L,&||true).unwrap_err().contains("停止"));
        let ring=[5,4,3,2,1,0];
        let triangles=triangulate_indexed(&L,&ring,&||false).unwrap();
        assert!(triangles.iter().flatten().all(|v| ring.contains(v)));
    }
}
