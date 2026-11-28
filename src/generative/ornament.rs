//! Bounded ornament curves and planar weighted cell kernels.
//! Independent Rust implementation; no upstream plugin code.
use super::{P2, area, clean, count, curve, d2, list, setting};
use crate::engine::{cancelled, num};
use crate::model::*;
use std::f64::consts::{FRAC_PI_2, TAU};

static TRUCHET: NodeSpec = NodeSpec {
    label: "トルシェタイル",
    category: "曲線",
    description: "正方形の各タイルに2本の四分円を配置。シードで向きを選び、隣のタイルと中点で接続します。折れ線近似。",
    keywords: "truchet tiles arc pattern maze トルシェ タイル 迷路 模様",
    inputs: &[
        i("nx", "列", 8., 1., 64.),
        i("ny", "行", 6., 1., 64.),
        n("size", "タイル辺長", 10., 0.01, 1000., 0.5),
        i("segments", "円弧分割", 12., 1., 64.),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Curve,
};
static ASANOHA: NodeSpec = NodeSpec {
    label: "麻の葉・組子格子",
    category: "曲線",
    description: "正三角形格子と各三角形の重心への3本の線で麻の葉を描きます。共有格子辺は重複させません。組子の中心線であり、接合加工は行いません。",
    keywords: "asanoha kumiko hemp leaf japanese ornament 麻の葉 組子 和柄",
    inputs: &[
        i("nx", "列", 6., 1., 32.),
        i("ny", "行", 5., 1., 32.),
        n("size", "三角形辺長", 12., 0.01, 1000., 0.5),
    ],
    output: DataType::Curve,
};
static SHIPPO: NodeSpec = NodeSpec {
    label: "七宝つなぎ",
    category: "曲線",
    description: "正方格子上に重なる円を配置。半径比は既定で1/√2（対角の円が接する）。円全体の折れ線で、外周の切り抜きは行いません。",
    keywords: "shippo shippou circle lattice japanese ornament 七宝 和柄 円",
    inputs: &[
        i("nx", "列", 6., 1., 32.),
        i("ny", "行", 5., 1., 32.),
        n("size", "中心間隔", 12., 0.01, 1000., 0.5),
        n(
            "ratio",
            "半径 / 間隔",
            std::f64::consts::FRAC_1_SQRT_2,
            0.1,
            1.,
            0.01,
        ),
        i("segments", "八分円の分割", 6., 1., 16.),
    ],
    output: DataType::Curve,
};
static POWER: NodeSpec = NodeSpec {
    label: "重み付きボロノイ（Power）",
    category: "曲線",
    description: "距離の二乗から重み（長さの二乗単位）を引いた値で長方形を分割。重み1個は共通、または点と同数。空セルは出力しません。中心が原点の境界内のXY点、最大256個。",
    keywords: "weighted voronoi power diagram laguerre 重み付き ボロノイ 分割",
    inputs: &[
        g("points", "点リスト", DataType::Point),
        n("weights", "重み（長さ²）", 0., -1e12, 1e12, 1.),
        n("width", "境界幅", 100., 0.01, 100000., 1.),
        n("height", "境界高さ", 100., 0.01, 100000., 1.),
    ],
    output: DataType::Curve,
};
static LLOYD: NodeSpec = NodeSpec {
    label: "ボロノイ重心緩和",
    category: "配列",
    description: "一様密度の長方形内でボロノイセルの重心へ点を反復移動します。XY点最大128個、重複点不可。結果の点をボロノイへ接続できます。",
    keywords: "lloyd relaxation centroidal voronoi cvt 重心 緩和 ボロノイ",
    inputs: &[
        g("points", "点リスト", DataType::Point),
        n("width", "境界幅", 100., 0.01, 100000., 1.),
        n("height", "境界高さ", 100., 0.01, 100000., 1.),
        i("steps", "反復", 4., 1., 12.),
    ],
    output: DataType::Point,
};

pub(super) fn is_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::TruchetPattern
            | NodeKind::AsanohaPattern
            | NodeKind::ShippoPattern
            | NodeKind::PowerDiagram2d
            | NodeKind::LloydRelax2d
    )
}
pub(super) fn spec(kind: NodeKind) -> &'static NodeSpec {
    match kind {
        NodeKind::TruchetPattern => &TRUCHET,
        NodeKind::AsanohaPattern => &ASANOHA,
        NodeKind::ShippoPattern => &SHIPPO,
        NodeKind::PowerDiagram2d => &POWER,
        NodeKind::LloydRelax2d => &LLOYD,
        _ => unreachable!("not an ornament node"),
    }
}

