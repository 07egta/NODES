//! Analytic mathematical surfaces sampled into indexed triangle meshes.
//! Formula/domain references: docs/development/OMEGA05_MATHEMATICAL_SURFACES.md.
//! Outputs are approximations, never exact NURBS, B-reps or printable solids.
use crate::engine::{as_mesh, cancelled, finite_point, num};
use crate::model::{DataType, Mesh, NodeKind, NodeSpec, Value, i, n};
use std::f64::consts::TAU;

static CATENOID: NodeSpec = NodeSpec {
    label: "カテノイド",
    category: "立体",
    description: "懸垂線を回転させた極小曲面。上下は開いた近似メッシュです。",
    keywords: "catenoid minimal surface 極小曲面 懸垂線",
    inputs: &[
        n("radius", "最小半径", 20., 0.1, 1000., 1.),
        n("extent", "高さ比（片側）", 1.2, 0.1, 4., 0.1),
        i("u_segments", "U 分割", 64., 8., 256.),
        i("v_segments", "V 分割", 32., 2., 256.),
    ],
    output: DataType::Mesh,
};

static HELICOID: NodeSpec = NodeSpec {
    label: "ヘリコイド",
    category: "立体",
    description: "直線を軸に沿って回転・移動させた極小曲面。ピッチは一周当たりの上昇量。開いたメッシュです。",
    keywords: "helicoid minimal ruled screw 螺旋 極小曲面",
    inputs: &[
        n("radius", "半径", 30., 0.1, 1000., 1.),
        n("pitch", "一周の上昇量", 25., 0.1, 1000., 1.),
        n("turns", "回転数", 1.5, 0.25, 4., 0.25),
        i("u_segments", "U 分割", 96., 8., 256.),
        i("v_segments", "V 分割", 24., 2., 256.),
    ],
    output: DataType::Mesh,
};

static ENNEPER_SURFACE: NodeSpec = NodeSpec {
    label: "エンネパー曲面",
    category: "立体",
    description: "多項式で表した極小曲面。範囲を広げると自己交差し得る開いたメッシュです。",
    keywords: "enneper minimal polynomial saddle 極小曲面 鞍型",
    inputs: &[
        n("scale", "尺度", 20., 0.1, 1000., 1.),
        n("extent", "パラメータ範囲（片側）", 1., 0.2, 2., 0.1),
        i("u_segments", "U 分割", 64., 8., 256.),
        i("v_segments", "V 分割", 64., 2., 256.),
    ],
    output: DataType::Mesh,
};

static MOBIUS_STRIP: NodeSpec = NodeSpec {
    label: "メビウスの帯",
    category: "立体",
    description: "半回転した帯の端を逆向きに結合。非向き付け可能な曲面で、厚みのない近似メッシュです。",
    keywords: "mobius moebius strip nonorientable メビウス 非向き付け",
    inputs: &[
        n("radius", "中心円半径", 30., 0.1, 1000., 1.),
        n("half_width", "半幅", 8., 0.05, 900., 0.5),
        i("u_segments", "U 分割", 96., 8., 256.),
        i("v_segments", "V 分割", 16., 2., 256.),
    ],
    output: DataType::Mesh,
};

static KLEIN_BOTTLE: NodeSpec = NodeSpec {
    label: "クラインの壺",
    category: "立体",
    description: "8の字断面の3次元浸入。境界はありませんが自己交差し、ソリッドではありません。",
    keywords: "klein bottle figure eight nonorientable クライン 自己交差",
    inputs: &[
        n("scale", "尺度", 10., 0.1, 1000., 1.),
        n("major_ratio", "中心半径比", 3., 2.5, 6., 0.1),
        i("u_segments", "U 分割", 96., 8., 256.),
        i("v_segments", "V 分割", 48., 8., 256.),
    ],
    output: DataType::Mesh,
};

