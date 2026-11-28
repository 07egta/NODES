//! Ideal planar rotating rigid squares. Not a material or hinge solver.
use crate::engine::{as_mesh, cancelled, num};
use crate::model::{DataType as D, InputSpec, Mesh, NodeKind as K, NodeSpec, Value, i, n};

const SIZE: InputSpec = n("size", "基準セル辺長 (mm)", 10., 0.1, 1000., 0.5);
const ANGLE: InputSpec = n("angle", "回転角 (度)", 30., 0., 45., 1.);
const INSET: InputSpec = n("inset", "片側の縮小率", 0.04, -0.2, 0.4, 0.01);
const START: InputSpec = n("start", "初期角 (度)", 5., 0., 45., 1.);
const END: InputSpec = n("end", "目標角 (度)", 30., 0., 45., 1.);
const KINDS: &[K] = &[
    K::MatterRotatingSquares,
    K::MatterExpansionRatio,
    K::MatterPlateContact,
    K::MatterInverseAngle,
];

pub fn is_kind(kind: K) -> bool {
    KINDS.contains(&kind)
}

static PLATES: NodeSpec = NodeSpec {
    label: "回転正方形格子",
    category: "物理",
    description: "交互に回転する剛体板の姿勢を生成。縮小率が正なら隙間、負なら干渉。ヒンジは未生成。",
    keywords: "auxetic rotating squares programmable matter 負ポアソン 回転格子",
    inputs: &[
        i("nx", "列", 6., 2., 32.),
        i("ny", "行", 6., 2., 32.),
        SIZE,
        ANGLE,
        INSET,
        n("thickness", "板厚 (mm)", 1., 0.01, 100., 0.1),
    ],
    output: D::Mesh,
};
static RATIO: NodeSpec = NodeSpec {
    label: "格子伸縮比",
    category: "数値",
    description: "理想周期ピッチの目標/初期比。縦横同じ倍率。材料のポアソン比ではありません。",
    keywords: "auxetic pitch ratio strain 伸縮比",
    inputs: &[START, END],
    output: D::Number,
};
static CONTACT: NodeSpec = NodeSpec {
    label: "回転板の干渉",
    category: "物理",
    description: "理想周期格子の板内部の重なりを判定。0=非重複（接触含む）、1=重複。ヒンジや他形状は対象外。",
    keywords: "auxetic collision contact interference 干渉 接触",
    inputs: &[ANGLE, INSET],
    output: D::Number,
};
static INVERSE: NodeSpec = NodeSpec {
    label: "伸縮比から角度",
    category: "数値",
    description: "初期角と目標周期伸縮比から0〜45度の角度を逆算。範囲外の目標はエラー。",
    keywords: "auxetic inverse design pitch angle 逆設計 角度",
    inputs: &[
        START,
        n("ratio", "目標倍率", 1.2, 0.5, 2., 0.01),
    ],
    output: D::Number,
};

pub fn spec(kind: K) -> &'static NodeSpec {
    match kind {
        K::MatterRotatingSquares => &PLATES,
        K::MatterExpansionRatio => &RATIO,
        K::MatterPlateContact => &CONTACT,
        _ => &INVERSE,
    }
}

fn bounded(value: f64, min: f64, max: f64) -> Result<f64, String> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err("回転格子の設定が範囲外です。".into())
    }
}

fn count(value: f64) -> Result<usize, String> {
    bounded(value, 2., 32.)?;
    if value.fract() != 0. {
        return Err("格子の個数は整数にしてください。".into());
    }
    Ok(value as usize)
}

fn pitch_factor(angle: f64) -> Result<f64, String> {
    let (s, c) = bounded(angle, 0., 45.)?.to_radians().sin_cos();
    Ok(s + c)
}

pub fn expansion_ratio(start: f64, end: f64) -> Result<f64, String> {
    Ok(pitch_factor(end)? / pitch_factor(start)?)
}

pub fn inverse_angle(start: f64, ratio: f64) -> Result<f64, String> {
    let target = pitch_factor(start)? * bounded(ratio, 0.5, 2.)?;
    let limit = std::f64::consts::SQRT_2;
    if target < 1. - 1e-14 || target > limit + 1e-14 {
        return Err("指定倍率は回転格子の可動範囲外です。".into());
    }
    Ok(45. - (target.clamp(1., limit) / limit).acos().to_degrees())
}

