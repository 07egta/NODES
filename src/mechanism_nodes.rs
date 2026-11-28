//! OMEGA 16: sampled involute profiles and analytic planar mechanism kinematics.
//! Lengths use model units (normally mm), UI angles use degrees, Jacobian uses radians.
//! No contact, load, manufacturing accuracy or ISO/JIS certification is implied.
use crate::engine::{cancelled, num};
use crate::model::{Curve, DataType as D, NodeKind as K, NodeSpec, Value, i, n};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

const MIN_LENGTH: f64 = 0.01;
const MAX_LENGTH: f64 = 1e5;
const ANGLE_LIMIT: f64 = 3600.;
const TOL: f64 = 1e-12;

macro_rules! spec {
    ($label:literal, $description:literal, $keywords:literal, $out:ident, [$($input:expr),* $(,)?]) => {{
        static SPEC: NodeSpec = NodeSpec {
            label: $label,
            category: "機構",
            description: $description,
            keywords: $keywords,
            inputs: &[$($input),*],
            output: D::$out,
        };
        Some(&SPEC)
    }};
}

pub fn spec(kind: K) -> Option<&'static NodeSpec> {
    match kind {
        K::MechSpurGearProfile => spec!(
            "平歯車の歯形",
            "転位なしの外歯インボリュートを折れ線近似。歯元は近似で、強度・かみ合い・加工精度は未検証。閉じたXY輪郭を押し出しへ接続できます。",
            "spur gear involute profile module tooth 歯車 平歯車 歯形 モジュール",
            Curve,
            [
                n("module", "モジュール", 2., 0.1, 100., 0.1),
                i("teeth", "歯数", 24., 3., 128.),
                n("pressure_angle", "圧力角 (度)", 20., 15., 35., 0.5),
                n("thinning", "ピッチ円上の歯厚減少", 0., 0., 100., 0.01),
                i("samples", "片歯面の分割数", 8., 4., 16.)
            ]
        ),
        K::MechGearAngle => spec!(
            "歯車の回転比",
            "かみ合いを仮定した従動歯車の角度変化。外歯は逆回転、内歯は同回転。接触や歯形は計算しません。",
            "gear ratio motion relation angle 歯車 減速比 回転",
            Number,
            [
                i("driver", "駆動歯数", 20., 1., 10000.),
                i("driven", "従動歯数", 40., 1., 10000.),
                n("angle", "駆動角度 (度)", 90., -ANGLE_LIMIT, ANGLE_LIMIT, 1.),
                i("internal", "内歯 (0:外歯 1:内歯)", 0., 0., 1.)
            ]
        ),
        K::MechRackTravel => spec!(
            "ラックの移動量",
            "ピニオンのピッチ円上の回転を直線変位に変換します。正方向は入力角度と同じ。歯形・接触は計算しません。",
            "rack pinion travel rotation translation ラック ピニオン 移動",
            Number,
            [
                n("module", "モジュール", 2., 0.1, 100., 0.1),
                i("teeth", "歯数", 20., 1., 10000.),
                n("angle", "回転角度 (度)", 90., -ANGLE_LIMIT, ANGLE_LIMIT, 1.)
            ]
        ),
        K::MechFourBar => spec!(
            "4節リンク",
            "固定軸を原点とX軸上に置く平面4節リンク。2円の交点で組立姿勢を選びます。閉じた骨格曲線を返し、組立不能な寸法はエラーにします。",
            "four bar linkage mechanism crank rocker 四節リンク 4節リンク",
            Curve,
            [
                n("ground", "固定軸間距離", 60., MIN_LENGTH, MAX_LENGTH, 1.),
                n("crank", "入力リンク長", 20., MIN_LENGTH, MAX_LENGTH, 1.),
                n("coupler", "中間リンク長", 60., MIN_LENGTH, MAX_LENGTH, 1.),
                n("rocker", "出力リンク長", 40., MIN_LENGTH, MAX_LENGTH, 1.),
                n("angle", "入力角度 (度)", 45., -ANGLE_LIMIT, ANGLE_LIMIT, 1.),
                i("branch", "組立姿勢 (0/1)", 0., 0., 1.)
            ]
        ),
        K::MechSliderCrank => spec!(
            "スライダークランク",
            "回転軸・クランク端・スライダー端の開いた骨格曲線。スライダーはY=オフセット上に拘束されます。",
            "slider crank piston offset linkage スライダー クランク ピストン",
            Curve,
            [
                n("crank", "クランク長", 20., MIN_LENGTH, MAX_LENGTH, 1.),
                n("rod", "連接棒長", 60., MIN_LENGTH, MAX_LENGTH, 1.),
                n("offset", "ガイドのY位置", 0., -MAX_LENGTH, MAX_LENGTH, 1.),
                n("angle", "入力角度 (度)", 45., -ANGLE_LIMIT, ANGLE_LIMIT, 1.),
                i("branch", "右/左 (0/1)", 0., 0., 1.)
            ]
        ),
        K::MechArm2r => spec!(
            "2関節アーム順運動学",
            "XY平面の2回転関節アーム。第2角度は第1リンクからの相対角。原点・肘・先端の曲線を返します。",
            "robot arm forward kinematics 2r ロボットアーム 順運動学",
            Curve,
            [
                n("first", "第1リンク長", 40., MIN_LENGTH, MAX_LENGTH, 1.),
                n("second", "第2リンク長", 30., MIN_LENGTH, MAX_LENGTH, 1.),
                n("q1", "第1角度 (度)", 30., -ANGLE_LIMIT, ANGLE_LIMIT, 1.),
                n("q2", "第2角度 (度)", 60., -ANGLE_LIMIT, ANGLE_LIMIT, 1.)
            ]
        ),
        K::MechArm2rInverse => spec!(
            "2関節アーム逆運動学",
            "XY目標点に対する2Rの解析解。角度の数値リスト [q1, q2] (度) を返すのでリスト項目で分けます。到達不能や解が一意に定まらない原点はエラー。",
            "robot arm inverse kinematics ik 2r ロボット 逆運動学",
            Number,
            [
                n("first", "第1リンク長", 40., MIN_LENGTH, MAX_LENGTH, 1.),
                n("second", "第2リンク長", 30., MIN_LENGTH, MAX_LENGTH, 1.),
                n("x", "目標X", 40., -2. * MAX_LENGTH, 2. * MAX_LENGTH, 1.),
                n("y", "目標Y", 30., -2. * MAX_LENGTH, 2. * MAX_LENGTH, 1.),
                i("branch", "肘姿勢 (0:正 1:負)", 0., 0., 1.)
            ]
        ),
        K::MechJacobian2r => spec!(
            "2関節アームヤコビアン",
            "平面2Rの位置ヤコビアンを行優先 [dx/dq1, dx/dq2, dy/dq1, dy/dq2] の数値リストで返します。微分単位はラジアンです。",
            "robot arm jacobian differential kinematics 2r ヤコビアン 微分",
            Number,
            [
                n("first", "第1リンク長", 40., MIN_LENGTH, MAX_LENGTH, 1.),
                n("second", "第2リンク長", 30., MIN_LENGTH, MAX_LENGTH, 1.),
                n("q1", "第1角度 (度)", 30., -ANGLE_LIMIT, ANGLE_LIMIT, 1.),
                n("q2", "第2角度 (度)", 60., -ANGLE_LIMIT, ANGLE_LIMIT, 1.)
            ]
        ),
        _ => None,
    }
}

