//! 拘束スケッチ: a constrained 2D sketch solved by SolidCraft's sketch solver (vendored as
//! `crates/sc-sketch`), written as a short text in the node (one command per line), output as
//! exact NURBS curves on the XY plane.
//!
//! ```text
//! 長方形 r 0 0 60 40        # rectangle: 4 lines + horizontal/vertical, ids r.b r.r r.t r.l
//! 円 c 30 20 8              # circle (centre point id c.c)
//! 半径 c 10                 # radius dimension
//! 距離x r.b.start c.c 30    # horizontal distance between points
//! ```
//! Commands (Japanese or English): 点/point id x y, 線/line id x1 y1 x2 y2 (or two point ids),
//! 円/circle id x y r, 弧/arc id cx cy x1 y1 x2 y2, 長方形/rect id x y w h; constraints
//! 水平/horizontal l, 垂直/vertical l, 一致/coincident p q, 平行/parallel a b, 直交/perpendicular a b,
//! 接する/tangent a b, 等しい/equal a b, 同心/concentric a b, 中点/midpoint p l, 固定/fix p,
//! 点on線/on p c; dimensions 距離/distance p q v, 距離x/dx, 距離y/dy, 長さ/length l v,
//! 半径/radius c v, 直径/diameter c v, 角度/angle a b deg. Points may be written as
//! `curve.start`, `curve.end`, `curve.center`, or `origin`. `#` starts a comment.
use crate::curve_geometry::{CurveGeometry, NurbsCurve};
use crate::kernel_bridge as kb;
use crate::model::{Curve, Tree, Value};
use solidcraft_geom::{Seg2, Vec2};
use solidcraft_sketch::{ConstraintKind, CurveKind, Sketch, find_profiles, solve};
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod session;

pub const MAX_SKETCH_TEXT: usize = 64 * 1024;
pub const MAX_COMMANDS: usize = 2000;

type R<T> = Result<T, String>;

fn num(word: Option<&&str>, what: &str, line: usize) -> R<f64> {
    let w = word.ok_or_else(|| format!("{line} 行目: {what}の数値が足りません。"))?;
    let v: f64 = w
        .parse()
        .map_err(|_| format!("{line} 行目: 「{w}」は数値ではありません（{what}）。"))?;
    if !v.is_finite() || v.abs() > 1e6 {
        return Err(format!("{line} 行目: {what}が大きすぎます。"));
    }
    Ok(v)
}

fn point(sk: &Sketch, word: Option<&&str>, line: usize) -> R<usize> {
    let w = word.ok_or_else(|| format!("{line} 行目: 点の名前が足りません。"))?;
    sk.resolve_point(w).ok_or_else(|| {
        format!(
            "{line} 行目: 点「{w}」が見つかりません。先に点か線を作ってください（例: l1.start）。"
        )
    })
}

fn curve(sk: &Sketch, word: Option<&&str>, line: usize) -> R<usize> {
    let w = word.ok_or_else(|| format!("{line} 行目: 曲線の名前が足りません。"))?;
    sk.curve_index(w)
        .ok_or_else(|| format!("{line} 行目: 曲線「{w}」が見つかりません。"))
}

fn err(line: usize, e: impl std::fmt::Display) -> String {
    format!("{line} 行目: {e}")
}

/// Build the sketch from its text.
pub fn parse(text: &str) -> R<Sketch> {
    parse_with_sources(text).map(|(sketch, _)| sketch)
}

