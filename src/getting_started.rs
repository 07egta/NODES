//! Compact, offline first-use guidance. Detailed node help lives in `help`.

use crate::theme;
use eframe::egui::{self, RichText};

pub enum Action {
    Examples,
    Dictionary,
    Shortcuts,
}

pub fn show(ctx: &egui::Context, open: &mut bool, scale: &mut f32) -> Option<Action> {
    if !*open {
        return None;
    }
    let mut action = None;
    let available = ctx.content_rect().size();
    egui::Window::new("NODES の使い方")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(570.0)
        .max_width((available.x - 64.0).clamp(280.0, 570.0))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("getting_started_body")
                .max_height((available.y - 110.0).max(180.0))
                .show(ui, |ui| {
                    ui.heading("つなぐ。変える。形になる。");
                    ui.label(theme::small("Rhino なしで使える、オフラインのノード式デザインスタジオ"));
                    ui.add_space(8.0);
                    for (title, body) in [
                        ("1  作品例を開く", "まずは「ねじれの花器」。ノードを選ぶと右下に設定が出ます。"),
                        ("2  数値を変える", "スライダーを動かすか、数値欄をクリックして直接入力します。Enter で入力を終え、Ctrl+Z で変更を戻せます。"),
                        ("3  処理を追加する", "左の検索欄や Tab で「円を作りたい」「点を並べたい」などを検索します。候補を選んで追加してください。"),
                        ("4  線をつなぐ", "右の出力の丸から左の入力の丸へドラッグします。出力→入力の順にクリックしてもつながります。入力の丸を右クリックすると切断できます。"),
                        ("5  保存して、つくる", ".morph でノードと数値を保存します。形状を「出力」ノードにつなぎ、「書き出し」でメッシュを確認して STL・OBJ・3MF（試験的）へ保存できます。"),
                    ] {
                        ui.label(RichText::new(title).strong().color(theme::teal()));
                        ui.label(body);
                        ui.add_space(8.0);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("作品例を開く").clicked() {
                            action = Some(Action::Examples);
                        }
                        if ui.button("ノード辞典").clicked() {
                            action = Some(Action::Dictionary);
                        }
                        if ui.button("キーボード操作").clicked() {
                            action = Some(Action::Shortcuts);
                        }
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(RichText::new("現在の対応範囲").strong());
                    ui.label("メッシュに加え、精密曲線・NURBS曲面の作成と評価に一部対応しています。曲面をそのまま保存する STEP・IGES・3DM の入出力は未対応です。");
                    ui.label("Grasshopper互換は開発中です。GHX は CLI での検査・限定変換に対応しています。GH バイナリと外部Grasshopperプラグインの実行には対応していません。");
                    ui.label(theme::small("書き出し前の検査は、自己交差・最小肉厚・実機での造形成功を保証するものではありません。"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("表示サイズ");
                        if ui.add(egui::Slider::new(scale, 0.85..=1.35).step_by(0.05)).changed() {
                            ctx.set_zoom_factor(*scale);
                        }
                    });
                    ui.label(theme::small(concat!("NODES ", env!("CARGO_PKG_VERSION"))));
                });
        });
    if action.is_some() {
        *open = false;
    }
    action
}
