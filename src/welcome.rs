//! First-run launcher and a progressive, editable geometry lesson.
use crate::{theme, ui_polish};
use eframe::egui::{self, RichText};
use nodes::model::{Graph, NodeId, NodeKind};
use std::path::{Path, PathBuf};

const RECENT_LIMIT: usize = 8;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Japanese,
    English,
}

impl Language {
    pub fn text(self, ja: &'static str, en: &'static str) -> &'static str {
        match self {
            Self::Japanese => ja,
            Self::English => en,
        }
    }
}

pub enum Action {
    New,
    Open,
    Recent(PathBuf),
    Learn,
    Examples,
    Dictionary,
    Shortcuts,
    Easy,
    Expert,
}

pub struct Welcome {
    pub open: bool,
    pub language: Language,
    recent: Vec<PathBuf>,
}

fn remember(recent: &mut Vec<PathBuf>, path: PathBuf) {
    recent.retain(|p| p != &path);
    recent.insert(0, path);
    recent.truncate(RECENT_LIMIT);
}

impl Welcome {
    pub fn new(allowed: bool) -> Self {
        let done = ui_polish::read_setting("welcome").is_some_and(|v| v == "done")
            || ui_polish::read_setting("tutorial").is_some_and(|v| v == "done");
        let language = match ui_polish::read_setting("welcome_language").as_deref() {
            Some("en") => Language::English,
            _ => Language::Japanese,
        };
        let mut recent: Vec<PathBuf> = ui_polish::read_setting("recent_projects")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        recent.truncate(RECENT_LIMIT);
        Self {
            open: allowed && !done,
            language,
            recent,
        }
    }

    pub fn record(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        remember(&mut self.recent, path);
        if let Ok(json) = serde_json::to_string(&self.recent) {
            let _ = ui_polish::save_setting_line(&format!("recent_projects={json}"));
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut action = None;
        let mut close = false;
        let lang = self.language;
        egui::Window::new(lang.text("NODES へようこそ", "Welcome to NODES"))
            .id(egui::Id::new("welcome_launcher"))
            .open(&mut open)
            .collapsible(false)
            .default_width(560.)
            .max_width((ctx.content_rect().width() - 48.).clamp(280., 660.))
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.language, Language::Japanese, "日本語");
                    ui.selectable_value(&mut self.language, Language::English, "English");
                });
                egui::ScrollArea::vertical()
                    .id_salt("welcome_scroll")
                    .max_height((ctx.content_rect().height() - 150.).max(180.))
                    .show(ui, |ui| {
                        ui.heading(lang.text("最初の形をつくろう", "Make your first shape"));
                        ui.label(lang.text(
                            "ノードをつなぐと形が生まれます。CAD の経験は必要ありません。",
                            "Connect nodes to create geometry. No CAD experience required.",
                        ));
                        ui.add_space(8.);
                        if ui.button(RichText::new(lang.text(
                            "7 ステップで球から STL へ",
                            "From a sphere to STL in 7 steps",
                        )).strong().size(16.)).clicked() {
                            action = Some(Action::Learn);
                        }
                        ui.label(theme::small(lang.text(
                            "球 → 半径 → 点群 → 立体 → パターン → 変形 → STL",
                            "Sphere → radius → points → solids → pattern → deform → STL",
                        )));
                        ui.add_space(10.);
                        ui.horizontal_wrapped(|ui| {
                            for (label, a) in [
                                (lang.text("新規作品", "New project"), Action::New),
                                (lang.text("ファイルを開く", "Open file"), Action::Open),
                                (lang.text("作品ギャラリー", "Sample gallery"), Action::Examples),
                            ] {
                                if ui.button(label).clicked() {
                                    action = Some(a);
                                }
                            }
                        });
                        ui.separator();
                        ui.label(RichText::new(lang.text("最近の作品", "Recent projects")).strong());
                        if self.recent.is_empty() {
                            ui.label(theme::small(lang.text("まだありません", "No recent projects")));
                        }
                        for path in &self.recent {
                            let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
                            if ui.button(name.as_ref()).on_hover_text(path.display().to_string()).clicked() {
                                action = Some(Action::Recent(path.clone()));
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new(lang.text("学ぶ・作業環境", "Learn and workspaces")).strong());
                        ui.horizontal_wrapped(|ui| {
                            for (label, a) in [
                                (lang.text("ノード辞典", "Node dictionary"), Action::Dictionary),
                                (lang.text("キー操作", "Shortcuts"), Action::Shortcuts),
                                (lang.text("初心者", "Beginner"), Action::Easy),
                                (lang.text("上級者", "Expert"), Action::Expert),
                            ] {
                                if ui.button(label).clicked() {
                                    action = Some(a);
                                }
                            }
                        });
                        if ui.button(lang.text("スキップ（次回から表示しない）", "Skip (do not show again)")).clicked() {
                            close = true;
                        }
                    });
            });
        if self.language != lang {
            let code = if self.language == Language::English { "en" } else { "ja" };
            let _ = ui_polish::save_setting_line(&format!("welcome_language={code}"));
        }
        if !open || close || action.is_some() {
            self.open = false;
            let _ = ui_polish::save_setting_line("welcome=done");
        }
        action
    }
}

