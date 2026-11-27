//! Affine operations retaining exact curve data and indexed triangle meshes.
use crate::engine::{cancelled, cross, dot, map_affine_shape, num, shape, sub};
use crate::math_nodes::{Frame, norm, plus, scale, unit, xyz};
use crate::model::{DataType, InputSpec, NodeKind, NodeSpec, Value, g, i, n};
use std::sync::Arc;

const GEOMETRY: InputSpec = g("geometry", "形状", DataType::Geometry);
const ORIGIN: InputSpec = g("origin", "平面の原点", DataType::Point);
const NORMAL: InputSpec = g("normal", "平面の法線", DataType::Point);
const FROM: InputSpec = g("from", "元の方向", DataType::Point);
const TO: InputSpec = g("to", "先の方向", DataType::Point);
const MAX_COPIES: usize = 1_024;

macro_rules! transform {
    ($name:ident, $label:literal, $category:literal, $description:literal, $keywords:literal, $ports:expr) => {
        static $name: NodeSpec = NodeSpec {
            label: $label,
            category: $category,
            description: $description,
            keywords: $keywords,
            inputs: $ports,
            output: DataType::Geometry,
        };
    };
}

transform!(
    CAMERA_OBSCURA,
    "点対称・拡縮",
    "変形",
    "中心の反対側へ形状を映し、中心からの距離に倍率を掛けます。倍率 1 で点対称。倍率は 0 以外。",
    "camera obscura point mirror central inversion 点対称 中心反転",
    &[
        GEOMETRY,
        g("center", "中心点", DataType::Point),
        n("factor", "倍率", 1., -1e6, 1e6, 0.1),
    ]
);

transform!(
    PROJECT,
    "形状を平面に投影",
    "変形",
    "点・曲線・メッシュを平面に垂直投影します。面がつぶれるメッシュはエラーになります。",
    "project geometry planar orthogonal projection 垂直投影 正射影",
    &[GEOMETRY, ORIGIN, NORMAL]
);

transform!(
    PROJECT_ALONG,
    "形状を指定方向に投影",
    "変形",
    "指定ベクトルに沿って平面へ投影します。平面に平行な方向と、面がつぶれるメッシュはエラーになります。",
    "project along directional oblique projection 平行投影 斜投影",
    &[
        GEOMETRY,
        ORIGIN,
        NORMAL,
        g("direction", "投影方向", DataType::Point)
    ]
);

transform!(
    ROTATE_DIRECTION,
    "方向を合わせて回転",
    "変形",
    "中心を固定し、元の方向から先の方向へ最小角で回転します。",
    "rotate direction align vector 方向 ベクトル 回転",
    &[GEOMETRY, g("center", "回転中心", DataType::Point), FROM, TO]
);

transform!(
    ORIENT_DIRECTION,
    "位置と方向を合わせる",
    "変形",
    "基準点を移動し、元の方向を先の方向に最小角で合わせます。",
    "orient direction align rotate transform 方向 位置合わせ",
    &[
        GEOMETRY,
        g("source", "元の基準点", DataType::Point),
        FROM,
        g("target", "先の基準点", DataType::Point),
        TO
    ]
);

transform!(
    ORIENT_PLANE,
    "平面から平面へ配置",
    "変形",
    "元の平面座標を先の平面で再現します。原点と X/Y 方向で指定した軸を直交化します。",
    "orient plane to plane frame change basis 平面 配置 座標系",
    &[
        GEOMETRY,
        g("source_origin", "元の原点", DataType::Point),
        g("source_x", "元の X 方向", DataType::Point),
        g("source_y", "元の Y 方向", DataType::Point),
        g("target_origin", "先の原点", DataType::Point),
        g("target_x", "先の X 方向", DataType::Point),
        g("target_y", "先の Y 方向", DataType::Point),
    ]
);

transform!(
    BOX_ARRAY,
    "箱状配列 3D",
    "配列",
    "ワールド X/Y/Z に沿って形状を複製します。元の形状を含めて最大 1024 個。",
    "box array 3d grid lattice copy 立体 三次元 配列 複製",
    &[
        GEOMETRY,
        i("nx", "X 個数", 2., 1., 64.),
        i("ny", "Y 個数", 2., 1., 64.),
        i("nz", "Z 個数", 2., 1., 64.),
        n("dx", "X 間隔", 20., -1e6, 1e6, 1.),
        n("dy", "Y 間隔", 20., -1e6, 1e6, 1.),
        n("dz", "Z 間隔", 20., -1e6, 1e6, 1.),
    ]
);

pub(crate) fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    use NodeKind::*;
    Some(match kind {
        CameraObscura => &CAMERA_OBSCURA,
        ProjectGeometry => &PROJECT,
        ProjectGeometryAlong => &PROJECT_ALONG,
        RotateDirection => &ROTATE_DIRECTION,
        OrientDirection => &ORIENT_DIRECTION,
        OrientPlane => &ORIENT_PLANE,
        BoxArray => &BOX_ARRAY,
        _ => return None,
    })
}

