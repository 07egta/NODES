//! Node adapters for the mathematical surface kernel. One output per node
//! because the current graph schema has one output per node.
use crate::engine::{as_mesh, cancelled, num, shape};
use crate::model::{DataType as D, NodeKind as K, NodeSpec, Value, g, i, n};
use crate::surface_kernel::{ClosedSolid, NurbsSurface, Surface, SurfaceValue};
use std::sync::Arc;

pub const KINDS: &[K] = &[
    K::PlaneSurface,
    K::NurbsPanel,
    K::BilinearSurface,
    K::CylinderSurface,
    K::FrustumSurface,
    K::SphereSurface,
    K::TorusSurface,
    K::EvaluateSurface,
    K::SurfaceDirection,
    K::SurfaceNormalLine,
    K::SurfaceArea,
    K::SurfaceClosestPoint,
    K::SurfaceIsoCurve,
    K::DivideSurface,
    K::SurfaceMesh,
    K::BrepBox,
    K::BrepVolume,
    K::BrepArea,
    K::BrepIntersection,
    K::BrepMove,
    K::BrepFaces,
    K::BrepEdges,
    K::BrepVertices,
    K::BrepMesh,
    K::NurbsVase,
    K::RuledSurface,
    K::SurfaceExtrude,
    K::BrepUnion,
    K::BrepDifference,
    K::BrepOffset,
    K::SurfaceGaussianCurvature,
    K::SurfaceMeanCurvature,
    K::SurfacePrincipalCurvatures,
];
const S: crate::model::InputSpec = g("surface", "曲面", D::Surface);
const B: crate::model::InputSpec = g("solid", "閉じた立体", D::Solid);
const U: crate::model::InputSpec = n("u", "U", 0.5, 0., 1., 0.01);
const V: crate::model::InputSpec = n("v", "V", 0.5, 0., 1., 0.01);
const W: crate::model::InputSpec = n("width", "幅", 40., 0.001, 1e6, 1.);
const DEP: crate::model::InputSpec = n("depth", "奥行", 40., 0.001, 1e6, 1.);
const H: crate::model::InputSpec = n("height", "高さ", 60., 0.001, 1e6, 1.);
const R: crate::model::InputSpec = n("radius", "半径", 20., 0.001, 1e6, 0.5);
macro_rules! spec {
    ($label:literal,$description:literal,$keywords:literal,$out:ident,[$($input:expr),* $(,)?]) => {{
        static SPEC:NodeSpec=NodeSpec{label:$label,category:"精密曲面",description:$description,
            keywords:$keywords,inputs:&[$($input),*],output:D::$out};
        &SPEC
    }};
}
pub fn spec(kind: K) -> &'static NodeSpec {
    match kind {
        K::PlaneSurface => spec!(
            "平面曲面",
            "数学的な長方形の平面。メッシュではありません。",
            "plane surface planar srf 平面",
            Surface,
            [W, DEP]
        ),
        K::NurbsPanel => spec!(
            "NURBS パネル",
            "二次×二次の制御格子。中央制御点の高さで膨らみを調整します。",
            "nurbs bezier surface patch panel 曲面",
            Surface,
            [W, DEP, n("rise", "中央制御点高さ", 20., -1e6, 1e6, 1.)]
        ),
        K::BilinearSurface => spec!(
            "4点から曲面",
            "4点の制御格子から双一次NURBSを作成。特異な表示面はエラーになります。",
            "four point bilinear nurbs ruled surface 4点",
            Surface,
            [
                g("p00", "P00", D::Point),
                g("p01", "P01", D::Point),
                g("p10", "P10", D::Point),
                g("p11", "P11", D::Point)
            ]
        ),
        K::CylinderSurface => spec!(
            "円筒曲面",
            "解析的な円筒の側面。上下の蓋はありません。",
            "cylinder surface pipe 円筒",
            Surface,
            [R, H]
        ),
        K::FrustumSurface => spec!(
            "円錐台曲面",
            "2つの正の半径と高さを指定する解析側面。蓋なし。",
            "frustum conical surface taper 花器 円錐台",
            Surface,
            [R, n("top", "上半径", 30., 0.001, 1e6, 0.5), H]
        ),
        K::SphereSurface => spec!(
            "球曲面",
            "球の数学的な曲面。閉曲面ですがB-rep立体ではありません。",
            "sphere analytic surface 球",
            Surface,
            [R]
        ),
        K::TorusSurface => spec!(
            "トーラス曲面",
            "大半径が小半径より大きい解析トーラス。",
            "torus analytic surface 円環",
            Surface,
            [
                n("major", "大半径", 35., 0.001, 1e6, 0.5),
                n("minor", "小半径", 8., 0.001, 1e6, 0.5)
            ]
        ),
        K::EvaluateSurface => spec!(
            "曲面上の点",
            "正規化されたU/Vで曲面上の点を評価します。",
            "evaluate surface uv point evalsrf",
            Point,
            [S, U, V]
        ),
        K::SurfaceDirection => spec!(
            "曲面方向成分",
            "方向 (0:法線 1:U接線 2:V接線) のXYZを点型で返します。接線は正規化UVに対する微分。",
            "surface normal tangent derivative vector",
            Point,
            [S, U, V, i("axis", "方向", 0., 0., 2.)]
        ),
        K::SurfaceNormalLine => spec!(
            "曲面法線の表示",
            "曲面上の点から単位法線方向に線を描きます。",
            "surface normal line 法線",
            Curve,
            [S, U, V, n("length", "表示長", 10., 0.001, 1e6, 0.5)]
        ),
        K::SurfaceGaussianCurvature => spec!(
            "ガウス曲率",
            "正確な二階微分からガウス曲率Kを求めます。特異点・非C2内部ノットはエラー。",
            "surface gaussian curvature K 曲率",
            Number,
            [S, U, V]
        ),
        K::SurfaceMeanCurvature => spec!(
            "平均曲率",
            "法線Su×Svと第二基本形式から平均曲率Hを求めます。外向き球はH=-1/r。",
            "surface mean curvature H 曲率",
            Number,
            [S, U, V]
        ),
        K::SurfacePrincipalCurvatures => spec!(
            "主曲率",
            "符号付き主曲率2値を昇順で返します。法線Su×Sv。外向き球の両曲率は-1/r。",
            "surface principal curvature k1 k2 曲率",
            Number,
            [S, U, V]
        ),
        K::SurfaceArea => spec!(
            "曲面積",
            "解析曲面は公式、NURBSはノット区間ごとの適応 Gauss 積分による近似値です。",
            "surface area integrate 面積",
            Number,
            [S, i("steps", "積分精度", 32., 4., 128.)]
        ),
        K::SurfaceClosestPoint => spec!(
            "曲面の最近点（平面）",
            "現在は長方形・平行四辺形の平面曲面に限定。",
            "surface closest point projection 最近点 投影",
            Point,
            [S, g("point", "点", D::Point)]
        ),
        K::SurfaceIsoCurve => spec!(
            "曲面の等値曲線",
            "UまたはVを固定した精密曲線。表示分割と独立したNURBSを保持。解析曲面の円弧パラメータは角度と異なります。",
            "iso curve isocurve surface 等値曲線",
            Curve,
            [
                S,
                i("axis", "固定軸 (0:U 1:V)", 0., 0., 1.),
                n("t", "固定値", 0.5, 0., 1., 0.01),
                i("steps", "分割", 48., 4., 512.)
            ]
        ),
        K::DivideSurface => spec!(
            "曲面を点に分割",
            "均等なUV格子上の点のリスト。面積の均等分割ではありません。",
            "divide surface grid uv 点格子",
            Point,
            [
                S,
                i("u_count", "U点数", 8., 2., 32.),
                i("v_count", "V点数", 8., 2., 32.)
            ]
        ),
        K::SurfaceMesh => spec!(
            "曲面をメッシュ化",
            "明示的に表示・書き出し用三角形へ近似変換します。",
            "surface mesh tessellation export",
            Mesh,
            [
                S,
                i("u_steps", "U分割", 32., 4., 128.),
                i("v_steps", "V分割", 24., 4., 128.)
            ]
        ),
        K::BrepBox => spec!(
            "B-rep 箱",
            "8頂点、12稜線、6面の閉じた箱。メッシュとは別の立体です。",
            "brep box solid exact 箱 立体",
            Solid,
            [W, DEP, H]
        ),
        K::BrepVolume => spec!(
            "B-rep 体積",
            "対応する箱立体の体積を寸法から算出します。",
            "brep solid volume 体積",
            Number,
            [B]
        ),
        K::BrepArea => spec!(
            "B-rep 表面積",
            "対応する箱立体の表面積を算出します。",
            "brep solid area 表面積",
            Number,
            [B]
        ),
        K::BrepIntersection => spec!(
            "B-rep 共通部分（箱）",
            "軸平行な箱同士の正の体積を持つ共通部分。空・接触・微小寸法はエラー。",
            "solid intersection boolean brep intersect 積",
            Solid,
            [B, g("other", "もう一つの立体", D::Solid)]
        ),
        K::BrepMove => spec!(
            "B-rep 移動",
            "精密な箱立体を平行移動します。",
            "brep move solid translate 移動",
            Solid,
            [
                B,
                n("x", "X", 10., -1e6, 1e6, 1.),
                n("y", "Y", 0., -1e6, 1e6, 1.),
                n("z", "Z", 0., -1e6, 1e6, 1.)
            ]
        ),
        K::BrepFaces => spec!(
            "B-rep の面",
            "箱の6つの支持平面曲面を外向きの順序で取り出します。",
            "brep faces deconstruct 面",
            Surface,
            [B]
        ),
        K::BrepEdges => spec!(
            "B-rep の稜線",
            "箱の12本の共有稜線を取り出します。",
            "brep edges deconstruct wireframe 稜線",
            Curve,
            [B]
        ),
        K::BrepVertices => spec!(
            "B-rep の頂点",
            "箱の8つの共有頂点を取り出します。",
            "brep vertices deconstruct 頂点",
            Point,
            [B]
        ),
        K::NurbsVase => spec!(
            "NURBS 回転花器",
            "正確な有理円と二次ベジェ断面の回転曲面。中央半径は制御値。開いた側面です。",
            "nurbs vase revolve rational surface 花器 回転",
            Surface,
            [
                R,
                n("middle", "中央制御半径", 36., 0.001, 1e6, 0.5),
                n("top", "上半径", 24., 0.001, 1e6, 0.5),
                H
            ]
        ),
        K::RuledSurface => spec!(
            "線織曲面（2断面）",
            "対応するNURBS断面を正確に直線補間。次数・ノット・重みが一致する全区間に対応。表示点列は形状に影響しません。",
            "ruled surface linear loft 曲面 線織",
            Surface,
            [
                g("a", "断面A", D::Curve),
                g("b", "断面B", D::Curve),
                i("reverse", "Bの方向反転", 0., 0., 1.)
            ]
        ),
        K::SurfaceExtrude => spec!(
            "曲面を押し出す",
            "NURBSまたは点列曲線をZ方向に正確に押し出す線織側面。蓋なし、最大32制御点・次数8。周期・部分曲線は未対応。",
            "extrude surface ruled 押し出し",
            Surface,
            [g("curve", "曲線", D::Curve), H]
        ),
        K::BrepUnion => spec!(
            "B-rep 結合（箱）",
            "軸平行な箱の和が単一の直方体になる場合のみ実行します。",
            "solid union boolean brep 和",
            Solid,
            [B, g("other", "もう一つの立体", D::Solid)]
        ),
        K::BrepDifference => spec!(
            "B-rep 差（箱）",
            "軸平行な箱の差が単一の直方体になる場合のみ実行します。穴や分離はエラー。",
            "solid difference subtract brep 差",
            Solid,
            [B, g("other", "切り取る立体", D::Solid)]
        ),
        K::BrepOffset => spec!(
            "B-rep オフセット（箱）",
            "箱の6支持平面を外向きに移動。負は内側へ。消滅はエラー。",
            "offset brep box オフセット",
            Solid,
            [B, n("distance", "距離", 2., -1e6, 1e6, 0.5)]
        ),
        K::BrepMesh => spec!(
            "B-rep をメッシュ化",
            "閉じた箱の境界を三角形メッシュに変換して書き出せるようにします。",
            "brep mesh tessellate stl export",
            Mesh,
            [B]
        ),
        _ => unreachable!("non-surface node"),
    }
}
fn surface<'a>(a: &[&'a Value]) -> Result<&'a Surface, String> {
    match shape(a)? {
        Value::Surface(s) => Ok(s.geometry()),
        _ => Err("曲面を接続してください。".into()),
    }
}
fn solid<'a>(a: &[&'a Value], at: usize) -> Result<&'a ClosedSolid, String> {
    match a.get(at).copied() {
        Some(Value::Solid(s)) => Ok(s.as_ref()),
        _ => Err("B-rep の閉じた立体を接続してください。".into()),
    }
}
fn curve<'a>(a: &[&'a Value], at: usize) -> Result<&'a crate::model::Curve, String> {
    match a.get(at).copied() {
        Some(Value::Curve(c)) => Ok(c.as_ref()),
        _ => Err("曲線を接続してください。".into()),
    }
}
fn point(a: &[&Value], at: usize) -> Result<[f64; 3], String> {
    match a.get(at).copied() {
        Some(Value::Point(p)) => Ok(*p),
        _ => Err("点を接続してください。".into()),
    }
}
fn made(s: Surface, stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    Ok(vec![Value::Surface(Arc::new(SurfaceValue::new(s, stop)?))])
}
fn body(s: ClosedSolid) -> Vec<Value> {
    vec![Value::Solid(Arc::new(s))]
}
pub fn run(kind: K, a: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    match kind {
        K::PlaneSurface => made(Surface::plane(num(a, 0)?, num(a, 1)?)?, stop),
        K::NurbsPanel => made(
            Surface::nurbs(NurbsSurface::panel(num(a, 0)?, num(a, 1)?, num(a, 2)?)?),
            stop,
        ),
        K::BilinearSurface => made(
            Surface::nurbs(NurbsSurface::bilinear(
                point(a, 0)?,
                point(a, 1)?,
                point(a, 2)?,
                point(a, 3)?,
            )?),
            stop,
        ),
        K::CylinderSurface => made(Surface::frustum(num(a, 0)?, num(a, 0)?, num(a, 1)?)?, stop),
        K::FrustumSurface => made(Surface::frustum(num(a, 0)?, num(a, 1)?, num(a, 2)?)?, stop),
        K::SphereSurface => made(Surface::sphere(num(a, 0)?)?, stop),
        K::TorusSurface => made(Surface::torus(num(a, 0)?, num(a, 1)?)?, stop),
        K::EvaluateSurface => Ok(vec![Value::Point(
            surface(a)?.evaluate(num(a, 1)?, num(a, 2)?)?.point,
        )]),
        K::SurfaceDirection => {
            let s = surface(a)?;
            let u = num(a, 1)?;
            let v = num(a, 2)?;
            let p = match num(a, 3)? as usize {
                0 => s.normal(u, v)?,
                1 => s.evaluate(u, v)?.du,
                _ => s.evaluate(u, v)?.dv,
            };
            Ok(vec![Value::Point(p)])
        }
        K::SurfaceNormalLine => {
            let s = surface(a)?;
            let u = num(a, 1)?;
            let v = num(a, 2)?;
            let l = num(a, 3)?;
            let p = s.evaluate(u, v)?.point;
            let n = s.normal(u, v)?;
            let q = std::array::from_fn(|i| p[i] + l * n[i]);
            Ok(vec![Value::Curve(Arc::new(crate::model::Curve {
                points: vec![p, q],
                closed: false,
                geometry: None,
            }))])
        }
        K::SurfaceGaussianCurvature | K::SurfaceMeanCurvature | K::SurfacePrincipalCurvatures => {
            let result = surface(a)?.curvature(num(a, 1)?, num(a, 2)?, stop)?;
            let values = match kind {
                K::SurfaceGaussianCurvature => vec![result.gaussian],
                K::SurfaceMeanCurvature => vec![result.mean],
                _ => result.principal.to_vec(),
            };
            Ok(values.into_iter().map(Value::Number).collect())
        }
        K::SurfaceArea => Ok(vec![Value::Number(
            surface(a)?.area(num(a, 1)? as usize, stop)?,
        )]),
        K::SurfaceClosestPoint => Ok(vec![Value::Point(
            surface(a)?.closest_plane_point(point(a, 1)?)?,
        )]),
        K::SurfaceIsoCurve => Ok(vec![Value::Curve(Arc::new(surface(a)?.iso_curve(
            num(a, 1)? == 0.,
            num(a, 2)?,
            num(a, 3)? as usize,
            stop,
        )?))]),
        K::DivideSurface => {
            let s = surface(a)?;
            let (nu, nv) = (num(a, 1)? as usize, num(a, 2)? as usize);
            let mut out = Vec::with_capacity(nu * nv);
            for u in 0..nu {
                cancelled(stop)?;
                for v in 0..nv {
                    out.push(Value::Point(
                        s.evaluate(u as f64 / (nu - 1) as f64, v as f64 / (nv - 1) as f64)?
                            .point,
                    ));
                }
            }
            Ok(out)
        }
        K::SurfaceMesh => as_mesh(
            surface(a)?.tessellate(num(a, 1)? as usize, num(a, 2)? as usize, stop)?,
            stop,
        ),
        K::BrepBox => Ok(body(ClosedSolid::centered_box(
            num(a, 0)?,
            num(a, 1)?,
            num(a, 2)?,
        )?)),
        K::BrepVolume => Ok(vec![Value::Number(solid(a, 0)?.volume())]),
        K::BrepArea => Ok(vec![Value::Number(solid(a, 0)?.area())]),
        K::BrepIntersection => Ok(body(solid(a, 0)?.intersection(solid(a, 1)?)?)),
        K::BrepMove => Ok(body(solid(a, 0)?.translated([
            num(a, 1)?,
            num(a, 2)?,
            num(a, 3)?,
        ])?)),
        K::BrepFaces => {
            let mut out = Vec::new();
            for f in solid(a, 0)?.brep().shell().faces() {
                out.extend(made(Surface::from_plane(f.surface.support.clone()), stop)?);
            }
            Ok(out)
        }
        K::BrepEdges => {
            let b = solid(a, 0)?.brep();
            Ok(b.edges()
                .iter()
                .map(|e| {
                    Value::Curve(Arc::new(crate::model::Curve {
                        points: e
                            .vertices
                            .iter()
                            .map(|&i| b.vertices()[i].position)
                            .collect(),
                        closed: false,
                        geometry: None,
                    }))
                })
                .collect())
        }
        K::BrepVertices => Ok(solid(a, 0)?
            .brep()
            .vertices()
            .iter()
            .map(|v| Value::Point(v.position))
            .collect()),
        K::NurbsVase => made(
            Surface::nurbs(NurbsSurface::vase(
                num(a, 0)?,
                num(a, 1)?,
                num(a, 2)?,
                num(a, 3)?,
            )?),
            stop,
        ),
        K::RuledSurface => made(
            Surface::nurbs(NurbsSurface::ruled(
                curve(a, 0)?,
                curve(a, 1)?,
                num(a, 2)? != 0.,
            )?),
            stop,
        ),
        K::SurfaceExtrude => {
            let a_curve = curve(a, 0)?;
            let h = num(a, 1)?;
            let mut b = a_curve.clone();
            for p in &mut b.points {
                p[2] += h;
            }
            b.geometry = a_curve
                .geometry
                .as_ref()
                .map(|g| {
                    g.affine(|mut p| {
                        p[2] += h;
                        p
                    })
                })
                .transpose()?;
            made(
                Surface::nurbs(NurbsSurface::ruled(a_curve, &b, false)?),
                stop,
            )
        }
        K::BrepUnion => Ok(body(solid(a, 0)?.union_box(solid(a, 1)?)?)),
        K::BrepDifference => Ok(body(solid(a, 0)?.difference_box(solid(a, 1)?)?)),
        K::BrepOffset => Ok(body(solid(a, 0)?.offset_box(num(a, 1)?)?)),
        K::BrepMesh => Ok(vec![Value::Mesh(solid(a, 0)?.preview().clone())]),
        _ => Err("未対応の曲面ノードです。".into()),
    }
}
