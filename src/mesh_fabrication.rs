//! Indexed triangular mesh analysis and editing. No external dependencies.
//! Index topology and oriented volume are not self-intersection or printability tests.
use crate::engine::{
    as_mesh, cancelled, checked_size, cross, dot, finite_point, length, num, shape, sub,
    validate_mesh,
};
use crate::model::*;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

type P = [f64; 3];
type Edge = (u32, u32);
type Uses = BTreeMap<Edge, Vec<(usize, bool)>>;
fn edge(a: u32, b: u32) -> Edge {
    (a.min(b), a.max(b))
}
fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: P, x: f64) -> P {
    a.map(|v| v * x)
}
fn midpoint(a: P, b: P) -> P {
    mul(add(a, b), 0.5)
}
fn tick(i: usize, stop: &dyn Fn() -> bool) -> Result<(), String> {
    if i.is_multiple_of(512) {
        cancelled(stop)?;
    }
    Ok(())
}
fn raw_check(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<(), String> {
    checked_size(m.vertices.len(), m.triangles.len())?;
    for (i, p) in m.vertices.iter().enumerate() {
        tick(i, stop)?;
        if !finite_point(*p) {
            return Err("無効な頂点座標です。".into());
        }
    }
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        if t.iter().any(|&v| v as usize >= m.vertices.len()) {
            return Err("面の頂点番号が範囲外です。".into());
        }
    }
    Ok(())
}
fn topology(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Uses, String> {
    raw_check(m, stop)?;
    let mut edges = Uses::new();
    for (f, t) in m.triangles.iter().enumerate() {
        tick(f, stop)?;
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.entry(edge(a, b)).or_default().push((f, a < b));
        }
    }
    Ok(edges)
}
fn pos_key(p: P) -> [u64; 3] {
    p.map(|x| if x == 0. { 0 } else { x.to_bits() })
}
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}
fn union(parent: &mut [usize], a: usize, b: usize) {
    let a = root(parent, a);
    let b = root(parent, b);
    if a != b {
        parent[b] = a;
    }
}
fn corner(m: &Mesh, f: usize, v: u32) -> Option<usize> {
    m.triangles[f]
        .iter()
        .position(|&x| x == v)
        .map(|k| 3 * f + k)
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Audit {
    pub boundary_edges: usize,
    pub non_manifold_edges: usize,
    pub orientation_conflicts: usize,
    pub non_manifold_vertices: usize,
    pub duplicate_vertices: usize,
    pub duplicate_faces: usize,
    pub degenerate_faces: usize,
    pub isolated_vertices: usize,
}
impl Audit {
    /// Necessary indexed-topology conditions only; no self-intersection or nesting test.
    pub fn closed_oriented(&self) -> bool {
        self.boundary_edges == 0
            && self.non_manifold_edges == 0
            && self.orientation_conflicts == 0
            && self.non_manifold_vertices == 0
            && self.duplicate_faces == 0
            && self.degenerate_faces == 0
    }
}
pub fn audit(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Audit, String> {
    let edges = topology(m, stop)?;
    let mut r = Audit::default();
    let mut coords = HashSet::new();
    for (i, &p) in m.vertices.iter().enumerate() {
        tick(i, stop)?;
        if !coords.insert(pos_key(p)) {
            r.duplicate_vertices += 1;
        }
    }
    let mut faces = HashSet::new();
    let mut used = vec![false; m.vertices.len()];
    for (f, t) in m.triangles.iter().enumerate() {
        tick(f, stop)?;
        for &v in t {
            used[v as usize] = true;
        }
        let mut key = *t;
        key.sort_unstable();
        if !faces.insert(key) {
            r.duplicate_faces += 1;
        }
        let [a, b, c] = t.map(|v| m.vertices[v as usize]);
        if length(cross(sub(b, a), sub(c, a))) < 1e-12 {
            r.degenerate_faces += 1;
        }
    }
    r.isolated_vertices = used.iter().filter(|&&v| !v).count();
    // Union face corners across each two-sided edge. A disconnected fan at one
    // vertex is non-manifold even when every incident edge has at most two faces.
    let mut parent: Vec<usize> = (0..3 * m.triangles.len()).collect();
    let mut boundary_degree = vec![0u32; m.vertices.len()];
    for (i, (&(a, b), us)) in edges.iter().enumerate() {
        tick(i, stop)?;
        match us.as_slice() {
            [_] => {
                r.boundary_edges += 1;
                boundary_degree[a as usize] += 1;
                boundary_degree[b as usize] += 1;
            }
            [(f, d), (g, e)] => {
                if d == e {
                    r.orientation_conflicts += 1;
                }
                for v in [a, b] {
                    if let (Some(x), Some(y)) = (corner(m, *f, v), corner(m, *g, v)) {
                        union(&mut parent, x, y);
                    }
                }
            }
            _ => r.non_manifold_edges += 1,
        }
    }
    let mut first = vec![usize::MAX; m.vertices.len()];
    let mut bad = vec![false; m.vertices.len()];
    for (f, t) in m.triangles.iter().enumerate() {
        tick(f, stop)?;
        for (k, &v) in t.iter().enumerate() {
            let v = v as usize;
            let x = root(&mut parent, 3 * f + k);
            if first[v] == usize::MAX {
                first[v] = x;
            } else if first[v] != x {
                bad[v] = true;
            }
        }
    }
    for (i, &d) in boundary_degree.iter().enumerate() {
        if d != 0 && d != 2 {
            bad[i] = true;
        }
    }
    for (&(a, b), uses) in &edges {
        if uses.len() > 2 {
            bad[a as usize] = true;
            bad[b as usize] = true;
        }
    }
    r.non_manifold_vertices = bad.iter().filter(|&&x| x).count();
    Ok(r)
}
pub fn area_center(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<(f64, P), String> {
    raw_check(m, stop)?;
    let origin = m.vertices.first().copied().unwrap_or([0.; 3]);
    let (mut area, mut moment) = (0., [0.; 3]);
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        let [a, b, c] = t.map(|v| sub(m.vertices[v as usize], origin));
        let w = 0.5 * length(cross(sub(b, a), sub(c, a)));
        area += w;
        moment = add(moment, mul(add(add(a, b), c), w / 3.));
    }
    Ok((
        area,
        if area > 0. {
            add(origin, mul(moment, 1. / area))
        } else {
            origin
        },
    ))
}
/// Oriented surface integral. Rejects open/non-manifold/inconsistent topology;
/// intersecting or incorrectly nested shells can still give misleading results.
pub fn signed_volume(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<f64, String> {
    let r = audit(m, stop)?;
    if m.triangles.is_empty() || !r.closed_oriented() {
        return Err("体積には閉じた一貫したメッシュが必要です。".into());
    }
    let origin = m.vertices[0];
    let (mut sum, mut correction) = (0., 0.);
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        let [a, b, c] = t.map(|v| sub(m.vertices[v as usize], origin));
        let x = dot(a, cross(b, c)) / 6. - correction;
        let next = sum + x;
        correction = (next - sum) - x;
        sum = next;
    }
    if sum.is_finite() {
        Ok(sum)
    } else {
        Err("体積が有限ではありません。".into())
    }
}
pub fn weld_exact(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Mesh, String> {
    raw_check(m, stop)?;
    let mut ids = HashMap::new();
    let mut vertices = Vec::new();
    let mut map = Vec::with_capacity(m.vertices.len());
    for (i, &p) in m.vertices.iter().enumerate() {
        tick(i, stop)?;
        let id = *ids.entry(pos_key(p)).or_insert_with(|| {
            let id = vertices.len() as u32;
            vertices.push(p);
            id
        });
        map.push(id);
    }
    let mut triangles = Vec::with_capacity(m.triangles.len());
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        triangles.push(t.map(|v| map[v as usize]));
    }
    let out = Mesh {
        vertices,
        triangles,
    };
    validate_mesh(&out, stop)?;
    Ok(out)
}
/// Orient neighbors consistently without guessing the exterior of nested shells.
pub fn unify(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Mesh, String> {
    validate_mesh(m, stop)?;
    let edges = topology(m, stop)?;
    let mut adjacency = vec![Vec::<(usize, bool)>::new(); m.triangles.len()];
    for (i, us) in edges.values().enumerate() {
        tick(i, stop)?;
        match us.as_slice() {
            [_] => (),
            [(a, x), (b, y)] => {
                adjacency[*a].push((*b, x == y));
                adjacency[*b].push((*a, x == y));
            }
            _ => return Err("非多様体の辺は法線統一できません。".into()),
        }
    }
    let mut flip = vec![None; m.triangles.len()];
    let mut q = VecDeque::new();
    for start in 0..flip.len() {
        tick(start, stop)?;
        if flip[start].is_some() {
            continue;
        }
        flip[start] = Some(false);
        q.push_back(start);
        let mut visited = 0;
        while let Some(f) = q.pop_front() {
            tick(visited, stop)?;
            visited += 1;
            let sign = flip[f].ok_or("面の向きが未確定です。")?;
            for &(g, x) in &adjacency[f] {
                let required = sign ^ x;
                match flip[g] {
                    Some(v) if v != required => {
                        return Err("一貫した向きにできないメッシュです。".into());
                    }
                    Some(_) => (),
                    None => {
                        flip[g] = Some(required);
                        q.push_back(g);
                    }
                }
            }
        }
    }
    let mut out = m.clone();
    for (i, t) in out.triangles.iter_mut().enumerate() {
        tick(i, stop)?;
        if flip[i] == Some(true) {
            t.swap(1, 2);
        }
    }
    Ok(out)
}
pub fn smooth(
    m: &Mesh,
    iterations: usize,
    amount: f64,
    stop: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    validate_mesh(m, stop)?;
    if iterations > 20 || !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err("平滑化の設定が範囲外です。".into());
    }
    let r = audit(m, stop)?;
    if r.non_manifold_edges > 0 || r.non_manifold_vertices > 0 || r.duplicate_faces > 0 {
        return Err("平滑化には多様体メッシュが必要です。".into());
    }
    let edges = topology(m, stop)?;
    let mut neighbors = vec![Vec::<usize>::new(); m.vertices.len()];
    let mut fixed = vec![false; m.vertices.len()];
    for (i, (&(a, b), us)) in edges.iter().enumerate() {
        tick(i, stop)?;
        let (a, b) = (a as usize, b as usize);
        neighbors[a].push(b);
        neighbors[b].push(a);
        if us.len() == 1 {
            fixed[a] = true;
            fixed[b] = true;
        }
    }
    let mut out = m.clone();
    for _ in 0..iterations {
        cancelled(stop)?;
        let mut next = out.vertices.clone();
        for (i, ns) in neighbors.iter().enumerate() {
            tick(i, stop)?;
            if fixed[i] || ns.is_empty() {
                continue;
            }
            let mean = mul(
                ns.iter().fold([0.; 3], |s, &j| add(s, out.vertices[j])),
                1. / ns.len() as f64,
            );
            next[i] = add(mul(out.vertices[i], 1. - amount), mul(mean, amount));
        }
        out.vertices = next;
        validate_mesh(&out, stop)?;
    }
    Ok(out)
}
fn limits(v: usize, t: usize) -> Result<(), String> {
    checked_size(v, t)?;
    if v.saturating_add(t.saturating_mul(3)) > MAX_TOTAL_WEIGHT / 2 {
        return Err("細分化結果がノードの計算量上限を超えます。".into());
    }
    Ok(())
}
/// Split each triangle into four, sharing one midpoint per indexed edge.
pub fn subdivide(m: &Mesh, levels: usize, stop: &dyn Fn() -> bool) -> Result<Mesh, String> {
    validate_mesh(m, stop)?;
    if levels > 5 {
        return Err("細分化は5段までです。".into());
    }
    let mut out = m.clone();
    for _ in 0..levels {
        cancelled(stop)?;
        let edges = topology(&out, stop)?;
        limits(out.vertices.len() + edges.len(), out.triangles.len() * 4)?;
        let mut mid = BTreeMap::new();
        for (i, &(a, b)) in edges.keys().enumerate() {
            tick(i, stop)?;
            let id = out.vertices.len() as u32;
            out.vertices
                .push(midpoint(out.vertices[a as usize], out.vertices[b as usize]));
            mid.insert((a, b), id);
        }
        let mut faces = Vec::with_capacity(out.triangles.len() * 4);
        for (i, &[a, b, c]) in out.triangles.iter().enumerate() {
            tick(i, stop)?;
            let (ab, bc, ca) = (mid[&edge(a, b)], mid[&edge(b, c)], mid[&edge(c, a)]);
            faces.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        out.triangles = faces;
    }
    validate_mesh(&out, stop)?;
    Ok(out)
}
/// A finite Catmull-Clark approximation. Internally retain quads between levels.
/// Boundary edges use midpoint rules; interior creases are not implemented.
pub fn catmull_clark(m: &Mesh, levels: usize, stop: &dyn Fn() -> bool) -> Result<Mesh, String> {
    validate_mesh(m, stop)?;
    if levels > 5 {
        return Err("Catmull–Clarkは5段までです。".into());
    }
    if levels == 0 {
        return Ok(m.clone());
    }
    let report = audit(m, stop)?;
    if report.non_manifold_edges > 0
        || report.non_manifold_vertices > 0
        || report.orientation_conflicts > 0
        || report.duplicate_faces > 0
        || report.isolated_vertices > 0
    {
        return Err("SubDには向きのそろった多様体制御メッシュが必要です。".into());
    }
    let mut points = m.vertices.clone();
    let mut faces: Vec<Vec<u32>> = m.triangles.iter().map(|t| t.to_vec()).collect();
    for _ in 0..levels {
        cancelled(stop)?;
        let n = points.len();
        let mut edges: BTreeMap<Edge, Vec<usize>> = BTreeMap::new();
        let mut fpoints = Vec::with_capacity(faces.len());
        let mut fsum = vec![[0.; 3]; n];
        let mut fcount = vec![0usize; n];
        let mut quads = 0usize;
        for (f, vs) in faces.iter().enumerate() {
            tick(f, stop)?;
            quads += vs.len();
            let p = mul(
                vs.iter().fold([0.; 3], |s, &v| add(s, points[v as usize])),
                1. / vs.len() as f64,
            );
            fpoints.push(p);
            for (k, &v) in vs.iter().enumerate() {
                let i = v as usize;
                fsum[i] = add(fsum[i], p);
                fcount[i] += 1;
                edges
                    .entry(edge(v, vs[(k + 1) % vs.len()]))
                    .or_default()
                    .push(f);
            }
        }
        limits(n + edges.len() + faces.len(), quads * 2)?;
        let mut next = points.clone();
        let mut esum = vec![[0.; 3]; n];
        let mut ecount = vec![0usize; n];
        let mut boundary = vec![Vec::<u32>::new(); n];
        let mut ids = BTreeMap::new();
        for (i, (&(a, b), adj)) in edges.iter().enumerate() {
            tick(i, stop)?;
            let (u, v) = (points[a as usize], points[b as usize]);
            let mid = midpoint(u, v);
            for k in [a as usize, b as usize] {
                esum[k] = add(esum[k], mid);
                ecount[k] += 1;
            }
            let p = match adj.as_slice() {
                [_] => {
                    boundary[a as usize].push(b);
                    boundary[b as usize].push(a);
                    mid
                }
                [f, g] => mul(add(add(u, v), add(fpoints[*f], fpoints[*g])), 0.25),
                _ => return Err("SubDの非多様体辺です。".into()),
            };
            ids.insert((a, b), next.len() as u32);
            next.push(p);
        }
        for i in 0..n {
            tick(i, stop)?;
            next[i] = match boundary[i].as_slice() {
                [a, b] => add(
                    mul(points[i], 0.75),
                    mul(add(points[*a as usize], points[*b as usize]), 0.125),
                ),
                [] if fcount[i] > 0 && ecount[i] == fcount[i] => {
                    let v = fcount[i] as f64;
                    mul(
                        add(
                            add(mul(fsum[i], 1. / v), mul(esum[i], 2. / v)),
                            mul(points[i], v - 3.),
                        ),
                        1. / v,
                    )
                }
                _ => return Err("SubDの境界または頂点ファンが無効です。".into()),
            };
        }
        let face_start = next.len() as u32;
        next.extend(fpoints);
        let mut refined = Vec::with_capacity(quads);
        for (f, vs) in faces.iter().enumerate() {
            tick(f, stop)?;
            for (k, &v) in vs.iter().enumerate() {
                let a = vs[(k + 1) % vs.len()];
                let b = vs[(k + vs.len() - 1) % vs.len()];
                refined.push(vec![
                    v,
                    ids[&edge(v, a)],
                    face_start + f as u32,
                    ids[&edge(b, v)],
                ]);
            }
        }
        points = next;
        faces = refined;
    }
    let mut triangles = Vec::with_capacity(faces.len() * 2);
    for (i, q) in faces.iter().enumerate() {
        tick(i, stop)?;
        triangles.extend([[q[0], q[1], q[2]], [q[0], q[2], q[3]]]);
    }
    let out = Mesh {
        vertices: points,
        triangles,
    };
    validate_mesh(&out, stop)?;
    Ok(out)
}
fn closest_segment(p: P, a: P, b: P) -> P {
    let d = sub(b, a);
    let dd = dot(d, d);
    if dd == 0. {
        a
    } else {
        add(a, mul(d, (dot(sub(p, a), d) / dd).clamp(0., 1.)))
    }
}
pub fn closest_point(m: &Mesh, p: P, stop: &dyn Fn() -> bool) -> Result<P, String> {
    validate_mesh(m, stop)?;
    if !finite_point(p) {
        return Err("無効な点です。".into());
    }
    let mut best = None::<(f64, P)>;
    for (i, t) in m.triangles.iter().enumerate() {
        tick(i, stop)?;
        let [a, b, c] = t.map(|v| m.vertices[v as usize]);
        let n = cross(sub(b, a), sub(c, a));
        let q = sub(p, mul(n, dot(sub(p, a), n) / dot(n, n)));
        let inside = dot(cross(sub(b, a), sub(q, a)), n) >= 0.
            && dot(cross(sub(c, b), sub(q, b)), n) >= 0.
            && dot(cross(sub(a, c), sub(q, c)), n) >= 0.;
        let mut consider = |x: P| {
            let d = sub(p, x);
            let d = dot(d, d);
            if best.is_none_or(|(s, _)| d < s) {
                best = Some((d, x));
            }
        };
        if inside {
            consider(q);
        } else {
            for (u, v) in [(a, b), (b, c), (c, a)] {
                consider(closest_segment(p, u, v));
            }
        }
    }
    best.map(|(_, p)| p)
        .ok_or_else(|| "面がありません。".into())
}
fn bounds(m: &Mesh) -> Result<(P, P), String> {
    let mut lo = *m.vertices.first().ok_or("頂点がありません。")?;
    let mut hi = lo;
    for p in &m.vertices {
        for j in 0..3 {
            lo[j] = lo[j].min(p[j]);
            hi[j] = hi[j].max(p[j]);
        }
    }
    Ok((lo, hi))
}
fn mesh_arg<'a>(a: &[&'a Value]) -> Result<&'a Mesh, String> {
    match shape(a)? {
        Value::Mesh(m) => Ok(m),
        _ => Err("メッシュを接続してください。".into()),
    }
}
fn numbers(a: impl IntoIterator<Item = f64>) -> Vec<Value> {
    a.into_iter().map(Value::Number).collect()
}
fn count(n: usize) -> Result<(), String> {
    if n > MAX_ITEMS {
        Err(format!("取り出す要素が{MAX_ITEMS}個を超えます。"))
    } else {
        Ok(())
    }
}
const M: InputSpec = g("mesh", "メッシュ", DataType::Mesh);
const A: InputSpec = i("a", "頂点 A", 0., 0., MAX_VERTICES as f64 - 1.);
const B: InputSpec = i("b", "頂点 B", 1., 0., MAX_VERTICES as f64 - 1.);
const C: InputSpec = i("c", "頂点 C", 2., 0., MAX_VERTICES as f64 - 1.);
const D: InputSpec = i("d", "頂点 D", 3., 0., MAX_VERTICES as f64 - 1.);
const INDICES: InputSpec = g("indices", "削除番号", DataType::Number);
const PATTERN: InputSpec = g("pattern", "パターン (0:保持 1:削除)", DataType::Number);
macro_rules! specs{
    ($($kind:ident => ($label:literal,$description:literal,$keywords:literal,$output:ident,[$($input:expr),*]);)+)=>{
        pub fn spec(kind:NodeKind)->&'static NodeSpec{
            match kind{
                $(NodeKind::$kind=>{static S:NodeSpec=NodeSpec{label:$label,category:"立体",
                    description:$description,keywords:$keywords,inputs:&[$($input),*],output:DataType::$output};&S},)+
                _=>unreachable!("mesh node dispatch"),
            }
        }
    }
}
specs! {
    MeshArea => ("メッシュ面積","三角形の表面積の合計。","mesh area 面積",Number,[M]);
    MeshVolume => ("メッシュ符号付き体積","閉じた向きのそろったメッシュの符号付き体積。交差・入れ子は未検査。","mesh signed volume 体積",Number,[M]);
    MeshCenter => ("メッシュ表面重心","面積で重み付けした表面の中心。質量重心ではありません。","mesh surface centroid center 重心",Point,[M]);
    MeshDimensions => ("メッシュ寸法","XYZ方向の外接箱の長さを点の3成分として出力。","mesh dimensions bounds size 寸法",Point,[M]);
    MeshMass => ("材料重量概算","符号付き体積と密度から100%充填相当のgを概算。交差・入れ子・実際のスライスは未検査。座標はmm。","mesh material mass weight density 重量",Number,[M,n("density","密度 g/cm³",1.24,0.01,30.,0.01)]);
    MeshAudit => ("メッシュ診断","境界辺、非多様体辺、面方向不一致、非多様体頂点、同一座標頂点、重複面、退化面、孤立頂点の順に8数値を出力。自己交差は未検査。","mesh topology audit diagnostics nonmanifold 検査",Number,[M]);
    MeshVertices => ("メッシュ頂点","頂点をインデックス順のリストに取り出します。","mesh vertices deconstruct 頂点",Point,[M]);
    MeshFaces => ("メッシュ面番号","三角形の頂点番号を3個ずつ連続して出力（0始まり）。","mesh faces indices deconstruct 面",Number,[M]);
    MeshNormals => ("メッシュ面法線","単位面法線をXYZ点として出力。","mesh face normals 法線",Point,[M]);
    MeshEdges => ("メッシュ辺","インデックスで共有する辺を1本ずつ線分として出力。","mesh edges topology 辺",Curve,[M]);
    MeshBoundary => ("メッシュ開境界","1面だけが接続する辺を線分として出力。","mesh boundary naked edges 開境界",Curve,[M]);
    MeshClosest => ("メッシュ最近点","全三角形を走査し、指定点に最も近い表面上の点を返します。","mesh closest point 最近点",Point,[M,g("point","点",DataType::Point)]);
    MeshFlip => ("メッシュ反転","全三角形の頂点順を反転します。","mesh flip normals 反転",Mesh,[M]);
    MeshWeldExact => ("メッシュ同一点結合","完全に同じXYZの頂点だけを結合。面の崩壊はエラー。Booleanではありません。","mesh weld merge exact vertices 溶接",Mesh,[M]);
    MeshUnweld => ("メッシュ全辺分離","各三角形を独立した頂点に分離します。","mesh unweld split faces 分離",Mesh,[M]);
    MeshUnify => ("メッシュ法線統一","隣接面の向きを一貫させます。内外は自動決定しません。","mesh unify normals 法線統一",Mesh,[M]);
    MeshSmooth => ("メッシュ平滑化","境界を固定したLaplacian平滑化。体積維持・自己交差防止は保証しません。","mesh smooth laplacian 平滑化",Mesh,[M,i("iterations","反復",1.,0.,20.),n("amount","強さ",0.25,0.,1.,0.05)]);
    MeshSubdivide => ("メッシュ中点細分化","面を4分割し共有辺を保ちます。曲面への平滑化は行いません。","mesh midpoint subdivide 細分化",Mesh,[M,i("levels","段数",1.,0.,5.)]);
    MeshCatmullClark => ("SubD Catmull–Clark近似","三角形制御メッシュから有限回の四角形細分化。境界ルールあり。極限曲面・内部クリースは未実装。","subd catmull clark subdivision 細分化",Mesh,[M,i("levels","段数",1.,0.,5.)]);
    MeshOnBed => ("メッシュ床置き","XY外接箱の中心を原点、最低Zを0に移動。配置同士の衝突は未検査。","mesh bed center placement 造形台",Mesh,[M]);
    MeshDeleteFace => ("メッシュ面削除","0始まりの面番号を1枚削除します。穴が開く可能性があります。","mesh delete face cull 面削除",Mesh,[M,i("index","面番号",0.,0.,499999.)]);
    MeshSelfContact => ("メッシュ自己接触候補","共有頂点のない面同士の接触ペア数。近似判定。隣接面の重なりは未検査。上限超過はエラー。","mesh self intersection contact 自己交差",Number,[M]);
    MeshThicknessSample => ("肉厚サンプル推定","面中心から内向き法線に沿った出口距離の最小サンプル値と実サンプル数。真の最小肉厚ではありません。","mesh thickness thin wall sample 肉厚",Number,[M,i("samples","サンプル数",64.,1.,512.)]);
    MeshCompact => ("未使用頂点削除","面が参照しない頂点を削除して面番号を更新します。","mesh compact cull unused vertices 整理",Mesh,[M]);
    MeshNonManifold => ("メッシュ非多様体辺","3枚以上の面が共有する辺を線分で出力。","mesh non manifold edges 非多様体 辺",Curve,[M]);
    MeshConstruct => ("メッシュ構築","各枝の点リストと0始まりの三角形番号列（3個ずつ）を使用。","construct mesh from points indices 構築",Mesh,[g("vertices","頂点リスト",DataType::Point),g("faces","面番号リスト",DataType::Number)]);
    MeshTriangle => ("三角メッシュ面番号","3個の0始まりの頂点番号を面の順序でまとめます。メッシュ構築の面番号に接続できます。","mesh triangle face index メッシュ三角形",Number,[A,B,C]);
    MeshQuad => ("四角メッシュ面番号","4個の0始まりの頂点番号を対角線ACで分割した2枚の三角形の面番号に変換します。","mesh quad face index メッシュ四角形",Number,[A,B,C,D]);
    MeshFaceBoundaries => ("メッシュ面境界","各三角形の境界を閉じたポリラインとして面番号順に出力します。","mesh face boundaries polyline 面境界",Curve,[M]);
    MeshFaceCenters => ("メッシュ面中心","各三角形の3頂点の平均を面番号順に出力します。","mesh face centers face normals centroid 面中心",Point,[M]);
    MeshFaceCircles => ("メッシュ面外接円","各三角形の頂点を通る精密外接円を面番号順に出力します。","mesh face circles circumcircle 面外接円",Curve,[M]);
    MeshFaceAspectRatio => ("メッシュ面縦横比","最長辺を底辺とした高さをその辺の長さで割り、面番号順に出力します。","mesh face circles ratio altitude quality 面品質",Number,[M]);
    MeshDisjoint => ("メッシュの連結成分","頂点番号を共有する面を連結成分に分けます。座標だけが一致する頂点は結合しません。未使用頂点は省きます。","mesh disjoint split components メッシュ分離",Mesh,[M]);
    MeshDeleteFaces => ("メッシュ面の一括削除","枝ごとに1個のメッシュから、指定した0始まりの三角形を削除します。","mesh delete faces indices 面削除",Mesh,[M,INDICES]);
    MeshCullFaces => ("メッシュ面のパターン除去","枝ごとに1個のメッシュを処理し、1の面を削除して0の面を保持します。パターンは繰り返し、空なら全て保持します。","mesh cull faces pattern 面除去",Mesh,[M,PATTERN]);
    MeshDeleteVertices => ("メッシュ頂点の一括削除","枝ごとに1個のメッシュから指定した0始まりの頂点と接する三角形を削除し、番号を振り直します。","mesh delete vertices indices 頂点削除",Mesh,[M,INDICES]);
    MeshCullVertices => ("メッシュ頂点のパターン除去","枝ごとに1個のメッシュを処理し、1の頂点と接する三角形を削除します。パターンは繰り返し、空なら全て保持します。","mesh cull vertices pattern 頂点除去",Mesh,[M,PATTERN]);
    MeshInteriorEdges => ("メッシュの内部辺","ちょうど2枚の面が頂点番号で共有する辺を線分で出力します。","mesh edges interior internal edges 内部辺",Curve,[M]);
    MeshVertexNormals => ("メッシュ頂点法線","面積で重み付けした単位法線を頂点番号順にXYZ点で出力します。法線を定められない頂点はゼロです。","mesh vertex normals deconstruct 頂点法線",Point,[M]);
}
pub fn run_list(lists: &[&[Value]], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let pts = lists.first().ok_or("頂点リストがありません。")?;
    let ids = lists.get(1).ok_or("面番号リストがありません。")?;
    if pts.is_empty() || ids.is_empty() || ids.len() % 3 != 0 {
        return Err("頂点と3個単位の面番号が必要です。".into());
    }
    limits(pts.len(), ids.len() / 3)?;
    let mut vertices = Vec::with_capacity(pts.len());
    for (k, v) in pts.iter().enumerate() {
        tick(k, stop)?;
        let Value::Point(p) = v else {
            return Err("点リストが必要です。".into());
        };
        vertices.push(*p);
    }
    let mut triangles = Vec::with_capacity(ids.len() / 3);
    for (k, vs) in ids.as_chunks::<3>().0.iter().enumerate() {
        tick(k, stop)?;
        let mut t = [0; 3];
        for j in 0..3 {
            let x = vs[j].number()?;
            if x < 0. || x.fract() != 0. || x >= vertices.len() as f64 {
                return Err("面番号が無効です。".into());
            }
            t[j] = x as u32;
        }
        triangles.push(t);
    }
    as_mesh(
        Mesh {
            vertices,
            triangles,
        },
        stop,
    )
}
pub fn run_item(
    kind: NodeKind,
    a: &[&Value],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    if matches!(kind, MeshTriangle | MeshQuad) {
        return face_indices(a, kind == MeshQuad);
    }
    let m = mesh_arg(a)?;
    raw_check(m, stop)?;
    match kind {
        MeshArea => Ok(numbers([area_center(m, stop)?.0])),
        MeshVolume => Ok(numbers([signed_volume(m, stop)?])),
        MeshCenter => {
            let (area, p) = area_center(m, stop)?;
            if area == 0. {
                return Err("面積がありません。".into());
            }
            Ok(vec![Value::Point(p)])
        }
        MeshDimensions => {
            let (lo, hi) = bounds(m)?;
            Ok(vec![Value::Point(sub(hi, lo))])
        }
        MeshMass => {
            let v = signed_volume(m, stop)?;
            if v <= 0. {
                return Err("正の体積が必要です。面方向を確認してください。".into());
            }
            Ok(numbers([v * num(a, 1)? / 1000.]))
        }
        MeshAudit => {
            let r = audit(m, stop)?;
            Ok(numbers(
                [
                    r.boundary_edges,
                    r.non_manifold_edges,
                    r.orientation_conflicts,
                    r.non_manifold_vertices,
                    r.duplicate_vertices,
                    r.duplicate_faces,
                    r.degenerate_faces,
                    r.isolated_vertices,
                ]
                .map(|x| x as f64),
            ))
        }
        MeshVertices => {
            count(m.vertices.len())?;
            Ok(m.vertices.iter().copied().map(Value::Point).collect())
        }
        MeshFaces => {
            count(3 * m.triangles.len())?;
            Ok(numbers(m.triangles.iter().flatten().map(|&x| x as f64)))
        }
        MeshNormals => {
            count(m.triangles.len())?;
            let mut values = Vec::new();
            for (i, t) in m.triangles.iter().enumerate() {
                tick(i, stop)?;
                let [a, b, c] = t.map(|v| m.vertices[v as usize]);
                let n = cross(sub(b, a), sub(c, a));
                let l = length(n);
                if l < 1e-12 {
                    return Err("退化面の法線は未定義です。".into());
                }
                values.push(Value::Point(mul(n, 1. / l)));
            }
            Ok(values)
        }
        MeshFaceBoundaries | MeshFaceCenters | MeshFaceCircles | MeshFaceAspectRatio => {
            face_properties(kind, m, stop)
        }
        MeshVertexNormals => vertex_normals(m, stop),
        MeshDisjoint => disjoint(m, stop),
        MeshEdges | MeshBoundary | MeshNonManifold | MeshInteriorEdges => {
            let es = topology(m, stop)?;
            let select = |uses: &Vec<(usize, bool)>| {
                kind == MeshEdges
                    || (kind == MeshBoundary && uses.len() == 1)
                    || (kind == MeshInteriorEdges && uses.len() == 2)
                    || (kind == MeshNonManifold && uses.len() > 2)
            };
            let total = es.values().filter(|us| select(us)).count();
            count(total)?;
            let mut values = Vec::new();
            for (i, (&(a, b), us)) in es.iter().enumerate() {
                tick(i, stop)?;
                if select(us) {
                    values.push(Value::Curve(Arc::new(Curve {
                        points: vec![m.vertices[a as usize], m.vertices[b as usize]],
                        closed: false,
                        geometry: None,
                    })));
                }
            }
            Ok(values)
        }
        MeshSelfContact => Ok(numbers([
            crate::mesh_geometry_checks::nonadjacent_contacts(m, stop)? as f64,
        ])),
        MeshThicknessSample => {
            let (w, n) =
                crate::mesh_geometry_checks::sampled_thickness(m, num(a, 1)? as usize, stop)?;
            Ok(numbers([w, n as f64]))
        }
        MeshCompact => {
            let mut used = vec![false; m.vertices.len()];
            for (i, t) in m.triangles.iter().enumerate() {
                tick(i, stop)?;
                for &v in t {
                    used[v as usize] = true;
                }
            }
            let mut out = Mesh::default();
            let mut map = vec![0u32; m.vertices.len()];
            for (i, &p) in m.vertices.iter().enumerate() {
                tick(i, stop)?;
                if used[i] {
                    map[i] = out.vertices.len() as u32;
                    out.vertices.push(p);
                }
            }
            for (i, t) in m.triangles.iter().enumerate() {
                tick(i, stop)?;
                out.triangles.push(t.map(|v| map[v as usize]));
            }
            as_mesh(out, stop)
        }
        MeshClosest => {
            let Some(Value::Point(p)) = a.get(1) else {
                return Err("点を接続してください。".into());
            };
            Ok(vec![Value::Point(closest_point(m, *p, stop)?)])
        }
        MeshFlip => {
            let mut out = m.clone();
            for (i, t) in out.triangles.iter_mut().enumerate() {
                tick(i, stop)?;
                t.swap(1, 2);
            }
            as_mesh(out, stop)
        }
        MeshWeldExact => as_mesh(weld_exact(m, stop)?, stop),
        MeshUnweld => {
            let n = 3 * m.triangles.len();
            limits(n, m.triangles.len())?;
            let mut out = Mesh::default();
            for (i, t) in m.triangles.iter().enumerate() {
                tick(i, stop)?;
                let start = out.vertices.len() as u32;
                out.vertices.extend(t.map(|v| m.vertices[v as usize]));
                out.triangles.push([start, start + 1, start + 2]);
            }
            as_mesh(out, stop)
        }
        MeshUnify => as_mesh(unify(m, stop)?, stop),
        MeshSmooth => as_mesh(smooth(m, num(a, 1)? as usize, num(a, 2)?, stop)?, stop),
        MeshSubdivide => as_mesh(subdivide(m, num(a, 1)? as usize, stop)?, stop),
        MeshCatmullClark => as_mesh(catmull_clark(m, num(a, 1)? as usize, stop)?, stop),
        MeshOnBed => {
            let (lo, hi) = bounds(m)?;
            let origin = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, lo[2]];
            let mut out = m.clone();
            for (i, p) in out.vertices.iter_mut().enumerate() {
                tick(i, stop)?;
                *p = sub(*p, origin);
            }
            as_mesh(out, stop)
        }
        MeshDeleteFace => {
            let k = num(a, 1)? as usize;
            if k >= m.triangles.len() {
                return Err("面番号が範囲外です。".into());
            }
            let mut out = m.clone();
            out.triangles.remove(k);
            as_mesh(out, stop)
        }
        _ => Err("メッシュノードの評価方式が不正です。".into()),
    }
}

