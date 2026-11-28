//! GENESIS v1: bounded, deterministic rectangular shape grammar.
//! Independent Rust implementation. No global RNG, recursion, or external dependency.

use crate::engine::{as_mesh, cancelled, num};
use crate::model::{Curve, DataType as D, NodeKind as K, NodeSpec, Value, g, i, n};
use std::sync::Arc;

const MAX_DEPTH: u32 = 10;
const MAX_SCOPES: usize = 1 << MAX_DEPTH;
const MAX_SEED: u32 = 1_000_000;

const RULE_INPUTS: &[crate::model::InputSpec] = &[
    n("width", "幅", 120., 0.01, 10000., 1.),
    n("height", "高さ", 80., 0.01, 10000., 1.),
    i("depth", "最大深さ", 5., 0., 10.),
    n("minimum", "最小辺", 5., 0.01, 10000., 0.5),
    n("x_weight", "X分割の確率", 0.5, 0., 1., 0.05),
    n("jitter", "分割の揺らぎ", 0.3, 0., 0.45, 0.05),
    i("seed", "シード", 7., 0., 1000000.),
    i("region", "再生成する領域ID（0=なし）", 0., 0., 2047.),
    i("region_seed", "領域シード", 19., 0., 1000000.),
    i("limit", "最大領域数", 1024., 1., 1024.),
];

static SCOPES: NodeSpec = NodeSpec {
    label: "形状文法・領域分割",
    category: "配列",
    description: "XY長方形を条件付きの二分割規則で生成します。領域IDを指定すると、その部分だけ再生成できます。",
    keywords: "genesis shape grammar scope split facade procedural recursion 形状文法 外装 再帰",
    inputs: RULE_INPUTS,
    output: D::Curve,
};

static IDS: NodeSpec = NodeSpec {
    label: "形状文法・領域ID",
    category: "配列",
    description: "同一設定の領域分割と同じ順で、安定した二分木IDを返します。",
    keywords: "genesis scope id provenance grammar trace 形状文法 規則 履歴",
    inputs: RULE_INPUTS,
    output: D::Number,
};

static RELIEF: NodeSpec = NodeSpec {
    label: "形状文法・立体パネル",
    category: "立体",
    description: "分割領域に余白を設け、確率的に低いパネルまたは高いパネルへ置換します。",
    keywords: "genesis grammar panel relief extrude facade sculpture 形状文法 外装 彫刻",
    inputs: &[
        g("scope", "領域曲線", D::Curve),
        n("height", "低い高さ", 2., 0.01, 10000., 0.5),
        n("tall_height", "高い高さ", 12., 0.01, 10000., 0.5),
        n("weight", "高い規則の確率", 0.5, 0., 1., 0.05),
        n("margin", "片側余白率", 0.06, 0.001, 0.45, 0.01),
        i("seed", "シード", 11., 0., 1000000.),
    ],
    output: D::Mesh,
};

pub(crate) fn is_kind(kind: K) -> bool {
    matches!(
        kind,
        K::GenesisScopes | K::GenesisScopeIds | K::GenesisRelief
    )
}