fn parse_with_sources(text: &str) -> R<(Sketch, BTreeMap<String, usize>)> {
    if text.len() > MAX_SKETCH_TEXT {
        return Err("スケッチの文章が長すぎます。".into());
    }
    let mut sk = Sketch::new();
    let mut sources = BTreeMap::new();
    let mut count = 0usize;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let body = raw.split('#').next().unwrap_or("").trim();
        if body.is_empty() {
            continue;
        }
        count += 1;
        if count > MAX_COMMANDS {
            return Err(format!("命令が多すぎます（上限 {MAX_COMMANDS}）。"));
        }
        let w: Vec<&str> = body.split_whitespace().collect();
        let (Some(&cmd), Some(&id)) = (w.first(), w.get(1)) else {
            return Err(err(line, "命令の後に名前や値を書いてください。"));
        };
        let a = |k: usize| w.get(k);
        let dim = |kind: ConstraintKind, sk: &mut Sketch| -> R<()> {
            sk.add_constraint(kind, None)
                .map(|_| ())
                .map_err(|e| err(line, e))
        };
        let constraints_before = sk.constraints.len();
        match cmd {
            "点" | "point" => {
                let p = Vec2::new(num(a(2), "X", line)?, num(a(3), "Y", line)?);
                sk.add_point(p, Some(id)).map_err(|e| err(line, e))?;
            }
            "線" | "line" => {
                if w.len() >= 6 {
                    let p0 = Vec2::new(num(a(2), "X1", line)?, num(a(3), "Y1", line)?);
                    let p1 = Vec2::new(num(a(4), "X2", line)?, num(a(5), "Y2", line)?);
                    sk.add_line(p0, p1, None, None, Some(id))
                        .map_err(|e| err(line, e))?;
                } else {
                    let (p, q) = (point(&sk, a(2), line)?, point(&sk, a(3), line)?);
                    sk.add_line_pts(p, q, Some(id)).map_err(|e| err(line, e))?;
                }
            }
            "円" | "circle" => {
                let c = Vec2::new(num(a(2), "中心 X", line)?, num(a(3), "中心 Y", line)?);
                let r = num(a(4), "半径", line)?;
                sk.add_circle(c, r, None, Some(id))
                    .map_err(|e| err(line, e))?;
            }
            "弧" | "arc" => {
                let c = Vec2::new(num(a(2), "中心 X", line)?, num(a(3), "中心 Y", line)?);
                let p0 = Vec2::new(num(a(4), "始点 X", line)?, num(a(5), "始点 Y", line)?);
                let p1 = Vec2::new(num(a(6), "終点 X", line)?, num(a(7), "終点 Y", line)?);
                sk.add_arc(c, p0, p1, [None, None, None], Some(id))
                    .map_err(|e| err(line, e))?;
            }
            "長方形" | "rect" => {
                let (x, y) = (num(a(2), "X", line)?, num(a(3), "Y", line)?);
                let (wd, ht) = (num(a(4), "幅", line)?, num(a(5), "高さ", line)?);
                if wd.abs() < 1e-9 || ht.abs() < 1e-9 {
                    return Err(err(line, "幅と高さは 0 以外にしてください。"));
                }
                let c = [
                    Vec2::new(x, y),
                    Vec2::new(x + wd, y),
                    Vec2::new(x + wd, y + ht),
                    Vec2::new(x, y + ht),
                ];
                let names = ["b", "r", "t", "l"];
                let mut pts = Vec::with_capacity(4);
                for (k, p) in c.iter().enumerate() {
                    pts.push(
                        sk.add_point(*p, Some(&format!("{id}.p{k}")))
                            .map_err(|e| err(line, e))?,
                    );
                }
                let mut lines = Vec::with_capacity(4);
                for k in 0..4 {
                    let (Some(&p), Some(&q)) = (pts.get(k), pts.get((k + 1) % 4)) else {
                        continue;
                    };
                    let name = format!("{id}.{}", names.get(k).copied().unwrap_or("x"));
                    lines.push(
                        sk.add_line_pts(p, q, Some(&name))
                            .map_err(|e| err(line, e))?,
                    );
                }
                for (k, &l) in lines.iter().enumerate() {
                    let kind = if k % 2 == 0 {
                        ConstraintKind::Horizontal { l }
                    } else {
                        ConstraintKind::Vertical { l }
                    };
                    dim(kind, &mut sk)?;
                }
            }
            "楕円" | "ellipse" => {
                let c = point(&sk, a(2), line)?;
                let m = point(&sk, a(3), line)?;
                let r = num(a(4), "短半径", line)?;
                let center = sk.point(c).ok_or_else(|| err(line, "中心点がありません。"))?;
                let major = sk.point(m).ok_or_else(|| err(line, "長軸点がありません。"))?;
                if r <= 1e-9 || center.dist(major) < r {
                    return Err(err(line, "短半径は正で長半径以下にしてください。"));
                }
                sk.add_curve(CurveKind::Ellipse { c, m, r }, Some(id))
                    .map_err(|e| err(line, e))?;
            }
            "ベジェ" | "bezier" | "Bスプライン" | "bspline" => {
                let (degree, start) = if matches!(cmd, "ベジェ" | "bezier") {
                    let degree = u8::try_from(w.len().saturating_sub(3))
                        .map_err(|_| err(line, "次数が大きすぎます。"))?;
                    (degree, 2)
                } else {
                    let degree = a(2)
                        .and_then(|v| v.parse::<u8>().ok())
                        .ok_or_else(|| err(line, "次数は正の整数にしてください。"))?;
                    (degree, 3)
                };
                let pts = (start..w.len())
                    .map(|i| point(&sk, a(i), line))
                    .collect::<R<Vec<_>>>()?;
                if degree == 0 || pts.len() <= usize::from(degree) {
                    return Err(err(line, "制御点数は次数より多くしてください。"));
                }
                sk.add_curve(
                    CurveKind::Spline {
                        pts,
                        control: true,
                        degree,
                        periodic: false,
                    },
                    Some(id),
                )
                .map_err(|e| err(line, e))?;
            }
            "補助線" | "construction" | "ブロック" | "block" => {
                let ci = curve(&sk, a(1), line)?;
                let entity = sk.curves.get_mut(ci).ok_or_else(|| err(line, "曲線がありません。"))?;
                if matches!(cmd, "補助線" | "construction") {
                    entity.construction = true;
                } else {
                    entity.fixed = true;
                }
            }
            "水平点" | "horizontal_points" | "垂直点" | "vertical_points" => {
                let (p, q) = (point(&sk, a(1), line)?, point(&sk, a(2), line)?);
                let kind = if matches!(cmd, "水平点" | "horizontal_points") {
                    ConstraintKind::HorizontalPoints { p, q }
                } else {
                    ConstraintKind::VerticalPoints { p, q }
                };
                dim(kind, &mut sk)?;
            }
            "対称" | "symmetric" => dim(
                ConstraintKind::Symmetric {
                    p: point(&sk, a(1), line)?,
                    q: point(&sk, a(2), line)?,
                    l: curve(&sk, a(3), line)?,
                },
                &mut sk,
            )?,
            "水平" | "horizontal" => dim(
                ConstraintKind::Horizontal {
                    l: curve(&sk, a(1), line)?,
                },
                &mut sk,
            )?,
            "垂直" | "vertical" => dim(
                ConstraintKind::Vertical {
                    l: curve(&sk, a(1), line)?,
                },
                &mut sk,
            )?,
            "固定" | "fix" => dim(
                ConstraintKind::Fix {
                    p: point(&sk, a(1), line)?,
                },
                &mut sk,
            )?,
            "一致" | "coincident" => dim(
                ConstraintKind::Coincident {
                    p: point(&sk, a(1), line)?,
                    q: point(&sk, a(2), line)?,
                },
                &mut sk,
            )?,
            "点on線" | "on" => dim(
                ConstraintKind::PointOnCurve {
                    p: point(&sk, a(1), line)?,
                    c: curve(&sk, a(2), line)?,
                },
                &mut sk,
            )?,
            "中点" | "midpoint" => dim(
                ConstraintKind::Midpoint {
                    p: point(&sk, a(1), line)?,
                    l: curve(&sk, a(2), line)?,
                },
                &mut sk,
            )?,
            "平行" | "parallel" | "直交" | "perpendicular" | "接する" | "tangent" | "等しい"
            | "equal" | "同心" | "concentric" | "滑らか" | "smooth" | "同一直線" | "collinear" => {
                let (x, y) = (curve(&sk, a(1), line)?, curve(&sk, a(2), line)?);
                let kind = match cmd {
                    "平行" | "parallel" => ConstraintKind::Parallel { a: x, b: y },
                    "直交" | "perpendicular" => ConstraintKind::Perpendicular { a: x, b: y },
                    "接する" | "tangent" => ConstraintKind::Tangent { a: x, b: y },
                    "等しい" | "equal" => ConstraintKind::Equal { a: x, b: y },
                    "同心" | "concentric" => ConstraintKind::Concentric { a: x, b: y },
                    "同一直線" | "collinear" => ConstraintKind::Collinear { a: x, b: y },
                    _ => ConstraintKind::Smooth { a: x, b: y },
                };
                dim(kind, &mut sk)?
            }
            "距離" | "distance" | "距離x" | "dx" | "距離y" | "dy" => {
                let (p, q) = (point(&sk, a(1), line)?, point(&sk, a(2), line)?);
                let value = num(a(3), "距離", line)?;
                let kind = match cmd {
                    "距離x" | "dx" => ConstraintKind::DistanceX { p, q, value },
                    "距離y" | "dy" => ConstraintKind::DistanceY { p, q, value },
                    _ => ConstraintKind::Distance { p, q, value },
                };
                dim(kind, &mut sk)?
            }
            "長さ" | "length" => dim(
                ConstraintKind::Length {
                    l: curve(&sk, a(1), line)?,
                    value: num(a(2), "長さ", line)?,
                },
                &mut sk,
            )?,
            "半径" | "radius" => dim(
                ConstraintKind::Radius {
                    c: curve(&sk, a(1), line)?,
                    value: num(a(2), "半径", line)?,
                },
                &mut sk,
            )?,
            "直径" | "diameter" => dim(
                ConstraintKind::Diameter {
                    c: curve(&sk, a(1), line)?,
                    value: num(a(2), "直径", line)?,
                },
                &mut sk,
            )?,
            "点線距離" | "point_line_distance" => dim(
                ConstraintKind::PointLineDistance {
                    p: point(&sk, a(1), line)?,
                    l: curve(&sk, a(2), line)?,
                    value: num(a(3), "距離", line)?,
                },
                &mut sk,
            )?,
            "点円距離" | "point_circle_distance" => {
                let inside = match a(4).copied() {
                    Some("inside" | "内側") => true,
                    Some("outside" | "外側") => false,
                    _ => return Err(err(line, "inside または outside を指定してください。")),
                };
                dim(
                    ConstraintKind::PointCircleDistance {
                        p: point(&sk, a(1), line)?,
                        c: curve(&sk, a(2), line)?,
                        value: num(a(3), "距離", line)?,
                        inside,
                    },
                    &mut sk,
                )?;
            }
            "弧長" | "arc_length" => dim(
                ConstraintKind::ArcLength {
                    c: curve(&sk, a(1), line)?,
                    value: num(a(2), "弧長", line)?,
                },
                &mut sk,
            )?,
            "弧角度" | "arc_sweep" => dim(
                ConstraintKind::ArcSweep {
                    c: curve(&sk, a(1), line)?,
                    value: num(a(2), "角度", line)?.to_radians(),
                },
                &mut sk,
            )?,
            "線角度" | "line_angle" => dim(
                ConstraintKind::LineAngle {
                    l: curve(&sk, a(1), line)?,
                    value: num(a(2), "角度", line)?.to_radians(),
                },
                &mut sk,
            )?,
            "角度" | "angle" => {
                let (x, y) = (curve(&sk, a(1), line)?, curve(&sk, a(2), line)?);
                let value = num(a(3), "角度", line)?.to_radians();
                dim(
                    ConstraintKind::Angle {
                        a: x,
                        b: y,
                        value,
                        flip: false,
                    },
                    &mut sk,
                )?
            }
            other => {
                return Err(err(
                    line,
                    format!(
                        "「{other}」は知らない命令です。点・線・円・弧・長方形・水平・垂直・一致・距離・半径などが使えます。"
                    ),
                ));
            }
        }
        for c in sk.constraints.iter().skip(constraints_before) {
            sources.insert(c.id.clone(), line);
        }
    }
    Ok((sk, sources))
}