pub(super) fn estimate(kind: NodeKind, a: &[&Value]) -> Result<(usize, usize), String> {
    let limit = if kind == NodeKind::TruchetPattern {
        64
    } else {
        32
    };
    let nx = count(a, 0, limit)?;
    let ny = count(a, 1, limit)?;
    match kind {
        NodeKind::TruchetPattern => {
            let items = 2 * nx * ny;
            Ok((items * (count(a, 3, 64)? + 1), items))
        }
        NodeKind::AsanohaPattern => {
            let items = 9 * nx * ny + nx + ny;
            Ok((2 * items, items))
        }
        NodeKind::ShippoPattern => Ok((8 * nx * ny * count(a, 4, 16)?, nx * ny)),
        _ => Err("リスト評価が必要です。".into()),
    }
}

fn tile_turn(x: usize, y: usize, seed: u64) -> bool {
    // Stateless coordinate hash: extending the rectangle does not change old tiles.
    let mut z = seed
        .wrapping_add((x as u64).wrapping_mul(0x9e3779b97f4a7c15))
        .wrapping_add((y as u64).wrapping_mul(0xd1b54a32d192ed03));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    (z ^ (z >> 31)) & 1 != 0
}

fn truchet_arc(
    x: usize,
    y: usize,
    s: f64,
    quadrant: usize,
    segments: usize,
) -> Result<Value, String> {
    // Cardinal endpoints are exact, including across the adjacent tile.
    let corners = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    let directions = [[1., 0.], [0., 1.], [-1., 0.], [0., -1.]];
    let c = corners[quadrant];
    let mut p = Vec::with_capacity(segments + 1);
    for k in 0..=segments {
        let d = if k == 0 {
            directions[quadrant]
        } else if k == segments {
            directions[(quadrant + 1) % 4]
        } else {
            let angle = FRAC_PI_2 * (quadrant as f64 + k as f64 / segments as f64);
            [angle.cos(), angle.sin()]
        };
        p.push([
            (x as f64 + c[0] + 0.5 * d[0]) * s,
            (y as f64 + c[1] + 0.5 * d[1]) * s,
            0.,
        ]);
    }
    curve(p, false)
}

fn lattice(x: usize, y: usize, s: f64) -> [f64; 3] {
    [
        (x as f64 + 0.5 * y as f64) * s,
        y as f64 * s * 3f64.sqrt() * 0.5,
        0.,
    ]
}

fn segment(out: &mut Vec<Value>, a: [f64; 3], b: [f64; 3]) -> Result<(), String> {
    out.push(curve(vec![a, b], false)?);
    Ok(())
}

