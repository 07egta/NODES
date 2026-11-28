//! Bounded procedural patterns for mathematical and biomorphic design.
//! All curves are sampled polylines; this module does not claim NURBS accuracy.
use crate::engine::{cancelled, finite_point, num};
use crate::model::*;
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

const GOLDEN_ANGLE: f64 = 2.399963229728653;
const KINDS: &[NodeKind] = &[
    NodeKind::Phyllotaxis,
    NodeKind::FibonacciSphere,
    NodeKind::RoseCurve,
    NodeKind::SuperformulaCurve,
    NodeKind::TorusKnot,
    NodeKind::Lissajous3d,
    NodeKind::HilbertCurve,
    NodeKind::KochSnowflake,
    NodeKind::SierpinskiCells,
    NodeKind::ReactionDiffusion,
    NodeKind::PoissonDisk2d,
    NodeKind::SpurGearProfile,
];

pub(crate) fn is_kind(kind: NodeKind) -> bool {
    KINDS.contains(&kind)
}

static PHYLLOTAXIS: NodeSpec = NodeSpec {
    label: "黄金角の葉序",
    category: "生成デザイン",
    description: "黄金角の点配置。間隔×平方根の半径で円盤状、1点ごとの高さで立体らせんにします。",
    keywords: "phyllotaxis golden angle fibonacci sunflower 葉序 ひまわり 黄金角",
    inputs: &[
        i("count", "点の数", 300., 1., 4096.),
        n("spacing", "半径係数", 2., 0.01, 100., 0.1),
        n("rise", "点ごとの高さ", 0., -10., 10., 0.1),
        n("angle", "角度 (度)", 137.50776405003785, 0., 360., 0.1),
    ],
    output: DataType::Point,
};

static FIBONACCI_SPHERE: NodeSpec = NodeSpec {
    label: "フィボナッチ球面",
    category: "生成デザイン",
    description: "球面に面積の偏りが少ない決定的な点群を配置します。極は含みません。",
    keywords: "fibonacci sphere spherical points 球面 点群 黄金角",
    inputs: &[
        i("count", "点の数", 256., 1., 4096.),
        n("radius", "半径", 30., 0.01, 1000., 0.5),
    ],
    output: DataType::Point,
};

static ROSE_CURVE: NodeSpec = NodeSpec {
    label: "バラ曲線",
    category: "生成デザイン",
    description: "整数kの極座標 r=R cos(kθ) を閉じた折れ線にします。奇数kはk枚、偶数kは2k枚の花弁。",
    keywords: "rose rhodonea polar curve 花弁 極座標 数式",
    inputs: &[
        n("radius", "半径", 30., 0.01, 1000., 0.5),
        i("petals", "周波数 k", 5., 1., 16.),
        i("samples", "分割数", 360., 64., 2048.),
    ],
    output: DataType::Curve,
};

static SUPERFORMULA_CURVE: NodeSpec = NodeSpec {
    label: "スーパーフォーミュラ",
    category: "生成デザイン",
    description: "Gielisの2次元式を正の指数と対称性で制御し、閉じた折れ線を生成します。a=b=1、mは4の倍数。",
    keywords: "gielis superformula superellipse organic 数式 生物 輪郭",
    inputs: &[
        n("radius", "半径係数", 30., 0.01, 1000., 0.5),
        i("symmetry", "対称数", 6., 1., 12.),
        n("n1", "指数 n1", 0.7, 0.2, 8., 0.1),
        n("n2", "指数 n2", 1.7, 0.2, 8., 0.1),
        n("n3", "指数 n3", 1.7, 0.2, 8., 0.1),
        i("samples", "分割数", 480., 64., 2048.),
    ],
    output: DataType::Curve,
};

static TORUS_KNOT: NodeSpec = NodeSpec {
    label: "トーラス結び目",
    category: "生成デザイン",
    description: "互いに素なp,qでトーラス上の閉じた結び目を折れ線生成します。小半径は大半径未満。",
    keywords: "torus knot trefoil topology 結び目 トーラス らせん",
    inputs: &[
        n("major", "大半径", 30., 0.01, 1000., 0.5),
        n("minor", "小半径", 10., 0.01, 999., 0.5),
        i("p", "周回数 p", 2., 1., 16.),
        i("q", "ねじれ数 q", 3., 1., 16.),
        i("samples", "分割数", 512., 64., 2048.),
    ],
    output: DataType::Curve,
};