pub(crate) fn spec(kind: K) -> &'static NodeSpec {
    match kind {
        K::GenesisScopes => &SCOPES,
        K::GenesisScopeIds => &IDS,
        K::GenesisRelief => &RELIEF,
        _ => unreachable!("not a GENESIS node"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Scope {
    id: u32,
    // x0, y0, x1, y1
    bounds: [f64; 4],
}

#[derive(Clone, Copy, Debug)]
struct Grammar {
    width: f64,
    height: f64,
    depth: u32,
    minimum: f64,
    x_weight: f64,
    jitter: f64,
    seed: u32,
    region: u32,
    region_seed: u32,
    limit: usize,
}

fn bounded(a: &[&Value], index: usize, min: f64, max: f64) -> Result<f64, String> {
    let v = num(a, index)?;
    if !(min..=max).contains(&v) {
        return Err("形状文法の入力が範囲外です。".into());
    }
    Ok(v)
}

fn integer(a: &[&Value], index: usize, max: u32) -> Result<u32, String> {
    let v = bounded(a, index, 0., f64::from(max))?;
    if v.fract() != 0. {
        return Err("形状文法のID・深さ・シードには整数が必要です。".into());
    }
    Ok(v as u32)
}

impl Grammar {
    fn read(a: &[&Value]) -> Result<Self, String> {
        let cfg = Self {
            width: bounded(a, 0, 0.01, 10000.)?,
            height: bounded(a, 1, 0.01, 10000.)?,
            depth: integer(a, 2, MAX_DEPTH)?,
            minimum: bounded(a, 3, 0.01, 10000.)?,
            x_weight: bounded(a, 4, 0., 1.)?,
            jitter: bounded(a, 5, 0., 0.45)?,
            seed: integer(a, 6, MAX_SEED)?,
            region: integer(a, 7, (1 << (MAX_DEPTH + 1)) - 1)?,
            region_seed: integer(a, 8, MAX_SEED)?,
            limit: integer(a, 9, MAX_SCOPES as u32)? as usize,
        };
        if cfg.limit == 0 || cfg.minimum > cfg.width.min(cfg.height) {
            return Err("最小辺または最大領域数を見直してください。".into());
        }
        Ok(cfg)
    }
}

// Fixed SplitMix64 finalizer. Each choice is a pure function of seed, path, and
// channel; traversal order and sibling edits cannot consume another node's RNG.
fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn sample(seed: u32, id: u64, channel: u64) -> f64 {
    let key = (u64::from(seed) << 32) ^ mix(id) ^ mix(channel);
    (mix(key) >> 11) as f64 / ((1u64 << 53) as f64)
}

fn leaves(cfg: Grammar, stop: &dyn Fn() -> bool) -> Result<Vec<Scope>, String> {
    let root = Scope {
        id: 1,
        bounds: [0., 0., cfg.width, cfg.height],
    };
    let mut pending = vec![(root, 0, false)];
    let mut out = Vec::new();
    let mut region_applied = cfg.region == 0;
    while let Some((scope, depth, local)) = pending.pop() {
        cancelled(stop)?;
        let selected = scope.id == cfg.region;
        let local = local || selected;
        let [x0, y0, x1, y1] = scope.bounds;
        let can_x = x0 + cfg.minimum <= x1 - cfg.minimum;
        let can_y = y0 + cfg.minimum <= y1 - cfg.minimum;
        if depth == cfg.depth || !(can_x || can_y) {
            out.push(scope);
            continue;
        }
        region_applied |= selected;
        if out.len() + pending.len() + 2 > cfg.limit {
            return Err("形状文法の最大領域数に達しました。深さを減らしてください。".into());
        }
        let seed = if local { cfg.region_seed } else { cfg.seed };
        let split_x = can_x && (!can_y || sample(seed, u64::from(scope.id), 1) < cfg.x_weight);
        let ratio = 0.5 + cfg.jitter * (2. * sample(seed, u64::from(scope.id), 2) - 1.);
        let (left, right) = if split_x {
            let cut = (x0 + (x1 - x0) * ratio).clamp(x0 + cfg.minimum, x1 - cfg.minimum);
            ([x0, y0, cut, y1], [cut, y0, x1, y1])
        } else {
            let cut = (y0 + (y1 - y0) * ratio).clamp(y0 + cfg.minimum, y1 - cfg.minimum);
            ([x0, y0, x1, cut], [x0, cut, x1, y1])
        };
        for (id, bounds) in [(2 * scope.id + 1, right), (2 * scope.id, left)] {
            pending.push((Scope { id, bounds }, depth + 1, local));
        }
    }
    if !region_applied {
        return Err("再生成する領域IDが存在しないか、すでに終端です。親IDを指定してください。".into());
    }
    Ok(out)
}

fn outline(scope: Scope) -> Value {
    let [x0, y0, x1, y1] = scope.bounds;
    Value::Curve(Arc::new(Curve {
        points: vec![[x0, y0, 0.], [x1, y0, 0.], [x1, y1, 0.], [x0, y1, 0.]],
        closed: true,
        geometry: None,
    }))
}

fn relief(a: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let Some(Value::Curve(c)) = a.first().copied() else {
        return Err("分割領域の曲線を接続してください。".into());
    };
    let [p, q, r, s] = c.points.as_slice() else {
        return Err("4点のXY長方形領域が必要です。".into());
    };
    let [x0, y0, z] = *p;
    let [x1, y1, _] = *r;
    if !c.closed
        || c.geometry.is_some()
        || !c.points.iter().all(|p| crate::engine::finite_point(*p))
        || x1 <= x0
        || y1 <= y0
        || *q != [x1, y0, z]
        || *r != [x1, y1, z]
        || *s != [x0, y1, z]
    {
        return Err("分割ノードが出力する順序のXY長方形領域を接続してください。".into());
    }
    let low = bounded(a, 1, 0.01, 10000.)?;
    let tall = bounded(a, 2, 0.01, 10000.)?;
    let weight = bounded(a, 3, 0., 1.)?;
    let margin = bounded(a, 4, 0.001, 0.45)?;
    let seed = integer(a, 5, MAX_SEED)?;
    let key = [x0, y0, x1, y1, z]
        .into_iter()
        .fold(0, |key, v| mix(key ^ v.to_bits()));
    let height = if sample(seed, key, 3) < weight { tall } else { low };
    let mut mesh = crate::engine::box_mesh(
        (x1 - x0) * (1. - 2. * margin),
        (y1 - y0) * (1. - 2. * margin),
        height,
    );
    let center = [x0 + (x1 - x0) * 0.5, y0 + (y1 - y0) * 0.5, z];
    for p in &mut mesh.vertices {
        for axis in 0..3 {
            p[axis] += center[axis];
        }
    }
    as_mesh(mesh, stop)
}

pub(crate) fn estimate(kind: K, a: &[&Value]) -> Result<(usize, usize), String> {
    if kind == K::GenesisRelief {
        return Ok((44, 1));
    }
    let cfg = Grammar::read(a)?;
    let n = (1usize << cfg.depth).min(cfg.limit);
    Ok((if kind == K::GenesisScopes { 4 * n } else { n }, n))
}

pub(crate) fn run_item(
    kind: K,
    a: &[&Value],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if kind == K::GenesisRelief {
        return relief(a, stop);
    }
    let cfg = Grammar::read(a)?;
    let scopes = leaves(cfg, stop)?;
    Ok(scopes
        .into_iter()
        .map(|scope| match kind {
            K::GenesisScopes => outline(scope),
            K::GenesisScopeIds => Value::Number(f64::from(scope.id)),
            _ => unreachable!("not a GENESIS node"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Grammar {
        Grammar {
            width: 120.,
            height: 80.,
            depth: 5,
            minimum: 0.1,
            x_weight: 0.5,
            jitter: 0.3,
            seed: 7,
            region: 0,
            region_seed: 19,
            limit: MAX_SCOPES,
        }
    }

    fn under(mut id: u32, parent: u32) -> bool {
        while id > parent {
            id /= 2;
        }
        id == parent
    }

    #[test]
    fn deterministic_partition_has_unique_ids_and_preserves_area() {
        let config = cfg();
        let a = leaves(config, &|| false).unwrap();
        assert_eq!(a, leaves(config, &|| false).unwrap());
        assert_eq!(a.len(), 32);
        let area: f64 = a.iter().map(|s| {
            let [x0, y0, x1, y1] = s.bounds;
            assert!(x0 >= 0. && y0 >= 0. && x1 <= config.width && y1 <= config.height);
            assert!(x1 > x0 && y1 > y0);
            (x1 - x0) * (y1 - y0)
        }).sum();
        assert!((area - config.width * config.height).abs() < 1e-8);
        for (i, p) in a.iter().enumerate() {
            for q in &a[i + 1..] {
                assert_ne!(p.id, q.id);
                assert!(p.bounds[2] <= q.bounds[0] || q.bounds[2] <= p.bounds[0]
                    || p.bounds[3] <= q.bounds[1] || q.bounds[3] <= p.bounds[1]);
            }
        }
    }

    #[test]
    fn local_regeneration_preserves_siblings() {
        let old = leaves(cfg(), &|| false).unwrap();
        let changed = leaves(Grammar { region: 2, ..cfg() }, &|| false).unwrap();
        assert_ne!(old, changed);
        assert_eq!(
            old.iter().filter(|s| !under(s.id, 2)).collect::<Vec<_>>(),
            changed.iter().filter(|s| !under(s.id, 2)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn limits_and_cancel_are_errors() {
        assert!(leaves(Grammar { limit: 1, ..cfg() }, &|| false).is_err());
        assert!(leaves(cfg(), &|| true).is_err());
        assert!(leaves(Grammar { region: 1024, ..cfg() }, &|| false).is_err());
        let terminal = leaves(Grammar { minimum: 80., ..cfg() }, &|| false).unwrap();
        assert_eq!(terminal.len(), 1);
    }

    #[test]
    fn relief_has_closed_positive_volume() {
        let v = outline(Scope { id: 1, bounds: [0., 0., 20., 10.] });
        let values = [Value::Number(2.), Value::Number(12.), Value::Number(1.),
            Value::Number(0.1), Value::Number(7.)];
        let args = [&v, &values[0], &values[1], &values[2], &values[3], &values[4]];
        let out = relief(&args, &|| false).unwrap();
        let Value::Mesh(m) = &out[0] else { panic!("expected mesh") };
        assert_eq!(m.vertices.len(), 8);
        assert_eq!(m.triangles.len(), 12);
        assert!(crate::mesh_fabrication::audit(m, &|| false).unwrap().closed_oriented());
        let volume = crate::mesh_fabrication::signed_volume(m, &|| false).unwrap();
        assert!((volume - 16. * 8. * 12.).abs() < 1e-8);
    }
}