fn asanoha(nx: usize, ny: usize, s: f64, cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let mut out = Vec::with_capacity(9 * nx * ny + nx + ny);
    for y in 0..=ny {
        cancelled(cancel)?;
        for x in 0..=nx {
            let p = lattice(x, y, s);
            if x < nx {
                segment(&mut out, p, lattice(x + 1, y, s))?;
            }
            if y < ny {
                segment(&mut out, p, lattice(x, y + 1, s))?;
            }
            if x < nx && y < ny {
                let q = lattice(x + 1, y, s);
                let r = lattice(x, y + 1, s);
                let t = lattice(x + 1, y + 1, s);
                segment(&mut out, q, r)?;
                for tri in [[p, q, r], [q, t, r]] {
                    let c = [
                        (tri[0][0] + tri[1][0] + tri[2][0]) / 3.,
                        (tri[0][1] + tri[1][1] + tri[2][1]) / 3.,
                        0.,
                    ];
                    for v in tri {
                        segment(&mut out, v, c)?;
                    }
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn run_item(
    kind: NodeKind,
    a: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let (_, items) = estimate(kind, a)?;
    let limit = if kind == NodeKind::TruchetPattern {
        64
    } else {
        32
    };
    let nx = count(a, 0, limit)?;
    let ny = count(a, 1, limit)?;
    let s = num(a, 2)?;
    if !(0.01..=1000.).contains(&s) {
        return Err("模様の間隔が範囲外です。".into());
    }
    if kind == NodeKind::AsanohaPattern {
        return asanoha(nx, ny, s, cancel);
    }
    let mut out = Vec::with_capacity(items);
    for y in 0..ny {
        cancelled(cancel)?;
        for x in 0..nx {
            match kind {
                NodeKind::TruchetPattern => {
                    let segments = count(a, 3, 64)?;
                    let seed = num(a, 4)?;
                    if !(0.0..=100000.).contains(&seed) || seed.fract() != 0. {
                        return Err("シードは0〜100000の整数です。".into());
                    }
                    let k = usize::from(tile_turn(x, y, seed as u64));
                    out.push(truchet_arc(x, y, s, k, segments)?);
                    out.push(truchet_arc(x, y, s, k + 2, segments)?);
                }
                NodeKind::ShippoPattern => {
                    let ratio = num(a, 3)?;
                    if !(0.1..=1.).contains(&ratio) {
                        return Err("七宝の半径比が範囲外です。".into());
                    }
                    let segments = 8 * count(a, 4, 16)?;
                    let p = (0..segments)
                        .map(|k| {
                            let t = TAU * k as f64 / segments as f64;
                            [
                                (x as f64 + ratio * t.cos()) * s,
                                (y as f64 + ratio * t.sin()) * s,
                                0.,
                            ]
                        })
                        .collect();
                    out.push(curve(p, true)?);
                }
                _ => return Err("未対応の装飾ノードです。".into()),
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Copy)]
struct Site {
    p: P2,
    weight: f64,
}

fn rectangle(w: f64, h: f64) -> Result<(f64, P2), String> {
    if !(0.01..=100000.).contains(&w) || !(0.01..=100000.).contains(&h) {
        return Err("境界寸法が範囲外です。".into());
    }
    let scale = w.max(h);
    Ok((scale, [w / scale * 0.5, h / scale * 0.5]))
}

fn sites(
    input: &[[f64; 3]],
    weights: &[f64],
    scale: f64,
    half: P2,
    reject_duplicates: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Site>, String> {
    let limit = if reject_duplicates { 128 } else { 256 };
    if input.len() > limit || (weights.len() != 1 && weights.len() != input.len()) {
        return Err("点の上限または重みの個数を確認してください。".into());
    }
    let mut out: Vec<Site> = Vec::with_capacity(input.len());
    for (index, p) in input.iter().enumerate() {
        cancelled(cancel)?;
        let p = [p[0] / scale, p[1] / scale];
        if p.iter().any(|x| !x.is_finite()) || p[0].abs() > half[0] || p[1].abs() > half[1] {
            return Err("点を境界の長方形内に配置してください。".into());
        }
        let weight = weights[if weights.len() == 1 { 0 } else { index }];
        if !(-1e12..=1e12).contains(&weight) {
            return Err("重みが範囲外です。".into());
        }
        if let Some(old) = out.iter_mut().find(|q| d2(q.p, p) <= 1e-18) {
            if reject_duplicates {
                return Err("重心緩和では重複点を除いてください。".into());
            }
            if weight > old.weight {
                *old = Site { p, weight };
            }
        } else {
            out.push(Site { p, weight });
        }
    }
    Ok(out)
}

fn cell(
    sites: &[Site],
    i: usize,
    half: P2,
    scale: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<P2>, String> {
    let [w, h] = half;
    let mut poly = vec![[-w, -h], [w, -h], [w, h], [-w, h]];
    let a = sites[i];
    for (j, b) in sites.iter().enumerate() {
        if j % 16 == 0 {
            cancelled(cancel)?;
        }
        if poly.is_empty() {
            break;
        }
        if i == j {
            continue;
        }
        let normal = [b.p[0] - a.p[0], b.p[1] - a.p[1]];
        let mid = [(a.p[0] + b.p[0]) * 0.5, (a.p[1] + b.p[1]) * 0.5];
        let offset = (b.weight - a.weight) / (2. * scale * scale);
        let side = |p: P2| (p[0] - mid[0]) * normal[0] + (p[1] - mid[1]) * normal[1] + offset;
        let mut clipped = Vec::with_capacity(poly.len() + 1);
        for (&p, &q) in poly.iter().zip(poly.iter().cycle().skip(1)) {
            let (fp, fq) = (side(p), side(q));
            if fp <= 0. {
                clipped.push(p);
            }
            if (fp <= 0.) != (fq <= 0.) {
                let t = (fp / (fp - fq)).clamp(0., 1.);
                clipped.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        poly = clipped;
    }
    Ok(clean(poly))
}

fn centroid(poly: &[P2]) -> Result<P2, String> {
    if poly.len() < 3 {
        return Err("重心緩和のセルが退化しました。".into());
    }
    let origin = poly[0];
    let mut total = 0.;
    let mut c = [0.; 2];
    for pair in poly[1..].windows(2) {
        let a = [pair[0][0] - origin[0], pair[0][1] - origin[1]];
        let b = [pair[1][0] - origin[0], pair[1][1] - origin[1]];
        let cross = a[0] * b[1] - a[1] * b[0];
        total += cross;
        for k in 0..2 {
            c[k] += cross * (a[k] + b[k]);
        }
    }
    if total <= 2e-16 {
        return Err("重心緩和のセルが小さすぎます。".into());
    }
    Ok([
        origin[0] + c[0] / (3. * total),
        origin[1] + c[1] / (3. * total),
    ])
}

pub(super) fn run_list(
    kind: NodeKind,
    input: &[[f64; 3]],
    lists: &[&[Value]],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    match kind {
        NodeKind::PowerDiagram2d => {
            let (scale, half) = rectangle(setting(lists, 2)?, setting(lists, 3)?)?;
            let raw = list(lists, 1)?;
            if raw.len() != 1 && raw.len() != input.len() {
                return Err("重みは1個または点と同数にしてください。".into());
            }
            let weights = raw
                .iter()
                .map(Value::number)
                .collect::<Result<Vec<_>, _>>()?;
            let sites = sites(input, &weights, scale, half, false, cancel)?;
            let mut out = Vec::with_capacity(sites.len());
            for i in 0..sites.len() {
                cancelled(cancel)?;
                let poly = cell(&sites, i, half, scale, cancel)?;
                // Dominated or negligible cells legitimately have no polygon.
                if poly.len() < 3 || area(&poly) <= 1e-16 {
                    continue;
                }
                out.push(curve(
                    poly.into_iter()
                        .map(|p| [p[0] * scale, p[1] * scale, 0.])
                        .collect(),
                    true,
                )?);
            }
            Ok(out)
        }
        NodeKind::LloydRelax2d => {
            let (scale, half) = rectangle(setting(lists, 1)?, setting(lists, 2)?)?;
            let steps = setting(lists, 3)?;
            if !(1.0..=12.).contains(&steps) || steps.fract() != 0. {
                return Err("反復は1〜12の整数です。".into());
            }
            let mut sites = sites(input, &[0.], scale, half, true, cancel)?;
            for _ in 0..steps as usize {
                let mut next = Vec::with_capacity(sites.len());
                for i in 0..sites.len() {
                    cancelled(cancel)?;
                    let p = centroid(&cell(&sites, i, half, scale, cancel)?)?;
                    next.push(Site { p, weight: 0. });
                }
                sites = next;
            }
            Ok(sites
                .into_iter()
                .map(|s| Value::Point([s.p[0] * scale, s.p[1] * scale, 0.]))
                .collect())
        }
        _ => Err("未対応の装飾リストノードです。".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(v: &Value) -> &[[f64; 3]] {
        let Value::Curve(c) = v else {
            panic!("curve expected")
        };
        &c.points
    }

    #[test]
    fn truchet_arcs_share_exact_edge_midpoints() {
        for k in 0..4 {
            let v = truchet_arc(0, 0, 10., k, 16).unwrap();
            let p = points(&v);
            for point in p {
                assert!((0.0..=10.).contains(&point[0]) && (0.0..=10.).contains(&point[1]));
            }
            for point in [p[0], p[16]] {
                assert!(point[0] == 5. || point[1] == 5.);
            }
        }
        let a = truchet_arc(0, 0, 10., 1, 16).unwrap();
        let b = truchet_arc(1, 0, 10., 3, 16).unwrap();
        assert_eq!(points(&a)[0], points(&b)[0]);
    }

    #[test]
    fn asanoha_has_no_duplicate_shared_edges() {
        let out = asanoha(2, 3, 12., &|| false).unwrap();
        assert_eq!(out.len(), 59);
        for (i, v) in out.iter().enumerate() {
            let p = points(v);
            assert_ne!(p[0], p[1]);
            for other in &out[..i] {
                let q = points(other);
                assert!(p != q && !(p[0] == q[1] && p[1] == q[0]));
            }
        }
    }

    #[test]
    fn weighted_boundary_and_empty_cell() {
        let (scale, half) = rectangle(10., 8.).unwrap();
        let input = [[-2., 0., 0.], [2., 0., 0.]];
        let mut s = sites(&input, &[8., 0.], scale, half, false, &|| false).unwrap();
        let a = cell(&s, 0, half, scale, &|| false).unwrap();
        let b = cell(&s, 1, half, scale, &|| false).unwrap();
        assert!((area(&a) * scale * scale - 48.).abs() < 1e-9);
        assert!((area(&b) * scale * scale - 32.).abs() < 1e-9);
        s[0].weight = 1000.;
        assert!(cell(&s, 1, half, scale, &|| false).unwrap().is_empty());
        assert!(cell(&s, 0, half, scale, &|| true).is_err());
    }

    #[test]
    fn lloyd_centroids_and_duplicate_rejection() {
        let (scale, half) = rectangle(10., 8.).unwrap();
        let s = sites(
            &[[-2., 0., 0.], [2., 0., 0.]],
            &[0.],
            scale,
            half,
            true,
            &|| false,
        )
        .unwrap();
        let c = centroid(&cell(&s, 0, half, scale, &|| false).unwrap()).unwrap();
        assert!((c[0] * scale + 2.5).abs() < 1e-10 && c[1].abs() < 1e-10);
        assert!(sites(&[[0.; 3]; 2], &[0.], scale, half, true, &|| false).is_err());
        let s = sites(&[[0.; 3]; 2], &[0., 10.], scale, half, false, &|| false).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].weight, 10.);
    }
}
