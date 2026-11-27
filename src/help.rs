//! Offline help: a node dictionary built from each node's spec plus short usage tips.
//!
//! Tips live in a lookup table (not an exhaustive `match`) so node kinds added later
//! still get a page; they fall back to [`FALLBACK_TIP`] until someone writes one.
use crate::theme;
use eframe::egui::{self, Align, Layout, RichText};
use morphlet::model::{DataType, NodeKind};

pub const FALLBACK_TIP: &str = "このノードの使い方のヒントはまだありません。上の説明と入力の一覧を参考に、作品例のグラフにつないで値を少しずつ変えてみてください。";

const TIPS: &[(NodeKind, &str)] = &[
    (
        NodeKind::Number,
        "同じ値を複数の入力につなぐと、まとめて調整できます。花器の高さと押し出しの高さを 1 つの数値で揃える、などに使います。",
    ),
    (
        NodeKind::Add,
        "A に数列（リスト）をつなぐと、全部の値に B を足したリストになります。位置を少しずつずらすときに便利です。",
    ),
    (
        NodeKind::Multiply,
        "倍率をかけるときに使います。数列 × 2 で間隔を 2 倍に、角度の数列 × 個数などで回転量を作れます。",
    ),
    (
        NodeKind::Divide,
        "B を 0 にするとエラーになります。全体の角度 ÷ 個数 のように「1 つ分」を求めるときに使います。",
    ),
    (
        NodeKind::Subtract,
        "A − B。順番に注意してください。リストどうしは同じ番号どうしで引き算します。",
    ),
    (
        NodeKind::Power,
        "指数 2 で二乗、0.5 で平方根。山の形や加速する変化を作るときに使います。",
    ),
    (
        NodeKind::Abs,
        "マイナスを取り除きます。三角関数の結果につなぐと、波が全部上向きの「はねる」形になります。",
    ),
    (
        NodeKind::Min,
        "上限を決めたいときは B に上限値を入れて「最小」を使います（大きすぎる値を抑える）。",
    ),
    (
        NodeKind::Max,
        "下限を決めたいときは B に下限値を入れて「最大」を使います（小さすぎる値を持ち上げる）。",
    ),
    (
        NodeKind::Modulo,
        "数列を剰余でくり返しパターンにできます。例: 0,1,2,3,4,5 を 3 で割った余り → 0,1,2,0,1,2。",
    ),
    (
        NodeKind::Sine,
        "入力に「範囲」(0〜1) をつなぐと 1 周期分の波のリストになります。高さや半径にずらして使うと波打つ形が作れます。",
    ),
    (
        NodeKind::Series,
        "「開始・間隔・個数」で等間隔のリストを作ります。移動の X につなぐと形が横に並びます。",
    ),
    (
        NodeKind::Range,
        "「開始〜終了」を等分します。0〜1 を作って三角関数や倍率に使うのが定番です。",
    ),
    (
        NodeKind::Point,
        "X・Y・Z に数列をつなぐと点のリストになります。",
    ),
    (
        NodeKind::Circle,
        "押し出し・ロフト・回転配列の断面として使います。分割数を上げるほど滑らかですが重くなります。",
    ),
    (
        NodeKind::Polygon,
        "辺の数 6 で六角形。押し出すと角柱、ロフトの断面にも使えます。",
    ),
    (
        NodeKind::Star,
        "内半径を外半径に近づけると丸みのある花形、遠ざけるとトゲの強い星になります。",
    ),
    (
        NodeKind::Rectangle,
        "押し出すと板や柱になります。中心が原点なので、移動で位置を決めます。",
    ),
    (
        NodeKind::Helix,
        "らせんは開いた曲線です。点列としてプレビューしたり、変形で動かしたりして使います。",
    ),
    (
        NodeKind::Extrude,
        "断面には閉じた曲線（円・多角形・星形・長方形）をつなぎます。縦の分割を増やすと、後でねじり・波打つがきれいにかかります。",
    ),
    (
        NodeKind::Loft,
        "1 つの枝に 2 本以上、同じ点数の曲線が必要です。移動に数列をつないで高さの違う円を作り、そのままロフトにつなぐのが簡単です。",
    ),
    (
        NodeKind::Revolve,
        "断面は XZ 平面（X > 0 側）に置きます。点や曲線を移動・回転して作った輪郭を回すと器の形になります。",
    ),
    (
        NodeKind::Vase,
        "これだけで 3D プリントできる花器になります。ねじり・波打つを後ろにつなぐと表情が変わります。",
    ),
    (
        NodeKind::Sphere,
        "分割を減らすとカクカクしたローポリ風になります。",
    ),
    (
        NodeKind::Box,
        "底面が Z=0 なので、積み重ねるときは移動の Z に高さを足します。",
    ),
    (
        NodeKind::Cylinder,
        "周の分割を 6 にすると六角柱になります。",
    ),
    (
        NodeKind::Cone,
        "周の分割 3 で三角錐、4 で四角錐。屋根や尖った飾りに使います。",
    ),
    (
        NodeKind::Torus,
        "副半径は主半径より小さくします。円形配列と組み合わせると鎖のような形も作れます。",
    ),
    (
        NodeKind::Plane,
        "開いた面なので、そのままでは 3D プリント用には閉じていません。波打つで地形のような面を作る練習に向いています。",
    ),
    (
        NodeKind::Move,
        "X・Y・Z に数列をつなぐと、1 つの形が数列の数だけ並びます（配列の代わりにもなります）。",
    ),
    (
        NodeKind::Rotate,
        "回転の中心は原点です。先に回転してから移動すると、その場で向きを変えられます。",
    ),
    (
        NodeKind::Scale,
        "中心は原点です。倍率に数列をつなぐと大きさ違いの形がまとめて作れます。",
    ),
    (
        NodeKind::ScaleXyz,
        "Z 倍率だけ変えると高さだけ伸び縮みします。球を楕円体にするときにも使います。",
    ),
    (
        NodeKind::Mirror,
        "「元も残す」を 1 にすると左右対称の形が 1 つにまとまります。",
    ),
    (
        NodeKind::Twist,
        "形に縦の分割が少ないと、ねじりが角ばって見えます。前のノードの分割数を増やしてください。",
    ),
    (
        NodeKind::Wave,
        "細かく分割した形（平面や分割の多い押し出し）に使います。波長を短くすると細かい波になります。",
    ),
    (
        NodeKind::LinearArray,
        "個数には元の形も含まれます。間隔の X・Y・Z を組み合わせると斜めにも並べられます。",
    ),
    (
        NodeKind::RadialArray,
        "原点の周りに並ぶので、先に「移動」で中心から離しておくと輪のように並びます。",
    ),
    (
        NodeKind::Merge,
        "同じ枝のメッシュを 1 つにまとめます。重なった部分は削られません（ブーリアンではありません）。",
    ),
    (
        NodeKind::Flatten,
        "ツリー（複数の枝）を 1 本のリストにまとめます。ロフトやまとめる前に枝を揃えたいときに使います。",
    ),
    (
        NodeKind::Graft,
        "各要素を別々の枝に分けます。リストどうしを「全部の組み合わせ」ではなく枝ごとに処理したいときに使います。",
    ),
    (
        NodeKind::Output,
        "完成形として表示・書き出しする形をつなぎます。複数置くと全部が書き出されます。",
    ),
    (
        NodeKind::Script,
        "Rhai の短いスクリプトで数値・点・メッシュを作れます。実行時間とメモリには上限があり、ファイルやネットにはアクセスできません。",
    ),
    (
        NodeKind::Cluster,
        "複数のノードを 1 つの部品にしたものです。右下の設定から中身の確認や .morphpart への書き出しができます。",
    ),
    (
        NodeKind::ArcCurve,
        "開始角度と終了角度は度で指定します。分割数を増やすほど円弧がなめらかになります。円にしたいときは、終了角度を 360 度ずらしてください。",
    ),
    (
        NodeKind::Polyline,
        "点のリストを順につなぎます。「閉じる」を 1 にすると最後の点と最初の点もつながり、閉じた輪郭になります（3 点以上が必要）。",
    ),
    (
        NodeKind::SmoothCurve,
        "点を通るなめらかな曲線です。サンプル数を増やすほど細かくなります。角のある輪郭には「点をつなぐ線」を使うと、角をそのまま残せます。",
    ),
    (
        NodeKind::OffsetCurve,
        "閉じた輪郭を外へ広げ（+）、内へ縮めます（−）。角の上限を超える角は面取りされます。ずらした結果が自分と交わるとエラーになるので、距離を小さくしてください。",
    ),
    (
        NodeKind::PointOnCurve,
        "曲線の長さに沿った位置です。0 が始点、1 が終点で、曲線の形に関係なく長さの割合で決まります。",
    ),
    (
        NodeKind::DivideCurve,
        "曲線を長さで等分した点を作ります。点の X・Y を取り出せば、等間隔の柱や穴の位置決めに使えます。",
    ),
    (
        NodeKind::Pipe,
        "曲線に沿って円い管を通します。太さが曲がりの半径に近いと内側がつぶれるので、太さを小さくするか、曲線を緩やかにしてください。",
    ),
    (
        NodeKind::Sweep,
        "閉じた断面（XY 平面の形）を、パスに沿って動かします。断面の大きさは XY の座標そのものです。両端は開いたままです。",
    ),
    (
        NodeKind::SmoothMesh,
        "角を丸めます。強さと回数を増やすほど形が丸くなります。テイコフ方式は、なめらかにしても小さくなりにくい方式です。細かく分割したメッシュに使うと効果が分かりやすくなります。",
    ),
    (
        NodeKind::Subdivide,
        "三角形を 4 つに分けて面を細かくします。1 段階で面は 4 倍になり、3 段階までです。細かくしてから「なめらかにする」や「ノイズ変形」を使うと効果的です。",
    ),
    (
        NodeKind::WeldMesh,
        "許容差より近い頂点をまとめます。STL を読み込んだあと、面どうしがつながっていないときに使います。許容差を大きくしすぎると形が崩れます。",
    ),
    (
        NodeKind::UnifyNormals,
        "隣り合う面の向きをそろえ、閉じた形は外向きにします。書き出し検査で向きの不一致が出たときに試してください。",
    ),
    (
        NodeKind::MeshInfo,
        "頂点数・三角形数・体積・表面積を数値で返します。値を見る表で確認でき、印刷の目安にも使えます。体積は閉じた形でのみ意味があります。",
    ),
    (
        NodeKind::TaperScale,
        "低いところと高いところの倍率で、X・Y を Z に沿って変えます。上の倍率を 0.5 にすると上がすぼまり、1 より大きいと広がります。",
    ),
    (
        NodeKind::Bend,
        "指定した軸のまわりに曲げます。角度を大きくすると急に曲がり、形の長さが小さな半径の円弧になります。曲げ角が大きすぎるとエラーになります。",
    ),
    (
        NodeKind::GridArray3d,
        "X・Y・Z の個数と間隔で格子状に並べます。個数の積は 512 までです。配列の結果は出力すると多くの形になるので、個数を確認してください。",
    ),
    (
        NodeKind::Gyroid,
        "箱の中の網目構造（ジャイロイド）です。単位の大きさを小さくすると網目が細かくなり、壁の太さを増やすと隙間が減ります。分解能を上げると細かくなりますが、計算に時間がかかります。",
    ),
    (
        NodeKind::RevolvePoints,
        "点のリストを輪郭として Z 軸のまわりに回します。輪郭の両端を Z 軸上（X・Y が 0）に置くと、閉じた器になります。",
    ),
];