static LISSAJOUS3D: NodeSpec = NodeSpec {
    label: "リサージュ 3D",
    category: "生成デザイン",
    description: "X/Y/Zの整数周波数と位相差で周期的な3D折れ線を生成します。自己交差する場合があります。",
    keywords: "lissajous harmonic sine 3d curve リサージュ 振動 数式",
    inputs: &[
        n("radius", "振幅", 30., 0.01, 1000., 0.5),
        i("fx", "X 周波数", 3., 1., 16.),
        i("fy", "Y 周波数", 4., 1., 16.),
        i("fz", "Z 周波数", 5., 1., 16.),
        n("phase", "位相差 (度)", 45., -360., 360., 1.),
        i("samples", "分割数", 512., 64., 2048.),
    ],
    output: DataType::Curve,
};

static HILBERT_CURVE: NodeSpec = NodeSpec {
    label: "ヒルベルト曲線",
    category: "生成デザイン",
    description: "正方形を埋めるヒルベルト曲線の有限段階を開いた折れ線で生成します。",
    keywords: "hilbert space filling fractal 再帰 空間充填 ヒルベルト",
    inputs: &[
        n("size", "正方形の幅", 100., 0.01, 1000., 1.),
        i("order", "再帰段数", 4., 1., 6.),
    ],
    output: DataType::Curve,
};

static KOCH_SNOWFLAKE: NodeSpec = NodeSpec {
    label: "コッホ雪片",
    category: "生成デザイン",
    description: "正三角形の辺を外向きに4分割する再帰を行い、閉じた折れ線を生成します。",
    keywords: "koch snowflake fractal 雪片 フラクタル 再帰",
    inputs: &[
        n("size", "初期辺長", 100., 0.01, 1000., 1.),
        i("depth", "再帰段数", 3., 0., 5.),
    ],
    output: DataType::Curve,
};

static SIERPINSKI_CELLS: NodeSpec = NodeSpec {
    label: "シェルピンスキー三角形",
    category: "生成デザイン",
    description: "正三角形の中央を抜く再帰で残ったセルを、個別の閉じた輪郭として返します。",
    keywords: "sierpinski triangle fractal gasket 三角形 再帰 セル",
    inputs: &[
        n("size", "初期辺長", 100., 0.01, 1000., 1.),
        i("depth", "再帰段数", 3., 0., 6.),
        n("fill", "セル倍率", 0.95, 0.05, 1., 0.05),
    ],
    output: DataType::Curve,
};

