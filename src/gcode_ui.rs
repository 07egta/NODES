//! Settings window for 「その他の書き出し → G-code（花瓶モード）」.
//! The geometry and file work lives in nodes::gcode.

use crate::theme;
use eframe::egui::{self, RichText};
use nodes::gcode::{self, GcodeReport, GcodeSettings, Preset};
use nodes::model::Tree;

#[derive(Default)]
pub struct GcodeDialog {
    open: bool,
    settings: GcodeSettings,
    estimate: Option<Result<GcodeReport, String>>,
}

impl GcodeDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.estimate = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn settings(&self) -> &GcodeSettings {
        &self.settings
    }

    /// Returns true when the user chose the save button with a valid estimate.
    /// The dialog then closes; the caller opens the file picker.
    pub fn show(&mut self, ctx: &egui::Context, trees: &Result<Vec<Tree>, String>) -> bool {
        if !self.open {
            return false;
        }
        let mut open = true;
        let mut save = false;
        egui::Window::new("G-code を書き出す（花瓶モード）")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("螺旋で壁を1枚、連続して印刷します（花瓶モード）。");
                ui.label(theme::small(
                    "未検証の機能です。初回は必ず印刷を見守ってください。",
                ));
                ui.add_space(8.0);
                egui::Grid::new("gcode_settings")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("機種");
                        egui::ComboBox::from_id_salt("gcode_preset")
                            .selected_text(self.settings.preset.label())
                            .show_ui(ui, |ui| {
                                for preset in Preset::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.preset,
                                            preset,
                                            preset.label(),
                                        )
                                        .clicked()
                                    {
                                        self.settings.bed_mm = preset.bed_mm();
                                    }
                                }
                            });
                        ui.end_row();

                        ui.label("ベッドの大きさ (mm)");
                        ui.horizontal(|ui| {
                            let [bx, by, bz] = &mut self.settings.bed_mm;
                            ui.label("X");
                            ui.add(egui::DragValue::new(bx).range(20.0..=500.0).speed(1.0));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(by).range(20.0..=500.0).speed(1.0));
                            ui.label("Z");
                            ui.add(egui::DragValue::new(bz).range(20.0..=500.0).speed(1.0));
                        });
                        ui.end_row();

                        ui.label("ノズル温度");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.nozzle_c)
                                .range(150.0..=300.0)
                                .speed(1.0)
                                .suffix(" ℃"),
                        );
                        ui.end_row();

                        ui.label("ベッド温度");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.bed_c)
                                .range(0.0..=120.0)
                                .speed(1.0)
                                .suffix(" ℃"),
                        );
                        ui.end_row();

                        ui.label("層の高さ");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.layer_height_mm)
                                .range(0.05..=0.6)
                                .speed(0.01)
                                .fixed_decimals(2)
                                .suffix(" mm"),
                        );
                        ui.end_row();

                        ui.label("線幅");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.line_width_mm)
                                .range(0.2..=1.2)
                                .speed(0.01)
                                .fixed_decimals(2)
                                .suffix(" mm"),
                        );
                        ui.end_row();

                        ui.label("押し出し速度");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.speed_mm_s)
                                .range(5.0..=300.0)
                                .speed(1.0)
                                .suffix(" mm/s"),
                        );
                        ui.end_row();

                        ui.label("移動速度");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.travel_mm_s)
                                .range(10.0..=500.0)
                                .speed(1.0)
                                .suffix(" mm/s"),
                        );
                        ui.end_row();

                        ui.label("ファン");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.fan_percent)
                                .range(0.0..=100.0)
                                .speed(1.0)
                                .suffix(" %"),
                        );
                        ui.end_row();

                        ui.label("底の塗りつぶし層数");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.bottom_layers)
                                .range(0..=50u32)
                                .speed(0.2),
                        );
                        ui.end_row();

                        ui.label("フィラメント直径");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.filament_mm)
                                .range(1.0..=3.5)
                                .speed(0.01)
                                .fixed_decimals(2)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.label(theme::small(
                    "開始・終了の G-code は一般的な例です。機種の説明書と照らしてください。Bambu Lab は Bambu Studio の .gcode.3mf が公式の方法です。",
                ));
                ui.add_space(8.0);

                if ui
                    .button("見積もりを計算")
                    .on_hover_text("層数・印刷時間・フィラメント量を計算します（ファイルは書きません）")
                    .clicked()
                {
                    self.estimate = Some(match trees {
                        Ok(trees) => {
                            gcode::generate(trees, &self.settings).map(|(_, report)| report)
                        }
                        Err(error) => Err(error.clone()),
                    });
                }

                match &self.estimate {
                    Some(Ok(report)) => {
                        egui::Grid::new("gcode_estimate")
                            .num_columns(2)
                            .spacing([24.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("層数");
                                ui.label(format!(
                                    "{}（底 {} 層 + 螺旋 {} 層）",
                                    report.layers, report.flat_layers, report.spiral_layers
                                ));
                                ui.end_row();
                                ui.label("印刷時間の目安");
                                ui.label(format!(
                                    "約 {} 分",
                                    (report.print_seconds / 60.0).ceil() as u64
                                ));
                                ui.end_row();
                                ui.label("フィラメント長");
                                ui.label(format!("{:.2} m", report.filament_mm / 1000.0));
                                ui.end_row();
                                ui.label("重さの目安");
                                ui.label(format!(
                                    "約 {:.1} g（PLA 1.24 g/cm³）",
                                    report.filament_grams
                                ));
                                ui.end_row();
                                ui.label("大きさ (mm)");
                                let s = report.size_mm;
                                ui.label(format!("{:.1} × {:.1} × {:.1}", s[0], s[1], s[2]));
                                ui.end_row();
                            });
                        ui.label(theme::small(
                            "加減速・加熱待ち・一時停止は含みません。",
                        ));
                    }
                    Some(Err(error)) => {
                        ui.colored_label(theme::ERROR, error.as_str());
                    }
                    None => {
                        ui.label(theme::small(
                            "「見積もりを計算」で印刷時間とフィラメント量を確認できます。",
                        ));
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = matches!(self.estimate, Some(Ok(_)));
                    if ui
                        .add_enabled(
                            ready,
                            egui::Button::new(
                                RichText::new("G-code として保存…")
                                    .strong()
                                    .color(theme::TEAL),
                            ),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                    ui.label(theme::small("設定を変えたら見積もりを計算し直してください。"));
                });
            });
        self.open = open && !save;
        save
    }
}