pub(crate) fn run(
    kind: NodeKind,
    args: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    let geometry = shape(args)?;
    let map = |f: &dyn Fn([f64; 3]) -> [f64; 3]| map_affine_shape(geometry, f, cancel);
    let result = match kind {
        CameraObscura => {
            let center = xyz(args, 1)?;
            let factor = num(args, 2)?;
            if factor == 0. {
                return Err("倍率は 0 以外を指定してください。".into());
            }
            let mut output = map(&|p| plus(center, scale(sub(center, p), factor)))?;
            if factor > 0.
                && let Value::Mesh(mesh) = &mut output
            {
                for (index, face) in Arc::make_mut(mesh).triangles.iter_mut().enumerate() {
                    if index % 512 == 0 {
                        cancelled(cancel)?;
                    }
                    face.swap(1, 2);
                }
            }
            output
        }
        ProjectGeometry | ProjectGeometryAlong => {
            let origin = xyz(args, 1)?;
            let normal = unit(xyz(args, 2)?)?;
            let direction = if kind == ProjectGeometryAlong {
                let direction = unit(xyz(args, 3)?)?;
                let alignment = dot(direction, normal);
                if alignment.abs() <= 1e-12 {
                    return Err("投影方向が平面と平行に近すぎます。".into());
                }
                scale(direction, 1. / alignment)
            } else {
                normal
            };
            map(&|p| sub(p, scale(direction, dot(sub(p, origin), normal))))?
        }
        RotateDirection | OrientDirection => {
            let source = xyz(args, 1)?;
            let from = xyz(args, 2)?;
            let (target, to) = if kind == OrientDirection {
                (xyz(args, 3)?, xyz(args, 4)?)
            } else {
                (source, xyz(args, 3)?)
            };
            if let Some(rotation) = DirectionRotation::between(from, to)? {
                map(&|p| plus(target, rotation.apply(sub(p, source))))?
            } else {
                let offset = sub(target, source);
                map(&|p| plus(p, offset))?
            }
        }
        OrientPlane => {
            let source = Frame::new(xyz(args, 1)?, xyz(args, 2)?, xyz(args, 3)?)?;
            let target = Frame::new(xyz(args, 4)?, xyz(args, 5)?, xyz(args, 6)?)?;
            map(&|p| target.world(source.local(p)))?
        }
        BoxArray => return box_array(args, cancel),
        _ => return Err("未対応の変形ノードです。".into()),
    };
    Ok(vec![result])
}

struct DirectionRotation {
    axis: [f64; 3],
    sine: f64,
    cosine: f64,
}

impl DirectionRotation {
    fn between(from: [f64; 3], to: [f64; 3]) -> Result<Option<Self>, String> {
        let from = unit(from)?;
        let to = unit(to)?;
        let axis = cross(from, to);
        let cosine = dot(from, to);
        if axis != [0.; 3] {
            let sine = norm(axis);
            let length = sine.hypot(cosine);
            return Ok(Some(Self {
                axis: unit(axis)?,
                sine: sine / length,
                cosine: cosine / length,
            }));
        }
        if cosine >= 0. {
            return Ok(None);
        }
        // Deterministically choose a half-turn axis perpendicular to the input.
        let [x, y, z] = from.map(f64::abs);
        let guide = if x <= y && x <= z {
            [1., 0., 0.]
        } else if y <= z {
            [0., 1., 0.]
        } else {
            [0., 0., 1.]
        };
        Ok(Some(Self {
            axis: unit(cross(from, guide))?,
            sine: 0.,
            cosine: -1.,
        }))
    }

    fn apply(&self, point: [f64; 3]) -> [f64; 3] {
        let complement = if self.cosine > 0. {
            self.sine * self.sine / (1. + self.cosine)
        } else {
            1. - self.cosine
        };
        plus(
            plus(
                scale(point, self.cosine),
                scale(cross(self.axis, point), self.sine),
            ),
            scale(self.axis, dot(self.axis, point) * complement),
        )
    }
}

fn array_counts(args: &[&Value]) -> Result<[usize; 3], String> {
    let mut counts = [0; 3];
    for (axis, count) in counts.iter_mut().enumerate() {
        let value = num(args, axis + 1)?;
        if !(1.0..=64.0).contains(&value) || value.fract() != 0. {
            return Err("箱状配列の個数は 1〜64 の整数を指定してください。".into());
        }
        *count = value as usize;
    }
    if counts.iter().product::<usize>() > MAX_COPIES {
        return Err(format!("箱状配列は {MAX_COPIES} 個までです。"));
    }
    Ok(counts)
}

pub(crate) fn array_estimate(args: &[&Value]) -> Result<(usize, usize), String> {
    let count = array_counts(args)?.into_iter().product::<usize>();
    Ok((shape(args)?.weight().saturating_mul(count), count))
}