pub(crate) fn is_list_kind(kind: NodeKind) -> bool {
    use NodeKind::*;
    matches!(
        kind,
        MeshConstruct | MeshDeleteFaces | MeshCullFaces | MeshDeleteVertices | MeshCullVertices
    )
}

pub(crate) fn run_list_kind(
    kind: NodeKind,
    lists: &[&[Value]],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    if kind == MeshConstruct {
        return run_list(lists, stop);
    }
    let [Value::Mesh(mesh)] = lists[0] else {
        return Err("各枝のメッシュ入力にはメッシュ1個が必要です。".into());
    };
    let by_vertex = matches!(kind, MeshDeleteVertices | MeshCullVertices);
    let by_pattern = matches!(kind, MeshCullFaces | MeshCullVertices);
    let size = if by_vertex {
        mesh.vertices.len()
    } else {
        mesh.triangles.len()
    };
    raw_check(mesh, stop)?;
    let removed = removed_elements(lists[1], size, by_pattern, stop)?;
    if !removed.contains(&true) {
        return Ok(vec![Value::Mesh(mesh.clone())]);
    }
    if by_vertex {
        as_mesh(remove_vertices(mesh, &removed, stop)?, stop)
    } else {
        let mut triangles = Vec::with_capacity(mesh.triangles.len());
        for (i, &triangle) in mesh.triangles.iter().enumerate() {
            tick(i, stop)?;
            if !removed[i] {
                triangles.push(triangle);
            }
        }
        as_mesh(
            Mesh {
                vertices: mesh.vertices.clone(),
                triangles,
            },
            stop,
        )
    }
}