static DINI_SURFACE: NodeSpec = NodeSpec {
    label: "ディニ曲面",
    category: "立体",
    description: "擬球をねじった負曲率曲面。特異点を避けるため角度範囲を制限した開いたメッシュです。",
    keywords: "dini pseudosphere negative curvature 負曲率 螺旋",
    inputs: &[
        n("scale", "尺度", 20., 0.1, 1000., 1.),
        n("twist", "ねじれ比", 0.2, 0.01, 2., 0.01),
        n("turns", "回転数", 2., 0.25, 4., 0.25),
        n("v_min", "断面角下限（rad）", 0.2, 0.05, 1.4, 0.05),
        n("v_max", "断面角上限（rad）", 1.35, 0.1, 1.45, 0.05),
        i("u_segments", "U 分割", 96., 8., 256.),
        i("v_segments", "V 分割", 32., 2., 256.),
    ],
    output: DataType::Mesh,
};

pub(crate) fn is_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Catenoid
            | NodeKind::Helicoid
            | NodeKind::EnneperSurface
            | NodeKind::MobiusStrip
            | NodeKind::KleinBottle
            | NodeKind::DiniSurface
    )
}

pub(crate) fn spec(kind: NodeKind) -> &'static NodeSpec {
    match kind {
        NodeKind::Catenoid => &CATENOID,
        NodeKind::Helicoid => &HELICOID,
        NodeKind::EnneperSurface => &ENNEPER_SURFACE,
        NodeKind::MobiusStrip => &MOBIUS_STRIP,
        NodeKind::KleinBottle => &KLEIN_BOTTLE,
        NodeKind::DiniSurface => &DINI_SURFACE,
        _ => unreachable!("not a mathematical surface"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Seam {
    Open,
    Periodic,
    Reversed,
}

#[derive(Clone, Copy)]
struct Grid {
    nu: usize,
    nv: usize,
    seam: Seam,
    wrap_v: bool,
}

impl Grid {
    fn columns(self) -> usize {
        self.nu + usize::from(self.seam == Seam::Open)
    }

    fn rows(self) -> usize {
        self.nv + usize::from(!self.wrap_v)
    }

    fn vertex_count(self) -> usize {
        self.columns() * self.rows()
    }

    fn triangle_count(self) -> usize {
        2 * self.nu * self.nv
    }

    // The reversed seam identifies (1, v) with (0, 1-v). For a closed V
    // direction this is modulo nv, so the Klein bottle has no boundary.
    fn index(self, mut u: usize, mut v: usize) -> u32 {
        if u == self.nu && self.seam != Seam::Open {
            u = 0;
            if self.seam == Seam::Reversed {
                v = self.nv - v;
            }
        }
        if self.wrap_v {
            v %= self.nv;
        }
        (u * self.rows() + v) as u32
    }
}

fn grid(kind: NodeKind, args: &[&Value]) -> Result<Grid, String> {
    let inputs = spec(kind).inputs;
    if args.len() != inputs.len() {
        return Err("数学曲面の入力数が正しくありません。".into());
    }
    // Validate direct kernel calls too, before any floating-to-integer cast.
    for (index, input) in inputs.iter().enumerate() {
        let x = num(args, index)?;
        if x < input.min || x > input.max || (input.integer && x.fract() != 0.) {
            return Err(format!("{} が入力範囲外です。", input.label));
        }
    }
    let nu = num(args, inputs.len() - 2)? as usize;
    let nv = num(args, inputs.len() - 1)? as usize;
    let seam = match kind {
        NodeKind::Catenoid => Seam::Periodic,
        NodeKind::MobiusStrip | NodeKind::KleinBottle => Seam::Reversed,
        _ => Seam::Open,
    };
    if kind == NodeKind::MobiusStrip && num(args, 1)? > 0.9 * num(args, 0)? {
        return Err("半幅は中心円半径の90%以下にしてください。".into());
    }
    if kind == NodeKind::DiniSurface && num(args, 3)? >= num(args, 4)? {
        return Err("断面角の下限は上限より小さくしてください。".into());
    }
    if matches!(kind, NodeKind::Helicoid | NodeKind::DiniSurface)
        && (nu as f64) < 12. * num(args, 2)?
    {
        return Err("一周当たり12以上のU分割が必要です。".into());
    }
    Ok(Grid {
        nu,
        nv,
        seam,
        wrap_v: kind == NodeKind::KleinBottle,
    })
}

pub(crate) fn estimate(kind: NodeKind, args: &[&Value]) -> Result<(usize, usize), String> {
    let grid = grid(kind, args)?;
    // Value::Mesh::weight = vertex count + 3 * triangle count.
    Ok((grid.vertex_count() + 3 * grid.triangle_count(), 1))
}

fn point(kind: NodeKind, p: &[f64], u: f64, v: f64) -> [f64; 3] {
    let scale = p[0];
    let t = TAU * u;
    match kind {
        NodeKind::Catenoid => {
            let h = p[1] * (2. * v - 1.);
            let r = scale * h.cosh();
            [r * t.cos(), r * t.sin(), scale * h]
        }
        NodeKind::Helicoid => {
            let r = scale * (2. * v - 1.);
            let angle = t * p[2];
            [r * angle.cos(), r * angle.sin(), p[1] * p[2] * u]
        }
        NodeKind::EnneperSurface => {
            let x = p[1] * (2. * u - 1.);
            let y = p[1] * (2. * v - 1.);
            [
                scale * (x - x.powi(3) / 3. + x * y * y),
                scale * (-y - x * x * y + y.powi(3) / 3.),
                scale * (x * x - y * y),
            ]
        }
        NodeKind::MobiusStrip => {
            let s = p[1] * (2. * v - 1.);
            let r = scale + s * (t / 2.).cos();
            [r * t.cos(), r * t.sin(), s * (t / 2.).sin()]
        }
        NodeKind::KleinBottle => {
            let s = TAU * v;
            let (a, b) = (s.sin(), (2. * s).sin());
            let (sin, cos) = (t / 2.).sin_cos();
            let r = scale * (p[1] + cos * a - sin * b);
            [r * t.cos(), r * t.sin(), scale * (sin * a + cos * b)]
        }
        NodeKind::DiniSurface => {
            let angle = t * p[2];
            let s = p[3] + (p[4] - p[3]) * v;
            [
                scale * angle.cos() * s.sin(),
                scale * angle.sin() * s.sin(),
                scale * (s.cos() + (s / 2.).tan().ln() + p[1] * angle),
            ]
        }
        _ => unreachable!("not a mathematical surface"),
    }
}

pub(crate) fn run(
    kind: NodeKind,
    args: &[&Value],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let grid = grid(kind, args)?;
    let params = args
        .iter()
        .map(|value| value.number())
        .collect::<Result<Vec<_>, _>>()?;
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(grid.vertex_count()),
        triangles: Vec::with_capacity(grid.triangle_count()),
    };
    for a in 0..grid.columns() {
        cancelled(stop)?;
        for b in 0..grid.rows() {
            let q = point(
                kind,
                &params,
                a as f64 / grid.nu as f64,
                b as f64 / grid.nv as f64,
            );
            if !finite_point(q) {
                return Err("数学曲面の座標が有限範囲を超えました。".into());
            }
            mesh.vertices.push(q);
        }
    }
    for a in 0..grid.nu {
        cancelled(stop)?;
        for b in 0..grid.nv {
            let p = grid.index(a, b);
            let q = grid.index(a + 1, b);
            let r = grid.index(a + 1, b + 1);
            let s = grid.index(a, b + 1);
            mesh.triangles.push([p, q, r]);
            mesh.triangles.push([p, r, s]);
        }
    }
    // Shared validation rejects out-of-range vertices/indices and zero-area
    // faces. It does not certify embeddedness or global orientability.
    as_mesh(mesh, stop)
}