pub fn is_kind(kind: K) -> bool {
    spec(kind).is_some()
}

fn inputs(kind: K, args: &[&Value]) -> Result<Vec<f64>, String> {
    let spec = spec(kind).ok_or("機構ノードが不明です。")?;
    if args.len() != spec.inputs.len() {
        return Err("機構ノードの入力数が正しくありません。".into());
    }
    spec.inputs
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let v = num(args, index)?;
            if !(port.min..=port.max).contains(&v) || (port.integer && v.fract() != 0.) {
                return Err(format!(
                    "{}は {}〜{} の{}値にしてください。",
                    port.label,
                    port.min,
                    port.max,
                    if port.integer { "整数" } else { "数" }
                ));
            }
            Ok(v)
        })
        .collect()
}

fn curve(points: Vec<[f64; 3]>, closed: bool) -> Vec<Value> {
    vec![Value::Curve(Arc::new(Curve {
        points,
        closed,
        geometry: None,
    }))]
}

fn polar(radius: f64, angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [radius * c, radius * s, 0.]
}

fn involute(t: f64) -> f64 {
    t - t.atan()
}

fn gear_profile(p: &[f64], stop: &dyn Fn() -> bool) -> Result<Vec<[f64; 3]>, String> {
    let (m, z, alpha, thinning, steps) = (p[0], p[1], p[2].to_radians(), p[3], p[4] as usize);
    // Conservative no-undercut condition for a full-depth, unshifted rack model.
    if z * alpha.sin().powi(2) < 2. {
        return Err("切下げの恐れがあります。歯数または圧力角を増やしてください。".into());
    }
    let pitch_r = m * z / 2.;
    let base = pitch_r * alpha.cos();
    let root = pitch_r - 1.25 * m;
    let tip = pitch_r + m;
    let pitch = TAU / z;
    let t0 = ((root.max(base) / base).powi(2) - 1.).max(0.).sqrt();
    let t1 = ((tip / base).powi(2) - 1.).sqrt();
    let half = |t: f64| PI / (2. * z) - thinning / (2. * pitch_r) + involute(alpha.tan()) - involute(t);
    let h0 = half(t0);
    let h1 = half(t1);
    if h1 <= TOL || h0 >= pitch / 2. - TOL {
        return Err("歯先または歯元の隙間が消失します。歯厚減少・歯数・圧力角を見直してください。".into());
    }
    let radial_root = root < base * (1. - TOL);
    let mut points = Vec::with_capacity(z as usize * (2 * steps + 12));
    for tooth in 0..z as usize {
        cancelled(stop)?;
        let theta = tooth as f64 * pitch;
        if radial_root {
            points.push(polar(root, theta - h0));
        }
        for j in 0..=steps {
            let t = t0 + (t1 - t0) * j as f64 / steps as f64;
            points.push(polar(base * t.hypot(1.), theta - half(t)));
        }
        // Tip arc, then the opposite involute. Do not duplicate endpoints.
        for j in 1..=4 {
            points.push(polar(tip, theta - h1 + 2. * h1 * j as f64 / 4.));
        }
        for j in (0..steps).rev() {
            let t = t0 + (t1 - t0) * j as f64 / steps as f64;
            points.push(polar(base * t.hypot(1.), theta + half(t)));
        }
        if radial_root {
            points.push(polar(root, theta + h0));
        }
        for j in 1..4 {
            points.push(polar(root, theta + h0 + (pitch - 2. * h0) * j as f64 / 4.));
        }
    }
    Ok(points)
}

