//! Gコード nodes: a Grasshopper-style node set that builds G-code from a mesh.
//!
//! One value type, `Value::Gcode`, carries the whole job (printer, settings, slices,
//! toolpaths). Setting nodes change the job; 輪切り and the path nodes fill it; the
//! 書き出し node marks it for writing (the file itself is written from the inspector,
//! never during evaluation). Maths and the writer live in `gcode_job.rs`; the slicing
//! and infill routines are the ones the vase-mode export already uses (`gcode.rs`).
//!
//! Every node works on whole trees: each G-code job in the first input is processed
//! on its own and the tree layout is kept. STL stays the main export; this is a
//! sub-feature and has not been verified on a real printer.

use crate::engine::cancelled;
use crate::gcode;
use crate::gcode_job::{
    ColorBy, GcodeJob, MAX_COLOR_CHANGES, MAX_POINTS, Nonplanar, PathKind, PathSeg, Preset,
    Preview, Printer, Script, ScriptMode, Slice, Texture, dist3, gradient_layers, validate_script,
};
use crate::model::{
    DataType as D, InputSpec, Mesh, Node, NodeKind as K, NodeSpec, Tree, Value, g, i, n,
};
use crate::persistence;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const KINDS: &[K] = &[
    K::GcodePrinter,
    K::GcodeSlice,
    K::GcodeContour,
    K::GcodeSpiral,
    K::GcodeSort,
    K::GcodeSplit,
    K::GcodeExtrusion,
    K::GcodeSpeed,
    K::GcodeTemp,
    K::GcodeRetract,
    K::GcodeZHop,
    K::GcodeNonplanar,
    K::GcodeTexture,
    K::GcodeClay,
    K::GcodeStart,
    K::GcodeEnd,
    K::GcodeJoin,
    K::GcodeWrite,
    K::GcodePreview,
    K::GcodeEstimate,
    K::GcodeSafety,
    K::GcodeColorChange,
    K::GcodeColorGradient,
];

pub fn is_kind(kind: K) -> bool {
    KINDS.contains(&kind)
}

/// Nodes whose text lives in `Node::script` (開始・終了Gコード).
pub fn uses_script(kind: K) -> bool {
    matches!(kind, K::GcodeStart | K::GcodeEnd)
}

const CATEGORY: &str = "Gコード";
const MAX_LOOPS_PER_LAYER: usize = 400;
const MAX_BUCKET_ENTRIES: usize = 8_000_000;
const JOB: InputSpec = g("job", "Gコード", D::Gcode);

macro_rules! spec {
    ($label:literal, $description:literal, $keywords:literal, $out:ident, [$($input:expr),* $(,)?]) => {{
        static SPEC: NodeSpec = NodeSpec {
            label: $label,
            category: CATEGORY,
            description: $description,
            keywords: $keywords,
            inputs: &[$($input),*],
            output: D::$out,
        };
        &SPEC
    }};
}

