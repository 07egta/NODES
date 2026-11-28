//! Bounded, deterministic biomorphic geometry. No new runtime dependencies.
use crate::engine::{cancelled, checked_size, dot, finite_point, num, sub, validate_mesh};
use crate::model::*;
use std::f64::consts::TAU;
use std::sync::Arc;

const MAX_ATTRACTORS: usize = 1024;
const MAX_BRANCHES: usize = 2047;
const MAX_DISTANCE_TESTS: usize = 12_000_000;
const CATEGORY: &str = "生物造形";

static COLONIZATION: NodeSpec = NodeSpec {
    label: "空間分割成長",
    category: CATEGORY,
    description: "各誘引点を最も近い枝に割り当て、平均方向に枝を成長させます。根の位置、探索半径、到達半径、成長回数を調整。点の乱数シードは上流の散布ノードで指定します。出力は枝の線分です。",
    keywords: "biomorph space colonization tree vein coral mycelium root leaf 葉脈 菌糸 樹木 サンゴ 成長",
    inputs: &[
        g("attractors", "誘引点リスト", DataType::Point),
        n("root_x", "根 X", 0., -100000., 100000., 1.),
        n("root_y", "根 Y", 0., -100000., 100000., 1.),
        n("root_z", "根 Z", 0., -100000., 100000., 1.),
        n("step", "成長距離", 2.5, 0.01, 1000., 0.1),
        n("influence", "探索半径", 24., 0.01, 10000., 1.),
        n("kill", "到達半径", 3., 0.01, 1000., 0.1),
        i("iterations", "成長回数", 48., 0., 128.),
        n("bias_z", "上向き成長傾向", 0., -0.8, 0.8, 0.05),
    ],
    output: DataType::Curve,
};

static COILED_SHELL: NodeSpec = NodeSpec {
    label: "らせん貝殻",
    category: CATEGORY,
    description: "対数らせんに沿って断面が拡大する中空メッシュ。内外面と両端の縁を接続します。巻き数、拡大率、肉厚、ひだを変更できます。自己交差や印刷適性は別途検査してください。",
    keywords: "biomorph coiled shell logarithmic spiral gastropod rib sculpture 貝殻 巻貝 らせん 彫刻",
    inputs: &[
        n("radius", "初期断面半径", 3., 0.5, 50., 0.1),
        n("turns", "巻き数", 2.5, 0.25, 4., 0.1),
        n("expansion", "1 周の拡大率", 1.8, 1.1, 2.5, 0.05),
        n("coil", "軸からの距離倍率", 2.8, 1.4, 8., 0.1),
        n("rise", "高さ方向の倍率", 4.5, 0., 12., 0.1),
        n("wall", "相対肉厚", 0.22, 0.05, 0.8, 0.01),
        i("ribs", "1 周のひだ数", 24., 0., 40.),
        n("rib_depth", "ひだの強さ", 0.07, 0., 0.2, 0.01),
        i("steps", "長手分割", 320., 24., 512.),
        i("sides", "断面分割", 32., 8., 64.),
    ],
    output: DataType::Mesh,
};

pub(crate) fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    match kind {
        NodeKind::BioSpaceColonization => Some(&COLONIZATION),
        NodeKind::BioCoiledShell => Some(&COILED_SHELL),
        _ => None,
    }
}

fn bounded(value: f64, min: f64, max: f64) -> Result<f64, String> {
    if !value.is_finite() || value < min || value > max {
        return Err("BioMorph の設定値が範囲外です。".into());
    }
    Ok(value)
}

fn integer(value: f64, min: usize, max: usize) -> Result<usize, String> {
    let v = bounded(value, min as f64, max as f64)?;
    if v.fract() != 0. {
        return Err("BioMorph の個数は整数で指定してください。".into());
    }
    Ok(v as usize)
}