/// Adjacent ideal squares touch at a vertex (edge at zero degrees).
/// Uniform in-plane scaling about each centre preserves separation when <=1.
/// Scaling above 1 puts the original shared point strictly inside both squares.
/// Diagonal and farther pairs cannot overlap at scale <=1 because their
/// centre distance is at least sqrt(2)*pitch >= sqrt(2)*side.
pub fn plate_contact(angle: f64, inset: f64) -> Result<bool, String> {
    pitch_factor(angle)?;
    bounded(inset, -0.2, 0.4)?;
    Ok(inset < 0.)
}

pub fn rotating_squares(
    nx: usize,
    ny: usize,
    size: f64,
    angle: f64,
    inset: f64,
    thickness: f64,
    stop: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if !(2..=32).contains(&nx) || !(2..=32).contains(&ny) {
        return Err("格子の個数は2〜32です。".into());
    }
    bounded(size, 0.1, 1000.)?;
    bounded(inset, -0.2, 0.4)?;
    bounded(thickness, 0.01, 100.)?;
    let pitch = size * pitch_factor(angle)?;
    let h = size * (0.5 - inset);
    let (s, c) = angle.to_radians().sin_cos();
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(nx * ny * 8),
        triangles: Vec::with_capacity(nx * ny * 12),
    };
    const FACES: [[u32; 3]; 12] = [
        [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
        [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
        [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
    ];
    for y in 0..ny {
        for x in 0..nx {
            cancelled(stop)?;
            let sin = if (x + y) % 2 == 0 { s } else { -s };
            let cx = x as f64 * pitch;
            let cy = y as f64 * pitch;
            let offset = mesh.vertices.len() as u32;
            for z in [0., thickness] {
                for [u, v] in [[-h, -h], [h, -h], [h, h], [-h, h]] {
                    mesh.vertices.push([cx + c * u - sin * v, cy + sin * u + c * v, z]);
                }
            }
            mesh.triangles.extend(FACES.iter().map(|f| f.map(|j| j + offset)));
        }
    }
    Ok(mesh)
}

pub(crate) fn estimate(kind: K, a: &[&Value]) -> Result<(usize, usize), String> {
    if kind == K::MatterRotatingSquares {
        Ok((20 * count(num(a, 0)?)? * count(num(a, 1)?)?, 1))
    } else {
        Ok((1, 1))
    }
}

pub(crate) fn run(kind: K, a: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let number = match kind {
        K::MatterRotatingSquares => {
            let mesh = rotating_squares(
                count(num(a, 0)?)?, count(num(a, 1)?)?, num(a, 2)?,
                num(a, 3)?, num(a, 4)?, num(a, 5)?, stop,
            )?;
            return as_mesh(mesh, stop);
        }
        K::MatterExpansionRatio => expansion_ratio(num(a, 0)?, num(a, 1)?)?,
        K::MatterPlateContact => {
            if plate_contact(num(a, 0)?, num(a, 1)?)? { 1. } else { 0. }
        }
        K::MatterInverseAngle => inverse_angle(num(a, 0)?, num(a, 1)?)?,
        _ => return Err("回転格子ノードではありません。".into()),
    };
    Ok(vec![Value::Number(number)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_endpoints_and_inverse() {
        assert!((expansion_ratio(0., 45.).unwrap() - 2_f64.sqrt()).abs() < 1e-12);
        assert_eq!(expansion_ratio(20., 20.).unwrap(), 1.);
        for end in [0., 5., 20., 30., 44.] {
            let ratio = expansion_ratio(10., end).unwrap();
            assert!((inverse_angle(10., ratio).unwrap() - end).abs() < 1e-9);
        }
        assert!(inverse_angle(0., 2.).is_err());
    }

    #[test]
    fn geometry_and_contact() {
        let mesh = rotating_squares(2, 2, 10., 30., 0.05, 2., &|| false).unwrap();
        assert_eq!(mesh.vertices.len(), 32);
        assert_eq!(mesh.triangles.len(), 48);
        let p = mesh.vertices[0];
        let q = mesh.vertices[1];
        assert!(((p[0] - q[0]).hypot(p[1] - q[1]) - 9.).abs() < 1e-12);
        for angle in [0., 20., 45.] {
            assert!(!plate_contact(angle, 0.).unwrap());
            assert!(!plate_contact(angle, 0.1).unwrap());
            assert!(plate_contact(angle, -0.1).unwrap());
        }
        assert!(rotating_squares(2, 2, 10., 30., 0., 1., &|| true).is_err());
        assert!(rotating_squares(usize::MAX, 2, 10., 30., 0., 1., &|| false).is_err());
        assert!(expansion_ratio(f64::NAN, 30.).is_err());
    }
}
