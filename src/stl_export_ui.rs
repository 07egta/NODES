//! The Japanese STL / 3MF / OBJ export dialog. Logic lives in `nodes::stl_export`.

use crate::file_picker::FileTask;
use crate::theme;
use eframe::egui::{self, Align2, Color32, RichText};
use nodes::model::{Mesh, Tree};
use nodes::stl_export::{
    BodyStats, Check, ExportOptions, Level, OutputFormat, RepairReport, StoredSettings,
    WriteFailure, WriteSummary, check_bodies, config_path, default_file_name, detect_slicer,
    gather_meshes, launch_slicer, load_stored, prepare_bodies, repair_meshes, save_stored,
    write_bodies_colored,
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn warning_color() -> Color32 {
    if theme::is_dark() {
        Color32::from_rgb(235, 187, 99)
    } else {
        Color32::from_rgb(132, 85, 12)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceMode {
    Output,
    Selected,
}

/// Everything the dialog needs from the app, captured for one frame.
pub struct StlSources {
    pub output: Result<Vec<Tree>, String>,
    pub selected: Result<Vec<Tree>, String>,
    pub revision: u64,
    pub base_name: String,
    pub start_dir: PathBuf,
}

/// Work the dialog asks the app to do (file dialogs live in the app).
pub enum UiRequest {
    Pick {
        task: FileTask,
        directory: PathBuf,
        file_name: String,
    },
    PickSlicer {
        directory: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisKey {
    revision: u64,
    mode: SourceMode,
    options: ExportOptions,
    repaired: bool,
}

#[derive(Clone, Debug)]
struct Analysis {
    checks: Vec<Check>,
    stats: BodyStats,
    skipped: usize,
}

struct RepairState {
    revision: u64,
    mode: SourceMode,
    meshes: Vec<Mesh>,
    report: RepairReport,
}

enum Action {
    Repair,
    Export,
    Quick,
    OpenSlicer,
    PickSlicer,
}

pub struct StlExportUi {
    open: bool,
    mode: SourceMode,
    options: ExportOptions,
    repair: Option<RepairState>,
    analysis: Option<(AnalysisKey, Result<Analysis, String>)>,
    stored: StoredSettings,
    config: Option<PathBuf>,
    last_files: Vec<PathBuf>,
    status: Option<(String, bool)>,
    notice: Option<(String, bool)>,
}

impl Default for StlExportUi {
    fn default() -> Self {
        Self::new()
    }
}

impl StlExportUi {
    pub fn new() -> Self {
        let config = config_path();
        let stored = config.as_deref().map(load_stored).unwrap_or_default();
        Self {
            open: false,
            mode: SourceMode::Output,
            options: ExportOptions::default(),
            repair: None,
            analysis: None,
            stored,
            config,
            last_files: Vec::new(),
            status: None,
            notice: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open_dialog(&mut self) {
        self.open = true;
        self.status = None;
    }

    /// A message for the app's toast line. Taken once.
    pub fn take_notice(&mut self) -> Option<(String, bool)> {
        self.notice.take()
    }

    pub fn set_slicer(&mut self, path: PathBuf) {
        self.stored.slicer_exe = Some(path);
        self.persist();
        self.say("スライサーを登録しました。", false);
    }

    fn say(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        self.status = Some((text.clone(), error));
        self.notice = Some((text, error));
    }

    fn persist(&mut self) {
        let Some(path) = self.config.clone() else {
            return;
        };
        if let Err(error) = save_stored(&path, &self.stored) {
            self.say(format!("設定を保存できませんでした: {error}"), true);
        }
    }

    fn trees<'a>(&self, sources: &'a StlSources) -> Result<&'a Vec<Tree>, String> {
        let result = match self.mode {
            SourceMode::Output => &sources.output,
            SourceMode::Selected => &sources.selected,
        };
        result.as_ref().map_err(Clone::clone)
    }

    fn repair_for(&self, sources: &StlSources) -> Option<&RepairState> {
        self.repair
            .as_ref()
            .filter(|r| r.revision == sources.revision && r.mode == self.mode)
    }

    /// Meshes after the optional repair, split, scale and placement; plus the skipped count.
    fn prepared(&self, sources: &StlSources) -> Result<(Vec<Mesh>, usize), String> {
        let trees = self.trees(sources)?;
        let (meshes, skipped) = gather_meshes(trees)?;
        let base = match self.repair_for(sources) {
            Some(state) => state.meshes.clone(),
            None => meshes,
        };
        Ok((prepare_bodies(&base, &self.options)?, skipped))
    }

    fn ensure_analysis(&mut self, sources: &StlSources) {
        let key = AnalysisKey {
            revision: sources.revision,
            mode: self.mode,
            options: self.options.clone(),
            repaired: self.repair_for(sources).is_some(),
        };
        if self
            .analysis
            .as_ref()
            .is_some_and(|(cached, _)| *cached == key)
        {
            return;
        }
        let max = self.options.max_size_mm;
        let result = self.prepared(sources).and_then(|(bodies, skipped)| {
            check_bodies(&bodies, max).map(|(checks, stats)| Analysis {
                checks,
                stats,
                skipped,
            })
        });
        self.analysis = Some((key, result));
    }

    fn repair(&mut self, sources: &StlSources) {
        let outcome = self
            .trees(sources)
            .and_then(|trees| gather_meshes(trees))
            .and_then(|(meshes, _)| repair_meshes(&meshes));
        match outcome {
            Ok((meshes, report)) => {
                let changed = report.changed();
                self.repair = Some(RepairState {
                    revision: sources.revision,
                    mode: self.mode,
                    meshes,
                    report,
                });
                self.analysis = None;
                if changed {
                    self.say(
                        "自動修復しました。内容を確認してから書き出してください。",
                        false,
                    );
                } else {
                    self.say("直す箇所は見つかりませんでした。", false);
                }
            }
            Err(error) => self.say(error, true),
        }
    }

    fn pick_request(&self, sources: &StlSources) -> UiRequest {
        let task = match self.options.format {
            OutputFormat::StlBinary | OutputFormat::StlAscii => FileTask::ExportStl,
            OutputFormat::Obj => FileTask::ExportObj,
            OutputFormat::Ply => FileTask::ExportPly,
            OutputFormat::ThreeMf => FileTask::ExportThreeMf,
        };
        let directory = self
            .stored
            .last_dir
            .clone()
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| sources.start_dir.clone());
        UiRequest::Pick {
            task,
            directory,
            file_name: default_file_name(
                &sources.base_name,
                self.options.format,
                SystemTime::now(),
            ),
        }
    }

    /// Writes to `base` (the chosen name; split bodies get numbered suffixes).
    pub fn export_to(&mut self, sources: &StlSources, base: &Path) {
        match self.write(sources, base) {
            Ok(summary) => {
                let text = match summary.files.first() {
                    Some(first) if summary.files.len() == 1 => format!(
                        "{} 面を書き出しました: {}",
                        summary.triangles,
                        display_name(first)
                    ),
                    _ => format!(
                        "{} 面を {} 個のファイルに書き出しました",
                        summary.triangles,
                        summary.files.len()
                    ),
                };
                self.last_files = summary.files;
                self.say(text, false);
            }
            Err(error) => {
                let message = error.to_string();
                if !error.completed.files.is_empty() {
                    self.last_files = error.completed.files;
                    self.remember_destination(base);
                }
                self.say(message, true);
            }
        }
    }

    fn remember_destination(&mut self, base: &Path) {
        self.stored.last_dir = base.parent().map(Path::to_path_buf);
        self.stored.last_base_file = Some(base.to_path_buf());
        self.persist();
    }

    fn write(&mut self, sources: &StlSources, base: &Path) -> Result<WriteSummary, WriteFailure> {
        self.options.validate()?;
        let (bodies, _) = self.prepared(sources)?;
        let summary = write_bodies_colored(
            self.options.format,
            base,
            &bodies,
            self.options.split_bodies,
            self.options.color_bodies,
        )?;
        self.remember_destination(base);
        Ok(summary)
    }

    /// Re-exports to the remembered file (hot reload for the slicer). Opens the picker if none.
    pub fn quick(&mut self, sources: &StlSources) -> Option<UiRequest> {
        let reusable = self
            .stored
            .last_base_file
            .clone()
            .filter(|p| has_extension(p, self.options.format.extension()));
        match reusable {
            Some(base) => {
                self.export_to(sources, &base);
                None
            }
            None => Some(self.pick_request(sources)),
        }
    }

    fn open_slicer(&mut self, sources: &StlSources) -> Option<UiRequest> {
        let Some(file) = self.last_files.first().cloned() else {
            self.say("先に書き出してから開いてください。", true);
            return None;
        };
        let exe = self
            .stored
            .slicer_exe
            .clone()
            .filter(|p| nodes::platform::is_slicer_executable(p))
            .or_else(detect_slicer);
        let Some(exe) = exe else {
            self.say("スライサーが見つかりません。実行ファイルを選んでください。", true);
            return Some(UiRequest::PickSlicer {
                directory: sources.start_dir.clone(),
            });
        };
        match launch_slicer(&exe, &file) {
            Ok(()) => {
                self.stored.slicer_exe = Some(exe);
                self.persist();
                self.say("スライサーを起動しました。", false);
            }
            Err(error) => self.say(error, true),
        }
        None
    }
}

/// Ctrl+Shift+E: re-export to the last file. Checked by the app before its other shortcuts.
pub fn quick_export_pressed(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::E,
        )
    })
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

/// Rough size only; the label says 約.
fn estimated_bytes(format: OutputFormat, triangles: usize) -> u64 {
    let t = triangles as u64;
    match format {
        OutputFormat::StlBinary => 84 + 50 * t,
        OutputFormat::StlAscii => 260 * t,
        OutputFormat::ThreeMf | OutputFormat::Obj => 100 * t,
        OutputFormat::Ply => 40 * t,
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{} KB", bytes.div_ceil(1000))
    }
}

impl StlExportUi {
    pub fn show(&mut self, ctx: &egui::Context, sources: &StlSources) -> Option<UiRequest> {
        if !self.open {
            return None;
        }
        self.ensure_analysis(sources);
        let mut open = true;
        let mut request: Option<UiRequest> = None;
        egui::Window::new("STL・3Dプリント用に書き出す")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .max_width((ctx.content_rect().width() - 64.0).clamp(280.0, 560.0))
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("stl_export_body")
                    .max_height((ctx.content_rect().height() - 110.0).max(180.0))
                    .show(ui, |ui| {
                        request = self.body(ui, sources);
                    });
            });
        if !open {
            self.open = false;
        }
        request
    }

    fn body(&mut self, ui: &mut egui::Ui, sources: &StlSources) -> Option<UiRequest> {
        let mut action: Option<Action> = None;
        let mut request: Option<UiRequest> = None;

        ui.label("「出力」ノードの形（または選んだノード）を、印刷用のファイルにします。");
        ui.horizontal_wrapped(|ui| {
            ui.label("書き出す形");
            ui.radio_value(&mut self.mode, SourceMode::Output, "完成形（出力ノード）");
            ui.radio_value(&mut self.mode, SourceMode::Selected, "選択ノード");
        });
        ui.separator();

        ui.label("形式");
        ui.horizontal_wrapped(|ui| {
            for format in OutputFormat::ALL {
                ui.radio_value(&mut self.options.format, format, format.label());
            }
        });
        ui.checkbox(
            &mut self.options.split_bodies,
            "離れた部品ごとに別ファイルにする（部品が複数あるとき）",
        );
        if matches!(
            self.options.format,
            OutputFormat::ThreeMf | OutputFormat::Ply
        ) {
            ui.checkbox(
                &mut self.options.color_bodies,
                "部品ごとに色を付ける（3MF・PLY。スライサーでの色の扱いは実機で未確認）",
            );
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("置き方");
            ui.radio_value(
                &mut self.options.place_on_bed,
                true,
                "ベッドに置く（一番低い所を Z=0）",
            );
            ui.radio_value(&mut self.options.place_on_bed, false, "作った位置のまま");
        });
        ui.add_enabled(
            self.options.place_on_bed,
            egui::Checkbox::new(&mut self.options.center_xy, "XY の中心を原点に合わせる"),
        );
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            ui.label("単位");
            ui.label(RichText::new("mm で書き出します").strong());
            ui.label("倍率");
            ui.add(
                egui::DragValue::new(&mut self.options.scale)
                    .speed(0.01)
                    .range(1.0e-6..=1.0e6),
            );
            for factor in [0.1, 10.0, 25.4] {
                if ui.small_button(format!("×{factor}")).clicked() {
                    self.options.scale = (self.options.scale * factor).clamp(1.0e-6, 1.0e6);
                }
            }
        });
        ui.label(theme::small(
            "STL には単位が書かれないため、スライサーは mm と見なします。cm のデータなら ×10、inch なら ×25.4 を押してください。",
        ));
        ui.horizontal_wrapped(|ui| {
            ui.label("印刷できる最大の大きさ");
            ui.add(
                egui::DragValue::new(&mut self.options.max_size_mm)
                    .speed(1.0)
                    .range(10.0..=1000.0)
                    .suffix(" mm"),
            );
            ui.label(theme::small("（Bambu A1 は 256 mm）"));
        });
        ui.separator();

        let mut ready = false;
        let mut has_error = false;
        match self.analysis.as_ref().map(|(_, result)| result.clone()) {
            Some(Ok(analysis)) => {
                ready = true;
                let [sx, sy, sz] = analysis.stats.size_mm;
                egui::Grid::new("stl_export_stats")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("大きさ（X × Y × Z）");
                        ui.label(format!("{sx:.1} × {sy:.1} × {sz:.1} mm"));
                        ui.end_row();
                        ui.label("部品数 / 三角形");
                        ui.label(format!(
                            "{} 個 / {} 枚",
                            analysis.stats.bodies, analysis.stats.triangles
                        ));
                        ui.end_row();
                        ui.label("ファイルの大きさ（約）");
                        ui.label(format_bytes(estimated_bytes(
                            self.options.format,
                            analysis.stats.triangles,
                        )));
                        ui.end_row();
                    });
                ui.add_space(4.0);
                for check in &analysis.checks {
                    check_row(ui, check);
                }
                has_error = analysis.checks.iter().any(|c| c.level == Level::Error);
                let (verdict, color) = if has_error {
                    (
                        "印刷には向かない箇所があります。「自動修復」を試してから書き出してください。",
                        theme::error(),
                    )
                } else if analysis.checks.iter().any(|c| c.level == Level::Warn) {
                    ("注意があります。内容を確認してください。", warning_color())
                } else {
                    (
                        "この検査では問題は見つかりませんでした。自己交差・最小肉厚は未検査です。",
                        theme::teal(),
                    )
                };
                ui.colored_label(color, verdict);
                if analysis.skipped > 0 {
                    ui.label(theme::small(format!(
                        "数値・点・曲線の {} 要素は書き出しに含みません。",
                        analysis.skipped
                    )));
                }
            }
            Some(Err(error)) => {
                ui.colored_label(theme::error(), error);
                ui.label(theme::small(
                    "立体を作り、「出力」ノードにつないでください。",
                ));
            }
            None => {
                ui.label("形状を確認しています…");
            }
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .button("自動修復")
                .on_hover_text("ほぼ同じ頂点の統合、つぶれた面と重なった面の削除、向きそろえ、小さな穴のふさぎ。作品は変えません。")
                .clicked()
            {
                action = Some(Action::Repair);
            }
        });
        if let Some(state) = self.repair_for(sources) {
            for line in state.report.lines() {
                ui.label(theme::small(line));
            }
        }
        ui.separator();

        let export_label = if has_error {
            "それでも書き出す…"
        } else {
            "書き出す…"
        };
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new(export_label).strong().color(
                        if theme::is_dark() {
                            Color32::from_rgb(18, 36, 31)
                        } else {
                            Color32::WHITE
                        },
                    ))
                    .fill(theme::teal()),
                )
                .clicked()
            {
                action = Some(Action::Export);
            }
            if ui
                .add_enabled(ready, egui::Button::new("すぐ書き出し（Ctrl+Shift+E）"))
                .on_hover_text("前回と同じファイルへ、今の設定で上書きします。スライサーの再読み込みに使えます。")
                .clicked()
            {
                action = Some(Action::Quick);
            }
        });
        ui.label(theme::small(format!(
            "保存名: {}",
            default_file_name(&sources.base_name, self.options.format, SystemTime::now())
        )));
        let slicer = self
            .stored
            .slicer_exe
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "未設定（開くときに自動で探します）".into());
        ui.add(egui::Label::new(theme::small(format!("スライサー: {slicer}"))).wrap());
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.last_files.is_empty(),
                    egui::Button::new("スライサーで開く"),
                )
                .clicked()
            {
                action = Some(Action::OpenSlicer);
            }
            if ui.small_button("変更…").clicked() {
                action = Some(Action::PickSlicer);
            }
        });
        if let Some((text, error)) = &self.status {
            let color = if *error {
                theme::error()
            } else {
                theme::teal()
            };
            ui.add(egui::Label::new(RichText::new(text.as_str()).color(color)).wrap());
        }

        match action {
            Some(Action::Repair) => self.repair(sources),
            Some(Action::Export) => request = Some(self.pick_request(sources)),
            Some(Action::Quick) => request = self.quick(sources),
            Some(Action::OpenSlicer) => request = self.open_slicer(sources),
            Some(Action::PickSlicer) => {
                request = Some(UiRequest::PickSlicer {
                    directory: sources.start_dir.clone(),
                })
            }
            None => {}
        }
        request
    }
}

fn check_row(ui: &mut egui::Ui, check: &Check) {
    let color = match check.level {
        Level::Ok => theme::teal(),
        Level::Warn => warning_color(),
        Level::Error => theme::error(),
    };
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(color, "●");
        ui.label(RichText::new(check.title).strong());
        ui.add(egui::Label::new(theme::small(check.detail.as_str())).wrap());
    });
}
