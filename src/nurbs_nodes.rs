//! Nodes for exact NURBS / B-rep / T-spline (SubD) modelling on the SolidCraft kernel
//! (through `kernel_bridge` and `subd`). Grasshopper/Rhino names are search keywords; labels
//! are Japanese. Inputs are few, with defaults that give a useful result straight away.
use crate::engine::{Evaluation, as_mesh, cancelled, node_inputs, num};
use crate::kernel_bridge::{self as kb, Bool, BrepValue};
use crate::model::{DataType as D, Node, NodeKind as K, NodeSpec, Tree, Value, g, i, n};
use crate::subd::Cage;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

pub const KINDS: &[K] = &[
    K::ExactBox,
    K::ExactCylinder,
    K::ExactSphere,
    K::ExactExtrude,
    K::ExactRevolve,
    K::ExactLoft,
    K::ExactLoftSurface,
    K::ExactSweep1,
    K::ExactSweep2,
    K::ExactNetworkSurface,
    K::ExactRebuildCurve,
    K::ExactCurveCurve,
    K::ExactCurveBrep,
    K::ExactBrepBrep,
    K::ExactUnion,
    K::ExactDifference,
    K::ExactIntersection,
    K::ExactFillet,
    K::ExactChamfer,
    K::ExactShell,
    K::ExactCapHoles,
    K::ExactBrepMesh,
    K::ExactMove,
    K::ExactVolume,
    K::ExactStepExport,
    K::ExactStepImport,
    K::SubdBoxCage,
    K::SubdCylinderCage,
    K::SubdSubdivide,
    K::SubdCrease,
    K::SubdExtrudeFace,
    K::SubdToBrep,
    K::SubdToMesh,
];

pub fn is_kind(kind: K) -> bool {
    KINDS.contains(&kind)
}
/// Kinds whose first input (and for the network surface, the first two) is a whole list.
pub fn is_list_kind(kind: K) -> bool {
    matches!(
        kind,
        K::ExactLoft | K::ExactLoftSurface | K::ExactNetworkSurface
    )
}
const B: crate::model::InputSpec = g("brep", "B-rep", D::Brep);
const B2: crate::model::InputSpec = g("other", "もう一方の B-rep", D::Brep);
const C: crate::model::InputSpec = g("curve", "曲線", D::Curve);
const CAGE: crate::model::InputSpec = g("cage", "ケージ", D::Cage);

macro_rules! spec {
    ($label:literal,$description:literal,$keywords:literal,$out:ident,[$($input:expr),* $(,)?]) => {{
        static SPEC:NodeSpec=NodeSpec{label:$label,category:"精密曲面",description:$description,
            keywords:$keywords,inputs:&[$($input),*],output:D::$out};
        &SPEC
    }};
}