pub fn tip(kind: NodeKind) -> &'static str {
    TIPS.iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, t)| *t)
        .unwrap_or(FALLBACK_TIP)
}

const CATEGORY_ORDER: [&str; 8] = [
    "立体",
    "精密曲面",
    "曲線",
    "変形",
    "配列",
    "数値",
    "データ",
    "出力",
];

fn type_hint(kind: DataType) -> &'static str {
    match kind {
        DataType::Number => "数値",
        DataType::Point => "点",
        DataType::Curve => "曲線",
        DataType::Mesh => "メッシュ",
        DataType::Geometry => "形状（点・曲線・メッシュ）",
        DataType::Any => "なんでも",
        DataType::Surface => "数学曲面（メッシュとは別）",
        DataType::Solid => "閉じたB-rep立体",
        DataType::DisplayGeometry => "表示可能な形状（出力）",
    }
}

fn trim_number(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn matches(kind: NodeKind, query: &str) -> bool {
    let q = query.trim().to_lowercase().replace('　', " ");
    if q.is_empty() {
        return true;
    }
    let s = kind.spec();
    let hay = format!(
        "{} {} {} {}",
        s.label, s.keywords, s.category, s.description
    )
    .to_lowercase();
    q.split_whitespace().all(|w| hay.contains(w))
}

/// The help page of one node kind, built from its spec.
pub fn node_page(ui: &mut egui::Ui, kind: NodeKind) {
    let spec = kind.spec();
    ui.horizontal(|ui| {
        theme::pill(ui, spec.category, theme::category_color(spec.category));
        ui.label(RichText::new(spec.label).strong().size(17.));
    });
    ui.add_space(4.);
    ui.label(spec.description);
    ui.add_space(8.);
    ui.label(RichText::new("入力").strong());
    if spec.inputs.is_empty() {
        ui.label(theme::small("入力はありません。"));
    } else {
        egui::Grid::new(("help_inputs", spec.label))
            .num_columns(4)
            .striped(true)
            .spacing([12., 4.])
            .show(ui, |ui| {
                ui.label(theme::small("名前"));
                ui.label(theme::small("種類"));
                ui.label(theme::small("既定値"));
                ui.label(theme::small("範囲"));
                ui.end_row();
                for input in spec.inputs {
                    ui.label(RichText::new(input.label).size(12.));
                    ui.label(
                        RichText::new(type_hint(input.kind))
                            .size(11.)
                            .color(theme::TEAL),
                    );
                    match input.default {
                        Some(d) => {
                            ui.label(RichText::new(trim_number(d)).size(11.));
                            ui.label(
                                RichText::new(format!(
                                    "{} 〜 {}{}",
                                    trim_number(input.min),
                                    trim_number(input.max),
                                    if input.integer { "（整数）" } else { "" }
                                ))
                                .size(11.)
                                .color(theme::MUTED),
                            );
                        }
                        None => {
                            ui.label(RichText::new("—").size(11.).color(theme::MUTED));
                            ui.label(
                                RichText::new("線でつなぐ（必須）")
                                    .size(11.)
                                    .color(theme::ERROR),
                            );
                        }
                    }
                    ui.end_row();
                }
            });
        ui.label(theme::small(
            "数値の入力は、つながないときはノード上の値を使います。数列などのリストをつなぐと、要素ごとに計算します。",
        ));
    }
    ui.add_space(8.);
    ui.horizontal(|ui| {
        ui.label(RichText::new("出力").strong());
        ui.label(RichText::new(type_hint(spec.output)).color(theme::TEAL));
    });
    ui.add_space(8.);
    egui::Frame::new()
        .fill(theme::TEAL_PALE)
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.label(RichText::new("使い方のヒント").strong().size(12.));
            ui.label(RichText::new(tip(kind)).size(12.));
        });
}