const STEPS: [(&str, &str, &str, &str); 7] = [
    ("球体を生成する", "Create a sphere", "球と出力ノードを追加します。出力は完成形を表示します。", "Add a Sphere and an Output. Output displays the final geometry."),
    ("サイズを変更する", "Change its size", "半径を 8 mm に変更します。右の設定でも自由に変えられます。", "Set the radius to 8 mm. You can edit it in the inspector too."),
    ("点群を生成する", "Generate points", "数列から X 座標を作り、30 mm 間隔の 3 点を表示します。", "Use a Series as X coordinates to display three points 30 mm apart."),
    ("点群を立体に変える", "Turn points into solids", "点の X 座標を取り出し、球をそれぞれの点へ移動します。", "Extract the X coordinate of each point and move a sphere to each position."),
    ("パターンを作る", "Make a pattern", "数列の個数を 5 に増やします。間隔と個数はあとから調整できます。", "Increase the Series count to five. You can change count and spacing later."),
    ("形状を変形する", "Deform the shape", "Z 倍率を 1.5 にして球を楕円体にします。", "Set the Z scale to 1.5 to stretch the spheres into ellipsoids."),
    ("STL として保存する", "Export STL", "書き出し画面で STL を選んで保存します。実機の造形成功は別途確認してください。", "Choose STL in the export dialog and save. Physical printability still needs checking."),
];

pub enum LessonAction {
    Apply,
    Export,
    Stop,
}

/// The lesson edits the actual graph, never a separate mock canvas.
#[derive(Default)]
pub struct Lesson {
    step: usize,
}

fn unique(graph: &Graph, kind: NodeKind) -> Result<NodeId, String> {
    let mut matches = graph.nodes.iter().filter(|n| n.kind == kind);
    let node = matches.next().ok_or_else(|| format!("{}: node missing", kind.spec().label))?;
    if matches.next().is_some() {
        return Err(format!("{}: more than one node; please undo or restart the lesson", kind.spec().label));
    }
    Ok(node.id)
}

fn param(graph: &mut Graph, id: NodeId, key: &str, value: f64) -> Result<(), String> {
    graph.node_mut(id).ok_or("Lesson node missing")?.params.insert(key.into(), value);
    Ok(())
}

impl Lesson {
    /// Transactional graph edit. A failed edit leaves both graph and lesson unchanged.
    pub fn apply(&mut self, graph: &mut Graph) -> Result<NodeId, String> {
        let mut next = graph.clone();
        let selected = match self.step {
            0 => {
                if !next.nodes.is_empty() {
                    return Err("Start the lesson with an empty graph".into());
                }
                let sphere = next.add(NodeKind::Sphere, [20., 30.])?;
                param(&mut next, sphere, "radius", 10.)?;
                let output = next.add(NodeKind::Output, [300., 30.])?;
                next.connect(sphere, output, "geometry")?;
                next.node_mut(sphere).ok_or("Sphere missing")?.preview = false;
                sphere
            }
            1 => {
                let sphere = unique(&next, NodeKind::Sphere)?;
                param(&mut next, sphere, "radius", 8.)?;
                sphere
            }
            2 => {
                let series = next.add(NodeKind::Series, [20., 300.])?;
                param(&mut next, series, "step", 30.)?;
                param(&mut next, series, "count", 3.)?;
                let point = next.add(NodeKind::Point, [300., 300.])?;
                next.connect(series, point, "x")?;
                next.node_mut(point).ok_or("Point missing")?.preview = true;
                series
            }
            3 => {
                let point = unique(&next, NodeKind::Point)?;
                let sphere = unique(&next, NodeKind::Sphere)?;
                let output = unique(&next, NodeKind::Output)?;
                let x = next.add(NodeKind::PointComponent, [580., 300.])?;
                param(&mut next, x, "axis", 0.)?;
                next.connect(point, x, "point")?;
                let moved = next.add(NodeKind::Move, [860., 30.])?;
                next.connect(sphere, moved, "geometry")?;
                next.connect(x, moved, "x")?;
                next.connect(moved, output, "geometry")?;
                next.node_mut(point).ok_or("Point missing")?.preview = false;
                next.node_mut(moved).ok_or("Move missing")?.preview = false;
                next.node_mut(output).ok_or("Output missing")?.position = [1140., 30.];
                moved
            }
            4 => {
                let series = unique(&next, NodeKind::Series)?;
                param(&mut next, series, "count", 5.)?;
                series
            }
            5 => {
                let moved = unique(&next, NodeKind::Move)?;
                let output = unique(&next, NodeKind::Output)?;
                let scale = next.add(NodeKind::ScaleXyz, [1140., 30.])?;
                param(&mut next, scale, "sz", 1.5)?;
                next.connect(moved, scale, "geometry")?;
                next.connect(scale, output, "geometry")?;
                next.node_mut(scale).ok_or("Scale missing")?.preview = false;
                next.node_mut(output).ok_or("Output missing")?.position = [1420., 30.];
                scale
            }
            _ => return Err("The remaining step is STL export".into()),
        };
        next.validate()?;
        *graph = next;
        self.step += 1;
        Ok(selected)
    }