/// Solve the sketch. Returns the remaining degrees of freedom.
pub fn solve_sketch(sk: &mut Sketch) -> R<usize> {
    let report = solve(sk);
    if !report.ok() {
        let why = report.failure(|names| names.to_string());
        return Err(format!(
            "拘束を満たせませんでした（{why}）。矛盾する拘束や寸法を外すか、値を変えてください。"
        ));
    }
    Ok(report.dof)
}

/// Homogeneous Bézier pieces of a sketch segment on the plane z = 0.
fn pieces(seg: &Seg2) -> Vec<Vec<[f64; 4]>> {
    let h = |p: Vec2, w: f64| [p.x * w, p.y * w, 0., w];
    match *seg {
        Seg2::Line { a, b } => vec![vec![h(a, 1.), h(b, 1.)]],
        Seg2::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let n = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.) as usize;
            let step = sweep / n as f64;
            let at = |t: f64, r: f64| Vec2::new(center.x + r * t.cos(), center.y + r * t.sin());
            (0..n)
                .map(|k| {
                    let t0 = start + step * k as f64;
                    let w = (step / 2.).cos();
                    vec![
                        h(at(t0, radius), 1.),
                        h(at(t0 + step / 2., radius / w), w),
                        h(at(t0 + step, radius), 1.),
                    ]
                })
                .collect()
        }
        Seg2::Cubic { p0, p1, p2, p3 } => vec![vec![h(p0, 1.), h(p1, 1.), h(p2, 1.), h(p3, 1.)]],
        Seg2::Conic { a, apex, b, w } => vec![vec![h(a, 1.), h(apex, w), h(b, 1.)]],
        Seg2::Bezier { n, p } => vec![
            p.iter()
                .take(usize::from(n) + 1)
                .map(|q| h(*q, 1.))
                .collect(),
        ],
    }
}