fn four_bar(p: &[f64]) -> Result<Vec<[f64; 3]>, String> {
    let (g, a, b, c) = (p[0], p[1], p[2], p[3]);
    let end = polar(a, p[4].to_radians());
    let dx = g - end[0];
    let dy = -end[1];
    let d = dx.hypot(dy);
    let eps = TOL * g.max(a).max(b).max(c);
    if d <= eps || d > b + c + eps || d < (b - c).abs() - eps {
        return Err("この角度では4節リンクの組立姿勢が定まりません。".into());
    }
    let x = ((b - c) * (b + c) + d * d) / (2. * d);
    let h = ((b - x) * (b + x)).max(0.).sqrt() * if p[5] == 0. { 1. } else { -1. };
    let ux = dx / d;
    let uy = dy / d;
    let joint = [end[0] + x * ux - h * uy, end[1] + x * uy + h * ux, 0.];
    Ok(vec![[0.; 3], end, joint, [g, 0., 0.]])
}

fn slider_crank(p: &[f64]) -> Result<Vec<[f64; 3]>, String> {
    let (a, b, offset) = (p[0], p[1], p[2]);
    let end = polar(a, p[3].to_radians());
    let dy = (offset - end[1]).abs();
    if dy > b + TOL * b.max(a).max(offset.abs()) {
        return Err("連接棒がガイドまで届きません。".into());
    }
    let dx = ((b - dy).max(0.) * (b + dy)).sqrt();
    let slider = [end[0] + if p[4] == 0. { dx } else { -dx }, offset, 0.];
    Ok(vec![[0.; 3], end, slider])
}