static REACTION_DIFFUSION: NodeSpec = NodeSpec {
    label: "反応拡散パターン",
    category: "生成デザイン",
    description: "Gray-Scottモデルを周期境界の格子で有限回反復し、B濃度が閾値以上の点を返します。近似計算。",
    keywords: "gray scott reaction diffusion turing pattern 反応拡散 チューリング 生物 模様",
    inputs: &[
        i("side", "格子の一辺", 48., 16., 64.),
        n("spacing", "格子間隔", 2., 0.01, 100., 0.5),
        i("steps", "反復回数", 100., 0., 200.),
        n("feed", "供給率 F", 0.035, 0., 0.09, 0.001),
        n("kill", "除去率 K", 0.06, 0., 0.09, 0.001),
        n("threshold", "B濃度の閾値", 0.15, 0.01, 0.9, 0.01),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Point,
};

static SPUR_GEAR_PROFILE: NodeSpec = NodeSpec {
    label: "インボリュート歯車輪郭（近似）",
    category: "曲線",
    description: "20度の標準平歯車の近似輪郭。歯元は半径方向の簡略接続で、軸穴・転位・歯元フィレット・かみ合い公差は含みません。",
    keywords: "gear spur gear involute module teeth 歯車 平歯車 インボリュート モジュール 歯数",
    inputs: &[
        i("teeth", "歯数", 24., 18., 64.),
        n("module", "モジュール (mm)", 2., 0.5, 5., 0.1),
    ],
    output: DataType::Curve,
};

static POISSON_DISK2D: NodeSpec = NodeSpec {
    label: "最小間隔ランダム点",
    category: "生成デザイン",
    description: "長方形内で最小距離を守る決定的な拒否サンプリング。試行上限で目標数より少なくなる場合があります。",
    keywords: "poisson disk blue noise random spacing 点群 最小間隔 パターン",
    inputs: &[
        n("width", "幅", 100., 0.01, 1000., 1.),
        n("height", "高さ", 100., 0.01, 1000., 1.),
        n("distance", "最小間隔", 5., 0.01, 1000., 0.5),
        i("count", "目標点数", 200., 1., 512.),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Point,
};

pub(crate) fn spec(kind: NodeKind) -> &'static NodeSpec {
    use NodeKind::*;
    match kind {
        Phyllotaxis => &PHYLLOTAXIS,
        FibonacciSphere => &FIBONACCI_SPHERE,
        RoseCurve => &ROSE_CURVE,
        SuperformulaCurve => &SUPERFORMULA_CURVE,
        TorusKnot => &TORUS_KNOT,
        Lissajous3d => &LISSAJOUS3D,
        HilbertCurve => &HILBERT_CURVE,
        KochSnowflake => &KOCH_SNOWFLAKE,
        SierpinskiCells => &SIERPINSKI_CELLS,
        ReactionDiffusion => &REACTION_DIFFUSION,
        PoissonDisk2d => &POISSON_DISK2D,
        SpurGearProfile => &SPUR_GEAR_PROFILE,
        _ => unreachable!("not a procedural pattern"),
    }
}

fn validate(kind: NodeKind, args: &[&Value]) -> Result<(), String> {
    let inputs = spec(kind).inputs;
    if args.len() != inputs.len() {
        return Err("入力数が正しくありません。".into());
    }
    for (index, input) in inputs.iter().enumerate() {
        let value = num(args, index)?;
        if !(input.min..=input.max).contains(&value) || (input.integer && value.fract() != 0.) {
            return Err(format!("{} が範囲外です。", input.label));
        }
    }
    Ok(())
}

fn count(args: &[&Value], index: usize) -> Result<usize, String> {
    Ok(num(args, index)? as usize)
}

pub(crate) fn estimate(kind: NodeKind, args: &[&Value]) -> Result<(usize, usize), String> {
    use NodeKind::*;
    validate(kind, args)?;
    let result = match kind {
        Phyllotaxis | FibonacciSphere => {
            let n = count(args, 0)?;
            (n, n)
        }
        RoseCurve => (count(args, 2)?, 1),
        SuperformulaCurve | Lissajous3d => (count(args, 5)?, 1),
        TorusKnot => (count(args, 4)?, 1),
        SpurGearProfile => (24 * count(args, 0)?, 1),
        HilbertCurve => (4usize.pow(count(args, 1)? as u32), 1),
        KochSnowflake => (3 * 4usize.pow(count(args, 1)? as u32), 1),
        SierpinskiCells => {
            let n = 3usize.pow(count(args, 1)? as u32);
            (3 * n, n)
        }
        ReactionDiffusion => {
            let n = count(args, 0)?.pow(2);
            (n, n)
        }
        PoissonDisk2d => {
            let n = count(args, 3)?;
            (n, n)
        }
        _ => return Err("生成デザインの種類が無効です。".into()),
    };
    Ok(result)
}

fn point(p: [f64; 3]) -> Result<Value, String> {
    if !finite_point(p) {
        return Err("生成した点の座標が無効です。".into());
    }
    Ok(Value::Point(p))
}

fn curve(points: Vec<[f64; 3]>, closed: bool) -> Result<Value, String> {
    if points.len() < if closed { 3 } else { 2 } {
        return Err("曲線の点が不足しています。".into());
    }
    if points.len() > MAX_VERTICES || points.iter().any(|p| !finite_point(*p)) {
        return Err("生成した曲線が大きすぎるか、座標が無効です。".into());
    }
    Ok(Value::Curve(Arc::new(Curve {
        points,
        closed,
        geometry: None,
    })))
}

fn sample(
    samples: usize,
    period: f64,
    f: impl Fn(f64) -> [f64; 3],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut points = Vec::with_capacity(samples);
    for index in 0..samples {
        cancelled(cancel)?;
        points.push(f(period * index as f64 / samples as f64));
    }
    Ok(vec![curve(points, true)?])
}

fn gcd(mut p: usize, mut q: usize) -> usize {
    while q != 0 {
        (p, q) = (q, p % q);
    }
    p
}

fn triangle(size: f64) -> [[f64; 3]; 3] {
    [
        [0., 0., 0.],
        [size, 0., 0.],
        [0.5 * size, 0.5 * 3f64.sqrt() * size, 0.],
    ]
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|j| a[j] + t * (b[j] - a[j]))
}

