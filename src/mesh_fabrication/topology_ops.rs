//! Deterministic indexed surface queries, sharing the fabrication edge map and audit.
//! No coordinate welding, implicit repairs or geometric solid certification.
use super::{Audit, Uses, audit, root, tick, topology, union};
use crate::engine::cancelled;
use crate::model::{MAX_ITEMS, MAX_TOTAL_WEIGHT, Mesh};

/// Euler counts of a finite indexed triangle complex; not a genus certificate.
/// Isolated vertices are included in euler_all and excluded from euler_surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologySummary {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    pub isolated_vertices: usize,
    pub edge_components: usize,
    pub boundary_edges: usize,
    pub euler_all: i64,
    pub euler_surface: i64,
}

fn checked_surface(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Audit, String> {
    let report = audit(m, stop)?;
    if report.duplicate_faces != 0 || report.degenerate_faces != 0 {
        return Err("重複面または退化面があります。先にメッシュを修復してください。".into());
    }
    Ok(report)
}

// Label every face, including faces on non-manifold edges. Stable ordering by
// the smallest original face index in each component, independent of DSU roots.
fn face_groups(
    edges: &Uses,
    face_count: usize,
    stop: &dyn Fn() -> bool,
) -> Result<(Vec<usize>, usize), String> {
    cancelled(stop)?;
    let mut parent: Vec<usize> = (0..face_count).collect();
    for (i, uses) in edges.values().enumerate() {
        tick(i, stop)?;
        if let Some(&(first, _)) = uses.first() {
            for (j, &(face, _)) in uses.iter().skip(1).enumerate() {
                tick(j, stop)?;
                union(&mut parent, first, face);
            }
        }
    }
    let mut root_labels = vec![usize::MAX; face_count];
    let mut labels = Vec::with_capacity(face_count);
    let mut count = 0;
    for face in 0..face_count {
        tick(face, stop)?;
        let label = &mut root_labels[root(&mut parent, face)];
        if *label == usize::MAX {
            *label = count;
            count += 1;
        }
        labels.push(*label);
    }
    Ok((labels, count))
}

pub fn topology_summary(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<TopologySummary, String> {
    let report = checked_surface(m, stop)?;
    let edges = topology(m, stop)?;
    let (_, edge_components) = face_groups(&edges, m.triangles.len(), stop)?;
    let (v, e, f) = (m.vertices.len(), edges.len(), m.triangles.len());
    let euler_all = v as i64 - e as i64 + f as i64;
    Ok(TopologySummary {
        vertices: v,
        edges: e,
        faces: f,
        isolated_vertices: report.isolated_vertices,
        edge_components,
        boundary_edges: report.boundary_edges,
        euler_all,
        euler_surface: euler_all - report.isolated_vertices as i64,
    })
}

/// Oriented index rings without repeating the initial vertex. A boundary
/// follows the winding of its incident face. Closed surfaces return no rings.
/// Reject branching/non-manifold fans and inconsistent orientation instead of
/// guessing a continuation. This does not test geometric self-intersections.
pub fn boundary_loops(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Vec<Vec<u32>>, String> {
    let report = checked_surface(m, stop)?;
    if report.non_manifold_edges != 0
        || report.non_manifold_vertices != 0
        || report.orientation_conflicts != 0
    {
        return Err("境界が分岐しているか、面の向きが一致していません。".into());
    }
    if report.boundary_edges == 0 {
        return Ok(Vec::new());
    }
    let edges = topology(m, stop)?;
    let mut next = vec![None; m.vertices.len()];
    let mut incoming = vec![false; m.vertices.len()];
    for (i, (&(a, b), uses)) in edges.iter().enumerate() {
        tick(i, stop)?;
        if let [(_, forward)] = uses.as_slice() {
            let (from, to) = if *forward { (a, b) } else { (b, a) };
            if next[from as usize].replace(to).is_some()
                || std::mem::replace(&mut incoming[to as usize], true)
            {
                return Err("境界に複数の接続があります。".into());
            }
        }
    }
    let mut loops = Vec::new();
    for start in 0..next.len() {
        tick(start, stop)?;
        if next[start].is_none() {
            continue;
        }
        let mut ring = Vec::new();
        let mut current = start as u32;
        loop {
            tick(ring.len(), stop)?;
            ring.push(current);
            current = next[current as usize]
                .take()
                .ok_or("境界が閉じていません。")?;
            if current == start as u32 {
                break;
            }
        }
        if ring.len() < 3 {
            return Err("境界が3頂点未満です。".into());
        }
        loops.push(ring);
    }
    Ok(loops)
}

/// Edge-connected face islands. Coordinates and triangle winding are retained;
/// isolated vertices are omitted and vertices shared only across islands are
/// duplicated. Non-manifold edges remain connected; this is not a repair.
pub fn split_islands(m: &Mesh, stop: &dyn Fn() -> bool) -> Result<Vec<Mesh>, String> {
    checked_surface(m, stop)?;
    let edges = topology(m, stop)?;
    let (labels, count) = face_groups(&edges, m.triangles.len(), stop)?;
    drop(edges);
    if count > MAX_ITEMS {
        return Err("メッシュ島の個数が上限を超えました。".into());
    }
    let mut groups = vec![Vec::new(); count];
    for (face, label) in labels.into_iter().enumerate() {
        tick(face, stop)?;
        groups[label].push(face);
    }
    let mut owner = vec![usize::MAX; m.vertices.len()];
    let mut local = vec![0u32; m.vertices.len()];
    let mut output = Vec::with_capacity(count);
    let mut weight = 0usize;
    for (id, faces) in groups.into_iter().enumerate() {
        tick(id, stop)?;
        let mut island = Mesh::default();
        for (i, face) in faces.into_iter().enumerate() {
            tick(i, stop)?;
            let t = m.triangles[face].map(|v| {
                let v = v as usize;
                if owner[v] != id {
                    owner[v] = id;
                    local[v] = island.vertices.len() as u32;
                    island.vertices.push(m.vertices[v]);
                    weight += 1;
                }
                local[v]
            });
            weight += 3;
            if weight > MAX_TOTAL_WEIGHT / 2 {
                return Err("分離したメッシュの合計サイズが上限を超えました。".into());
            }
            island.triangles.push(t);
        }
        output.push(island);
    }
    Ok(output)
}