pub fn spec(kind: K) -> &'static NodeSpec {
    match kind {
        K::ExactBox => spec!(
            "厳密直方体",
            "B-rep の直方体。フィレット・ブーリアン・STEP 書き出しの出発点になります。",
            "box brep solid cube 直方体 箱 厳密",
            Brep,
            [
                n("width", "幅", 40., 0.01, 1e5, 1.),
                n("depth", "奥行", 40., 0.01, 1e5, 1.),
                n("height", "高さ", 30., 0.01, 1e5, 1.)
            ]
        ),
        K::ExactCylinder => spec!(
            "厳密円柱",
            "Z 軸に立つ B-rep の円柱（側面は厳密な円）。",
            "cylinder brep solid 円柱 厳密",
            Brep,
            [
                n("radius", "半径", 15., 0.01, 1e5, 0.5),
                n("height", "高さ", 40., 0.01, 1e5, 1.)
            ]
        ),
        K::ExactSphere => spec!(
            "厳密球",
            "原点を中心とする B-rep の球。",
            "sphere brep solid 球 厳密",
            Brep,
            [n("radius", "半径", 20., 0.01, 1e5, 0.5)]
        ),
        K::ExactExtrude => spec!(
            "押し出して立体",
            "閉じた平らな曲線を押し出して、閉じた B-rep 立体にします。円は厳密な円柱面になります。",
            "extrude extrusion solid brep 押し出し 立体化 cap",
            Brep,
            [C, n("height", "高さ", 20., -1e5, 1e5, 1.)]
        ),
        K::ExactRevolve => spec!(
            "回転して立体",
            "平らな輪郭曲線を Z 軸のまわりに回して B-rep にします。開いた輪郭は直線で閉じます。輪郭は XZ 平面などの軸を含む平面に描きます。",
            "revolve revolution rail revolve lathe 回転体 旋盤",
            Brep,
            [C, n("angle", "角度", 360., 0.01, 360., 1.)]
        ),
        K::ExactLoft => spec!(
            "ロフトして立体",
            "閉じた断面曲線のリストを滑らかにつなぎ、両端をふさいだ B-rep 立体にします（断面が 2 つなら直線的につなぎます）。",
            "loft skin solid brep ロフト 器 vase",
            Brep,
            [
                g("curves", "断面曲線（リスト）", D::Curve),
                i("smooth", "滑らか (1=はい)", 1., 0., 1.)
            ]
        ),
        K::ExactLoftSurface => spec!(
            "ロフト面",
            "断面曲線のリストを通る滑らかな B-spline 面（ふたなし）。開いた曲線も使えます。",
            "loft surface sheet ロフト 面",
            Brep,
            [g("curves", "断面曲線（リスト）", D::Curve)]
        ),
        K::ExactSweep1 => spec!(
            "スイープ（1 レール）",
            "閉じた断面をレール曲線に沿って動かし、B-rep 立体にします。断面はレールの始点に置きます。",
            "sweep1 sweep rail スイープ パイプ",
            Brep,
            [
                g("profile", "断面曲線", D::Curve),
                g("rail", "レール", D::Curve)
            ]
        ),
        K::ExactSweep2 => spec!(
            "スイープ（2 レール）",
            "断面を 1 本目のレールに沿って動かし、2 本目のレールまでの距離に合わせて形を変えます。",
            "sweep2 two rail sweep スイープ 2レール",
            Brep,
            [
                g("profile", "断面曲線", D::Curve),
                g("rail", "レール 1", D::Curve),
                g("rail2", "レール 2", D::Curve)
            ]
        ),
        K::ExactNetworkSurface => spec!(
            "ネットワーク曲面",
            "交差する 2 方向の曲線リストから滑らかな面を作ります（Coons / Gordon）。",
            "network surface networksrf gordon coons ネットワーク",
            Brep,
            [
                g("u_curves", "U 方向の曲線（リスト）", D::Curve),
                g("v_curves", "V 方向の曲線（リスト）", D::Curve)
            ]
        ),
        K::ExactRebuildCurve => spec!(
            "曲線を再構築",
            "曲線を、指定した制御点数と次数のなめらかな NURBS に作り直します。",
            "rebuild curve rebuild 再構築 単純化 nurbs",
            Curve,
            [
                C,
                i("count", "制御点数", 12., 2., 128.),
                i("degree", "次数", 3., 1., 11.)
            ]
        ),
        K::ExactCurveCurve => spec!(
            "曲線どうしの交点",
            "2 本の曲線が交わる点（厳密な曲線上で 1e-6 mm まで詰めます）。",
            "curve curve intersection ccx 交点 交差",
            Point,
            [g("a", "曲線 A", D::Curve), g("b", "曲線 B", D::Curve)]
        ),
        K::ExactCurveBrep => spec!(
            "曲線と面の交点",
            "曲線が B-rep の面を通る点（表示メッシュの精度で計算する近似です）。",
            "curve brep surface intersection cbx csx 交点",
            Point,
            [C, B]
        ),
        K::ExactBrepBrep => spec!(
            "面どうしの交線",
            "2 つの B-rep の面が交わる線（表示メッシュから求める近似の折れ線）。",
            "brep brep surface surface intersection bbx ssx 交線",
            Curve,
            [B, B2]
        ),
        K::ExactUnion => spec!(
            "立体の和",
            "2 つの閉じた B-rep を 1 つにまとめます。",
            "solid union boolean brep 和 結合 ブーリアン",
            Brep,
            [B, B2]
        ),
        K::ExactDifference => spec!(
            "立体の差",
            "1 つ目の B-rep から 2 つ目を削り取ります。",
            "solid difference boolean brep 差 引く 穴",
            Brep,
            [B, B2]
        ),
        K::ExactIntersection => spec!(
            "立体の積",
            "2 つの B-rep が重なる部分だけを残します。",
            "solid intersection boolean brep 積 共通",
            Brep,
            [B, B2]
        ),
        K::ExactFillet => spec!(
            "フィレット",
            "稜線を丸めます。稜線番号 -1 ですべての稜線を丸めます。",
            "fillet filletedge round edge 角を丸める フィレット",
            Brep,
            [
                B,
                n("radius", "半径", 3., 0.01, 1e4, 0.5),
                i("edge", "稜線番号 (-1=すべて)", -1., -1., 1e5)
            ]
        ),
        K::ExactChamfer => spec!(
            "面取り",
            "稜線を平らに削ります。稜線番号 -1 ですべての稜線を面取りします。",
            "chamfer chamferedge bevel 面取り",
            Brep,
            [
                B,
                n("distance", "距離", 2., 0.01, 1e4, 0.5),
                i("edge", "稜線番号 (-1=すべて)", -1., -1., 1e5)
            ]
        ),
        K::ExactShell => spec!(
            "シェル（中空化）",
            "立体を指定の厚みの殻にします。開ける面: -1 = 一番上の面、-2 = 開けない、0 以上 = 面番号。平面で囲まれた凸形状に対応します。",
            "shell hollow thickness シェル 中空 器",
            Brep,
            [
                B,
                n("thickness", "厚み", 2., 0.01, 1e4, 0.2),
                i("open", "開ける面", -1., -2., 1e5)
            ]
        ),
        K::ExactCapHoles => spec!(
            "穴をふさぐ",
            "面の開いた縁をふさいで、閉じた立体にします。",
            "cap capholes close solid 穴をふさぐ キャップ",
            Brep,
            [B]
        ),
        K::ExactBrepMesh => spec!(
            "B-rep → メッシュ",
            "B-rep を水密な三角形メッシュにします（STL・メッシュ系ノード用）。",
            "brep mesh meshbrep tessellate stl メッシュ化",
            Mesh,
            [B]
        ),
        K::ExactMove => spec!(
            "B-rep を移動",
            "B-rep を X・Y・Z 方向に動かします（厳密形状のまま）。",
            "move brep translate 移動",
            Brep,
            [
                B,
                n("x", "X", 0., -1e5, 1e5, 1.),
                n("y", "Y", 0., -1e5, 1e5, 1.),
                n("z", "Z", 0., -1e5, 1e5, 1.)
            ]
        ),
        K::ExactVolume => spec!(
            "B-rep の体積",
            "閉じた B-rep の体積（mm³）。",
            "volume brep 体積 mass",
            Number,
            [B]
        ),
        K::ExactStepExport => spec!(
            "STEP 書き出し",
            "入力の B-rep を STEP (AP242) に変換し、バイト数を出力します。保存先は「STEP を保存」から選べます。",
            "step stp export 書き出し cad",
            Number,
            [g("breps", "B-rep（リスト）", D::Brep)]
        ),
        K::ExactStepImport => spec!(
            "STEP 読み込み",
            "選んだ .step または .stp ファイルの立体を B-rep として読み込みます。",
            "step stp import 読み込み cad",
            Brep,
            []
        ),
        K::SubdBoxCage => spec!(
            "箱ケージ (SubD)",
            "8 点の箱の制御ケージ。細分すると丸みのある形になります。",
            "subd box cage tspline t-spline 箱 ケージ",
            Cage,
            [
                n("width", "幅", 40., 0.01, 1e5, 1.),
                n("depth", "奥行", 40., 0.01, 1e5, 1.),
                n("height", "高さ", 40., 0.01, 1e5, 1.)
            ]
        ),
        K::SubdCylinderCage => spec!(
            "円柱ケージ (T-spline)",
            "円柱状の格子ケージ。厳密な NURBS / B-rep に変換できます。膨らみで器の形を作れます。",
            "subd cylinder cage tspline t-spline 円柱 ケージ 器 vase",
            Cage,
            [
                n("radius", "半径", 25., 0.01, 1e5, 0.5),
                n("height", "高さ", 80., 0.01, 1e5, 1.),
                i("segments", "周の点数", 8., 4., 64.),
                i("rings", "段数", 6., 4., 32.),
                n("bulge", "膨らみ", 0.3, -0.9, 3., 0.05)
            ]
        ),
        K::SubdSubdivide => spec!(
            "細分 (SubD)",
            "ケージを細かくします。円柱ケージは形を変えずに点を増やし（T-spline の細分）、箱ケージは Catmull-Clark で丸めます。",
            "subdivide refine catmull clark tspline 細分",
            Cage,
            [CAGE, i("times", "回数", 1., 1., 3.)]
        ),
        K::SubdCrease => spec!(
            "折り目 (SubD)",
            "ケージの辺に鋭い折り目を付けます。",
            "crease sharp edge subd 折り目 シャープ",
            Cage,
            [CAGE, i("edge", "辺番号", 0., 0., 1e5)]
        ),
        K::SubdExtrudeFace => spec!(
            "面の押し出し (SubD)",
            "ケージの面を法線方向に押し出して、形を伸ばします。",
            "extrude face subd cage 面 押し出し",
            Cage,
            [
                CAGE,
                i("face", "面番号", 1., 0., 1e5),
                n("distance", "距離", 15., -1e5, 1e5, 1.)
            ]
        ),
        K::SubdToBrep => spec!(
            "T-spline → B-rep",
            "円柱ケージの滑らかな面を厳密な NURBS 面の B-rep にし、上下をふさいで立体にします。",
            "tspline subd to brep nurbs convert 変換",
            Brep,
            [CAGE, i("cap", "ふさぐ (1=はい)", 1., 0., 1.)]
        ),
        K::SubdToMesh => spec!(
            "SubD → メッシュ",
            "ケージの滑らかな面を三角形メッシュにします。",
            "subd mesh tspline メッシュ",
            Mesh,
            [CAGE]
        ),
        _ => spec!("不明なノード", "不明なノードです。", "unknown", Any, []),
    }
}