// Standard distance-to-coordinate Hilbert traversal, integer arithmetic.
fn hilbert(n: usize, mut d: usize) -> [usize; 2] {
    let (mut x, mut y) = (0, 0);
    let mut s = 1;
    while s < n {
        let rx = (d / 2) & 1;
        let ry = (d ^ rx) & 1;
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        d /= 4;
        s *= 2;
    }
    [x, y]
}

fn random(state: &mut u64) -> f64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*state >> 11) as f64 / (1_u64 << 53) as f64
}

fn poisson(args: &[&Value], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let (width, height, distance) = (num(args, 0)?, num(args, 1)?, num(args, 2)?);
    let target = count(args, 3)?;
    let mut state = count(args, 4)? as u64;
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(target);
    let min_squared = distance * distance;
    for _ in 0..8192 {
        cancelled(cancel)?;
        if points.len() == target {
            break;
        }
        let p = [width * random(&mut state), height * random(&mut state), 0.];
        if points.iter().all(|q| {
            let dx = p[0] - q[0];
            let dy = p[1] - q[1];
            dx * dx + dy * dy >= min_squared
        }) {
            points.push(p);
        }
    }
    points.into_iter().map(point).collect()
}

// Gray-Scott: explicit Euler, unit grid, periodic boundary, separate old/new buffers.
fn reaction(args: &[&Value], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let side = count(args, 0)?;
    let spacing = num(args, 1)?;
    let steps = count(args, 2)?;
    let (feed, kill, threshold) = (num(args, 3)?, num(args, 4)?, num(args, 5)?);
    let mut state = count(args, 6)? as u64;
    let mut a = vec![1.; side * side];
    let mut b = vec![0.; side * side];
    let mut next_a = a.clone();
    let mut next_b = b.clone();
    let r = (side / 12).max(1);
    for k in 0..5 {
        let (x, y) = if k == 0 {
            (side / 2, side / 2)
        } else {
            (
                (random(&mut state) * side as f64) as usize,
                (random(&mut state) * side as f64) as usize,
            )
        };
        for dy in 0..=2 * r {
            for dx in 0..=2 * r {
                let j = ((y + side + dy - r) % side) * side + (x + side + dx - r) % side;
                a[j] = 0.5;
                b[j] = 0.5;
            }
        }
    }
    for _ in 0..steps {
        cancelled(cancel)?;
        for y in 0..side {
            for x in 0..side {
                let j = y * side + x;
                let neighbors = [
                    y * side + (x + 1) % side,
                    y * side + (x + side - 1) % side,
                    ((y + 1) % side) * side + x,
                    ((y + side - 1) % side) * side + x,
                ];
                let lap_a = neighbors.iter().map(|&k| a[k]).sum::<f64>() - 4. * a[j];
                let lap_b = neighbors.iter().map(|&k| b[k]).sum::<f64>() - 4. * b[j];
                let uvv = a[j] * b[j] * b[j];
                next_a[j] = (a[j] + 0.2 * (lap_a - uvv + feed * (1. - a[j]))).clamp(0., 1.);
                next_b[j] = (b[j] + 0.2 * (0.5 * lap_b + uvv - (feed + kill) * b[j])).clamp(0., 1.);
            }
        }
        std::mem::swap(&mut a, &mut next_a);
        std::mem::swap(&mut b, &mut next_b);
    }
    let mut out = Vec::new();
    for (j, &concentration) in b.iter().enumerate() {
        cancelled(cancel)?;
        if concentration >= threshold {
            out.push(point([
                (j % side) as f64 * spacing,
                (j / side) as f64 * spacing,
                0.,
            ])?);
        }
    }
    Ok(out)
}

