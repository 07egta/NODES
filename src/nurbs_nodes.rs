//! Nodes for exact NURBS / B-rep / T-spline (SubD) modelling on the SolidCraft kernel
//! (through `kernel_bridge` and `subd`). Grasshopper/Rhino names are search keywords; labels
//! are Japanese. Inputs are few, with defaults that give a useful result straight away.
use crate::engine::{Evaluation, as_mesh, cancelled, node_inputs, num};
use crate::kernel_bridge::{self as kb, Bool, BrepValue};
use crate::kernel_ops as ko;
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
    K::SubdCageEdit,
    K::ExactTrimPlane,
    K::ExactFilletVariable,
    K::ExactOffsetSurface,
    K::ExactDraft,
    K::ExactMirror,
    K::ExactLinearArray,
    K::ExactPolarArray,
    K::ExactPipe,
    K::ExactPatch,
    K::ExactBlendCurve,
    K::ExactMatchCurve,
    K::ExactContinuity,
    K::ExactCurvatureComb,
    K::ExactOffsetCurve,
    K::ExactTorus,
    K::ExactSketch,
    K::ExactZebra,
    K::ExactDraftAnalysis,
    K::ExactDiagnose,
    K::SubdTsplinePatch,
    K::ExactThicken,
    K::ExactBlendSurface,
    K::ExactMatchSurface,
    K::ExactTrimByPlane,
    K::ExactSeamReport,
    K::ExactCurvatureMap,
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
        K::ExactTorus => spec!(
            "厳密トーラス",
            "Z 軸まわりのリング状 B-rep。大半径は管の中心まで、小半径は管の半径（mm）。大半径 > 小半径。",
            "torus ring donut brep solid トーラス ドーナツ 厳密",
            Brep,
            [
                n("major", "大半径", 30., 0.01, 1e5, 1.),
                n("minor", "小半径", 8., 0.01, 1e5, 0.5)
            ]
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
            "入力の B-rep を STEP (AP242) に変換し、バイト数を出力します。保存先は「STEP を保存」から選べます。拡張子を .igs / .iges にすると IGES で保存します。",
            "step stp iges igs export 書き出し cad",
            Number,
            [g("breps", "B-rep（リスト）", D::Brep)]
        ),
        K::ExactStepImport => spec!(
            "STEP 読み込み",
            "選んだ .step / .stp / .iges / .igs ファイルの立体を B-rep として読み込みます。",
            "step stp iges igs import 読み込み cad",
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
        K::SubdCageEdit => spec!(
            "ケージ編集 (SubD)",
            "ケージをビューポートで直接編集します。点・辺・面を選び、移動・押し出し・折り目・細分ができます（対称編集つき）。編集はノードに記録されます。",
            "cage edit subd tspline 編集 直接編集 gumball 移動 押し出し",
            Cage,
            [CAGE]
        ),
        K::ExactTrimPlane => spec!(
            "平面で切る",
            "B-rep 立体を高さ z の水平面で切り、下（または上）を残します。切り口は厳密な平面です。",
            "trim split plane cut 切る トリム 分割",
            Brep,
            [
                B,
                n("height", "高さ z", 10., -1e5, 1e5, 1.),
                i("above", "上を残す (1=はい)", 0., 0., 1.)
            ]
        ),
        K::ExactFilletVariable => spec!(
            "可変フィレット",
            "1 本の稜線を、始点と終点で違う半径で丸めます（直線の稜線）。",
            "variable fillet filletedge 可変 フィレット",
            Brep,
            [
                B,
                i("edge", "稜線番号", 0., 0., 1e5),
                n("r0", "始点の半径", 2., 0.01, 1e4, 0.5),
                n("r1", "終点の半径", 6., 0.01, 1e4, 0.5)
            ]
        ),
        K::ExactOffsetSurface => spec!(
            "オフセット面",
            "すべての面を法線方向に指定の距離だけずらした面を作ります（負で内側）。",
            "offset surface offsetsrf オフセット 面",
            Brep,
            [B, n("distance", "距離", 2., -1e4, 1e4, 0.5)]
        ),
        K::ExactDraft => spec!(
            "ドラフト角",
            "垂直な側面を、床 (z = 0) を基準に内側へ傾けます。型抜きや印刷の抜け勾配に。",
            "draft angle taper 抜き勾配 ドラフト",
            Brep,
            [B, n("angle", "角度（度）", 3., -60., 60., 0.5)]
        ),
        K::ExactMirror => spec!(
            "B-rep ミラー",
            "B-rep を X・Y・Z の平面で反転します。元を残すと結合します。",
            "mirror brep ミラー 反転 対称",
            Brep,
            [
                B,
                i("axis", "軸 (0=X,1=Y,2=Z)", 0., 0., 2.),
                i("keep", "元を残す (1=はい)", 1., 0., 1.)
            ]
        ),
        K::ExactLinearArray => spec!(
            "B-rep 直線配列",
            "B-rep を X・Y・Z の間隔で個数分並べます。",
            "array linear brep 配列 並べる",
            Brep,
            [
                B,
                i("count", "個数", 3., 1., 500.),
                n("x", "X 間隔", 30., -1e5, 1e5, 1.),
                n("y", "Y 間隔", 0., -1e5, 1e5, 1.),
                n("z", "Z 間隔", 0., -1e5, 1e5, 1.)
            ]
        ),
        K::ExactPolarArray => spec!(
            "B-rep 円形配列",
            "B-rep を Z 軸のまわりに個数分並べます。",
            "array polar radial brep 円形配列",
            Brep,
            [
                B,
                i("count", "個数", 6., 1., 500.),
                n("angle", "角度", 360., -360., 360., 1.)
            ]
        ),
        K::ExactPipe => spec!(
            "パイプ（厳密）",
            "レール曲線に沿って厳密な円の断面を動かし、閉じた管の B-rep にします。",
            "pipe tube brep パイプ 管",
            Brep,
            [
                g("rail", "レール", D::Curve),
                n("radius", "半径", 3., 0.01, 1e4, 0.5)
            ]
        ),
        K::ExactPatch => spec!(
            "パッチ（平面）",
            "閉じた平らな曲線の内側を面でふさぎます。",
            "patch planarsrf 平面 面 ふさぐ",
            Brep,
            [C]
        ),
        K::ExactBlendCurve => spec!(
            "ブレンド曲線",
            "曲線 A の終点から曲線 B の始点へ、なめらかにつなぐ曲線を作ります（G1 または G2）。",
            "blend curve blendcrv ブレンド つなぐ g1 g2",
            Curve,
            [
                g("a", "曲線 A", D::Curve),
                g("b", "曲線 B", D::Curve),
                i("g2", "曲率まで合わせる (1=G2)", 1., 0., 1.),
                n("bulge", "ふくらみ", 1., 0.01, 10., 0.05)
            ]
        ),
        K::ExactMatchCurve => spec!(
            "曲線のマッチ",
            "曲線 A の終わりを動かして、曲線 B の始点に位置 (G0)・接線 (G1)・曲率 (G2) を合わせます。",
            "match curve matchcrv マッチ 合わせる 連続",
            Curve,
            [
                g("a", "曲線 A", D::Curve),
                g("b", "曲線 B", D::Curve),
                i("level", "連続性 (0〜2)", 2., 0., 2.)
            ]
        ),
        K::ExactContinuity => spec!(
            "連続性チェック",
            "曲線 A の終点と B の始点のつながりを調べ、[レベル (-1=離れている,0=G0,1=G1,2=G2), すき間 mm, 角度°, 曲率差] を出します。",
            "continuity gcon g0 g1 g2 連続性 解析",
            Number,
            [g("a", "曲線 A", D::Curve), g("b", "曲線 B", D::Curve)]
        ),
        K::ExactCurvatureComb => spec!(
            "曲率くし",
            "曲線の曲率を「くし」の線で表示します。くしの先をつなぐ線も出力します。",
            "curvature graph comb 曲率 くし 解析",
            Curve,
            [
                C,
                i("count", "本数", 60., 2., 1000.),
                n("scale", "倍率", 200., 0.001, 1e6, 10.)
            ]
        ),
        K::ExactOffsetCurve => spec!(
            "オフセット曲線（厳密）",
            "平らな曲線を、その平面内で指定の距離だけずらしたなめらかな NURBS 曲線にします。",
            "offset curve offsetcrv オフセット",
            Curve,
            [C, n("distance", "距離", 5., -1e5, 1e5, 0.5)]
        ),
        K::ExactSketch => spec!(
            "拘束スケッチ",
            "文章で書いた線・円・弧に拘束（水平・一致・接する…）と寸法を付けて解き、厳密な曲線にします。例: 「長方形 r 0 0 60 40」「円 c 30 20 8」「半径 c 10」。出力 0 = 閉じた領域の輪郭、1 = 各曲線。",
            "sketch constraint solver dimension 拘束 スケッチ 寸法 2d",
            Curve,
            [i("mode", "出力 (0=輪郭,1=各曲線)", 0., 0., 1.)]
        ),
        K::ExactZebra => spec!(
            "ゼブラ（連続性の表示）",
            "B-rep に縞模様（等照線）を重ねて表示します。縞が面の境目で折れたり切れたりすると、そこはなめらかにつながっていません。",
            "zebra isophote continuity analysis ゼブラ 縞 解析",
            Mesh,
            [
                B,
                n("stripes", "縞の本数", 12., 1., 200., 1.),
                i("axis", "向き (0=X,1=Y,2=Z)", 0., 0., 2.)
            ]
        ),
        K::ExactDraftAnalysis => spec!(
            "抜き勾配の解析",
            "上向き (+Z) に対して、指定角度より立っている面（抜き勾配が足りない面）をメッシュで示します。",
            "draft angle analysis 抜き勾配 解析 ドラフト",
            Mesh,
            [B, n("angle", "必要な角度（度）", 2., 0., 89., 0.5)]
        ),
        K::ExactDiagnose => spec!(
            "B-rep 診断（トレランス）",
            "B-rep の状態を数値で出します: [閉じている, 面数, 稜線数, シェル数, メッシュの開いた辺, メッシュ水密, 交差した面ループ, メッシュ化できなかった面, 体積, 表示の弦誤差 mm, 同一点の許容差 mm]。",
            "check diagnose tolerance validity brep 診断 トレランス 確認",
            Number,
            [B]
        ),
        K::SubdTsplinePatch => spec!(
            "T-spline パッチケージ",
            "四角い T-spline 面の制御ケージ。「ケージ編集」の局所細分で、選んだ面だけ点を増やせます（T 字の接合。形は変わりません）。",
            "tspline t-spline patch local refine t-junction パッチ 局所細分",
            Cage,
            [
                n("width", "幅", 80., 0.01, 1e5, 1.),
                n("depth", "奥行", 60., 0.01, 1e5, 1.),
                i("nu", "U の点数", 6., 4., 64.),
                i("nv", "V の点数", 5., 4., 64.),
                n("bulge", "膨らみ", 12., -1e5, 1e5, 1.)
            ]
        ),
        K::ExactThicken => spec!(
            "厚みをつける",
            "開いた面（シート）に厚みをつけて閉じた立体にします。負の値で裏側へ。両側にすると半分ずつ。",
            "thicken offset solid offsetsrf solid 厚み 立体化",
            Brep,
            [
                B,
                n("thickness", "厚み", 2., -1e4, 1e4, 0.2),
                i("both", "両側 (1=はい)", 0., 0., 1.)
            ]
        ),
        K::ExactBlendSurface => spec!(
            "ブレンド曲面",
            "2 つの B-rep の縁（稜線番号で指定）を、なめらかな面でつなぎます。0=位置だけ（G0）、1=接線まで（G1）、2=曲率まで（G2）。稜線番号は 0 から数えた番号です。",
            "blend surface blendsrf ブレンド 曲面 つなぐ g1 g2",
            Brep,
            [
                g("a", "B-rep A", D::Brep),
                i("edge_a", "A の稜線番号", 0., 0., 100000.),
                g("b", "B-rep B", D::Brep),
                i("edge_b", "B の稜線番号", 0., 0., 100000.),
                i("grade_a", "A 側の連続性 (0〜2)", 1., 0., 2.),
                i("grade_b", "B 側の連続性 (0〜2)", 1., 0., 2.),
                n("crown", "ふくらみ", 0., -0.9, 3., 0.1)
            ]
        ),
        K::ExactMatchSurface => spec!(
            "曲面のマッチ",
            "開いた面（シート）の縁を動かして、相手の B-rep の稜線に位置・接線・曲率を合わせます。0=G0、1=G1、2=G2。",
            "match surface matchsrf マッチ 曲面 合わせる 連続",
            Brep,
            [
                g("sheet", "合わせる面（シート）", D::Brep),
                i("face", "面番号", 0., 0., 100000.),
                g("target", "相手の B-rep", D::Brep),
                i("edge", "相手の稜線番号", 0., 0., 100000.),
                i("grade", "連続性 (0〜2)", 1., 0., 2.)
            ]
        ),
        K::ExactTrimByPlane => spec!(
            "任意の平面で切る",
            "B-rep を、点と法線で決めた平面で切り、法線側（または反対側）を残します。切り口は厳密な平面です。",
            "trim split plane cut 平面 切る 分割 トリム 任意",
            Brep,
            [
                B,
                n("ox", "平面の点 X", 0., -1e5, 1e5, 1.),
                n("oy", "平面の点 Y", 0., -1e5, 1e5, 1.),
                n("oz", "平面の点 Z", 0., -1e5, 1e5, 1.),
                n("nx", "法線 X", 0., -1., 1., 0.1),
                n("ny", "法線 Y", 0., -1., 1., 0.1),
                n("nz", "法線 Z", 1., -1., 1., 0.1),
                i("keep", "残す側 (1=法線側, 0=反対側)", 1., 0., 1.)
            ]
        ),
        K::ExactSeamReport => spec!(
            "連続性の解析（面どうし）",
            "面と面の継ぎ目を、稜線の上で測ります。[継ぎ目の数, なめらか, 曲率に段差, 折れ, 隙間, 最大の隙間 mm, 最大の法線の角度 °, 最大の曲率の差 1/mm, 対象外, 測れなかった]。",
            "continuity seam analysis g0 g1 g2 連続性 解析 継ぎ目 曲率",
            Number,
            [
                B,
                i("samples", "稜線ごとの測定点", 17., 3., 257.),
                n("gap", "隙間の許容 mm", 0.001, 0., 1e3, 0.001),
                n("angle", "角度の許容 °", 0.5, 0., 180., 0.1),
                n("curvature", "曲率の許容 1/mm", 0.01, 0., 1e3, 0.001)
            ]
        ),
        K::ExactCurvatureMap => spec!(
            "曲率の表示（ガウス曲率）",
            "表示メッシュの角の欠け（角度欠損）からガウス曲率を求め、等しい値の帯をメッシュで出します。帯の境目が曲率の変わり目です（凸・凹・鞍型を見分けられます）。",
            "curvature gaussian map analysis 曲率 ガウス 解析 表示",
            Mesh,
            [B, n("gain", "感度", 4., 0.01, 1e3, 0.1)]
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
fn outs(list: Vec<BrepValue>) -> Result<Vec<Value>, String> {
    Ok(list.into_iter().map(|b| Value::Brep(Arc::new(b))).collect())
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
        K::ExactTorus => out(kb::torus(num(a, 0)?, num(a, 1)?, stop)?),
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
        K::ExactTrimPlane => out(ko::trim_by_plane(
            brep(a, 0)?,
            num(a, 1)?,
            num(a, 2)? != 0.,
            stop,
        )?),
        K::ExactFilletVariable => out(ko::fillet_variable(
            brep(a, 0)?,
            num(a, 1)? as usize,
            num(a, 2)?,
            num(a, 3)?,
            stop,
        )?),
        K::ExactOffsetSurface => out(ko::offset_surface(brep(a, 0)?, num(a, 1)?, stop)?),
        K::ExactDraft => out(ko::draft(brep(a, 0)?, num(a, 1)?, stop)?),
        K::ExactMirror => outs(ko::mirror(
            brep(a, 0)?,
            num(a, 1)? as usize,
            num(a, 2)? != 0.,
            stop,
        )?),
        K::ExactLinearArray => outs(ko::linear_array(
            brep(a, 0)?,
            [num(a, 2)?, num(a, 3)?, num(a, 4)?],
            num(a, 1)? as usize,
            stop,
        )?),
        K::ExactPolarArray => outs(ko::polar_array(
            brep(a, 0)?,
            num(a, 1)? as usize,
            num(a, 2)?,
            stop,
        )?),
        K::ExactPipe => out(ko::pipe(curve(a, 0)?, num(a, 1)?, stop)?),
        K::ExactPatch => out(ko::planar_patch(curve(a, 0)?, stop)?),
        K::ExactBlendCurve => Ok(vec![Value::Curve(Arc::new(ko::blend_curve(
            curve(a, 0)?,
            curve(a, 1)?,
            num(a, 2)? != 0.,
            num(a, 3)?,
            stop,
        )?))]),
        K::ExactMatchCurve => Ok(vec![Value::Curve(Arc::new(ko::match_curve(
            curve(a, 0)?,
            curve(a, 1)?,
            num(a, 2)? as usize,
            stop,
        )?))]),
        K::ExactContinuity => Ok(ko::continuity(curve(a, 0)?, curve(a, 1)?)?
            .into_iter()
            .map(Value::Number)
            .collect()),
        K::ExactCurvatureComb => {
            Ok(
                ko::curvature_comb(curve(a, 0)?, num(a, 1)? as usize, num(a, 2)?, stop)?
                    .into_iter()
                    .map(|c| Value::Curve(Arc::new(c)))
                    .collect(),
            )
        }
        K::ExactOffsetCurve => Ok(vec![Value::Curve(Arc::new(ko::offset_curve(
            curve(a, 0)?,
            num(a, 1)?,
            stop,
        )?))]),
        K::SubdCageEdit => out_cage(cage(a, 0)?.as_ref().clone()),
        K::SubdTsplinePatch => out_cage(Cage::tspline_patch(
            num(a, 0)?,
            num(a, 1)?,
            num(a, 2)? as usize,
            num(a, 3)? as usize,
            num(a, 4)?,
            stop,
        )?),
        K::ExactThicken => out(ko::thicken(
            brep(a, 0)?,
            num(a, 1)?,
            num(a, 2)? != 0.,
            stop,
        )?),
        K::ExactZebra => as_mesh(
            ko::zebra(brep(a, 0)?, num(a, 1)?, num(a, 2)? as usize, stop)?,
            stop,
        ),
        K::ExactDraftAnalysis => as_mesh(ko::draft_analysis(brep(a, 0)?, num(a, 1)?, stop)?, stop),
        K::ExactDiagnose => Ok(ko::diagnose(brep(a, 0)?, stop)?
            .into_iter()
            .map(Value::Number)
            .collect()),
        K::ExactBlendSurface => out(ko::blend_surface(
            brep(a, 0)?,
            num(a, 1)?,
            brep(a, 2)?,
            num(a, 3)?,
            (num(a, 4)?, num(a, 5)?),
            num(a, 6)?,
            stop,
        )?),
        K::ExactMatchSurface => out(ko::match_sheet(
            brep(a, 0)?,
            num(a, 1)?,
            brep(a, 2)?,
            num(a, 3)?,
            num(a, 4)?,
            stop,
        )?),
        K::ExactTrimByPlane => out(ko::trim_by_any_plane(
            brep(a, 0)?,
            [num(a, 1)?, num(a, 2)?, num(a, 3)?],
            [num(a, 4)?, num(a, 5)?, num(a, 6)?],
            num(a, 7)? != 0.,
            stop,
        )?),
        K::ExactSeamReport => Ok(ko::seam_report(
            brep(a, 0)?,
            num(a, 1)?,
            [num(a, 2)?, num(a, 3)?, num(a, 4)?],
            stop,
        )?
        .into_iter()
        .map(Value::Number)
        .collect()),
        K::ExactCurvatureMap => as_mesh(ko::curvature_map(brep(a, 0)?, num(a, 1)?, stop)?, stop),
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

pub(crate) fn iges_text(inputs: &[Tree], stop: &dyn Fn() -> bool) -> Result<String, String> {
    let bodies: Vec<&BrepValue> = inputs
        .iter()
        .flat_map(|t| t.branches.values())
        .flatten()
        .filter_map(|value| match value {
            Value::Brep(brep) => Some(brep.as_ref()),
            _ => None,
        })
        .collect();
    kb::iges_export(&bodies, stop)
}

fn check_step_extension(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["step", "stp", "iges", "igs"]
                .iter()
                .any(|e| ext.eq_ignore_ascii_case(e))
        })
    {
        Ok(())
    } else {
        Err("拡張子が .step / .stp / .iges / .igs のファイルを指定してください。".into())
    }
}

/// Whether the path names an IGES file (by extension).
fn is_iges(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("iges") || ext.eq_ignore_ascii_case("igs"))
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
    let text = if is_iges(path) {
        iges_text(&inputs, &stop)?
    } else {
        step_text(&inputs, &stop)?
    };
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
    let bodies = if is_iges(path) {
        kb::iges_import(&text, stop)?
    } else {
        kb::step_import(&text, stop)?
    };
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

/// ケージ編集: every cage of the input with the node's edit list (`script`, JSON) applied.
pub fn run_cage_edit(
    node: &Node,
    inputs: &[Tree],
    stop: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    let input = inputs.first().ok_or("ケージを接続してください。")?;
    let mut out = Tree::default();
    for (path, values) in &input.branches {
        let mut list = Vec::with_capacity(values.len());
        for v in values {
            cancelled(stop)?;
            match v {
                Value::Cage(c) => {
                    list.push(Value::Cage(Arc::new(c.apply_edits(&node.script, stop)?)))
                }
                _ => return Err("ケージを接続してください。".into()),
            }
        }
        out.branches.insert(path.clone(), list);
    }
    Ok(out)
}