pub fn spec(kind: K) -> &'static NodeSpec {
    match kind {
        K::GcodePrinter => spec!(
            "プリンタ設定",
            "機種を選んで G-code の土台を作ります。機種 0=Bambu A1 mini（原点は角）1=Bambu A1 mini（原点は中心 90,90）2=FlashForge Creator 5 3=TRONXY Moore 1（粘土）4=Marlin 汎用。ベッド寸法は仮の値なので、実機の仕様で確認してください。M600 は Marlin 汎用で実機の ADVANCED_PAUSE_FEATURE を確認した場合のみ許可してください。",
            "printer preset bambu a1 mini flashforge tronxy moore marlin gcode プリンタ 機種 ベッド",
            Gcode,
            [
                i("preset", "機種（0〜4）", 0., 0., 4.),
                n("bed_x", "ベッドX（0=機種の値）", 0., 0., 1000., 1.),
                n("bed_y", "ベッドY（0=機種の値）", 0., 0., 1000., 1.),
                n("bed_z", "ベッド高さ（0=機種の値）", 0., 0., 1000., 1.),
                n("filament_d", "フィラメント径（0=既定）", 0., 0., 100., 0.05),
                n("nozzle_d", "ノズル径（0=既定）", 0., 0., 20., 0.05),
                i(
                    "m600_confirmed",
                    "Marlin M600 実機対応確認（1=許可）",
                    0.,
                    0.,
                    1.
                )
            ]
        ),
        K::GcodeSlice => spec!(
            "輪切り",
            "閉じたメッシュを水平に輪切りにします。自動配置を 1 にすると、ベッドの中央・床に置きます。プリンタ・速度などの設定ノードは、このノードより前につなぎます。",
            "slice slicer layers 輪切り スライス 層 gcode",
            Gcode,
            [
                g("mesh", "形状", D::Mesh),
                JOB,
                n("layer_height", "層の高さ", 0.2, 0.02, 10., 0.01),
                i("place", "自動配置（0=しない 1=する）", 1., 0., 1.)
            ]
        ),
        K::GcodeContour => spec!(
            "等高線パス",
            "輪切りの輪郭から、壁（外壁・内壁）の経路を作ります。順序 0=外壁が先 1=内壁が先。底の層は塗りつぶします。壁を 2 本以上にする単純なオフセットは、尖った角や薄い部分で崩れることがあります。",
            "contour perimeter walls extrude path gcode 等高線 壁 パス 経路",
            Gcode,
            [
                JOB,
                i("walls", "壁の本数", 2., 1., 10.),
                i("order", "順序（0=外壁が先 1=内壁が先）", 0., 0., 1.),
                i("bottom_layers", "底の塗りつぶし層数", 3., 0., 50.),
                n("spacing", "サンプル間隔 (mm)", 1., 0.2, 10., 0.1),
                i("inset", "外寸を保つ（1=壁を半線幅内側に）", 1., 0., 1.)
            ]
        ),
        K::GcodeSpiral => spec!(
            "らせん連続Z",
            "花瓶モード。底を塗りつぶし、その上は 1 本のらせんで壁を連続して印刷します。1 周で 1 層分、周の長さに比例して Z を上げるので、ピッチが均一です。リトラクトはしません。",
            "spiral vase mode continuous z gcode らせん 花瓶 連続 螺旋",
            Gcode,
            [
                JOB,
                i("bottom_layers", "底の塗りつぶし層数", 3., 1., 50.),
                n("spacing", "サンプル間隔 (mm)", 1., 0.2, 10., 0.1),
                i("inset", "外寸を保つ（1=壁を半線幅内側に）", 1., 0., 1.)
            ]
        ),
        K::GcodeSort => spec!(
            "経路の並び替え",
            "層ごと・種類ごとに、いまのノズル位置から近い経路を順に選びます（最近傍）。閉じた経路は近い点から始め、開いた経路は必要なら向きを反転します。らせんは動かしません。",
            "sort order nearest travel optimize 並び替え 最近傍 移動 gcode",
            Gcode,
            [JOB]
        ),
        K::GcodeSplit => spec!(
            "経路の分割",
            "長い経路を最大長で分けます。角度を指定すると、その角度より急な角でも分けます（0 は角で分けない）。閉じた経路は開いた経路になります。",
            "split divide segment path 分割 経路 gcode",
            Gcode,
            [
                JOB,
                n("max_len", "最大長 (mm)", 50., 1., 1000., 1.),
                n(
                    "corner_deg",
                    "角で分ける角度（0=分けない）",
                    0.,
                    0.,
                    179.,
                    1.
                )
            ]
        ),
        K::GcodeExtrusion => spec!(
            "押出量計算",
            "押出量 E = 長さ × 線幅 × 層の高さ ÷ (π(フィラメント径/2)²) × 流量。E の書き方は 0=絶対（M82）1=相対（M83）。線幅・層の高さが 0 のときは今の値のままです。",
            "extrusion flow e width height volume 押出量 流量 線幅 gcode",
            Gcode,
            [
                JOB,
                n("flow", "流量 (%)", 100., 1., 400., 1.),
                i(
                    "relative",
                    "E の書き方（0=絶対 M82 1=相対 M83）",
                    0.,
                    0.,
                    1.
                ),
                n("line_width", "線幅（0=そのまま）", 0., 0., 20., 0.01),
                n("layer_height", "層の高さ（0=そのまま）", 0., 0., 10., 0.01)
            ]
        ),
        K::GcodeSpeed => spec!(
            "速度設定",
            "外壁・内側（内壁と充填）・移動・1層目の速さ（mm/s）を決めます。",
            "speed feed perimeter travel first layer 速度 速さ 移動 gcode",
            Gcode,
            [
                JOB,
                n("outer", "外壁 (mm/s)", 40., 1., 500., 1.),
                n("inner", "内側・充填 (mm/s)", 60., 1., 500., 1.),
                n("travel", "移動 (mm/s)", 150., 5., 1000., 1.),
                n("first", "1層目 (mm/s)", 20., 1., 500., 1.),
                i("first_layers", "1層目扱いの層数", 1., 0., 50.)
            ]
        ),
        K::GcodeTemp => spec!(
            "温度・ファン",
            "ノズル温度・ベッド温度・ファンを決めます。粘土用プリンタでは書き出されません。",
            "temperature nozzle bed fan cooling 温度 ファン ノズル ベッド gcode",
            Gcode,
            [
                JOB,
                n("nozzle", "ノズル (℃)", 220., 0., 400., 1.),
                n("bed", "ベッド (℃)", 60., 0., 200., 1.),
                n("fan", "ファン (%)", 100., 0., 100., 1.),
                i("fan_from_layer", "ファン開始の層", 2., 0., 3000.)
            ]
        ),
        K::GcodeRetract => spec!(
            "リトラクト",
            "離れた場所へ移動する前にフィラメントを引き戻します。短い移動では引き戻しません。",
            "retract retraction stringing リトラクト 引き戻し 糸引き gcode",
            Gcode,
            [
                JOB,
                n("length", "引き戻し量 (mm)", 0.8, 0., 30., 0.1),
                n("speed", "引き戻し速度 (mm/s)", 30., 1., 200., 1.),
                n(
                    "min_travel",
                    "この距離以上の移動で引き戻す (mm)",
                    2.,
                    0.,
                    500.,
                    0.5
                )
            ]
        ),
        K::GcodeZHop => spec!(
            "Zホップ",
            "離れた場所へ移動するとき、ノズルを持ち上げて印刷済みの部分をこすらないようにします。0 で無効。",
            "z hop lift zhop ホップ 持ち上げ gcode",
            Gcode,
            [
                JOB,
                n("height", "持ち上げる高さ (mm)", 0.4, 0., 50., 0.1),
                n(
                    "min_travel",
                    "この距離以上の移動で持ち上げる (mm)",
                    2.,
                    0.,
                    500.,
                    0.5
                )
            ]
        ),
        K::GcodeNonplanar => spec!(
            "ノンプラナー変形",
            "経路の Z を波で変形します。モード 0=X方向の波 1=Y方向の波 2=同心円 3=格子のうねり。開始層までは平らのままで、フェード層数で少しずつ強くなります。傾斜が 45° を超えるとノズルが当たる恐れがあり、安全チェックが知らせます。",
            "nonplanar non-planar wave z deform ノンプラナー 波 変形 gcode",
            Gcode,
            [
                JOB,
                i("mode", "モード（0〜3）", 0., 0., 3.),
                n("amplitude", "振幅 (mm)", 2., -20., 20., 0.1),
                n("wavelength", "波長 (mm)", 30., 1., 1000., 1.),
                n("phase", "位相 (度)", 0., -360., 360., 1.),
                i("start_layer", "開始する層", 3., 0., 3000.),
                i("fade_layers", "フェード層数", 5., 1., 3000.)
            ]
        ),
        K::GcodeTexture => spec!(
            "波・テクスチャ付加",
            "壁を横方向に波打たせます（外向きに出入り）。モード 0=波 1=三角 2=角ばった波。縦波長を入れると上下にも模様が変わり、層ごとのねじりでうろこ模様になります。閉じた壁は、つなぎ目が合うように波長を調整します。",
            "texture wave ripple surface clay pattern テクスチャ 波 模様 粘土 gcode",
            Gcode,
            [
                JOB,
                i("mode", "モード（0〜2）", 0., 0., 2.),
                n("amplitude", "振幅 (mm)", 0.6, 0., 20., 0.05),
                n("wavelength", "波長 (mm)", 12., 1., 200., 0.5),
                n("z_wavelength", "縦の波長 (mm・0=なし)", 0., 0., 500., 1.),
                n("twist", "層ごとのねじり (度)", 0., -360., 360., 1.)
            ]
        ),
        K::GcodeClay => spec!(
            "粘土用設定",
            "粘土プリンタ向けの値にします。加熱とファンを切り、太いノズル・線幅、遅い速度、相対 E にします。押出筒の直径は E の単位に影響するので、実機で校正してください（未検証）。",
            "clay ceramic paste pressure dwell 粘土 陶芸 押出 gcode",
            Gcode,
            [
                JOB,
                n("nozzle_d", "ノズル径 (mm)", 2., 0.5, 20., 0.1),
                n("layer_height", "層の高さ (mm)", 1., 0.1, 10., 0.05),
                n("width", "線幅 (mm)", 2.4, 0.5, 20., 0.1),
                n("cartridge_d", "押出筒の直径 (mm)", 30., 5., 100., 0.5),
                n("flow", "流量 (%)", 100., 1., 400., 1.),
                n("speed", "印刷速度 (mm/s)", 20., 1., 200., 1.),
                n("retract", "圧抜き（引き戻し）(mm)", 2., 0., 30., 0.1),
                n("pressure_dwell", "印刷後の待ち (ms)", 0., 0., 5000., 10.),
                n("preflow_dwell", "印刷前の待ち (ms)", 0., 0., 5000., 10.)
            ]
        ),
        K::GcodeStart => spec!(
            "開始Gコード",
            "開始の G-code を決めます。モード 0=機種ごとの標準 1=標準＋下の文章 2=下の文章だけ。どのモードでも、そのあとに M82 または M83 と G92 E0 を必ず出します。文章は右の欄（このノードを選ぶと出ます）に書きます。",
            "start gcode header preheat home 開始 ヘッダ gcode",
            Gcode,
            [
                JOB,
                i(
                    "mode",
                    "モード（0=標準 1=標準+文章 2=文章のみ）",
                    0.,
                    0.,
                    2.
                )
            ]
        ),
        K::GcodeEnd => spec!(
            "終了Gコード",
            "終了の G-code を決めます。モード 0=標準（引き戻し・持ち上げ・加熱停止）1=標準＋文章 2=文章だけ。",
            "end gcode footer cooldown 終了 フッタ gcode",
            Gcode,
            [
                JOB,
                i(
                    "mode",
                    "モード（0=標準 1=標準+文章 2=文章のみ）",
                    0.,
                    0.,
                    2.
                )
            ]
        ),
        K::GcodeJoin => spec!(
            "Gコード結合",
            "A の経路のあとに B の経路を続けます。設定と開始Gコードは A のもの、終了Gコードは B に指定があれば B のものです。",
            "join merge combine concat 結合 連結 gcode",
            Gcode,
            [g("a", "A（先）", D::Gcode), g("b", "B（後）", D::Gcode)]
        ),
        K::GcodeWrite => spec!(
            "Gコード書き出し",
            "このノードを選ぶと、右のパネルに「.gcode を書き出す」ボタンが出ます。安全チェックでエラーがあると書き出せません。既存のファイルは確認なしに上書きしません。Bambu の .gcode.3mf は未対応（TODO）です。",
            "write export save file gcode 書き出し 保存 出力 .gcode",
            Gcode,
            [JOB]
        ),
        K::GcodePreview => spec!(
            "パスプレビュー",
            "経路を色分けして 3D 表示します。色 0=速度 1=押出の進み 2=層 3=種類。移動を灰色の線で見せることもできます。",
            "preview path color speed extrusion view プレビュー 色 速度 経路 gcode",
            Gcode,
            [
                JOB,
                i(
                    "color_by",
                    "色（0=速度 1=押出の進み 2=層 3=種類）",
                    0.,
                    0.,
                    3.
                ),
                i("show_travel", "移動も表示（0=しない 1=する）", 0., 0., 1.)
            ]
        ),
        K::GcodeEstimate => spec!(
            "時間・材料見積り",
            "経路から数値を計算します。出す値 0=印刷時間（分）1=フィラメント長（m）2=重さ（g・PLA 1.24 g/cm³）3=層数 4=印刷距離（m）5=体積（cm³）6=色替え回数。加減速と加熱待ちは含まない目安です。",
            "estimate time material weight filament 見積り 時間 材料 重さ gcode",
            Number,
            [JOB, i("item", "出す値（0〜6）", 0., 0., 6.)]
        ),
        K::GcodeColorChange => spec!(
            "色替え（M600）",
            "指定した層の直前に、フィラメント交換の指令 M600 を入れます。層番号は 0 が 1 層目で、0 は無視されます。2 回以上入れるときは「くり返し」に間隔を入れます（0=1 回だけ）。交換中の待ち時間は見積りに入りません。プリンタ設定で Marlin の M600 対応を明示確認した場合のみ書き出せます。実機では未確認です。",
            "color change filament swap m600 multicolor 色替え フィラメント交換 多色 gcode",
            Gcode,
            [
                JOB,
                i("layer", "色替えする層（0 が 1 層目）", 10., 1., 100000.),
                i("every", "くり返しの間隔（0=1 回だけ）", 0., 0., 100000.),
                i("times", "回数（くり返すとき）", 1., 1., 256.)
            ]
        ),
        K::GcodeColorGradient => spec!(
            "グラデーション層",
            "はじめの層から終わりの層まで、色替え M600 を等間隔に入れます（最初と最後の層にも入る）。色の数だけ交換が必要なので、フィラメントを順に並べて使います。プリンタ設定で Marlin の M600 対応を明示確認してください。色の見た目は実機で確認してください。",
            "gradient color change layers filament m600 グラデーション 色替え 層 多色 gcode",
            Gcode,
            [
                JOB,
                i("from_layer", "はじめの層（0 が 1 層目）", 10., 1., 100000.),
                i("to_layer", "終わりの層", 100., 1., 100000.),
                i("count", "色替えの回数", 5., 1., 256.)
            ]
        ),
        K::GcodeSafety => spec!(
            "安全チェック",
            "ベッドの外、数値でない座標、過押出、Zホップなしで印刷済みの部分を横切る移動、急な傾斜を調べます。結果は「値を見る」に日本語で出ます。厳格 1 にすると、エラーがあるときこのノードが赤くなります。",
            "safety check validate warning bed over extrusion 安全 チェック 警告 gcode",
            Gcode,
            [JOB, i("strict", "厳格（1=エラーで停止）", 0., 0., 1.)]
        ),
        _ => spec!(
            "不明な Gコード ノード",
            "このノードは Gコード の一覧にありません。",
            "gcode",
            Gcode,
            []
        ),
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

type JobTree = Vec<(Vec<u32>, Vec<Arc<GcodeJob>>)>;

fn scalar(inputs: &[Tree], index: usize) -> Result<f64, String> {
    inputs
        .get(index)
        .and_then(|t| t.branches.values().flatten().next())
        .ok_or("数値の入力がありません。")?
        .number()
}

fn index_of(inputs: &[Tree], index: usize) -> Result<i64, String> {
    let v = scalar(inputs, index)?;
    if v.fract() != 0.0 || v.abs() > 1e9 {
        return Err("整数を指定してください。".into());
    }
    Ok(v as i64)
}

fn jobs_of(tree: Option<&Tree>) -> Result<JobTree, String> {
    let tree = tree.ok_or("Gコードをつないでください。")?;
    let mut out: JobTree = Vec::new();
    for (path, values) in &tree.branches {
        let mut jobs = Vec::new();
        for value in values {
            match value {
                Value::Gcode(job) => jobs.push(Arc::clone(job)),
                _ => return Err("Gコードの入力に別の種類のデータが混ざっています。".into()),
            }
        }
        out.push((path.clone(), jobs));
    }
    if out.iter().all(|(_, jobs)| jobs.is_empty()) {
        return Err("Gコードがありません。「プリンタ設定」から順につないでください。".into());
    }
    Ok(out)
}

fn rebuild(jobs: Vec<(Vec<u32>, Vec<GcodeJob>)>) -> Tree {
    let mut branches = BTreeMap::new();
    for (path, list) in jobs {
        branches.insert(
            path,
            list.into_iter()
                .map(|j| Value::Gcode(Arc::new(j)))
                .collect(),
        );
    }
    Tree { branches }
}

fn finish(job: GcodeJob) -> Result<GcodeJob, String> {
    job.profile.validate()?;
    job.printer.validate()?;
    job.check_limits()?;
    Ok(job)
}

pub fn run(node: &Node, inputs: &[Tree], cancel: &dyn Fn() -> bool) -> Result<Tree, String> {
    let kind = node.kind;
    match kind {
        K::GcodePrinter => return printer(inputs),
        K::GcodeJoin => return join(inputs),
        K::GcodeSlice => return slice(inputs, cancel),
        _ => {}
    }
    let source = jobs_of(inputs.first())?;
    if kind == K::GcodeEstimate {
        let item = index_of(inputs, 1)?;
        let mut branches = BTreeMap::new();
        for (path, jobs) in source {
            let mut values = Vec::new();
            for job in jobs {
                cancelled(cancel)?;
                let stats = job.stats(cancel)?;
                let v = match item {
                    0 => stats.seconds / 60.0,
                    1 => stats.filament_mm / 1000.0,
                    2 => stats.grams,
                    3 => f64::from(stats.layers),
                    4 => stats.print_mm / 1000.0,
                    5 => stats.volume_cm3,
                    6 => stats.color_changes as f64,
                    _ => return Err("出す値は 0〜6 で指定してください。".into()),
                };
                values.push(Value::Number(v));
            }
            branches.insert(path, values);
        }
        return Ok(Tree { branches });
    }
    let mut result = Vec::with_capacity(source.len());
    for (path, jobs) in source {
        let mut list = Vec::with_capacity(jobs.len());
        for job in jobs {
            cancelled(cancel)?;
            list.push(finish(apply(node, inputs, &job, cancel)?)?);
        }
        result.push((path, list));
    }
    Ok(rebuild(result))
}

fn apply(
    node: &Node,
    inputs: &[Tree],
    source: &GcodeJob,
    cancel: &dyn Fn() -> bool,
) -> Result<GcodeJob, String> {
    let mut job = source.clone();
    let s = |index: usize| scalar(inputs, index);
    match node.kind {
        K::GcodeContour => {
            let cfg = ContourConfig {
                walls: index_of(inputs, 1)?.clamp(1, 10) as usize,
                inner_first: index_of(inputs, 2)? == 1,
                bottom: index_of(inputs, 3)?.clamp(0, 50) as usize,
                spacing: s(4)?,
                inset: index_of(inputs, 5)? == 1,
            };
            job.paths = contour_paths(&job, cfg, cancel)?;
            job.notes.clear();
        }
        K::GcodeSpiral => {
            let bottom = index_of(inputs, 1)?.clamp(1, 50) as usize;
            job.paths = spiral_paths(&job, bottom, s(2)?, index_of(inputs, 3)? == 1, cancel)?;
            job.notes.clear();
        }
        K::GcodeSort => {
            require_paths(&job)?;
            job.sort_nearest(cancel)?;
        }
        K::GcodeSplit => {
            require_paths(&job)?;
            job.apply_split(s(1)?, s(2)?, cancel)?;
        }
        K::GcodeExtrusion => {
            job.profile.flow_pct = s(1)?;
            job.profile.relative_e = index_of(inputs, 2)? == 1;
            let width = s(3)?;
            let height = s(4)?;
            if width > 0.0 {
                job.profile.width = width;
            }
            if height > 0.0 {
                job.profile.layer_h = height;
            }
        }
        K::GcodeSpeed => {
            job.profile.speed_outer = s(1)?;
            job.profile.speed_inner = s(2)?;
            job.profile.speed_travel = s(3)?;
            job.profile.speed_first = s(4)?;
            job.profile.first_layers = index_of(inputs, 5)?.clamp(0, 50) as u32;
        }
        K::GcodeTemp => {
            job.profile.nozzle_c = s(1)?;
            job.profile.bed_c = s(2)?;
            job.profile.fan_pct = s(3)?;
            job.profile.fan_from_layer = index_of(inputs, 4)?.clamp(0, 3000) as u32;
        }
        K::GcodeRetract => {
            job.profile.retract_mm = s(1)?;
            job.profile.retract_speed = s(2)?;
            job.profile.retract_min_travel = s(3)?;
        }
        K::GcodeZHop => {
            job.profile.hop_mm = s(1)?;
            job.profile.hop_min_travel = s(2)?;
        }
        K::GcodeNonplanar => {
            require_paths(&job)?;
            let np = Nonplanar {
                mode: index_of(inputs, 1)?.clamp(0, 3) as u8,
                amplitude: s(2)?,
                wavelength: s(3)?,
                phase_deg: s(4)?,
                start_layer: index_of(inputs, 5)?.clamp(0, 3000) as u32,
                fade_layers: index_of(inputs, 6)?.clamp(1, 3000) as u32,
            };
            job.apply_nonplanar(np, cancel)?;
        }
        K::GcodeTexture => {
            require_paths(&job)?;
            let tx = Texture {
                mode: index_of(inputs, 1)?.clamp(0, 2) as u8,
                amplitude: s(2)?,
                wavelength: s(3)?,
                z_wavelength: s(4)?,
                twist_deg: s(5)?,
            };
            job.apply_texture(tx, cancel)?;
        }
        K::GcodeClay => {
            let p = &mut job.profile;
            job.printer.nozzle_d = s(1)?;
            p.layer_h = s(2)?;
            p.width = s(3)?;
            job.printer.filament_d = s(4)?;
            p.flow_pct = s(5)?;
            let speed = s(6)?;
            p.speed_outer = speed;
            p.speed_inner = speed;
            p.speed_first = speed.min(p.speed_first);
            p.retract_mm = s(7)?;
            p.pressure_dwell_ms = s(8)?.round().clamp(0.0, 60_000.0) as u32;
            p.preflow_dwell_ms = s(9)?.round().clamp(0.0, 60_000.0) as u32;
            p.relative_e = true;
            p.nozzle_c = 0.0;
            p.bed_c = 0.0;
            p.fan_pct = 0.0;
            p.max_flow_mm3_s = 0.0;
            job.printer.heated = false;
        }
        K::GcodeStart | K::GcodeEnd => {
            let mode = match index_of(inputs, 1)? {
                0 => ScriptMode::Auto,
                1 => ScriptMode::AutoPlus,
                2 => ScriptMode::Custom,
                _ => return Err("モードは 0〜2 で指定してください。".into()),
            };
            validate_script(&node.script)?;
            let script = Script {
                mode,
                text: node.script.clone(),
            };
            if node.kind == K::GcodeStart {
                job.start = script;
            } else {
                job.end = script;
            }
        }
        K::GcodeWrite => {
            require_paths(&job)?;
            validate_script(&job.start.text)?;
            validate_script(&job.end.text)?;
        }
        K::GcodePreview => {
            let color_by = ColorBy::from_index(index_of(inputs, 1)?)
                .ok_or("色は 0〜3 で指定してください。")?;
            job.preview = Some(Preview {
                color_by,
                show_travel: index_of(inputs, 2)? == 1,
            });
        }
        K::GcodeColorChange => {
            require_paths(&job)?;
            let layer = index_of(inputs, 1)?.clamp(0, 100_000) as u32;
            let every = index_of(inputs, 2)?.clamp(0, 100_000) as u32;
            let times = if every == 0 {
                1
            } else {
                index_of(inputs, 3)?.clamp(1, MAX_COLOR_CHANGES as i64) as u32
            };
            let layers: Vec<u32> = (0..times)
                .map(|k| layer.saturating_add(k.saturating_mul(every)))
                .collect();
            job.add_color_changes(&layers)?;
        }
        K::GcodeColorGradient => {
            require_paths(&job)?;
            let layers = gradient_layers(
                index_of(inputs, 1)?.clamp(0, 100_000) as u32,
                index_of(inputs, 2)?.clamp(0, 100_000) as u32,
                index_of(inputs, 3)?.clamp(0, 100_000) as u32,
            )?;
            job.add_color_changes(&layers)?;
        }
        K::GcodeSafety => {
            job.notes = job.check(cancel)?;
            if index_of(inputs, 1)? == 1 && job.notes.iter().any(|n| n.error) {
                let lines: Vec<&str> = job
                    .notes
                    .iter()
                    .filter(|n| n.error)
                    .map(|n| n.text.as_str())
                    .collect();
                return Err(format!("安全チェックのエラー:\n{}", lines.join("\n")));
            }
        }
        _ => return Err("このノードは Gコード の処理に対応していません。".into()),
    }
    Ok(job)
}

fn require_paths(job: &GcodeJob) -> Result<(), String> {
    if job.paths.is_empty() {
        Err("経路がありません。「輪切り」のあとに「等高線パス」または「らせん連続Z」をつないでください。".into())
    } else {
        Ok(())
    }
}

fn printer(inputs: &[Tree]) -> Result<Tree, String> {
    let preset =
        Preset::from_index(index_of(inputs, 0)?).ok_or("機種は 0〜4 で指定してください。")?;
    let mut printer = Printer::from_preset(preset);
    for (axis, index) in [(0usize, 1usize), (1, 2), (2, 3)] {
        let v = scalar(inputs, index)?;
        if v > 0.0
            && let Some(slot) = printer.bed.get_mut(axis)
        {
            *slot = v;
        }
    }
    let filament = scalar(inputs, 4)?;
    if filament > 0.0 {
        printer.filament_d = filament;
    }
    let nozzle = scalar(inputs, 5)?;
    if nozzle > 0.0 {
        printer.nozzle_d = nozzle;
    }
    // Older callers/projects without the new port must default to unconfirmed.
    if inputs.get(6).is_some() {
        printer.m600_confirmed = index_of(inputs, 6)? == 1;
    }
    let job = finish(GcodeJob::new(printer))?;
    Ok(Tree::single(Value::Gcode(Arc::new(job))))
}

fn join(inputs: &[Tree]) -> Result<Tree, String> {
    let a = jobs_of(inputs.first())?;
    let b = jobs_of(inputs.get(1))?;
    let first = a
        .iter()
        .flat_map(|(_, j)| j.iter())
        .next()
        .ok_or("A に Gコードがありません。")?;
    let mut job = (**first).clone();
    let mut layer_shift = job.layer_count();
    for extra in a.iter().flat_map(|(_, j)| j.iter()).skip(1) {
        append(&mut job, extra, &mut layer_shift);
    }
    let mut last_end: Option<Script> = None;
    for extra in b.iter().flat_map(|(_, j)| j.iter()) {
        append(&mut job, extra, &mut layer_shift);
        if extra.end.mode != ScriptMode::Auto {
            last_end = Some(extra.end.clone());
        }
    }
    if let Some(end) = last_end {
        job.end = end;
    }
    job.slices.clear();
    job.notes.clear();
    let job = finish(job)?;
    Ok(Tree::single(Value::Gcode(Arc::new(job))))
}

fn append(job: &mut GcodeJob, other: &GcodeJob, layer_shift: &mut u32) {
    for p in &other.paths {
        let mut p = p.clone();
        p.layer = p.layer.saturating_add(*layer_shift);
        job.paths.push(p);
    }
    let shifted: Vec<u32> = other
        .color_changes
        .iter()
        .map(|l| l.saturating_add(*layer_shift))
        .collect();
    // Past the limit the extra changes are dropped; the safety check still sees the rest.
    let _ = job.add_color_changes(&shifted);
    *layer_shift = layer_shift.saturating_add(other.layer_count());
}

// ---------------------------------------------------------------------------
// Slicing
// ---------------------------------------------------------------------------

fn slice(inputs: &[Tree], cancel: &dyn Fn() -> bool) -> Result<Tree, String> {
    let mesh_tree = inputs.first().ok_or("形状をつないでください。")?;
    let meshes: Vec<&Arc<Mesh>> = mesh_tree
        .branches
        .values()
        .flatten()
        .filter_map(|v| match v {
            Value::Mesh(m) => Some(m),
            _ => None,
        })
        .collect();
    let mesh = match meshes.as_slice() {
        [one] => one,
        [] => return Err("形状（メッシュ）をつないでください。".into()),
        _ => {
            return Err(
                "輪切りにできる形状は 1 つだけです。「結合」で 1 つにするか、別々に処理してください。"
                    .into(),
            );
        }
    };
    let source = jobs_of(inputs.get(1))?;
    let height = scalar(inputs, 2)?;
    let place = index_of(inputs, 3)? == 1;
    let mut result = Vec::with_capacity(source.len());
    for (path, jobs) in source {
        let mut list = Vec::with_capacity(jobs.len());
        for job in jobs {
            let mut job = (*job).clone();
            job.profile.layer_h = height;
            job.slices = slice_mesh(mesh_tree, mesh, &job, height, place, cancel)?;
            job.paths.clear();
            job.notes.clear();
            list.push(finish(job)?);
        }
        result.push((path, list));
    }
    Ok(rebuild(result))
}

fn slice_mesh(
    tree: &Tree,
    mesh: &Mesh,
    job: &GcodeJob,
    h: f64,
    place: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Slice>, String> {
    if !persistence::mesh_stats(std::slice::from_ref(tree))?.is_closed() {
        return Err(
            "閉じた形状ではないため、輪切りにできません。「肉厚」などで閉じた立体にしてください。"
                .into(),
        );
    }
    let (low, high) = gcode::bounds(&mesh.vertices)?;
    let offset = if place {
        let c = job.printer.bed_center_design();
        [
            c[0] - (low[0] + high[0]) / 2.0,
            c[1] - (low[1] + high[1]) / 2.0,
            -low[2],
        ]
    } else {
        if low[2] < -0.01 {
            return Err(
                "形状が床（Z=0）より下にあります。自動配置を 1 にするか、形状を上げてください。"
                    .into(),
            );
        }
        [0.0; 3]
    };
    let verts: Vec<[f64; 3]> = mesh
        .vertices
        .iter()
        .map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
        .collect();
    let top = high[2] + offset[2];
    let layers = gcode::planned_layers(top, h)?;

    // Put each triangle only into the layers it crosses, so the work stays bounded.
    let mut buckets: Vec<Vec<[u32; 3]>> = vec![Vec::new(); layers];
    let mut entries = 0usize;
    for (index, tri) in mesh.triangles.iter().enumerate() {
        if index % 4096 == 0 {
            cancelled(cancel)?;
        }
        let zs = tri.map(|v| verts.get(v as usize).map_or(f64::NAN, |p| p[2]));
        if zs.iter().any(|z| !z.is_finite()) {
            continue;
        }
        let z_min = zs.iter().copied().fold(f64::INFINITY, f64::min);
        let z_max = zs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let first = ((z_min / h) - 0.5).ceil().max(0.0) as usize;
        let last = (((z_max / h) - 0.5).floor().max(-1.0) + 1.0) as usize;
        for k in first..last.min(layers) {
            if let Some(bucket) = buckets.get_mut(k) {
                bucket.push(*tri);
                entries += 1;
            }
        }
        if entries > MAX_BUCKET_ENTRIES {
            return Err(
                "メッシュが大きすぎるか層が多すぎて、輪切りの計算量が上限を超えました。層の高さを大きくするか、メッシュを簡略化してください。"
                    .into(),
            );
        }
    }

    let mut slices = Vec::with_capacity(layers);
    let mut points = 0usize;
    for (k, bucket) in buckets.iter().enumerate() {
        cancelled(cancel)?;
        let plane = (k as f64 + 0.5) * h;
        let loops: Vec<Vec<[f64; 2]>> = gcode::slice_plane(&verts, bucket, plane)
            .into_iter()
            .filter(|l| gcode::signed_area(l).abs() >= 1.0)
            .collect();
        if loops.is_empty() {
            return Err(format!(
                "高さ {plane:.2} mm で断面が得られません。形状が閉じているか確認してください。"
            ));
        }
        points += loops.iter().map(Vec::len).sum::<usize>();
        if points > MAX_POINTS {
            return Err(
                "輪切りの点が多すぎます。層の高さを大きくするか、メッシュを簡略化してください。"
                    .into(),
            );
        }
        slices.push(Slice {
            z_top: (k as f64 + 1.0) * h,
            loops,
        });
    }
    Ok(slices)
}

// ---------------------------------------------------------------------------
// Paths from slices
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ContourConfig {
    walls: usize,
    inner_first: bool,
    bottom: usize,
    spacing: f64,
    inset: bool,
}

fn point_in(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for (a, b) in poly.iter().zip(poly.iter().cycle().skip(1)) {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// How many other loops contain each loop: even = outer boundary, odd = hole.
fn depths(loops: &[Vec<[f64; 2]>]) -> Vec<usize> {
    loops
        .iter()
        .enumerate()
        .map(|(a, lp)| {
            let Some(&p) = lp.first() else { return 0 };
            loops
                .iter()
                .enumerate()
                .filter(|(b, other)| *b != a && point_in(other, p))
                .count()
        })
        .collect()
}

/// Move a ring to its left by `d` (inside the material for counter-clockwise outer
/// loops and clockwise holes). Mitred corners, clamped.
fn offset_left(ring: &[[f64; 2]], d: f64) -> Option<Vec<[f64; 2]>> {
    if d == 0.0 {
        return Some(ring.to_vec());
    }
    let count = ring.len();
    if count < 3 {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let p = *ring.get(index)?;
        let a = *ring.get((index + count - 1) % count)?;
        let b = *ring.get((index + 1) % count)?;
        let e1 = [p[0] - a[0], p[1] - a[1]];
        let e2 = [b[0] - p[0], b[1] - p[1]];
        let l1 = e1[0].hypot(e1[1]);
        let l2 = e2[0].hypot(e2[1]);
        if l1 < 1e-9 || l2 < 1e-9 {
            return None;
        }
        let n1 = [-e1[1] / l1, e1[0] / l1];
        let n2 = [-e2[1] / l2, e2[0] / l2];
        let denom = (1.0 + n1[0] * n2[0] + n1[1] * n2[1]).max(0.3);
        out.push([
            p[0] + (n1[0] + n2[0]) * d / denom,
            p[1] + (n1[1] + n2[1]) * d / denom,
        ]);
    }
    out.iter()
        .all(|p| p[0].is_finite() && p[1].is_finite())
        .then_some(out)
}

fn lift(ring: &[[f64; 2]], z: f64) -> Vec<[f64; 3]> {
    ring.iter().map(|p| [p[0], p[1], z]).collect()
}

struct Wall {
    rank: u8,
    order_key: usize,
    loop_index: usize,
    kind: PathKind,
    ring: Vec<[f64; 2]>,
}

fn contour_paths(
    job: &GcodeJob,
    cfg: ContourConfig,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<PathSeg>, String> {
    if job.slices.is_empty() {
        return Err("輪切りがありません。「輪切り」のあとにつないでください。".into());
    }
    let width = job.profile.width;
    let half = if cfg.inset { 0.5 } else { 0.0 };
    let mut paths: Vec<PathSeg> = Vec::new();
    let mut points = 0usize;
    let mut seam: Option<[f64; 2]> = None;
    for (layer, slice) in job.slices.iter().enumerate() {
        cancelled(cancel)?;
        if slice.loops.len() > MAX_LOOPS_PER_LAYER {
            return Err(format!(
                "層 {} の輪郭が {MAX_LOOPS_PER_LAYER} 個を超えています。形状を簡略化してください。",
                layer + 1
            ));
        }
        let depth = depths(&slice.loops);
        let mut walls: Vec<Wall> = Vec::new();
        let mut innermost: Vec<Vec<[f64; 2]>> = Vec::new();
        for (loop_index, lp) in slice.loops.iter().enumerate() {
            let Some(mut base) = gcode::resample_ring(lp, cfg.spacing) else {
                continue;
            };
            let hole = depth.get(loop_index).copied().unwrap_or(0) % 2 == 1;
            if hole {
                base.reverse();
            }
            let area0 = gcode::signed_area(&base);
            let mut previous = area0.abs();
            let mut last_good: Option<Vec<[f64; 2]>> = None;
            for w in 0..cfg.walls {
                let d = (w as f64 + half) * width;
                let Some(mut ring) = offset_left(&base, d) else {
                    break;
                };
                let area = gcode::signed_area(&ring);
                let same_side = area * area0 > 0.0;
                let sensible = if hole {
                    area.abs() >= previous * 0.999
                } else {
                    area.abs() > 0.5 && area.abs() <= previous * 1.001
                };
                if !same_side || !sensible {
                    break;
                }
                previous = area.abs();
                if let Some(s) = seam {
                    gcode::rotate_to_nearest(&mut ring, s);
                }
                last_good = Some(ring.clone());
                let outer = w == 0;
                let (rank, order_key) = match (cfg.inner_first, outer) {
                    (false, true) => (0, 0),
                    (false, false) => (1, w),
                    (true, true) => (1, 0),
                    (true, false) => (0, cfg.walls - w),
                };
                walls.push(Wall {
                    rank,
                    order_key,
                    loop_index,
                    kind: if outer {
                        PathKind::Outer
                    } else {
                        PathKind::Inner
                    },
                    ring,
                });
            }
            if let Some(ring) = last_good {
                innermost.push(ring);
            }
        }
        walls.sort_by_key(|w| (w.rank, w.order_key, w.loop_index));
        let z = slice.z_top;
        if seam.is_none()
            && let Some(first) = walls.first().and_then(|w| w.ring.first())
        {
            seam = Some(*first);
        }
        for wall in walls {
            points += wall.ring.len();
            paths.push(PathSeg {
                pts: lift(&wall.ring, z),
                kind: wall.kind,
                layer: layer as u32,
                rank: wall.rank,
                closed: true,
            });
        }
        if layer < cfg.bottom {
            for (a, b) in gcode::infill(&innermost, width) {
                points += 2;
                paths.push(PathSeg {
                    pts: vec![[a[0], a[1], z], [b[0], b[1], z]],
                    kind: PathKind::Infill,
                    layer: layer as u32,
                    rank: 2,
                    closed: false,
                });
            }
        }
        if points > MAX_POINTS || paths.len() > crate::gcode_job::MAX_PATHS {
            return Err(
                "経路が多すぎます。層の高さやサンプル間隔を大きくするか、形状を小さくしてください。"
                    .into(),
            );
        }
    }
    if paths.is_empty() {
        return Err("経路ができませんでした。壁の本数を減らすか、形状を確認してください。".into());
    }
    Ok(paths)
}

fn spiral_paths(
    job: &GcodeJob,
    bottom: usize,
    spacing: f64,
    inset: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<PathSeg>, String> {
    if job.slices.is_empty() {
        return Err("輪切りがありません。「輪切り」のあとにつないでください。".into());
    }
    let width = job.profile.width;
    let h = job.profile.layer_h;
    let d = if inset { 0.5 * width } else { 0.0 };
    let flat = bottom.min(job.slices.len());
    if flat >= job.slices.len() {
        return Err("らせん部分がありません。底の層数を減らすか、形状を高くしてください。".into());
    }
    let mut paths: Vec<PathSeg> = Vec::new();
    let mut spiral: Vec<[f64; 3]> = Vec::new();
    let mut prev_start: Option<[f64; 2]> = None;
    let mut last_ring_start: Option<[f64; 3]> = None;
    for (layer, slice) in job.slices.iter().enumerate() {
        cancelled(cancel)?;
        let outer = slice
            .loops
            .iter()
            .max_by(|a, b| {
                gcode::signed_area(a)
                    .abs()
                    .total_cmp(&gcode::signed_area(b).abs())
            })
            .ok_or_else(|| format!("層 {} に輪郭がありません。", layer + 1))?;
        let base = gcode::resample_ring(outer, spacing)
            .ok_or("断面の輪郭が小さすぎるため、らせんにできません。")?;
        let area0 = gcode::signed_area(&base);
        let mut ring = offset_left(&base, d).ok_or("壁の位置を計算できません。")?;
        let area = gcode::signed_area(&ring);
        if !(area > 0.5 && area <= area0 * 1.001) {
            return Err(format!(
                "層 {} は壁が細すぎてらせんにできません。外寸を保つを 0 にするか、形状を太くしてください。",
                layer + 1
            ));
        }
        if let Some(target) = prev_start {
            gcode::rotate_to_nearest(&mut ring, target);
        }
        prev_start = ring.first().copied();
        let z_top = slice.z_top;
        if layer < flat {
            paths.push(PathSeg {
                pts: lift(&ring, z_top),
                kind: PathKind::Outer,
                layer: layer as u32,
                rank: 0,
                closed: true,
            });
            for (a, b) in gcode::infill(&slice.loops, width) {
                paths.push(PathSeg {
                    pts: vec![[a[0], a[1], z_top], [b[0], b[1], z_top]],
                    kind: PathKind::Infill,
                    layer: layer as u32,
                    rank: 2,
                    closed: false,
                });
            }
        } else {
            // Z rises with the distance travelled, so the pitch is equal everywhere.
            let count = ring.len();
            let perimeter: f64 = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
                .sum();
            if !(perimeter.is_finite() && perimeter > 0.0) {
                return Err("輪郭の長さが無効です。".into());
            }
            let z0 = z_top - h;
            let mut run = 0.0;
            for index in 0..count {
                let (Some(p), Some(q)) = (ring.get(index), ring.get((index + 1) % count)) else {
                    break;
                };
                spiral.push([p[0], p[1], z0 + h * run / perimeter]);
                run += (p[0] - q[0]).hypot(p[1] - q[1]);
            }
            last_ring_start = ring.first().map(|p| [p[0], p[1], z_top]);
            if spiral.len() > MAX_POINTS {
                return Err(
                    "らせんの点が多すぎます。層の高さやサンプル間隔を大きくしてください。".into(),
                );
            }
        }
    }
    if let Some(end) = last_ring_start {
        spiral.push(end);
    }
    paths.push(PathSeg {
        pts: spiral,
        kind: PathKind::Spiral,
        layer: flat as u32,
        rank: 0,
        closed: false,
    });
    Ok(paths)
}

/// Render a job and write it atomically. The old file is kept if anything fails.
/// The caller has already asked the user before replacing an existing file.
pub fn write_job(
    path: &std::path::Path,
    job: &GcodeJob,
) -> Result<crate::gcode_job::Stats, String> {
    use std::io::Write;
    let (text, stats) = job.render(&|| false)?;
    persistence::atomic_write(path, |file| file.write_all(text.as_bytes()))?;
    Ok(stats)
}

/// The first G-code job in a node's evaluated output.
pub fn first_job(tree: &Tree) -> Option<Arc<GcodeJob>> {
    tree.branches.values().flatten().find_map(|v| match v {
        Value::Gcode(job) => Some(Arc::clone(job)),
        _ => None,
    })
}

/// Total printed distance of all paths (for tests and messages).
pub fn print_length(job: &GcodeJob) -> f64 {
    job.paths.iter().map(PathSeg::length).sum()
}

/// Straight-line distance between the first and last point of a path.
pub fn span(path: &PathSeg) -> f64 {
    match (path.pts.first(), path.pts.last()) {
        (Some(a), Some(b)) => dist3(*a, *b),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Graph;

    fn never() -> bool {
        false
    }

    fn cube_mesh(size: f64) -> Mesh {
        let s = size;
        let vertices = vec![
            [0.0, 0.0, 0.0],
            [s, 0.0, 0.0],
            [s, s, 0.0],
            [0.0, s, 0.0],
            [0.0, 0.0, s],
            [s, 0.0, s],
            [s, s, s],
            [0.0, s, s],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh {
            vertices,
            triangles,
        }
    }

    fn eval(g: &Graph, id: u64) -> Result<Tree, String> {
        let e = crate::engine::Evaluator::default().evaluate(g, &|| false);
        e.outputs
            .get(&id)
            .cloned()
            .ok_or_else(|| "no output".to_string())?
    }

    #[test]
    fn every_kind_has_a_spec_with_japanese_label_and_category() {
        for &kind in KINDS {
            let s = spec(kind);
            assert_eq!(s.category, "Gコード", "{kind:?}");
            assert!(!s.label.is_empty() && !s.description.is_empty());
            assert!(
                s.keywords.contains("gcode") || s.keywords.contains("slice"),
                "{kind:?}"
            );
        }
        assert_eq!(KINDS.len(), 23);
    }

    #[test]
    fn offset_left_shrinks_a_counter_clockwise_square() {
        let ring = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let inner = offset_left(&ring, 1.0).unwrap_or_default();
        assert_eq!(inner.len(), 4);
        assert!((gcode::signed_area(&inner) - 64.0).abs() < 1e-9);
        assert!(offset_left(&ring[..2], 1.0).is_none());
    }

    #[test]
    fn holes_are_found_by_nesting_depth() {
        let outer = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
        let hole = vec![[5.0, 5.0], [15.0, 5.0], [15.0, 15.0], [5.0, 15.0]];
        assert_eq!(depths(&[outer, hole]), vec![0, 1]);
    }

    #[test]
    fn slice_contour_of_a_cube_gives_walls_and_a_filled_bottom() {
        let printer = Printer::from_preset(Preset::BambuA1MiniCorner);
        let job = GcodeJob::new(printer);
        let mesh = cube_mesh(20.0);
        let tree = Tree::single(Value::Mesh(Arc::new(mesh.clone())));
        let slices = slice_mesh(&tree, &mesh, &job, 1.0, true, &never).unwrap();
        assert_eq!(slices.len(), 20);
        // Placed on the bed centre.
        let first = slices[0].loops[0][0];
        assert!(first[0] >= 80.0 - 1e-6 && first[0] <= 100.0 + 1e-6);
        let mut j = job.clone();
        j.slices = slices;
        j.profile.layer_h = 1.0;
        let cfg = ContourConfig {
            walls: 2,
            inner_first: false,
            bottom: 2,
            spacing: 1.0,
            inset: true,
        };
        let paths = contour_paths(&j, cfg, &never).unwrap();
        let outer = paths.iter().filter(|p| p.kind == PathKind::Outer).count();
        let inner = paths.iter().filter(|p| p.kind == PathKind::Inner).count();
        let infill = paths.iter().filter(|p| p.kind == PathKind::Infill).count();
        assert_eq!(outer, 20);
        assert_eq!(inner, 20);
        assert!(infill > 10);
        assert!(
            paths
                .iter()
                .all(|p| p.pts.iter().all(|q| q.iter().all(|v| v.is_finite())))
        );
        // Order 0 puts the outer wall first inside a layer; order 1 the inner wall.
        assert_eq!(paths[0].kind, PathKind::Outer);
        let cfg = ContourConfig {
            inner_first: true,
            ..cfg
        };
        let swapped = contour_paths(&j, cfg, &never).unwrap();
        assert_eq!(swapped[0].kind, PathKind::Inner);
    }

    #[test]
    fn spiral_has_equal_pitch_and_rises_one_layer_per_turn() {
        let printer = Printer::from_preset(Preset::BambuA1MiniCorner);
        let mut job = GcodeJob::new(printer);
        let mesh = cube_mesh(20.0);
        let tree = Tree::single(Value::Mesh(Arc::new(mesh.clone())));
        job.slices = slice_mesh(&tree, &mesh, &job, 0.5, true, &never).unwrap();
        job.profile.layer_h = 0.5;
        let paths = spiral_paths(&job, 2, 1.0, true, &never).unwrap();
        let spiral = paths.iter().find(|p| p.kind == PathKind::Spiral).unwrap();
        let (mut min_rate, mut max_rate) = (f64::INFINITY, 0.0f64);
        for w in spiral.pts.windows(2) {
            let dxy = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            if dxy > 0.5 {
                let rate = (w[1][2] - w[0][2]) / dxy;
                min_rate = min_rate.min(rate);
                max_rate = max_rate.max(rate);
            }
        }
        // 0.5 mm per turn of 4 x 19 = 76 mm: about 0.0066 mm per mm, the same everywhere on straight runs.
        assert!((min_rate - max_rate).abs() < 0.002, "{min_rate} {max_rate}");
        let top = spiral.pts.last().unwrap()[2];
        assert!((top - 20.0).abs() < 1e-6, "{top}");
        assert!(spiral_paths(&job, 50, 1.0, true, &never).is_err());
    }

    #[test]
    fn open_mesh_is_refused_with_a_japanese_error() {
        let job = GcodeJob::new(Printer::from_preset(Preset::GenericMarlin));
        let mut mesh = cube_mesh(10.0);
        mesh.triangles.pop();
        let tree = Tree::single(Value::Mesh(Arc::new(mesh.clone())));
        let err = slice_mesh(&tree, &mesh, &job, 0.5, true, &never).unwrap_err();
        assert!(err.contains("閉じた"));
    }

    #[test]
    fn graph_vase_to_gcode_end_to_end() {
        let mut g = Graph::default();
        let cube = g.add(K::Box, [0., 0.]).unwrap();
        let printer = g.add(K::GcodePrinter, [0., 0.]).unwrap();
        let slice = g.add(K::GcodeSlice, [0., 0.]).unwrap();
        g.connect(cube, slice, "mesh").unwrap();
        g.connect(printer, slice, "job").unwrap();
        let spiral = g.add(K::GcodeSpiral, [0., 0.]).unwrap();
        g.connect(slice, spiral, "job").unwrap();
        let est = g.add(K::GcodeEstimate, [0., 0.]).unwrap();
        g.connect(spiral, est, "job").unwrap();
        let tree = eval(&g, spiral).unwrap();
        let Some(Value::Gcode(job)) = tree.branches.values().flatten().next() else {
            panic!("no job");
        };
        let (text, stats) = job.render(&never).unwrap();
        assert!(text.contains("M82") && text.contains("G92 E0"));
        assert!(stats.seconds > 0.0);
        let minutes = eval(&g, est).unwrap();
        assert!(
            minutes
                .branches
                .values()
                .flatten()
                .next()
                .unwrap()
                .number()
                .unwrap()
                > 0.0
        );
    }

    #[test]
    fn color_change_nodes_put_m600_in_the_written_file() {
        let mut g = Graph::default();
        let cube = g.add(K::Box, [0., 0.]).unwrap();
        let printer = g.add(K::GcodePrinter, [0., 0.]).unwrap();
        let slice = g.add(K::GcodeSlice, [0., 0.]).unwrap();
        g.connect(cube, slice, "mesh").unwrap();
        g.connect(printer, slice, "job").unwrap();
        let contour = g.add(K::GcodeContour, [0., 0.]).unwrap();
        g.connect(slice, contour, "job").unwrap();
        let gradient = g.add(K::GcodeColorGradient, [0., 0.]).unwrap();
        g.connect(contour, gradient, "job").unwrap();
        g.node_mut(gradient).unwrap().params.extend([
            ("from_layer".to_string(), 5.),
            ("to_layer".to_string(), 25.),
            ("count".to_string(), 3.),
        ]);
        let change = g.add(K::GcodeColorChange, [0., 0.]).unwrap();
        g.connect(gradient, change, "job").unwrap();
        g.node_mut(change)
            .unwrap()
            .params
            .extend([("layer".to_string(), 30.)]);
        let tree = eval(&g, change).unwrap();
        let Some(Value::Gcode(job)) = tree.branches.values().flatten().next() else {
            panic!("no job");
        };
        assert_eq!(job.color_changes, vec![5, 15, 25, 30]);
        assert!(job.render(&never).unwrap_err().contains("M600"));

        g.node_mut(printer).unwrap().params.extend([
            ("preset".to_string(), 4.),
            ("m600_confirmed".to_string(), 1.),
        ]);
        let tree = eval(&g, change).unwrap();
        let Some(Value::Gcode(job)) = tree.branches.values().flatten().next() else {
            panic!("no job");
        };
        let (text, stats) = job.render(&never).unwrap();
        assert_eq!(text.matches("M600").count(), 4);
        assert_eq!(stats.color_changes, 4);
    }

    #[test]
    fn missing_inputs_give_japanese_errors_not_panics() {
        let mut g = Graph::default();
        for &kind in KINDS {
            let id = g.add(kind, [0., 0.]).unwrap();
            let r = eval(&g, id);
            if let Err(e) = r {
                assert!(!e.is_empty());
            }
        }
    }
}