// Standard full-depth 20-degree external spur gear. Involute flanks are
// sampled in radius; below the base circle use a radial root connector.
// Root/tip arcs are sampled too. This is a design blank, not a gear cutter.
fn spur_gear_profile(
    teeth: usize,
    module: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let pressure = 20_f64.to_radians();
    let pitch_radius = module * teeth as f64 / 2.;
    let base = pitch_radius * pressure.cos();
    let root = pitch_radius - 1.25 * module;
    let tip = pitch_radius + module;
    let start = root.max(base);
    let pitch_angle = TAU / teeth as f64;
    let flank = |radius: f64| {
        let alpha = (base / radius).clamp(0., 1.).acos();
        pitch_angle / 4. + pressure.tan() - pressure - (alpha.tan() - alpha)
    };
    let start_angle = flank(start);
    let tip_angle = flank(tip);
    if !(0. < tip_angle && tip_angle < start_angle && start_angle < pitch_angle / 2.) {
        return Err("歯車の輪郭が重なります。".into());
    }
    let mut points = Vec::with_capacity(24 * teeth);
    let mut push = |radius: f64, angle: f64| {
        let (s, c) = angle.sin_cos();
        points.push([radius * c, radius * s, 0.]);
    };
    for tooth in 0..teeth {
        cancelled(cancel)?;
        let center = tooth as f64 * pitch_angle;
        push(root, center - start_angle);
        if start > root {
            push(start, center - start_angle);
        }
        for step in 1..=6 {
            let radius = start + (tip - start) * step as f64 / 6.;
            push(radius, center - flank(radius));
        }
        // Left tip point already exists; include right tip exactly once.
        for step in 1..=3 {
            push(tip, center - tip_angle + 2. * tip_angle * step as f64 / 3.);
        }
        for step in (0..6).rev() {
            let radius = start + (tip - start) * step as f64 / 6.;
            push(radius, center + flank(radius));
        }
        if start > root {
            push(root, center + start_angle);
        }
        // Intermediate root arc samples; the next tooth supplies its endpoint.
        for step in 1..3 {
            push(
                root,
                center + start_angle + (pitch_angle - 2. * start_angle) * step as f64 / 3.,
            );
        }
    }
    curve(points, true)
}

