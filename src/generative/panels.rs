//! Bounded sampled shape interpolation and panel construction.
use crate::engine::{cancelled, cross, finite_point, length, map_affine_shape, num, shape, sub};
use crate::model::{Curve, DataType, MAX_ITEMS, Mesh, NodeKind, NodeSpec, Value, g, n};
use std::sync::Arc;

pub(super) static MORPH: NodeSpec = NodeSpec {
    label: "形状モーフ",
    category: "変形",
    description: "同じ頂点対応を持つ2形状を補間します。点、同じ点数・閉じ方の曲線、同じ面番号のメッシュに対応。中間曲線は折れ線近似です。",
    keywords: "shape morph blend interpolation mesh curve 形状 補間 モーフ",
    inputs: &[
        g("a", "開始形状", DataType::Geometry),
        g("b", "終了形状", DataType::Geometry),
        n("t", "補間率", 0.5, 0., 1., 0.01),
    ],
    output: DataType::Geometry,
};

pub(super) static INSET: NodeSpec = NodeSpec {
    label: "セルを縮める",
    category: "変形",
    description: "閉曲線の表示頂点の平均位置を中心として相似縮小します。一定距離のオフセットではありません。",
    keywords: "inset cell panel scale shrink セル パネル 縮小 隙間",
    inputs: &[
        g("curve", "セル輪郭", DataType::Curve),
        n("factor", "倍率", 0.85, 0.01, 1., 0.01),
    ],
    output: DataType::Curve,
};

pub(super) static FACE_PANELS: NodeSpec = NodeSpec {
    label: "メッシュ面パネル",
    category: "曲線",
    description: "メッシュの各三角形から、その面の中心で縮小した閉輪郭を面番号順に作ります。厚みや面の結合は行いません。",
    keywords: "mesh face panels triangular cells facade tessellation パネル 三角面 格子",
    inputs: &[
        g("mesh", "メッシュ", DataType::Mesh),
        n("factor", "面倍率", 0.85, 0.01, 1., 0.01),
    ],
    output: DataType::Curve,
};

fn fraction(t: f64, min: f64) -> Result<f64, String> {
    if (min..=1.).contains(&t) {
        Ok(t)
    } else {
        Err("倍率が範囲外です。".into())
    }
}

fn blend_point(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    // Coordinates are already bounded to 1e9. Use a convex combination to
    // keep intermediate coordinates inside the model's coordinate range.
    if t == 0. {
        return a;
    }
    if t == 1. {
        return b;
    }
    std::array::from_fn(|k| a[k] * (1. - t) + b[k] * t)
}