fn setting(lists: &[&[Value]], index: usize) -> Result<f64, String> {
    lists
        .get(index)
        .and_then(|v| v.first())
        .ok_or("BioMorph の設定値がありません。")?
        .number()
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

fn charge(tests: &mut usize, amount: usize) -> Result<(), String> {
    *tests = tests.saturating_add(amount);
    if *tests > MAX_DISTANCE_TESTS {
        return Err(
            "成長の距離計算が上限を超えました。誘引点数や成長回数を減らしてください。".into(),
        );
    }
    Ok(())
}

fn segment(a: [f64; 3], b: [f64; 3]) -> Value {
    Value::Curve(Arc::new(Curve {
        points: vec![a, b],
        closed: false,
        geometry: None,
    }))
}

#[derive(Clone, Copy)]
struct GrowthSettings {
    root: [f64; 3],
    step: f64,
    influence: f64,
    kill: f64,
    iterations: usize,
    bias_z: f64,
}

fn grow(
    mut active: Vec<[f64; 3]>,
    s: GrowthSettings,
    limit: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut nodes = vec![s.root];
    let mut out = Vec::new();
    let mut tests = 0;
    let kill2 = s.kill * s.kill;
    let influence2 = s.influence * s.influence;
    let spacing2 = (s.step * 0.25).powi(2);
    // A complete assignment pass precedes every simultaneous growth round.
    // New branches can influence only the next round; ties choose the older node.
    for _ in 0..s.iterations {
        cancelled(cancel)?;
        if active.is_empty() {
            break;
        }
        charge(&mut tests, active.len().saturating_mul(nodes.len()))?;
        let previous_count = nodes.len();
        let mut sums = vec![[0.; 3]; previous_count];
        let mut counts = vec![0usize; previous_count];
        let mut remaining = Vec::with_capacity(active.len());
        for (index, target) in active.into_iter().enumerate() {
            if index % 32 == 0 {
                cancelled(cancel)?;
            }
            let mut nearest = 0;
            let mut best = f64::INFINITY;
            for (j, &p) in nodes.iter().enumerate() {
                let d2 = dist2(target, p);
                if d2 < best {
                    nearest = j;
                    best = d2;
                }
            }
            if best <= kill2 {
                continue;
            }
            remaining.push(target);
            if best <= influence2 {
                let d = sub(target, nodes[nearest]);
                let norm = best.sqrt();
                for (sum, component) in sums[nearest].iter_mut().zip(d) {
                    *sum += component / norm;
                }
                counts[nearest] += 1;
            }
        }
        active = remaining;
        let mut added = 0;
        for j in 0..previous_count {
            if j % 32 == 0 {
                cancelled(cancel)?;
            }
            if counts[j] == 0 {
                continue;
            }
            let mut direction = sums[j].map(|x| x / counts[j] as f64);
            direction[2] += s.bias_z;
            let norm = dot(direction, direction).sqrt();
            if norm <= 1e-12 {
                continue;
            }
            let p = nodes[j];
            let q = std::array::from_fn(|k| p[k] + direction[k] / norm * s.step);
            if !finite_point(q) {
                return Err("成長した枝の座標が無効です。".into());
            }
            charge(&mut tests, nodes.len())?;
            if nodes.iter().any(|&v| dist2(v, q) < spacing2) {
                continue;
            }
            if out.len() >= limit {
                return Err(
                    "成長した枝の数が上限を超えました。成長回数を減らしてください。".into(),
                );
            }
            nodes.push(q);
            out.push(segment(p, q));
            added += 1;
        }
        if added == 0 {
            break;
        }
    }
    Ok(out)
}

pub(crate) fn run_colonization(
    lists: &[&[Value]],
    remaining_weight: usize,
    remaining_items: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(cancel)?;
    let targets = lists.first().copied().ok_or("誘引点を接続してください。")?;
    if targets.len() > MAX_ATTRACTORS {
        return Err("誘引点は 1024 点までです。".into());
    }
    let mut active = Vec::with_capacity(targets.len());
    for v in targets {
        match v {
            Value::Point(p) if finite_point(*p) => active.push(*p),
            _ => return Err("有効な誘引点を接続してください。".into()),
        }
    }
    let s = GrowthSettings {
        root: [
            bounded(setting(lists, 1)?, -100000., 100000.)?,
            bounded(setting(lists, 2)?, -100000., 100000.)?,
            bounded(setting(lists, 3)?, -100000., 100000.)?,
        ],
        step: bounded(setting(lists, 4)?, 0.01, 1000.)?,
        influence: bounded(setting(lists, 5)?, 0.01, 10000.)?,
        kill: bounded(setting(lists, 6)?, 0.01, 1000.)?,
        iterations: integer(setting(lists, 7)?, 0, 128)?,
        bias_z: bounded(setting(lists, 8)?, -0.8, 0.8)?,
    };
    if s.step > s.kill || s.kill >= s.influence {
        return Err("成長距離 ≤ 到達半径 < 探索半径にしてください。".into());
    }
    let limit = MAX_BRANCHES.min(remaining_items).min(remaining_weight / 2);
    grow(active, s, limit, cancel)
}

struct ShellSettings {
    radius: f64,
    turns: f64,
    expansion: f64,
    coil: f64,
    rise: f64,
    wall: f64,
    ribs: usize,
    rib_depth: f64,
    steps: usize,
    sides: usize,
}

impl ShellSettings {
    fn read(a: &[&Value]) -> Result<Self, String> {
        let s = Self {
            radius: bounded(num(a, 0)?, 0.5, 50.)?,
            turns: bounded(num(a, 1)?, 0.25, 4.)?,
            expansion: bounded(num(a, 2)?, 1.1, 2.5)?,
            coil: bounded(num(a, 3)?, 1.4, 8.)?,
            rise: bounded(num(a, 4)?, 0., 12.)?,
            wall: bounded(num(a, 5)?, 0.05, 0.8)?,
            ribs: integer(num(a, 6)?, 0, 40)?,
            rib_depth: bounded(num(a, 7)?, 0., 0.2)?,
            steps: integer(num(a, 8)?, 24, 512)?,
            sides: integer(num(a, 9)?, 8, 64)?,
        };
        // At least eight samples per turn, and four per rib when ribs are active.
        let required = if s.ribs > 0 && s.rib_depth > 0. {
            8usize.max(4 * s.ribs)
        } else {
            8
        };
        if (s.steps as f64) < s.turns * required as f64 {
            return Err("巻き数やひだ数に対して長手分割が不足しています。".into());
        }
        Ok(s)
    }

    fn size(&self) -> (usize, usize) {
        // Two layers, one quad per angular segment, and a rim at each end.
        (
            2 * (self.steps + 1) * self.sides,
            4 * (self.steps + 1) * self.sides,
        )
    }
}

pub(crate) fn estimate_shell(a: &[&Value]) -> Result<(usize, usize), String> {
    let s = ShellSettings::read(a)?;
    let (vertices, triangles) = s.size();
    checked_size(vertices, triangles)?;
    Ok((vertices + 3 * triangles, 1))
}

fn quad(triangles: &mut Vec<[u32; 3]>, a: u32, b: u32, c: u32, d: u32) {
    triangles.extend([[a, b, c], [a, c, d]]);
}

fn coiled_shell(s: ShellSettings, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let (vertices, triangles) = s.size();
    checked_size(vertices, triangles)?;
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(vertices),
        triangles: Vec::with_capacity(triangles),
    };
    let ring = s.sides;
    let layer = (s.steps + 1) * ring;
    for inner in [false, true] {
        for j in 0..=s.steps {
            cancelled(cancel)?;
            let turn = s.turns * j as f64 / s.steps as f64;
            let theta = TAU * turn;
            let (sin_t, cos_t) = theta.sin_cos();
            let scale = s.expansion.powf(turn);
            let radius = s.radius * scale;
            let center = s.coil * radius;
            let z = s.rise * s.radius * (scale - 1.) / (s.expansion - 1.);
            let rib = if s.ribs == 0 {
                1.
            } else {
                1. + s.rib_depth * (theta * s.ribs as f64).cos()
            };
            let tube = radius * rib * if inner { 1. - s.wall } else { 1. };
            for k in 0..ring {
                let phi = TAU * k as f64 / ring as f64;
                let (sin_p, cos_p) = phi.sin_cos();
                let radial = center + tube * cos_p;
                mesh.vertices
                    .push([radial * cos_t, radial * sin_t, z + tube * sin_p]);
            }
        }
    }
    let index =
        |inner: bool, j: usize, k: usize| (if inner { layer } else { 0 } + j * ring + k) as u32;
    for j in 0..s.steps {
        cancelled(cancel)?;
        for k in 0..ring {
            let next = (k + 1) % ring;
            let a = index(false, j, k);
            let b = index(false, j + 1, k);
            let c = index(false, j + 1, next);
            let d = index(false, j, next);
            quad(&mut mesh.triangles, a, b, c, d);
            let a = index(true, j, k);
            let b = index(true, j + 1, k);
            let c = index(true, j + 1, next);
            let d = index(true, j, next);
            quad(&mut mesh.triangles, a, d, c, b);
        }
    }
    for k in 0..ring {
        let next = (k + 1) % ring;
        quad(
            &mut mesh.triangles,
            index(false, 0, k),
            index(false, 0, next),
            index(true, 0, next),
            index(true, 0, k),
        );
        quad(
            &mut mesh.triangles,
            index(false, s.steps, next),
            index(false, s.steps, k),
            index(true, s.steps, k),
            index(true, s.steps, next),
        );
    }
    validate_mesh(&mesh, cancel)?;
    Ok(mesh)
}