pub(crate) fn run_item(
    kind: NodeKind,
    args: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    validate(kind, args)?;
    match kind {
        Phyllotaxis => {
            let n = count(args, 0)?;
            let (spacing, rise, angle) = (num(args, 1)?, num(args, 2)?, num(args, 3)?.to_radians());
            let mut out = Vec::with_capacity(n);
            for k in 0..n {
                cancelled(cancel)?;
                let radius = spacing * (k as f64).sqrt();
                let (s, c) = (angle * k as f64).sin_cos();
                out.push(point([radius * c, radius * s, rise * k as f64])?);
            }
            Ok(out)
        }
        FibonacciSphere => {
            let n = count(args, 0)?;
            let radius = num(args, 1)?;
            let mut out = Vec::with_capacity(n);
            for k in 0..n {
                cancelled(cancel)?;
                let z = 1. - 2. * (k as f64 + 0.5) / n as f64;
                let r = (1. - z * z).sqrt();
                let (s, c) = (GOLDEN_ANGLE * k as f64).sin_cos();
                out.push(point([radius * r * c, radius * r * s, radius * z])?);
            }
            Ok(out)
        }
        RoseCurve => {
            let radius = num(args, 0)?;
            let k = count(args, 1)?;
            let period = if k % 2 == 0 { TAU } else { PI };
            sample(
                count(args, 2)?,
                period,
                |t| {
                    let r = radius * (k as f64 * t).cos();
                    [r * t.cos(), r * t.sin(), 0.]
                },
                cancel,
            )
        }
        SuperformulaCurve => {
            let radius = num(args, 0)?;
            let symmetry = num(args, 1)?;
            let (n1, n2, n3) = (num(args, 2)?, num(args, 3)?, num(args, 4)?);
            sample(
                count(args, 5)?,
                TAU,
                |t| {
                    let (s, c) = (symmetry * t).sin_cos();
                    let r = radius * (c.abs().powf(n2) + s.abs().powf(n3)).powf(-1. / n1);
                    [r * t.cos(), r * t.sin(), 0.]
                },
                cancel,
            )
        }
        TorusKnot => {
            let (major, minor) = (num(args, 0)?, num(args, 1)?);
            let (p, q) = (count(args, 2)?, count(args, 3)?);
            if minor >= major || gcd(p, q) != 1 {
                return Err("小半径は大半径未満、p と q は互いに素にしてください。".into());
            }
            sample(
                count(args, 4)?,
                TAU,
                |t| {
                    let (s, c) = (p as f64 * t).sin_cos();
                    let (qs, qc) = (q as f64 * t).sin_cos();
                    let r = major + minor * qc;
                    [r * c, r * s, minor * qs]
                },
                cancel,
            )
        }
        Lissajous3d => {
            let radius = num(args, 0)?;
            let (fx, fy, fz) = (num(args, 1)?, num(args, 2)?, num(args, 3)?);
            let phase = num(args, 4)?.to_radians();
            sample(
                count(args, 5)?,
                TAU,
                |t| {
                    [
                        radius * (fx * t).sin(),
                        radius * (fy * t + phase).sin(),
                        radius * (fz * t + 2. * phase).sin(),
                    ]
                },
                cancel,
            )
        }
        HilbertCurve => {
            let n = 1usize << count(args, 1)?;
            let scale = num(args, 0)? / (n - 1) as f64;
            let mut points = Vec::with_capacity(n * n);
            for d in 0..n * n {
                cancelled(cancel)?;
                let [x, y] = hilbert(n, d);
                points.push([scale * x as f64, scale * y as f64, 0.]);
            }
            Ok(vec![curve(points, false)?])
        }
        KochSnowflake => {
            let mut points = triangle(num(args, 0)?).to_vec();
            for _ in 0..count(args, 1)? {
                let mut next = Vec::with_capacity(4 * points.len());
                for (j, &p) in points.iter().enumerate() {
                    cancelled(cancel)?;
                    let q = points[(j + 1) % points.len()];
                    let a = mix(p, q, 1. / 3.);
                    let b = mix(p, q, 2. / 3.);
                    let mid = mix(a, b, 0.5);
                    let h = 0.5 * 3f64.sqrt();
                    let tip = [mid[0] + h * (b[1] - a[1]), mid[1] - h * (b[0] - a[0]), 0.];
                    next.extend([p, a, tip, b]);
                }
                points = next;
            }
            Ok(vec![curve(points, true)?])
        }
        SierpinskiCells => {
            let mut cells = vec![triangle(num(args, 0)?)];
            for _ in 0..count(args, 1)? {
                let mut next = Vec::with_capacity(3 * cells.len());
                for [a, b, c] in cells {
                    cancelled(cancel)?;
                    let ab = mix(a, b, 0.5);
                    let bc = mix(b, c, 0.5);
                    let ca = mix(c, a, 0.5);
                    next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c]]);
                }
                cells = next;
            }
            let fill = num(args, 2)?;
            let mut out = Vec::with_capacity(cells.len());
            for [a, b, c] in cells {
                cancelled(cancel)?;
                let center = std::array::from_fn(|j| (a[j] + b[j] + c[j]) / 3.);
                out.push(curve(
                    [a, b, c].map(|p| mix(center, p, fill)).to_vec(),
                    true,
                )?);
            }
            Ok(out)
        }
        SpurGearProfile => Ok(vec![spur_gear_profile(
            count(args, 0)?,
            num(args, 1)?,
            cancel,
        )?]),
        ReactionDiffusion => reaction(args, cancel),
        PoissonDisk2d => poisson(args, cancel),
        _ => Err("生成デザインの種類が無効です。".into()),
    }
}

#[cfg(test)]
mod reality_gear_tests {
    use super::*;

    #[test]
    fn gear_profile_has_expected_tip_root_and_bounded_sampling() {
        for teeth in [18, 24, 64] {
            let module = 2.;
            let value = spur_gear_profile(teeth, module, &|| false).unwrap();
            let Value::Curve(profile) = value else {
                panic!("expected a curve");
            };
            assert!(profile.closed);
            assert!(profile.points.len() <= 24 * teeth);
            let pitch = teeth as f64 * module / 2.;
            let radii = profile.points.iter().map(|p| p[0].hypot(p[1]));
            let minimum = radii.clone().fold(f64::INFINITY, f64::min);
            let maximum = radii.fold(0., f64::max);
            assert!((minimum - (pitch - 1.25 * module)).abs() < 1e-10);
            assert!((maximum - (pitch + module)).abs() < 1e-10);
            for (i, point) in profile.points.iter().enumerate() {
                let next = profile.points[(i + 1) % profile.points.len()];
                assert!((point[0] - next[0]).hypot(point[1] - next[1]) > 1e-8);
            }
        }
        assert!(spur_gear_profile(24, 2., &|| true).is_err());
    }
}