fn brep<'a>(a: &[&'a Value], i: usize) -> Result<&'a Arc<BrepValue>, String> {
    match a.get(i) {
        Some(Value::Brep(b)) => Ok(b),
        _ => Err("B-rep を接続してください。".into()),
    }
}
fn curve<'a>(a: &[&'a Value], i: usize) -> Result<&'a crate::model::Curve, String> {
    match a.get(i) {
        Some(Value::Curve(c)) => Ok(c),
        _ => Err("曲線を接続してください。".into()),
    }
}
fn cage<'a>(a: &[&'a Value], i: usize) -> Result<&'a Arc<Cage>, String> {
    match a.get(i) {
        Some(Value::Cage(c)) => Ok(c),
        _ => Err("ケージを接続してください。".into()),
    }
}
fn index(v: f64) -> Option<usize> {
    (v >= 0.).then_some(v as usize)
}
fn out(b: BrepValue) -> Result<Vec<Value>, String> {
    Ok(vec![Value::Brep(Arc::new(b))])
}
fn out_cage(c: Cage) -> Result<Vec<Value>, String> {
    Ok(vec![Value::Cage(Arc::new(c))])
}

/// Item-wise evaluation.
pub fn run(kind: K, a: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    match kind {
        K::ExactBox => out(kb::box_solid(num(a, 0)?, num(a, 1)?, num(a, 2)?, stop)?),
        K::ExactCylinder => out(kb::cylinder(
            [0.; 3],
            [0., 0., 1.],
            num(a, 0)?,
            num(a, 1)?,
            stop,
        )?),
        K::ExactSphere => out(kb::sphere([0.; 3], num(a, 0)?, stop)?),
        K::ExactExtrude => out(kb::extrude(curve(a, 0)?, &[], num(a, 1)?, stop)?),
        K::ExactRevolve => out(kb::revolve(
            curve(a, 0)?,
            [0.; 3],
            [0., 0., 1.],
            num(a, 1)?,
            stop,
        )?),
        K::ExactSweep1 => out(kb::sweep1(curve(a, 0)?, curve(a, 1)?, stop)?),
        K::ExactSweep2 => out(kb::sweep2(curve(a, 0)?, curve(a, 1)?, curve(a, 2)?, stop)?),
        K::ExactRebuildCurve => {
            let c =
                kb::rebuild_curve(curve(a, 0)?, num(a, 1)? as usize, num(a, 2)? as usize, stop)?;
            Ok(vec![Value::Curve(Arc::new(c))])
        }
        K::ExactCurveCurve => Ok(kb::curve_curve(curve(a, 0)?, curve(a, 1)?, stop)?
            .into_iter()
            .map(Value::Point)
            .collect()),
        K::ExactCurveBrep => Ok(kb::curve_brep(curve(a, 0)?, brep(a, 1)?, stop)?
            .into_iter()
            .map(Value::Point)
            .collect()),
        K::ExactBrepBrep => Ok(kb::brep_brep(brep(a, 0)?, brep(a, 1)?, stop)?
            .into_iter()
            .map(|c| Value::Curve(Arc::new(c)))
            .collect()),
        K::ExactUnion => out(kb::boolean(brep(a, 0)?, brep(a, 1)?, Bool::Union, stop)?),
        K::ExactDifference => out(kb::boolean(
            brep(a, 0)?,
            brep(a, 1)?,
            Bool::Difference,
            stop,
        )?),
        K::ExactIntersection => out(kb::boolean(
            brep(a, 0)?,
            brep(a, 1)?,
            Bool::Intersection,
            stop,
        )?),
        K::ExactFillet => {
            let only: Vec<usize> = index(num(a, 2)?).into_iter().collect();
            out(kb::fillet(brep(a, 0)?, num(a, 1)?, &only, stop)?)
        }
        K::ExactChamfer => {
            let only: Vec<usize> = index(num(a, 2)?).into_iter().collect();
            out(kb::chamfer(brep(a, 0)?, num(a, 1)?, &only, stop)?)
        }
        K::ExactShell => {
            let open = match num(a, 2)? {
                v if v <= -1.5 => None,
                v if v < 0. => Some(usize::MAX),
                v => Some(v as usize),
            };
            out(kb::shell(brep(a, 0)?, num(a, 1)?, open, stop)?)
        }
        K::ExactCapHoles => out(kb::cap_holes(brep(a, 0)?, stop)?),
        K::ExactBrepMesh => as_mesh(brep(a, 0)?.preview().as_ref().clone(), stop),
        K::ExactMove => out(kb::translate(
            brep(a, 0)?,
            [num(a, 1)?, num(a, 2)?, num(a, 3)?],
            stop,
        )?),
        K::ExactVolume => {
            let v = brep(a, 0)?
                .volume()
                .ok_or("開いた B-rep には体積がありません。「穴をふさぐ」で閉じてください。")?;
            Ok(vec![Value::Number(v)])
        }
        K::SubdBoxCage => out_cage(Cage::box_cage(num(a, 0)?, num(a, 1)?, num(a, 2)?, stop)?),
        K::SubdCylinderCage => out_cage(Cage::cylinder_cage(
            num(a, 0)?,
            num(a, 1)?,
            num(a, 2)? as usize,
            num(a, 3)? as usize,
            num(a, 4)?,
            stop,
        )?),
        K::SubdSubdivide => {
            let mut c = cage(a, 0)?.as_ref().clone();
            for _ in 0..(num(a, 1)? as usize).clamp(1, 3) {
                c = c.subdivide(stop)?;
            }
            out_cage(c)
        }
        K::SubdCrease => out_cage(cage(a, 0)?.crease(&[num(a, 1)? as usize], stop)?),
        K::SubdExtrudeFace => {
            out_cage(cage(a, 0)?.extrude_face(num(a, 1)? as usize, num(a, 2)?, stop)?)
        }
        K::SubdToBrep => out(cage(a, 0)?.to_brep(num(a, 1)? != 0., stop)?),
        K::SubdToMesh => as_mesh(cage(a, 0)?.preview().as_ref().clone(), stop),
        _ => Err("このノードはリスト評価が必要です。".into()),
    }
}

