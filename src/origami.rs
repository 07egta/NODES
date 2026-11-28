//! Analytic Miura sheet. No physics solver or shared value-type changes.
use crate::engine::{cancelled, num};
use crate::model::{Curve, DataType, Mesh, NodeKind, NodeSpec, Value, i, n};
use std::sync::Arc;

const INPUTS: &[crate::model::InputSpec] = &[
    i("columns", "列数", 8., 1., 64.),
    i("rows", "行数", 6., 1., 64.),
    n("a", "横辺", 20., 0.1, 1000., 1.),
    n("b", "斜辺", 20., 0.1, 1000., 1.),
    n("alpha", "鋭角 (度)", 60., 10., 85., 1.),
    n("fold", "折り率", 0.6, 0., 0.95, 0.01),
];
static SHEET: NodeSpec = NodeSpec {
    label: "ミウラ折り",
    category: "折り紙",
    description: "平行四辺形の辺長と角度を保つ薄い折り紙面。厚みや衝突は計算しません。",
    keywords: "miura origami rigid fold ミウラ 三浦 折り紙",
    inputs: INPUTS,
    output: DataType::Mesh,
};
static CREASES: NodeSpec = NodeSpec {
    label: "ミウラ展開図",
    category: "折り紙",
    description: "平面の折線と外周を重複なしで生成します。山谷の区別や切断指示は含みません。",
    keywords: "miura origami crease pattern 展開図 折線",
    inputs: &[
        INPUTS[0], INPUTS[1], INPUTS[2], INPUTS[3], INPUTS[4],
    ],
    output: DataType::Curve,
};

pub(crate) fn spec(kind: NodeKind) -> &'static NodeSpec {
    if kind == NodeKind::OrigamiMiura {
        &SHEET
    } else {
        &CREASES
    }
}

#[derive(Clone, Copy)]
struct Parameters {
    nx: usize,
    ny: usize,
    a: f64,
    b: f64,
    alpha: f64,
    fold: f64,
}

fn count(x: f64) -> Result<usize, String> {
    if (1. ..=64.).contains(&x) && x.fract() == 0. {
        Ok(x as usize)
    } else {
        Err("行数と列数は1〜64の整数にしてください。".into())
    }
}

fn parameters(kind: NodeKind, args: &[&Value]) -> Result<Parameters, String> {
    let p = Parameters {
        nx: count(num(args, 0)?)?,
        ny: count(num(args, 1)?)?,
        a: num(args, 2)?,
        b: num(args, 3)?,
        alpha: num(args, 4)?,
        fold: if kind == NodeKind::OrigamiMiura {
            num(args, 5)?
        } else {
            0.
        },
    };
    if !(0.1..=1000.).contains(&p.a)
        || !(0.1..=1000.).contains(&p.b)
        || !(10. ..=85.).contains(&p.alpha)
        || !(0. ..=0.95).contains(&p.fold)
    {
        return Err("ミウラ折りの寸法・角度・折り率が範囲外です。".into());
    }
    Ok(p)
}

impl Parameters {
    fn vertices(self, stop: &dyn Fn() -> bool) -> Result<Vec<[f64; 3]>, String> {
        let alpha = self.alpha.to_radians();
        let theta = alpha * self.fold;
        let dx = self.a * theta.cos();
        let dz = self.a * theta.sin();
        let shift = self.b * alpha.cos() / theta.cos();
        let dy = (self.b * self.b - shift * shift).sqrt();
        let mut vertices = Vec::with_capacity((self.nx + 1) * (self.ny + 1));
        for y in 0..=self.ny {
            cancelled(stop)?;
            for x in 0..=self.nx {
                vertices.push([
                    x as f64 * dx + (y % 2) as f64 * shift,
                    y as f64 * dy,
                    (x % 2) as f64 * dz,
                ]);
            }
        }
        Ok(vertices)
    }

    fn mesh(self, stop: &dyn Fn() -> bool) -> Result<Mesh, String> {
        let vertices = self.vertices(stop)?;
        let mut triangles = Vec::with_capacity(2 * self.nx * self.ny);
        for y in 0..self.ny {
            cancelled(stop)?;
            for x in 0..self.nx {
                let a = (y * (self.nx + 1) + x) as u32;
                let b = a + 1;
                let d = a + (self.nx + 1) as u32;
                let c = d + 1;
                triangles.extend([[a, b, c], [a, c, d]]);
            }
        }
        Ok(Mesh { vertices, triangles })
    }
}

pub(crate) fn estimate(kind: NodeKind, args: &[&Value]) -> Result<(usize, usize), String> {
    let p = parameters(kind, args)?;
    if kind == NodeKind::OrigamiMiura {
        Ok(((p.nx + 1) * (p.ny + 1) + 6 * p.nx * p.ny, 1))
    } else {
        let edges = p.nx * (p.ny + 1) + p.ny * (p.nx + 1);
        Ok((2 * edges, edges))
    }
}

pub(crate) fn run(
    kind: NodeKind,
    args: &[&Value],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let p = parameters(kind, args)?;
    if kind == NodeKind::OrigamiMiura {
        return crate::engine::as_mesh(p.mesh(stop)?, stop);
    }
    let vertices = p.vertices(stop)?;
    let mut out = Vec::new();
    let stride = p.nx + 1;
    let segment = |a, b| {
        Value::Curve(Arc::new(Curve {
            points: vec![vertices[a], vertices[b]],
            closed: false,
            geometry: None,
        }))
    };
    for y in 0..=p.ny {
        cancelled(stop)?;
        for x in 0..=p.nx {
            let a = y * stride + x;
            if x < p.nx {
                out.push(segment(a, a + 1));
            }
            if y < p.ny {
                out.push(segment(a, a + stride));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
        a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt()
    }

    #[test]
    fn all_face_edges_and_diagonals_are_preserved() {
        let mut p = Parameters { nx: 4, ny: 3, a: 13., b: 21., alpha: 55., fold: 0. };
        let flat = p.mesh(&|| false).unwrap();
        for fold in [0.1, 0.5, 0.95] {
            p.fold = fold;
            let bent = p.mesh(&|| false).unwrap();
            assert_eq!(flat.triangles, bent.triangles);
            for tri in &flat.triangles {
                for j in 0..3 {
                    let a = tri[j] as usize;
                    let b = tri[(j + 1) % 3] as usize;
                    assert!((distance(flat.vertices[a], flat.vertices[b])
                        - distance(bent.vertices[a], bent.vertices[b])).abs() < 1e-9);
                }
            }
        }
    }

    #[test]
    fn flat_crease_count_and_cancellation() {
        let v = [2., 3., 10., 10., 60.].map(Value::Number);
        let args: Vec<_> = v.iter().collect();
        let curves = run(NodeKind::OrigamiMiuraCreases, &args, &|| false).unwrap();
        assert_eq!(curves.len(), 17);
        assert!(run(NodeKind::OrigamiMiuraCreases, &args, &|| true).is_err());
        assert!(count(1.5).is_err());
    }
}