fn blend_points(
    a: &[[f64; 3]],
    b: &[[f64; 3]],
    t: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<[f64; 3]>, String> {
    let mut points = Vec::with_capacity(a.len());
    for (i, (&p, &q)) in a.iter().zip(b).enumerate() {
        if i % 256 == 0 {
            cancelled(cancel)?;
        }
        let v = blend_point(p, q, t);
        if !finite_point(v) {
            return Err("補間結果の座標が無効です。".into());
        }
        points.push(v);
    }
    Ok(points)
}

fn morph(a: &Value, b: &Value, t: f64, cancel: &dyn Fn() -> bool) -> Result<Value, String> {
    cancelled(cancel)?;
    let compatible = match (a, b) {
        (Value::Point(_), Value::Point(_)) => true,
        (Value::Curve(a), Value::Curve(b)) => {
            a.closed == b.closed && a.points.len() == b.points.len()
        }
        (Value::Mesh(a), Value::Mesh(b)) => {
            if a.vertices.len() != b.vertices.len() || a.triangles.len() != b.triangles.len() {
                false
            } else {
                let mut same = true;
                for (i, (x, y)) in a.triangles.iter().zip(&b.triangles).enumerate() {
                    if i % 256 == 0 {
                        cancelled(cancel)?;
                    }
                    if x != y {
                        same = false;
                        break;
                    }
                }
                same
            }
        }
        _ => false,
    };
    if !compatible {
        return Err("同じ種類・頂点対応・面番号の形状が必要です。".into());
    }
    if t == 0. {
        return Ok(a.clone());
    }
    if t == 1. {
        return Ok(b.clone());
    }
    match (a, b) {
        (Value::Point(a), Value::Point(b)) => {
            let p = blend_point(*a, *b, t);
            if !finite_point(p) {
                return Err("補間結果の座標が無効です。".into());
            }
            Ok(Value::Point(p))
        }
        (Value::Curve(a), Value::Curve(b)) => {
            let points = blend_points(&a.points, &b.points, t, cancel)?;
            Ok(Value::Curve(Arc::new(Curve {
                points,
                closed: a.closed,
                geometry: None,
            })))
        }
        (Value::Mesh(a), Value::Mesh(b)) => {
            let mesh = Mesh {
                vertices: blend_points(&a.vertices, &b.vertices, t, cancel)?,
                triangles: a.triangles.clone(),
            };
            crate::engine::validate_mesh(&mesh, cancel)?;
            Ok(Value::Mesh(Arc::new(mesh)))
        }
        _ => Err("点・曲線・メッシュを接続してください。".into()),
    }
}

fn inset(value: &Value, factor: f64, cancel: &dyn Fn() -> bool) -> Result<Value, String> {
    let Value::Curve(c) = value else {
        return Err("閉曲線が必要です。".into());
    };
    if !c.closed || c.points.len() < 3 {
        return Err("3点以上の閉曲線が必要です。".into());
    }
    if factor == 1. {
        return Ok(value.clone());
    }
    let mut center = [0.; 3];
    for (i, p) in c.points.iter().enumerate() {
        if i % 256 == 0 {
            cancelled(cancel)?;
        }
        for k in 0..3 {
            center[k] += p[k] / c.points.len() as f64;
        }
    }
    map_affine_shape(value, |p| blend_point(center, p, factor), cancel)
}

fn face_panels(mesh: &Mesh, factor: f64, cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    if mesh.triangles.len() > MAX_ITEMS {
        return Err("面パネルの個数が上限を超えます。".into());
    }
    let mut out = Vec::with_capacity(mesh.triangles.len());
    for (i, t) in mesh.triangles.iter().enumerate() {
        if i % 128 == 0 {
            cancelled(cancel)?;
        }
        let [Some(a), Some(b), Some(c)] = t.map(|j| mesh.vertices.get(j as usize)) else {
            return Err("面の頂点番号が無効です。".into());
        };
        let center = std::array::from_fn(|k| a[k] / 3. + b[k] / 3. + c[k] / 3.);
        let points = [*a, *b, *c].map(|p| blend_point(center, p, factor));
        if points.iter().any(|p| !finite_point(*p))
            || length(cross(sub(points[1], points[0]), sub(points[2], points[0]))) < 1e-12
        {
            return Err("面パネルが退化しました。倍率を大きくしてください。".into());
        }
        out.push(Value::Curve(Arc::new(Curve {
            points: points.to_vec(),
            closed: true,
            geometry: None,
        })));
    }
    Ok(out)
}

pub(super) fn run(
    kind: NodeKind,
    args: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(cancel)?;
    let value = shape(args)?;
    match kind {
        NodeKind::ShapeMorph => {
            let other = args.get(1).ok_or("終了形状が必要です。")?;
            Ok(vec![morph(
                value,
                other,
                fraction(num(args, 2)?, 0.)?,
                cancel,
            )?])
        }
        NodeKind::InsetCell => Ok(vec![inset(value, fraction(num(args, 1)?, 0.01)?, cancel)?]),
        NodeKind::MeshFacePanels => {
            let Value::Mesh(mesh) = value else {
                return Err("メッシュが必要です。".into());
            };
            face_panels(mesh, fraction(num(args, 1)?, 0.01)?, cancel)
        }
        _ => Err("不明なパネルノードです。".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_and_endpoints() {
        let a = Value::Point([0., 2., 4.]);
        let b = Value::Point([6., 4., 0.]);
        assert_eq!(morph(&a, &b, 0., &|| false).unwrap(), a);
        assert_eq!(morph(&a, &b, 1., &|| false).unwrap(), b);
        assert_eq!(
            morph(&a, &b, 0.5, &|| false).unwrap(),
            Value::Point([3., 3., 2.])
        );
        assert!(morph(&a, &b, 0.5, &|| true).is_err());
    }

    #[test]
    fn panels_preserve_face_center_and_scale_area() {
        let mesh = Mesh {
            vertices: vec![[0., 0., 0.], [4., 0., 0.], [0., 4., 0.]],
            triangles: vec![[0, 1, 2]],
        };
        let v = face_panels(&mesh, 0.5, &|| false).unwrap();
        let Value::Curve(c) = &v[0] else {
            panic!("curve");
        };
        assert!(c.closed && c.geometry.is_none());
        let p = &c.points;
        let area = length(cross(sub(p[1], p[0]), sub(p[2], p[0]))) * 0.5;
        assert!((area - 2.).abs() < 1e-12);
        for k in 0..3 {
            let original = mesh.vertices.iter().map(|p| p[k] / 3.).sum::<f64>();
            let actual = p.iter().map(|p| p[k] / 3.).sum::<f64>();
            assert!((original - actual).abs() < 1e-12);
        }
        assert!(face_panels(&mesh, 0.5, &|| true).is_err());
    }

    #[test]
    fn rejects_mismatched_mesh_topology_and_degenerate_interpolation() {
        let a = Mesh {
            vertices: vec![[0., 0., 0.], [4., 0., 0.], [0., 4., 0.]],
            triangles: vec![[0, 1, 2]],
        };
        let mut b = a.clone();
        b.triangles[0] = [0, 2, 1];
        assert!(
            morph(
                &Value::Mesh(Arc::new(a.clone())),
                &Value::Mesh(Arc::new(b)),
                0.5,
                &|| false
            )
            .is_err()
        );
        let mut b = a.clone();
        b.vertices[1] = [-4., 0., 0.];
        assert!(
            morph(
                &Value::Mesh(Arc::new(a)),
                &Value::Mesh(Arc::new(b)),
                0.5,
                &|| false
            )
            .is_err()
        );
    }
}