/// The data-shape explanation shown at the top of the dictionary.
fn shapes_page(ui: &mut egui::Ui) {
    ui.label(RichText::new("データの形（1 個・リスト・ツリー）").strong());
    for (head, body) in [
        ("細い線 · 「1」", "値が 1 個だけ流れています。"),
        (
            "二重の太い線 · 「12」",
            "1 本のリストに 12 個の値。数列・範囲・配列などから出ます。",
        ),
        (
            "点線 · 「{3}×12」",
            "ツリー: 3 本の枝に合計 12 個。枝を分ける・ロフトの前などで出ます。困ったら「フラット化」で 1 本のリストに戻せます。",
        ),
        (
            "灰色 · 「0」",
            "空。無効にしたノードや、値が流れていない線です。",
        ),
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(head).size(11.).color(theme::TEAL));
            ui.label(RichText::new(body).size(11.));
        });
    }
    ui.label(theme::small(
        "リストどうしは同じ番号どうしで計算し、短い側は最後の値をくり返します。中身はノードを選んで「値を見る」で確認できます。",
    ));
}

#[derive(Default)]
pub struct HelpWindow {
    pub open: bool,
    pub kind: Option<NodeKind>,
    search: String,
}

impl HelpWindow {
    pub fn open_for(&mut self, kind: NodeKind) {
        self.open = true;
        self.kind = Some(kind);
    }
    pub fn open_index(&mut self) {
        self.open = true;
        self.kind = None;
    }
    /// Draws the dictionary. Returns a node kind the user asked to add to the canvas.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<NodeKind> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut add = None;
        egui::Window::new("ヘルプ · ノード辞典")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([760., 540.])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("さがす: 円 / twist / 並べる")
                            .desired_width(220.),
                    );
                    if ui.small_button("データの形について").clicked() {
                        self.kind = None;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(theme::small("オフラインで読めます · F1"));
                    });
                });
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(200.);
                        egui::ScrollArea::vertical()
                            .id_salt("help_index")
                            .max_height(440.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.index(ui));
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("help_page")
                            .max_height(440.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.kind {
                                Some(kind) => {
                                    node_page(ui, kind);
                                    ui.add_space(10.);
                                    if kind != NodeKind::Cluster
                                        && ui.button("＋ キャンバスに追加").clicked()
                                    {
                                        add = Some(kind);
                                    }
                                }
                                None => shapes_page(ui),
                            });
                    });
                });
            });
        self.open = open;
        add
    }
    fn index(&mut self, ui: &mut egui::Ui) {
        let mut kinds: Vec<NodeKind> = NodeKind::all()
            .iter()
            .copied()
            .filter(|k| matches(*k, &self.search))
            .collect();
        kinds.sort_by_key(|k| {
            CATEGORY_ORDER
                .iter()
                .position(|c| *c == k.spec().category)
                .unwrap_or(CATEGORY_ORDER.len())
        });
        if kinds.is_empty() {
            ui.label(theme::small("見つかりません。"));
        }
        let mut last = "";
        for kind in kinds {
            let spec = kind.spec();
            if spec.category != last {
                ui.add_space(6.);
                ui.label(theme::small(spec.category));
                last = spec.category;
            }
            let chosen = self.kind == Some(kind);
            if ui
                .selectable_label(
                    chosen,
                    RichText::new(spec.label)
                        .size(12.)
                        .color(theme::category_color(spec.category)),
                )
                .clicked()
            {
                self.kind = Some(kind);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_listed_kind_has_a_page_and_a_tip() {
        for kind in NodeKind::all() {
            assert!(!kind.spec().label.is_empty());
            assert!(!tip(*kind).is_empty());
        }
        assert_eq!(trim_number(2.5), "2.5");
        assert_eq!(trim_number(64.), "64");
        assert!(matches(NodeKind::Twist, "twist"));
    }
}