/// Raise a homogeneous Bézier piece by one degree.
fn elevate(c: &[[f64; 4]]) -> Vec<[f64; 4]> {
    let n = c.len().saturating_sub(1);
    let mut out = Vec::with_capacity(c.len() + 1);
    for i in 0..=n + 1 {
        let a = i as f64 / (n + 1) as f64;
        let prev = if i > 0 { c.get(i - 1).copied() } else { None };
        let cur = c.get(i).copied();
        let p = match (prev, cur) {
            (Some(p), Some(q)) => std::array::from_fn(|k| a * p[k] + (1. - a) * q[k]),
            (Some(p), None) => p,
            (None, Some(q)) => q,
            (None, None) => [0.; 4],
        };
        out.push(p);
    }
    out
}

/// One exact NURBS curve through a chain of sketch segments (pieces raised to a common degree
/// and joined with full-multiplicity knots).
pub fn chain_curve(segs: &[Seg2], closed: bool, stop: &dyn Fn() -> bool) -> R<Curve> {
    let mut all: Vec<Vec<[f64; 4]>> = segs.iter().flat_map(pieces).collect();
    if all.is_empty() {
        return Err("曲線が空です。".into());
    }
    let degree = all
        .iter()
        .map(|p| p.len().saturating_sub(1))
        .max()
        .unwrap_or(1)
        .max(1);
    for p in &mut all {
        while p.len() < degree + 1 {
            *p = elevate(p);
        }
    }
    let mut ctrl: Vec<[f64; 4]> = Vec::new();
    let mut knots = vec![0.; degree + 1];
    for (k, p) in all.iter().enumerate() {
        let skip = usize::from(k > 0);
        ctrl.extend(p.iter().skip(skip));
        knots.extend(std::iter::repeat_n(
            (k + 1) as f64,
            if k + 1 == all.len() {
                degree + 1
            } else {
                degree
            },
        ));
    }
    let count = all.len() as f64;
    for k in &mut knots {
        *k /= count;
    }
    let points: Vec<[f64; 3]> = ctrl
        .iter()
        .map(|h| [h[0] / h[3], h[1] / h[3], h[2] / h[3]])
        .collect();
    let weights: Vec<f64> = ctrl.iter().map(|h| h[3]).collect();
    let nurbs = NurbsCurve::new(degree, points, weights, knots)?;
    let mut c = kb::curve_value(CurveGeometry::nurbs(nurbs), stop)?;
    c.closed = closed || c.closed;
    Ok(c)
}