fn curves(list: &[Value]) -> Result<Vec<&crate::model::Curve>, String> {
    list.iter()
        .map(|v| match v {
            Value::Curve(c) => Ok(c.as_ref()),
            _ => Err("曲線のリストを接続してください。".to_string()),
        })
        .collect()
}

/// List evaluation (one branch).
pub fn run_list(
    kind: K,
    lists: &[&[Value]],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let first = lists.first().copied().unwrap_or(&[]);
    match kind {
        K::ExactLoft => {
            let smooth = lists
                .get(1)
                .and_then(|l| l.first())
                .map(Value::number)
                .transpose()?
                .unwrap_or(1.)
                != 0.;
            out(kb::loft(&curves(first)?, smooth, stop)?)
        }
        K::ExactLoftSurface => out(kb::loft_sheet(&curves(first)?, stop)?),
        K::ExactNetworkSurface => {
            let second = lists.get(1).copied().unwrap_or(&[]);
            out(kb::network_surface(
                &curves(first)?,
                &curves(second)?,
                stop,
            )?)
        }
        _ => Err("このノードはリスト評価に対応していません。".into()),
    }
}

pub(crate) fn step_text(inputs: &[Tree], stop: &dyn Fn() -> bool) -> Result<String, String> {
    let bodies: Vec<&BrepValue> = inputs
        .iter()
        .flat_map(|t| t.branches.values())
        .flatten()
        .filter_map(|value| match value {
            Value::Brep(brep) => Some(brep.as_ref()),
            _ => None,
        })
        .collect();
    kb::step_export(&bodies, stop)
}