    pub fn exported_stl(&mut self) {
        if self.step == 6 {
            self.step = 7;
        }
    }

    pub fn show(&self, ctx: &egui::Context, lang: Language) -> Option<LessonAction> {
        let mut open = true;
        let mut action = None;
        egui::Window::new(lang.text("最初の作品 · 7 ステップ", "First project · 7 steps"))
            .id(egui::Id::new("welcome_lesson"))
            .open(&mut open)
            .collapsible(true)
            .default_width(310.)
            .default_pos([18., 110.])
            .show(ctx, |ui| {
                ui.add(egui::ProgressBar::new(self.step as f32 / 7.)
                    .text(format!("{}/7", self.step)));
                if let Some(&(ja, en, body_ja, body_en)) = STEPS.get(self.step) {
                    ui.label(RichText::new(lang.text(ja, en)).strong());
                    ui.label(lang.text(body_ja, body_en));
                    if self.step == 6 {
                        if ui.button(lang.text("STL 書き出しを開く", "Open STL export")).clicked() {
                            action = Some(LessonAction::Export);
                        }
                    } else if ui.button(lang.text("実際のグラフに適用 →", "Apply to real graph →")).clicked() {
                        action = Some(LessonAction::Apply);
                    }
                } else {
                    ui.label(RichText::new(lang.text("STL の保存が完了しました！", "STL saved!")).strong());
                    ui.label(lang.text("Ctrl+S で編集可能な作品も保存できます。", "Use Ctrl+S to save the editable project as well."));
                }
                ui.label(theme::small(lang.text(
                    "グラフは自由に編集できます。案内を閉じても作品は残ります。",
                    "Edit the graph freely. Closing this guide keeps your project.",
                )));
                if ui.small_button(lang.text("案内を終了", "Close guide")).clicked() {
                    action = Some(LessonAction::Stop);
                }
            });
        if !open {
            action = Some(LessonAction::Stop);
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodes::{engine::Evaluator, model::Value};

    #[test]
    fn recent_projects_are_unique_and_bounded() {
        let mut recent = Vec::new();
        for i in 0..12 {
            remember(&mut recent, PathBuf::from(format!("{i}.morph")));
        }
        remember(&mut recent, PathBuf::from("8.morph"));
        assert_eq!(recent.len(), RECENT_LIMIT);
        assert_eq!(recent[0], PathBuf::from("8.morph"));
        assert_eq!(recent.iter().filter(|p| p.as_path() == Path::new("8.morph")).count(), 1);
    }

    #[test]
    fn lesson_evaluates_each_stage_and_finishes_only_after_export() {
        let mut lesson = Lesson::default();
        let mut graph = Graph::default();
        for step in 0..6 {
            lesson.apply(&mut graph).unwrap();
            graph.validate().unwrap();
            let evaluated = Evaluator::default().evaluate(&graph, &|| false);
            assert!(evaluated.outputs.values().all(Result::is_ok), "step {step}");
            let output = unique(&graph, NodeKind::Output).unwrap();
            let tree = evaluated.outputs.get(&output).unwrap().as_ref().unwrap();
            let count = tree.branches.values().flatten().filter(|v| matches!(v, Value::Mesh(_))).count();
            assert_eq!(count, if step < 3 { 1 } else if step == 3 { 3 } else { 5 });
        }
        assert_eq!(lesson.step, 6);
        assert!(lesson.apply(&mut graph).is_err());
        lesson.exported_stl();
        assert_eq!(lesson.step, 7);
    }

    #[test]
    fn failed_step_keeps_the_graph_and_progress() {
        let mut graph = Graph::default();
        graph.add(NodeKind::Sphere, [0., 0.]).unwrap();
        let mut lesson = Lesson::default();
        assert!(lesson.apply(&mut graph).is_err());
        assert_eq!(lesson.step, 0);
        assert_eq!(graph.nodes.len(), 1);
    }
}