fn arm(p: &[f64]) -> Vec<[f64; 3]> {
    let q1 = p[2].to_radians();
    let q2 = p[3].to_radians();
    let elbow = polar(p[0], q1);
    let second = polar(p[1], q1 + q2);
    vec![
        [0.; 3],
        elbow,
        [elbow[0] + second[0], elbow[1] + second[1], 0.],
    ]
}

fn inverse(p: &[f64]) -> Result<[f64; 2], String> {
    let (a, b, x, y) = (p[0], p[1], p[2], p[3]);
    let r = x.hypot(y);
    let eps = TOL * a.max(b);
    if r > a + b + eps || r < (a - b).abs() - eps {
        return Err("目標点はアームの可動範囲外です。".into());
    }
    if r <= eps {
        return Err("原点の姿勢を一意に決められません。".into());
    }
    let c = ((r * r - a * a - b * b) / (2. * a * b)).clamp(-1., 1.);
    let q2 = c.acos() * if p[4] == 0. { 1. } else { -1. };
    let q1 = y.atan2(x) - (b * q2.sin()).atan2(a + b * q2.cos());
    Ok([q1.to_degrees(), q2.to_degrees()])
}

fn jacobian(p: &[f64]) -> [f64; 4] {
    let q1 = p[2].to_radians();
    let q12 = q1 + p[3].to_radians();
    let (s1, c1) = q1.sin_cos();
    let (s12, c12) = q12.sin_cos();
    [
        -p[0] * s1 - p[1] * s12,
        -p[1] * s12,
        p[0] * c1 + p[1] * c12,
        p[1] * c12,
    ]
}

pub fn estimate(kind: K, args: &[&Value]) -> Result<(usize, usize), String> {
    let p = inputs(kind, args)?;
    Ok(match kind {
        K::MechSpurGearProfile => (p[1] as usize * (2 * p[4] as usize + 12), 1),
        K::MechFourBar => (4, 1),
        K::MechSliderCrank | K::MechArm2r => (3, 1),
        K::MechArm2rInverse => (2, 2),
        K::MechJacobian2r => (4, 4),
        _ => (1, 1),
    })
}

pub fn run(kind: K, args: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let p = inputs(kind, args)?;
    match kind {
        K::MechSpurGearProfile => Ok(curve(gear_profile(&p, stop)?, true)),
        K::MechGearAngle => {
            let sign = if p[3] == 0. { -1. } else { 1. };
            Ok(vec![Value::Number(sign * p[0] / p[1] * p[2])])
        }
        K::MechRackTravel => Ok(vec![Value::Number(p[0] * p[1] / 2. * p[2].to_radians())]),
        K::MechFourBar => Ok(curve(four_bar(&p)?, true)),
        K::MechSliderCrank => Ok(curve(slider_crank(&p)?, false)),
        K::MechArm2r => Ok(curve(arm(&p), false)),
        K::MechArm2rInverse => Ok(inverse(&p)?.into_iter().map(Value::Number).collect()),
        K::MechJacobian2r => Ok(jacobian(&p).into_iter().map(Value::Number).collect()),
        _ => Err("機構ノードが不明です。".into()),
    }
}