fn removed_elements(
    selection: &[Value],
    size: usize,
    by_pattern: bool,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<bool>, String> {
    let mut removed = vec![false; size];
    if by_pattern && !selection.is_empty() {
        let mut pattern = Vec::with_capacity(selection.len());
        for (i, value) in selection.iter().enumerate() {
            tick(i, stop)?;
            pattern.push(match value.number()? {
                0. => false,
                1. => true,
                _ => return Err("除去パターンには0または1が必要です。".into()),
            });
        }
        for (i, flag) in removed.iter_mut().enumerate() {
            tick(i, stop)?;
            *flag = pattern[i % pattern.len()];
        }
    } else {
        for (i, value) in selection.iter().enumerate() {
            tick(i, stop)?;
            let index = value.number()?;
            if !(0. ..size as f64).contains(&index) || index.fract() != 0. {
                return Err("削除番号は範囲内の整数である必要があります。".into());
            }
            removed[index as usize] = true;
        }
    }
    Ok(removed)
}

fn remove_vertices(mesh: &Mesh, removed: &[bool], stop: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let mut result = Mesh::default();
    let mut index_map = vec![None; mesh.vertices.len()];
    for (i, &point) in mesh.vertices.iter().enumerate() {
        tick(i, stop)?;
        if !removed[i] {
            index_map[i] = Some(result.vertices.len() as u32);
            result.vertices.push(point);
        }
    }
    for (i, triangle) in mesh.triangles.iter().enumerate() {
        tick(i, stop)?;
        if let [Some(a), Some(b), Some(c)] = triangle.map(|index| index_map[index as usize]) {
            result.triangles.push([a, b, c]);
        }
    }
    Ok(result)
}

fn face_indices(args: &[&Value], quad: bool) -> Result<Vec<Value>, String> {
    let mut indices = Vec::with_capacity(args.len());
    for value in args {
        let index = value.number()?;
        if !(0. ..MAX_VERTICES as f64).contains(&index) || index.fract() != 0. {
            return Err("面の頂点番号は範囲内の整数である必要があります。".into());
        }
        if indices.contains(&index) {
            return Err("面の頂点番号が重複しています。".into());
        }
        indices.push(index);
    }
    if quad {
        let [a, b, c, d] = indices[..] else {
            return Err("四角形には4個の頂点番号が必要です。".into());
        };
        Ok(numbers([a, b, c, a, c, d]))
    } else {
        Ok(numbers(indices))
    }
}

fn face_properties(
    kind: NodeKind,
    mesh: &Mesh,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    count(mesh.triangles.len())?;
    let mut result = Vec::with_capacity(mesh.triangles.len());
    for (i, triangle) in mesh.triangles.iter().enumerate() {
        tick(i, stop)?;
        let [a, b, c] = triangle.map(|index| mesh.vertices[index as usize]);
        let value = match kind {
            MeshFaceBoundaries => crate::curve_nodes::polyline(vec![a, b, c], true)?,
            MeshFaceCenters => Value::Point(mul(add(add(a, b), c), 1. / 3.)),
            MeshFaceCircles => crate::curve_nodes::preview(
                crate::curve_geometry::CurveGeometry::through_three(a, b, c, true)?,
                32,
                stop,
            )?,
            MeshFaceAspectRatio => {
                let [ab, ac, bc] = [sub(b, a), sub(c, a), sub(c, b)];
                let longest_squared = dot(ab, ab).max(dot(ac, ac)).max(dot(bc, bc));
                if longest_squared == 0. {
                    return Err("面の辺長が全てゼロです。".into());
                }
                Value::Number(length(cross(ab, ac)) / longest_squared)
            }
            _ => unreachable!("mesh face property dispatch"),
        };
        result.push(value);
    }
    Ok(result)
}

fn vertex_normals(mesh: &Mesh, stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    count(mesh.vertices.len())?;
    let mut normals = vec![[0.; 3]; mesh.vertices.len()];
    for (i, triangle) in mesh.triangles.iter().enumerate() {
        tick(i, stop)?;
        let [a, b, c] = triangle.map(|index| mesh.vertices[index as usize]);
        let weighted_normal = cross(sub(b, a), sub(c, a));
        for &index in triangle {
            normals[index as usize] = add(normals[index as usize], weighted_normal);
        }
    }
    let mut result = Vec::with_capacity(normals.len());
    for (i, normal) in normals.into_iter().enumerate() {
        tick(i, stop)?;
        let norm = length(normal);
        result.push(Value::Point(if norm > 0. {
            mul(normal, 1. / norm)
        } else {
            [0.; 3]
        }));
    }
    Ok(result)
}

fn disjoint(mesh: &Mesh, stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let mut parents: Vec<_> = (0..mesh.vertices.len()).collect();
    for (i, &[a, b, c]) in mesh.triangles.iter().enumerate() {
        tick(i, stop)?;
        union(&mut parents, a as usize, b as usize);
        union(&mut parents, a as usize, c as usize);
    }

    let mut components = BTreeMap::new();
    let mut pieces: Vec<Mesh> = Vec::new();
    let mut vertex_indices = vec![None; mesh.vertices.len()];
    for (i, triangle) in mesh.triangles.iter().enumerate() {
        tick(i, stop)?;
        let representative = root(&mut parents, triangle[0] as usize);
        let piece = match components.get(&representative) {
            Some(&index) => index,
            None => {
                let index = pieces.len();
                count(index + 1)?;
                components.insert(representative, index);
                pieces.push(Mesh::default());
                index
            }
        };
        let out = &mut pieces[piece];
        out.triangles.push(triangle.map(|index| {
            let index = index as usize;
            *vertex_indices[index].get_or_insert_with(|| {
                let new_index = out.vertices.len() as u32;
                out.vertices.push(mesh.vertices[index]);
                new_index
            })
        }));
    }
    pieces
        .into_iter()
        .map(|piece| {
            validate_mesh(&piece, stop)?;
            Ok(Value::Mesh(Arc::new(piece)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tetra() -> Mesh {
        Mesh {
            vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            triangles: vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        }
    }
    fn no() -> bool {
        false
    }
    #[test]
    fn closed_and_open() {
        let mut m = tetra();
        assert!(audit(&m, &no).unwrap().closed_oriented());
        assert!((signed_volume(&m, &no).unwrap() - 1. / 6.).abs() < 1e-12);
        assert!((area_center(&m, &no).unwrap().0 - (1.5 + 3f64.sqrt() / 2.)).abs() < 1e-12);
        m.triangles.pop();
        assert_eq!(audit(&m, &no).unwrap().boundary_edges, 3);
        assert!(signed_volume(&m, &no).is_err());
    }
    #[test]
    fn reversed_and_unified() {
        let mut m = tetra();
        m.triangles[1].swap(1, 2);
        assert_eq!(audit(&m, &no).unwrap().orientation_conflicts, 3);
        let fixed = unify(&m, &no).unwrap();
        assert!(audit(&fixed, &no).unwrap().closed_oriented());
        assert!((signed_volume(&fixed, &no).unwrap() - 1. / 6.).abs() < 1e-12);
    }
    #[test]
    fn vertex_bowtie() {
        let mut m = tetra();
        m.vertices
            .extend([[0., 0., -1.], [-1., 0., 0.], [0., -1., 0.]]);
        m.triangles
            .extend([[0, 4, 5], [0, 6, 4], [4, 6, 5], [5, 6, 0]]);
        assert_eq!(audit(&m, &no).unwrap().non_manifold_vertices, 1);
        assert!(signed_volume(&m, &no).is_err());
    }
    #[test]
    fn midpoint_and_subd() {
        let m = tetra();
        let s = subdivide(&m, 2, &no).unwrap();
        assert_eq!(s.triangles.len(), 64);
        assert!(audit(&s, &no).unwrap().closed_oriented());
        assert!((signed_volume(&s, &no).unwrap() - 1. / 6.).abs() < 1e-12);
        let c = catmull_clark(&m, 2, &no).unwrap();
        assert_eq!(c.triangles.len(), 96);
        assert!(audit(&c, &no).unwrap().closed_oriented());
    }
    #[test]
    fn subd_boundary() {
        let m = Mesh {
            vertices: vec![[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]],
            triangles: vec![[0, 1, 2]],
        };
        let s = catmull_clark(&m, 1, &no).unwrap();
        assert_eq!(s.vertices[0], [0.25, 0.25, 0.]);
        assert_eq!(s.triangles.len(), 6);
        assert_eq!(audit(&s, &no).unwrap().boundary_edges, 6);
    }
    #[test]
    fn weld_and_nearest() {
        let mut m = tetra();
        m.vertices.push([-0., 0., 0.]);
        m.triangles[0][0] = 4;
        assert_eq!(audit(&m, &no).unwrap().duplicate_vertices, 1);
        assert_eq!(weld_exact(&m, &no).unwrap(), tetra());
        assert_eq!(
            closest_point(&tetra(), [0.25, 0.25, -1.], &no).unwrap(),
            [0.25, 0.25, 0.]
        );
    }
    #[test]
    fn malformed_and_cancelled() {
        let mut m = tetra();
        m.triangles[0][0] = 100;
        assert!(audit(&m, &no).is_err());
        assert!(subdivide(&tetra(), 1, &|| true).is_err());
        assert!(catmull_clark(&tetra(), 6, &no).is_err());
    }
    #[test]
    fn smooth_boundary_fixed() {
        let m = Mesh {
            vertices: vec![[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]],
            triangles: vec![[0, 1, 2]],
        };
        assert_eq!(smooth(&m, 5, 0.5, &no).unwrap(), m);
    }
}