fn box_array(args: &[&Value], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let geometry = shape(args)?;
    let [nx, ny, nz] = array_counts(args)?;
    let [dx, dy, dz] = [num(args, 4)?, num(args, 5)?, num(args, 6)?];
    let mut result = Vec::with_capacity(nx * ny * nz);
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                cancelled(cancel)?;
                let offset = [x as f64 * dx, y as f64 * dy, z as f64 * dz];
                result.push(map_affine_shape(geometry, |p| plus(p, offset), cancel)?);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Evaluator;
    use crate::model::{Graph, Mesh};

    const O: Value = Value::Point([0., 0., 0.]);
    const X: Value = Value::Point([1., 0., 0.]);
    const Y: Value = Value::Point([0., 1., 0.]);
    const Z: Value = Value::Point([0., 0., 1.]);

    #[test]
    fn projection_is_geometric_and_rejects_parallel_rays() {
        let p = Value::Point([4., 3., 9.]);
        let origin = Value::Point([0., 0., 2.]);
        let oblique = Value::Point([1., 0., -1.]);
        assert_eq!(
            run(NodeKind::ProjectGeometry, &[&p, &origin, &Z], &|| false).unwrap(),
            [Value::Point([4., 3., 2.])]
        );
        let projected = run(
            NodeKind::ProjectGeometryAlong,
            &[&p, &origin, &Z, &oblique],
            &|| false,
        )
        .unwrap();
        assert!(norm(sub(xyz(&[&projected[0]], 0).unwrap(), [11., 3., 2.])) < 1e-12);
        assert!(
            run(
                NodeKind::ProjectGeometryAlong,
                &[&p, &origin, &Z, &X],
                &|| false
            )
            .is_err()
        );
    }

    #[test]
    fn rotation_handles_opposite_directions_and_translation() {
        assert_eq!(
            run(NodeKind::RotateDirection, &[&X, &O, &X, &Y], &|| false).unwrap(),
            [Y]
        );
        let opposite = Value::Point([-2., 0., 0.]);
        assert_eq!(
            run(NodeKind::RotateDirection, &[&X, &O, &X, &opposite], &|| {
                false
            })
            .unwrap(),
            [Value::Point([-1., 0., 0.])]
        );
        let target = Value::Point([4., 5., 6.]);
        assert_eq!(
            run(
                NodeKind::OrientDirection,
                &[&X, &O, &X, &target, &Z],
                &|| false
            )
            .unwrap(),
            [Value::Point([4., 5., 7.])]
        );
    }

    #[test]
    fn point_mirror_preserves_outward_mesh_winding() {
        let mesh = Value::Mesh(Arc::new(Mesh {
            vertices: vec![[0.; 3], [1., 0., 0.], [0., 1., 0.]],
            triangles: vec![[0, 1, 2]],
        }));
        let result = run(
            NodeKind::CameraObscura,
            &[&mesh, &O, &Value::Number(2.)],
            &|| false,
        )
        .unwrap();
        let Value::Mesh(result) = &result[0] else {
            panic!("expected mesh")
        };
        assert_eq!(result.vertices, [[0.; 3], [-2., 0., 0.], [0., -2., 0.]]);
        assert_eq!(result.triangles, [[0, 2, 1]]);
    }

    #[test]
    fn orientation_preserves_curve_geometry() {
        let mut graph = Graph::default();
        let circle = graph.add(NodeKind::Circle, [0.; 2]).unwrap();
        let result = Evaluator::default().evaluate(&graph, &|| false);
        let curve = &result.outputs[&circle].as_ref().unwrap().branches[&vec![0]][0];
        let target = Value::Point([11., 12., 13.]);
        let transformed = run(
            NodeKind::OrientPlane,
            &[curve, &O, &X, &Y, &target, &Y, &Z],
            &|| false,
        )
        .unwrap();
        let (Value::Curve(before), Value::Curve(after)) = (curve, &transformed[0]) else {
            panic!("expected curves")
        };
        let before = before.geometry.as_ref().unwrap().point(0.25).unwrap();
        let after = after.geometry.as_ref().unwrap().point(0.25).unwrap();
        assert!(
            norm(sub(
                after,
                [11. + before[2], 12. + before[0], 13. + before[1]]
            )) < 1e-9
        );
    }

    #[test]
    fn box_array_order_budget_and_cancellation() {
        let one = Value::Number(1.);
        let two = Value::Number(2.);
        let four = Value::Number(4.);
        let args = [&X, &two, &one, &two, &four, &one, &two];
        assert_eq!(array_estimate(&args).unwrap(), (4, 4));
        assert_eq!(
            run(NodeKind::BoxArray, &args, &|| false).unwrap(),
            [[1., 0., 0.], [5., 0., 0.], [1., 0., 2.], [5., 0., 2.]].map(Value::Point)
        );
        let max = Value::Number(64.);
        assert!(array_estimate(&[&X, &max, &max, &max, &one, &one, &one]).is_err());
        assert!(run(NodeKind::BoxArray, &args, &|| true).is_err());
    }
}