pub(crate) fn run_shell(a: &[&Value], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let mesh = coiled_shell(ShellSettings::read(a)?, cancel)?;
    Ok(vec![Value::Mesh(Arc::new(mesh))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn colonization_straight_target_and_bounds() {
        let s = GrowthSettings {
            root: [0.; 3],
            step: 1.,
            influence: 10.,
            kill: 1.,
            iterations: 20,
            bias_z: 0.,
        };
        let targets = vec![[5., 0., 0.]];
        let out = grow(targets.clone(), s, 10, &|| false).unwrap();
        assert_eq!(out.len(), 4);
        for (i, v) in out.iter().enumerate() {
            let Value::Curve(c) = v else {
                panic!("expected curve")
            };
            assert_eq!(c.points, vec![[i as f64, 0., 0.], [i as f64 + 1., 0., 0.]]);
        }
        assert_eq!(out, grow(targets.clone(), s, 10, &|| false).unwrap());
        assert!(grow(targets.clone(), s, 2, &|| false).is_err());
        assert!(grow(targets, s, 10, &|| true).is_err());
    }

    #[test]
    fn shell_is_closed_oriented_and_matches_estimate() {
        let s = ShellSettings {
            radius: 3.,
            turns: 1.,
            expansion: 1.8,
            coil: 2.8,
            rise: 4.5,
            wall: 0.22,
            ribs: 8,
            rib_depth: 0.07,
            steps: 64,
            sides: 24,
        };
        let size = s.size();
        let mesh = coiled_shell(s, &|| false).unwrap();
        assert_eq!((mesh.vertices.len(), mesh.triangles.len()), size);
        let mut edges = BTreeMap::<(u32, u32), (usize, i32)>::new();
        let mut volume6 = 0.;
        for &[a, b, c] in &mesh.triangles {
            for (u, v) in [(a, b), (b, c), (c, a)] {
                let entry = edges.entry((u.min(v), u.max(v))).or_default();
                entry.0 += 1;
                entry.1 += if u < v { 1 } else { -1 };
            }
            volume6 += dot(
                mesh.vertices[a as usize],
                crate::engine::cross(mesh.vertices[b as usize], mesh.vertices[c as usize]),
            );
        }
        assert!(
            edges
                .values()
                .all(|&(count, direction)| count == 2 && direction == 0)
        );
        assert!(volume6 > 0.);
    }
}