/// Evaluate the 拘束スケッチ node: `mode` 0 gives the outer loop of every closed region (for
/// extrusion), 1 every sketch curve separately.
pub fn run(text: &str, mode: usize, stop: &dyn Fn() -> bool) -> R<Tree> {
    if text.trim().is_empty() {
        return Err(
            "スケッチが空です。ノードの文章欄に「長方形 r 0 0 60 40」のように書いてください。"
                .into(),
        );
    }
    if stop() {
        return Err(kb::STOPPED.into());
    }
    let mut session = session::SketchSession::from_text(text)?;
    let attempt = session.solve();
    if stop() {
        return Err(kb::STOPPED.into());
    }
    if !attempt.committed {
        return Err(session.failure_message(&attempt));
    }
    let sk = session.sketch();
    let mut out = Vec::new();
    if mode == 0 {
        for profile in find_profiles(sk) {
            if stop() {
                return Err(kb::STOPPED.into());
            }
            out.push(Value::Curve(Arc::new(chain_curve(
                &profile.region.outer.segs,
                true,
                stop,
            )?)));
            for hole in &profile.region.holes {
                out.push(Value::Curve(Arc::new(chain_curve(&hole.segs, true, stop)?)));
            }
        }
        if out.is_empty() {
            return Err("閉じた領域がありません。線の端を「一致」でつなぐか、出力を 1（各曲線）にしてください。".into());
        }
    } else {
        for (ci, c) in sk.curves.iter().enumerate() {
            if stop() {
                return Err(kb::STOPPED.into());
            }
            if c.construction {
                continue;
            }
            let segs = sk.exact_segs(ci).unwrap_or_else(|| sk.segs(ci));
            let closed = matches!(c.kind, CurveKind::Circle { .. } | CurveKind::Ellipse { .. });
            out.push(Value::Curve(Arc::new(chain_curve(&segs, closed, stop)?)));
        }
    }
    Ok(Tree {
        branches: std::collections::BTreeMap::from([(vec![0], out)]),
    })
}