fn check_step_extension(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("step") || ext.eq_ignore_ascii_case("stp"))
    {
        Ok(())
    } else {
        Err("拡張子が .step / .stp のファイルを指定してください。".into())
    }
}

/// Saves a STEP export node using its evaluated upstream data. The caller confirms
/// any overwrite before invoking this entry point.
pub fn write_step(path: &Path, node: &Node, evaluation: &Evaluation) -> Result<usize, String> {
    if node.kind != K::ExactStepExport || node.off {
        return Err("STEP 書き出しノードを有効にしてください。".into());
    }
    check_step_extension(path)?;
    let stop = || false;
    let inputs = node_inputs(node, &evaluation.outputs, &stop)?;
    let text = step_text(&inputs, &stop)?;
    crate::persistence::atomic_write(path, |file| file.write_all(text.as_bytes()))?;
    Ok(text.len())
}

pub(crate) fn read_step(path: &str, stop: &dyn Fn() -> bool) -> Result<Tree, String> {
    cancelled(stop)?;
    let path = path.trim();
    if path.is_empty() {
        return Err("「ファイルを選ぶ」から .step / .stp ファイルを指定してください。".into());
    }
    let path = Path::new(path);
    check_step_extension(path)?;
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("STEP ファイルを開けません（{e}）。パスを確認してください。"))?;
    if !meta.is_file() {
        return Err("フォルダーではなくファイルを選んでください。".into());
    }
    let max_bytes = kb::limits::MAX_STEP_BYTES as u64;
    if meta.len() > max_bytes {
        return Err("STEP ファイルが大きすぎます。".into());
    }
    let mut text = String::new();
    File::open(path)
        .and_then(|file| file.take(max_bytes + 1).read_to_string(&mut text))
        .map_err(|e| format!("STEP ファイルを読めません（{e}）。"))?;
    let bodies = kb::step_import(&text, stop)?;
    Ok(Tree {
        branches: BTreeMap::from([(
            vec![0],
            bodies
                .into_iter()
                .map(|body| Value::Brep(Arc::new(body)))
                .collect(),
        )]),
    })
}
