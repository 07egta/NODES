use crate::file_picker::{FilePicker, FileTask, PickerEvent};
use crate::{
    canvas::GraphCanvas,
    theme,
    viewport::{PreviewItem, Viewport},
};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Stroke};
use morphlet::{
    engine::{Evaluation, Evaluator},
    model::{Graph, NodeKind, Tree, Value},
    persistence::{self, History, Project},
    templates,
};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

struct ComputeRequest {
    revision: u64,
    graph: Graph,
}
struct ComputeResult {
    revision: u64,
    evaluation: Evaluation,
}
struct Worker {
    sender: mpsc::Sender<ComputeRequest>,
    receiver: mpsc::Receiver<ComputeResult>,
    latest: Arc<AtomicU64>,
}
impl Worker {
    fn new(ctx: egui::Context) -> Self {
        let (sender, requests) = mpsc::channel::<ComputeRequest>();
        let (results, receiver) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0));
        let revision = latest.clone();
        std::thread::spawn(move || {
            let mut evaluator = Evaluator::default();
            while let Ok(mut request) = requests.recv() {
                while let Ok(newer) = requests.try_recv() {
                    request = newer;
                }
                let result = evaluator.evaluate(&request.graph, &|| {
                    revision.load(Ordering::Relaxed) != request.revision
                });
                if !result.cancelled && revision.load(Ordering::Relaxed) == request.revision {
                    if results
                        .send(ComputeResult {
                            revision: request.revision,
                            evaluation: result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Self {
            sender,
            receiver,
            latest,
        }
    }
}

#[derive(Clone)]
enum PendingAction {
    New,
    Template(usize),
    Open(PathBuf),
    Exit,
}

pub struct MorphletApp {
    project: Project,
    history: History,
    canvas: GraphCanvas,
    viewport: Viewport,
    worker: Worker,
    evaluation: Option<Evaluation>,
    revision: u64,
    result_revision: u64,
    pending_compute: bool,
    last_submit: Instant,
    file_path: Option<PathBuf>,
    file_picker: Option<FilePicker>,
    after_save: Option<PendingAction>,
    dirty: bool,
    search: String,
    category: String,
    palette_open: bool,
    palette_focus: bool,
    palette_position: [f32; 2],
    template_open: bool,
    help_open: bool,
    export_open: bool,
    export_stats: Option<Result<persistence::ExportReport, String>>,
    toast: Option<(String, bool, Instant)>,
    confirm_action: Option<PendingAction>,
    allow_close: bool,
    preview_mode: u8,
    gesture_before: Option<Project>,
    last_autosave: Instant,
    recovery: Option<Project>,
    ui_scale: f32,
    screenshot: Option<PathBuf>,
    started: Instant,
    screenshot_requested: bool,
    suppress_history: bool,
}

impl MorphletApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        open_file: Option<PathBuf>,
        screenshot: Option<PathBuf>,
    ) -> Self {
        theme::configure(&cc.egui_ctx);
        let mut project = templates::create(0);
        let mut file_path = None;
        let mut initial_error = None;
        if let Some(path) = open_file {
            match persistence::load(&path) {
                Ok(p) => {
                    project = p;
                    file_path = Some(path)
                }
                Err(e) => initial_error = Some(e),
            }
        }
        let mut canvas = GraphCanvas::default();
        canvas.fit_requested = true;
        if let Some(n) = project
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Vase)
        {
            canvas.select_only(n.id);
        }
        let recovery = if file_path.is_none() && screenshot.is_none() {
            recovery_path().and_then(|p| persistence::load(&p).ok())
        } else {
            None
        };
        let mut app = Self {
            project,
            history: History::default(),
            canvas,
            viewport: Viewport::default(),
            worker: Worker::new(cc.egui_ctx.clone()),
            evaluation: None,
            revision: 0,
            result_revision: u64::MAX,
            pending_compute: true,
            last_submit: Instant::now() - Duration::from_secs(1),
            file_path,
            file_picker: None,
            after_save: None,
            dirty: false,
            search: String::new(),
            category: "すべて".into(),
            palette_open: false,
            palette_focus: false,
            palette_position: [0., 0.],
            template_open: false,
            help_open: false,
            export_open: false,
            export_stats: None,
            toast: None,
            confirm_action: None,
            allow_close: false,
            preview_mode: 0,
            gesture_before: None,
            last_autosave: Instant::now(),
            recovery,
            ui_scale: 1.,
            screenshot,
            started: Instant::now(),
            screenshot_requested: false,
            suppress_history: false,
        };
        if let Some(e) = initial_error {
            app.notify(e, true);
        }
        app
    }
    fn notify(&mut self, message: impl Into<String>, error: bool) {
        self.toast = Some((message.into(), error, Instant::now()));
    }
    fn stale(&self) -> bool {
        self.result_revision != self.revision || self.pending_compute
    }
    fn queue_compute(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.worker.latest.store(self.revision, Ordering::Relaxed);
        self.pending_compute = true;
        self.export_stats = None;
    }
    fn compute_tick(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.worker.receiver.try_recv() {
            if result.revision == self.revision {
                self.result_revision = result.revision;
                self.evaluation = Some(result.evaluation);
            }
        }
        if self.pending_compute && self.last_submit.elapsed() >= Duration::from_millis(35) {
            self.worker.latest.store(self.revision, Ordering::Relaxed);
            if self
                .worker
                .sender
                .send(ComputeRequest {
                    revision: self.revision,
                    graph: self.project.graph.clone(),
                })
                .is_ok()
            {
                self.pending_compute = false;
                self.last_submit = Instant::now();
            } else {
                self.notify(
                    "計算スレッドが停止しました。作品を保存して再起動してください。",
                    true,
                );
            }
        }
        if self.stale() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    fn result_trees(&self) -> Result<Vec<Tree>, String> {
        if self.stale() {
            return Err("形状を更新中です。更新が終わってから書き出してください。".into());
        }
        let Some(e) = &self.evaluation else {
            return Err("形状の計算が完了していません。".into());
        };
        collect_export_trees(&self.project.graph, e, &self.canvas.selected)
    }
    fn previews(&self) -> Vec<PreviewItem> {
        if self.stale() {
            return vec![];
        }
        let Some(e) = &self.evaluation else {
            return vec![];
        };
        let ids: Vec<_> = match self.preview_mode {
            1 => self.canvas.selected.iter().copied().collect(),
            2 => self
                .project
                .graph
                .nodes
                .iter()
                .filter(|n| n.preview)
                .map(|n| n.id)
                .collect(),
            _ => {
                let outs: Vec<_> = self
                    .project
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Output)
                    .map(|n| n.id)
                    .collect();
                if outs.is_empty() {
                    self.canvas.selected.iter().copied().collect()
                } else {
                    outs
                }
            }
        };
        ids.into_iter()
            .filter_map(|id| {
                e.outputs
                    .get(&id)
                    .and_then(|r| r.as_ref().ok())
                    .map(|tree| PreviewItem {
                        node_id: id,
                        tree: tree.clone(),
                        selected: self.canvas.selected.contains(&id) || self.preview_mode == 0,
                    })
            })
            .collect()
    }
    fn save(&mut self, save_as: bool) -> bool {
        if save_as || self.file_path.is_none() {
            self.file_picker = Some(FilePicker::new(
                FileTask::Save,
                self.start_directory(),
                format!("{}.morph", safe_name(&self.project.name)),
            ));
            return false;
        }
        let Some(path) = self.file_path.clone() else {
            return false;
        };
        self.save_to(path)
    }
    fn save_to(&mut self, path: PathBuf) -> bool {
        match persistence::save(&path, &self.project) {
            Ok(()) => {
                self.file_path = Some(path);
                self.dirty = false;
                self.notify("作品を保存しました", false);
                true
            }
            Err(e) => {
                self.notify(e, true);
                false
            }
        }
    }
    fn request(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.dirty {
            self.confirm_action = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }
    fn perform(&mut self, action: PendingAction, ctx: &egui::Context) {
        let new = match action {
            PendingAction::New => Some((Project::new("新しい作品", Graph::default()), None)),
            PendingAction::Template(index) => Some((templates::create(index), None)),
            PendingAction::Open(path) => match persistence::load(&path) {
                Ok(p) => Some((p, Some(path))),
                Err(e) => {
                    self.notify(e, true);
                    None
                }
            },
            PendingAction::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                None
            }
        };
        if let Some((project, path)) = new {
            self.project = project;
            self.file_path = path;
            self.dirty = false;
            self.history = History::default();
            self.gesture_before = None;
            self.suppress_history = true;
            self.canvas.selected.clear();
            self.canvas.connecting = None;
            self.canvas.fit_requested = true;
            self.viewport = Viewport::default();
            self.evaluation = None;
            self.queue_compute();
            self.preview_mode = 0;
            if let Some(n) = self.project.graph.nodes.first() {
                self.canvas.select_only(n.id);
            }
        }
    }
    fn open_dialog(&mut self, _ctx: &egui::Context) {
        self.file_picker = Some(FilePicker::new(
            FileTask::Open,
            self.start_directory(),
            String::new(),
        ));
    }
    fn start_directory(&self) -> PathBuf {
        self.file_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .map(PathBuf::from)
                    .map(|p| {
                        if p.join("Documents").is_dir() {
                            p.join("Documents")
                        } else {
                            p
                        }
                    })
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }
    fn file_dialog(&mut self, ctx: &egui::Context) {
        let response = self
            .file_picker
            .as_mut()
            .and_then(|picker| picker.show(ctx).map(|r| (picker.task, r)));
        if let Some((task, event)) = response {
            self.file_picker = None;
            match event {
                PickerEvent::Cancelled => {
                    self.after_save = None;
                }
                PickerEvent::Chosen(path) => match task {
                    FileTask::Open => self.request(PendingAction::Open(path), ctx),
                    FileTask::Save => {
                        if self.save_to(path) {
                            if let Some(action) = self.after_save.take() {
                                self.perform(action, ctx);
                            }
                        } else {
                            self.after_save = None;
                        }
                    }
                    FileTask::ExportStl | FileTask::ExportObj => {
                        let result = self.result_trees().and_then(|trees| {
                            if task == FileTask::ExportStl {
                                persistence::export_stl(&path, &trees)
                            } else {
                                persistence::export_obj(&path, &trees)
                            }
                        });
                        match result {
                            Ok(r) => {
                                self.notify(
                                    format!("{} 面の形状を書き出しました", r.triangles),
                                    false,
                                );
                                self.export_open = false;
                            }
                            Err(e) => self.notify(e, true),
                        }
                    }
                },
            }
        }
    }
    fn add_node(&mut self, kind: NodeKind, position: Option<[f32; 2]>) {
        let selected = self.canvas.selected.iter().next_back().copied();
        let at = position.unwrap_or_else(|| {
            selected
                .and_then(|id| self.project.graph.node(id))
                .map(|n| [n.position[0] + 280., n.position[1] + 40.])
                .unwrap_or_else(|| self.canvas.center_for_add(&self.project))
        });
        match self.project.graph.add(kind, at) {
            Ok(id) => {
                if let Some(source) = selected {
                    let output = self
                        .project
                        .graph
                        .node(source)
                        .map(|n| n.kind.spec().output);
                    if let Some(port) =
                        kind.spec().inputs.iter().find(|s| {
                            s.default.is_none() && output.is_some_and(|t| s.kind.accepts(t))
                        })
                    {
                        let _ = self.project.graph.connect(source, id, port.key);
                    }
                }
                self.canvas.select_only(id);
                self.palette_open = false;
                self.search.clear();
                self.notify(format!("「{}」を追加しました", kind.spec().label), false);
            }
            Err(e) => self.notify(e, true),
        }
    }
    fn delete_selected(&mut self) {
        let ids: Vec<_> = self.canvas.selected.iter().copied().collect();
        for id in ids {
            self.project.graph.remove(id);
        }
        self.canvas.selected.clear();
        self.canvas.connecting = None;
    }
    fn duplicate_selected(&mut self) {
        let nodes: Vec<_> = self
            .project
            .graph
            .nodes
            .iter()
            .filter(|n| self.canvas.selected.contains(&n.id))
            .cloned()
            .collect();
        let mut map = std::collections::BTreeMap::new();
        for old in &nodes {
            match self
                .project
                .graph
                .add(old.kind, [old.position[0] + 35., old.position[1] + 40.])
            {
                Ok(id) => {
                    map.insert(old.id, id);
                    if let Some(n) = self.project.graph.node_mut(id) {
                        n.params = old.params.clone();
                        n.name = format!("{} コピー", old.name);
                        n.preview = old.preview;
                    }
                }
                Err(e) => {
                    self.notify(e, true);
                    return;
                }
            }
        }
        for old in &nodes {
            if let Some(id) = map.get(&old.id) {
                for (key, source) in &old.inputs {
                    let from = *map.get(source).unwrap_or(source);
                    let _ = self.project.graph.connect(from, *id, key);
                }
            }
        }
        self.canvas.selected = map.values().copied().collect();
    }
    fn undo(&mut self) {
        if self.history.undo(&mut self.project) {
            self.gesture_before = None;
            self.suppress_history = true;
            self.dirty = true;
            self.queue_compute();
            self.canvas
                .selected
                .retain(|id| self.project.graph.node(*id).is_some());
        }
    }
    fn redo(&mut self) {
        if self.history.redo(&mut self.project) {
            self.gesture_before = None;
            self.suppress_history = true;
            self.dirty = true;
            self.queue_compute();
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        let wants = ctx.wants_keyboard_input();
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            )
        }) {
            self.save(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.open_dialog(ctx);
        }
        if !wants {
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                )
            }) {
                self.redo();
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                self.undo();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)) {
                self.redo();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D)) {
                self.duplicate_selected();
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                self.delete_selected();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
                self.canvas.selected = self.project.graph.nodes.iter().map(|n| n.id).collect();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F)) {
                let items = self.previews();
                self.viewport.fit(&items);
                self.canvas.fit_requested = true;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Tab))
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K))
            {
                self.palette_open = true;
                self.palette_focus = true;
                self.palette_position = self.canvas.center_for_add(&self.project);
                self.search.clear();
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.help_open = !self.help_open;
        }
    }
    fn topbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("topbar")
            .exact_height(66.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(28, 48, 48))
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(232, 241, 232));
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_rgb(42, 66, 64);
                ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(42, 66, 64);
                ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1_f32, Color32::WHITE);
                ui.horizontal_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(30., 30.), Sense::hover());
                    let p = ui.painter();
                    p.line_segment(
                        [
                            r.left_bottom() - egui::vec2(0., 5.),
                            r.center_top() + egui::vec2(-2., 4.),
                        ],
                        Stroke::new(3_f32, Color32::from_rgb(152, 215, 181)),
                    );
                    p.line_segment(
                        [
                            r.center_top() + egui::vec2(-2., 4.),
                            r.right_bottom() - egui::vec2(0., 5.),
                        ],
                        Stroke::new(3_f32, Color32::from_rgb(152, 215, 181)),
                    );
                    p.circle_filled(
                        r.center() + egui::vec2(0., 4.),
                        3.5,
                        Color32::from_rgb(232, 201, 120),
                    );
                    ui.label(
                        RichText::new("morphlet")
                            .size(24.)
                            .strong()
                            .color(Color32::from_rgb(242, 247, 233)),
                    );
                    ui.add_space(13.);
                    ui.label(
                        RichText::new("GEOMETRY STUDIO")
                            .size(9.)
                            .color(Color32::from_rgb(159, 187, 172)),
                    );
                    ui.add_space(18.);
                    ui.separator();
                    ui.add_space(8.);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project.name)
                            .font(egui::TextStyle::Body)
                            .desired_width(185.)
                            .frame(false)
                            .text_color(Color32::from_rgb(228, 235, 226)),
                    )
                    .on_hover_text("作品名を編集");
                    if self.dirty {
                        ui.label(
                            RichText::new("●")
                                .size(9.)
                                .color(Color32::from_rgb(228, 193, 115)),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("書き出し ↗")
                                        .color(Color32::from_rgb(20, 58, 45)),
                                )
                                .fill(Color32::from_rgb(177, 222, 184)),
                            )
                            .on_hover_text("完成形状を STL / OBJ へ")
                            .clicked()
                        {
                            self.export_stats = Some(
                                self.result_trees()
                                    .and_then(|trees| persistence::mesh_stats(&trees)),
                            );
                            self.export_open = true;
                        }
                        if ui.button("保存").on_hover_text("Ctrl+S").clicked() {
                            self.save(false);
                        }
                        if ui.button("開く").on_hover_text("Ctrl+O").clicked() {
                            self.open_dialog(ctx);
                        }
                        if ui.button("作品例").clicked() {
                            self.template_open = true;
                        }
                        if ui.button("?").on_hover_text("使い方 · F1").clicked() {
                            self.help_open = true;
                        }
                    });
                });
            });
    }
    fn library(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .exact_width(196.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(249, 251, 247))
                    .inner_margin(egui::Margin::symmetric(14, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ライブラリ").strong().size(13.));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(theme::small(format!("{}", NodeKind::all().len())));
                    });
                });
                ui.add_space(7.);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("円・ねじる・並べる…")
                        .desired_width(f32::INFINITY),
                );
                search.on_hover_text("日本語・英語で検索できます");
                ui.add_space(10.);
                egui::ComboBox::from_id_salt("category")
                    .selected_text(&self.category)
                    .width(156.)
                    .show_ui(ui, |ui| {
                        for category in [
                            "すべて",
                            "数値",
                            "曲線",
                            "立体",
                            "変形",
                            "配列",
                            "データ",
                            "出力",
                        ] {
                            ui.selectable_value(&mut self.category, category.into(), category);
                        }
                    });
                ui.add_space(12.);
                let mut add = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 130.).max(140.))
                    .show(ui, |ui| {
                        let mut last = "";
                        let mut ordered: Vec<_> = NodeKind::all().to_vec();
                        ordered.sort_by_key(|kind| category_rank(kind.spec().category));
                        for kind in &ordered {
                            let spec = kind.spec();
                            if self.category != "すべて" && self.category != spec.category {
                                continue;
                            }
                            if !matches_search(*kind, &self.search) {
                                continue;
                            }
                            if last != spec.category {
                                ui.add_space(9.);
                                ui.label(theme::small(spec.category));
                                ui.add_space(2.);
                                last = spec.category;
                            }
                            let color = theme::category_color(spec.category);
                            let resp = ui.add_sized(
                                [162., 30.],
                                egui::Button::new(
                                    RichText::new(format!("＋  {}", spec.label))
                                        .size(12.)
                                        .color(color),
                                )
                                .frame(false),
                            );
                            if resp
                                .clone()
                                .on_hover_text(format!(
                                    "{}\n\n{}",
                                    spec.description, "クリックで追加。選択中の形状に接続します。"
                                ))
                                .clicked()
                            {
                                add = Some(*kind);
                            }
                        }
                    });
                if let Some(kind) = add {
                    self.add_node(kind, None);
                }
                ui.add_space(12.);
                ui.separator();
                ui.add_space(8.);
                ui.label(
                    RichText::new("小さな操作から、\n思いがけない形へ。")
                        .size(12.)
                        .color(theme::MUTED),
                );
                ui.add_space(8.);
                if ui
                    .add_sized([164., 31.], egui::Button::new("作品例からはじめる"))
                    .clicked()
                {
                    self.template_open = true;
                }
                if ui.small_button("空のキャンバスを開く").clicked() {
                    self.request(PendingAction::New, ctx);
                }
            });
    }
    fn statusbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(30.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(237, 242, 234))
                    .inner_margin(egui::Margin::symmetric(14, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.stale() {
                        ui.spinner();
                        ui.label(theme::small("計算中…"));
                    } else if let Some(e) = &self.evaluation {
                        let errors = e.outputs.values().filter(|r| r.is_err()).count();
                        if errors == 0 {
                            ui.label(RichText::new("●").size(9.).color(theme::TEAL));
                            ui.label(theme::small("更新済み"));
                        } else {
                            ui.label(
                                RichText::new(format!("!  {} 個の入力を確認", errors))
                                    .size(11.)
                                    .color(theme::ERROR),
                            );
                        }
                        ui.label(theme::small(format!(
                            "  {:.1} ms  ·  計算 {} / 再利用 {}",
                            e.elapsed_ms, e.computed, e.cached
                        )));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(theme::small("mm  /  °"));
                        ui.separator();
                        ui.label(theme::small("オフライン"));
                        ui.separator();
                        ui.label(theme::small(format!(
                            "{} ノード",
                            self.project.graph.nodes.len()
                        )));
                    });
                });
            });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.canvas.selected.iter().next_back().copied() else {
            ui.add_space(15.);
            ui.label(RichText::new("ノードを選んで調整").strong());
            ui.label(theme::small(
                "左のキャンバスでノードをクリックすると、\nパラメータと処理の結果を確認できます。",
            ));
            return;
        };
        let Some(node) = self.project.graph.node(id).cloned() else {
            return;
        };
        let spec = node.kind.spec();
        ui.horizontal(|ui| {
            theme::pill(ui, spec.category, theme::category_color(spec.category));
            ui.label(RichText::new(spec.label).strong().size(15.));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(theme::small(format!("#{id:02}")));
            });
        });
        ui.add_space(3.);
        ui.label(theme::small(spec.description));
        ui.add_space(9.);
        let mut disconnect = None;
        for input in spec.inputs {
            let linked = node.inputs.get(input.key).copied();
            ui.horizontal(|ui| {
                ui.add_sized(
                    [100., 23.],
                    egui::Label::new(RichText::new(input.label).size(12.)),
                );
                if let Some(source) = linked {
                    let name = self
                        .project
                        .graph
                        .node(source)
                        .map(|n| n.name.as_str())
                        .unwrap_or("不明");
                    ui.label(
                        RichText::new(format!("← {name}"))
                            .size(11.)
                            .color(theme::TEAL),
                    );
                    if ui.small_button("切断").clicked() {
                        disconnect = Some(input.key);
                    }
                } else if let Some(default) = input.default {
                    if let Some(n) = self.project.graph.node_mut(id) {
                        let value = n.params.entry(input.key.into()).or_insert(default);
                        let (low, high) = friendly_range(input.key, input.min, input.max, *value);
                        let slider = egui::Slider::new(value, low..=high)
                            .step_by(input.step)
                            .max_decimals(if input.integer { 0 } else { 3 });
                        if ui.add(slider).changed() && input.integer {
                            *value = value.round();
                        }
                    }
                } else {
                    ui.label(
                        RichText::new("形状を接続してください")
                            .size(11.)
                            .color(theme::ERROR),
                    );
                }
            });
        }
        if let Some(key) = disconnect {
            self.project.graph.disconnect(id, key);
        }
        ui.add_space(9.);
        egui::CollapsingHeader::new("データを見る")
            .id_salt("data_inspector")
            .show(ui, |ui| {
                if self.stale() {
                    ui.label(theme::small("更新を待っています…"));
                    return;
                }
                match self.evaluation.as_ref().and_then(|e| e.outputs.get(&id)) {
                    Some(Ok(tree)) => {
                        ui.label(theme::small(format!(
                            "{} ブランチ / {} 要素",
                            tree.branches.len(),
                            tree.item_count()
                        )));
                        for (path, values) in tree.branches.iter().take(12) {
                            ui.label(
                                RichText::new(format!(
                                    "{{{}}}  ·  {} 個",
                                    path.iter()
                                        .map(|x| x.to_string())
                                        .collect::<Vec<_>>()
                                        .join(";"),
                                    values.len()
                                ))
                                .size(11.)
                                .color(theme::TEAL),
                            );
                            for value in values.iter().take(4) {
                                ui.label(theme::small(value_summary(value)));
                            }
                            if values.len() > 4 {
                                ui.label(theme::small(format!("… 残り {} 個", values.len() - 4)));
                            }
                        }
                    }
                    Some(Err(e)) => {
                        ui.colored_label(theme::ERROR, e);
                    }
                    None => {
                        ui.label(theme::small("未計算"));
                    }
                }
            });
        egui::CollapsingHeader::new("ノードの設定")
            .id_salt("node_settings")
            .show(ui, |ui| {
                if let Some(n) = self.project.graph.node_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("名前");
                        ui.text_edit_singleline(&mut n.name);
                    });
                    ui.checkbox(&mut n.preview, "「表示中」モードで表示");
                }
                ui.horizontal(|ui| {
                    if ui.button("複製").clicked() {
                        self.duplicate_selected();
                    }
                    if ui.button("削除").clicked() {
                        self.delete_selected();
                    }
                });
            });
    }
    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("workspace")
            .default_width(485.)
            .min_width(390.)
            .max_width(850.)
            .resizable(true)
            .frame(egui::Frame::new().fill(Color32::from_rgb(251, 253, 249)))
            .show(ctx, |ui| {
                let items = self.previews();
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("3D プレビュー").strong().size(13.));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(theme::small("LIVE"));
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.preview_mode, 0, "完成形");
                            ui.selectable_value(&mut self.preview_mode, 1, "選択ノード");
                            ui.selectable_value(&mut self.preview_mode, 2, "表示中");
                            if ui
                                .small_button("全体")
                                .on_hover_text("形を画面に収める · F")
                                .clicked()
                            {
                                self.viewport.fit(&items);
                            }
                        });
                    });
                let h = (ui.available_height() * 0.51).max(220.);
                let available = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(egui::vec2(available, h), Sense::hover());
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    if let Some(id) = self.viewport.show(ui, &items) {
                        self.canvas.select_only(id);
                    }
                    if self.stale() {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "形を更新しています…",
                            FontId::proportional(13.),
                            theme::MUTED,
                        );
                    }
                });
                ui.add_space(2.);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.viewport.wireframe, "ワイヤー");
                            ui.checkbox(&mut self.viewport.show_grid, "グリッド");
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("上").clicked() {
                                    self.viewport.set_view("top");
                                }
                                if ui.small_button("前").clicked() {
                                    self.viewport.set_view("front");
                                }
                                if ui.small_button("斜め").clicked() {
                                    self.viewport.set_view("iso");
                                }
                            });
                        });
                    });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(14)
                            .show(ui, |ui| self.inspector(ui));
                    });
            });
    }
    fn center_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(253, 254, 250))
                    .inner_margin(egui::Margin::symmetric(15, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("ノードキャンバス").size(13.).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .button("＋ 追加")
                                    .on_hover_text("Tab またはダブルクリック")
                                    .clicked()
                                {
                                    self.palette_open = true;
                                    self.palette_focus = true;
                                    self.palette_position =
                                        self.canvas.center_for_add(&self.project);
                                    self.search.clear();
                                }
                                if ui.small_button("整列").clicked() {
                                    self.canvas.auto_arrange(&mut self.project);
                                }
                                if ui.small_button("全体").clicked() {
                                    self.canvas.fit_requested = true;
                                }
                                if ui
                                    .add_enabled(self.history.can_redo(), egui::Button::new("↪"))
                                    .on_hover_text("やり直す · Ctrl+Y")
                                    .clicked()
                                {
                                    self.redo();
                                }
                                if ui
                                    .add_enabled(self.history.can_undo(), egui::Button::new("↩"))
                                    .on_hover_text("元に戻す · Ctrl+Z")
                                    .clicked()
                                {
                                    self.undo();
                                }
                            });
                        });
                    });
                let stale = self.stale();
                let outcome =
                    self.canvas
                        .show(ui, &mut self.project, self.evaluation.as_ref(), stale);
                if let Some(e) = outcome.error {
                    self.notify(e, true);
                }
                if outcome.delete {
                    self.delete_selected();
                }
                if outcome.duplicate {
                    self.duplicate_selected();
                }
                if let Some(at) = outcome.add_at {
                    self.palette_open = true;
                    self.palette_focus = true;
                    self.palette_position = at;
                    self.search.clear();
                }
            });
    }
    fn palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            return;
        }
        let mut open = true;
        let mut add = None;
        egui::Window::new("ノードを追加")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.)
            .anchor(egui::Align2::CENTER_CENTER, [0., -60.])
            .show(ctx, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("何をしたい？  円 / twist / 並べる")
                        .desired_width(f32::INFINITY),
                );
                if self.palette_focus {
                    r.request_focus();
                    self.palette_focus = false;
                }
                ui.add_space(7.);
                let matches: Vec<_> = NodeKind::all()
                    .iter()
                    .copied()
                    .filter(|k| matches_search(*k, &self.search))
                    .collect();
                // IME Enter belongs to the input method; selecting a result is always explicit.
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .show(ui, |ui| {
                        for kind in &matches {
                            let spec = kind.spec();
                            if ui
                                .add_sized(
                                    [410., 34.],
                                    egui::Button::new(
                                        RichText::new(format!(
                                            "{}     ·     {}",
                                            spec.label, spec.category
                                        ))
                                        .color(theme::category_color(spec.category)),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text(spec.description)
                                .clicked()
                            {
                                add = Some(*kind);
                            }
                        }
                        if matches.is_empty() {
                            ui.label(
                                "見つかりません。「移動」「花器」「数列」などを試してください。",
                            );
                        }
                    });
                ui.label(theme::small("クリックで追加  ·  Esc で閉じる"));
            });
        if let Some(kind) = add {
            self.add_node(kind, Some(self.palette_position));
        } else {
            self.palette_open = open && !ctx.input(|i| i.key_pressed(egui::Key::Escape));
        }
    }
    fn template_dialog(&mut self, ctx: &egui::Context) {
        if !self.template_open {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        egui::Window::new("作品例からはじめる")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(570.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.label("完成したグラフを開いて、ひとつ数値を変えてみよう。");
                ui.add_space(12.);
                for (index, t) in templates::TEMPLATES.iter().enumerate() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(240, 245, 237))
                        .corner_radius(8)
                        .inner_margin(14)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(t.name).strong().size(15.));
                                    ui.label(theme::small(t.description));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("開く →").clicked() {
                                        chosen = Some(index);
                                    }
                                });
                            });
                        });
                    ui.add_space(3.);
                }
            });
        self.template_open = open;
        if let Some(index) = chosen {
            self.template_open = false;
            self.request(PendingAction::Template(index), ctx);
        }
    }
    fn export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export_open {
            return;
        }
        let mut open = true;
        let mut format = None;
        egui::Window::new("形を書き出す").open(&mut open).collapsible(false).resizable(false).default_width(455.).anchor(egui::Align2::CENTER_CENTER,[0.,0.]).show(ctx,|ui|{
            ui.label("「出力」ノードの形状を、mm 単位で保存します。");ui.add_space(12.);
            if self.stale(){ui.colored_label(theme::ERROR,"形状を更新中です。更新完了後に開き直してください。");return;}
            match self.export_stats.as_ref(){
                Some(Ok(stats))=>{
                    egui::Grid::new("export_stats").num_columns(2).spacing([24.,10.]).show(ui,|ui|{
                        ui.label("メッシュ");ui.label(stats.mesh_count.to_string());ui.end_row();
                        ui.label("三角形");ui.label(stats.triangles.to_string());ui.end_row();
                        ui.label("開いた辺");ui.label(stats.open_edges.to_string());ui.end_row();
                        ui.label("非多様体の辺");ui.label(stats.non_manifold_edges.to_string());ui.end_row();
                    });
                    ui.add_space(10.);
                    if stats.is_closed() {ui.colored_label(theme::TEAL,"閉じたメッシュです（辺のつながりを確認）。");}
                    else{ui.colored_label(theme::ERROR,"開いた辺や面の向きに問題があります。書き出し後にスライサーで確認してください。");}
                    ui.label(theme::small("自己交差や造形可否の判定は含みません。"));
                    if stats.skipped_values > 0 { ui.label(theme::small(format!("数値・点・曲線の {} 要素は書き出しに含みません。", stats.skipped_values))); }
                    if stats.degenerate_triangles > 0 { ui.colored_label(theme::ERROR, format!("面が潰れています（{} 面）。形状を修正してください。", stats.degenerate_triangles)); }
                    ui.add_space(12.);
                    ui.horizontal(|ui|{
                        if ui.add_sized([195.,40.],egui::Button::new("STL · 3D プリント").fill(theme::TEAL_PALE)).clicked(){format=Some("stl");}
                        if ui.add_sized([195.,40.],egui::Button::new("OBJ · 3D ソフト")).clicked(){format=Some("obj");}
                    });
                },Some(Err(e))=>{ui.colored_label(theme::ERROR,e);ui.label("立体を作り、「出力」ノードにつないでください。");},None=>{ui.label("形状を確認できませんでした。");}
            }
        });
        self.export_open = open;
        if let Some(format) = format {
            self.file_picker = Some(FilePicker::new(
                if format == "stl" {
                    FileTask::ExportStl
                } else {
                    FileTask::ExportObj
                },
                self.start_directory(),
                format!("{}.{}", safe_name(&self.project.name), format),
            ));
            self.export_open = false;
        }
    }
    fn help_dialog(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Morphlet の使い方").open(&mut open).collapsible(false).default_width(570.).anchor(egui::Align2::CENTER_CENTER,[0.,0.]).show(ctx,|ui|{
            ui.heading("つなぐ。変える。形になる。");ui.add_space(10.);
            for (title,body) in [("1  作品例を開く","まずは「ねじれの花器」。ノードを選ぶと右下に設定が出ます。"),("2  数値を動かす","スライダーを動かすか、数値を直接入力。形がすぐに変わります。"),("3  処理を追加する","左で「移動」「円形配列」などを検索。選択した形状に自動でつながります。"),("4  線をつなぐ","右の丸から左の丸へドラッグ。または出力→入力の順にクリック。入力の丸を右クリックで切断。"),("5  保存して、つくる",".morph は編集できる作品ファイル。STL はスライサー用、OBJ は3Dソフト用です。")]{
                ui.label(RichText::new(title).strong());ui.label(body);ui.add_space(8.);
            }
            ui.separator();
            egui::Grid::new("shortcuts").num_columns(2).spacing([24.,6.]).show(ui,|ui|{
                for (a,b) in [("Ctrl+S / Ctrl+O","保存 / 開く"),("Ctrl+Z / Ctrl+Y","元に戻す / やり直す"),("Ctrl+D / Delete","選択ノードを複製 / 削除"),("Tab / Ctrl+K","ノードを検索"),("Shift+クリック / 範囲ドラッグ","複数のノードを選ぶ"),("F","グラフと形状を画面に収める"),("3D: 左ドラッグ / Shift+ドラッグ","回転 / 移動"),("ホイール","拡大・縮小")]{ui.label(RichText::new(a).size(11.).color(theme::TEAL));ui.label(RichText::new(b).size(11.));ui.end_row();}
            });
            ui.add_space(12.);ui.separator();ui.label(theme::small("Morphlet 0.1 · 独立したノード式メッシュモデラ\nこの版は NURBS / STEP / GH・GHX / Grasshopperプラグインには対応していません。"));
            ui.horizontal(|ui|{ui.label("表示サイズ");if ui.add(egui::Slider::new(&mut self.ui_scale,0.85..=1.35).step_by(0.05)).changed(){ctx.set_zoom_factor(self.ui_scale);}});
        });
        self.help_open = open;
    }
    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.confirm_action.clone() else {
            return;
        };
        egui::Modal::new(egui::Id::new("unsaved_changes")).show(ctx, |ui| {
            ui.set_width(390.);
            ui.heading("作品に未保存の変更があります");
            ui.add_space(9.);
            ui.label("変更を保存してから進みますか？");
            ui.add_space(14.);
            ui.horizontal(|ui| {
                if ui.button("保存して進む").clicked() {
                    self.confirm_action = None;
                    if self.save(false) {
                        self.perform(action.clone(), ctx);
                    } else if self.file_picker.is_some() {
                        self.after_save = Some(action.clone());
                    }
                }
                if ui.button("保存せず進む").clicked() {
                    self.confirm_action = None;
                    self.perform(action.clone(), ctx);
                }
                if ui.button("戻る").clicked() {
                    self.confirm_action = None;
                }
            });
        });
    }
    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(recovery) = self.recovery.clone() else {
            return;
        };
        egui::Modal::new(egui::Id::new("recovery")).show(ctx, |ui| {
            ui.set_width(410.);
            ui.heading("前回の作業が残っています");
            ui.label(format!("自動保存: {}", recovery.name));
            ui.add_space(12.);
            ui.horizontal(|ui| {
                if ui.button("続きから開く").clicked() {
                    self.project = recovery.clone();
                    self.recovery = None;
                    self.dirty = true;
                    self.suppress_history = true;
                    self.canvas.fit_requested = true;
                    self.queue_compute();
                }
                if ui.button("作品例から始める").clicked() {
                    self.recovery = None;
                }
            });
        });
    }
    fn save_recovery(&mut self) {
        if !self.dirty || self.last_autosave.elapsed() < Duration::from_secs(20) {
            return;
        }
        if let Some(path) = recovery_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = persistence::save(&path, &self.project);
        }
        self.last_autosave = Instant::now();
    }
}

impl eframe::App for MorphletApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.compute_tick(ctx);
        let before = self.project.clone();
        self.suppress_history = false;
        if ctx.input(|i| i.pointer.any_pressed()) && self.gesture_before.is_none() {
            self.gesture_before = Some(before.clone());
        }
        if self.confirm_action.is_none()
            && self.recovery.is_none()
            && !self.palette_open
            && !self.template_open
            && !self.help_open
            && !self.export_open
            && self.file_picker.is_none()
        {
            self.keyboard(ctx);
        }
        self.topbar(ctx);
        self.statusbar(ctx);
        self.library(ctx);
        self.right_panel(ctx);
        self.center_panel(ctx);
        self.palette(ctx);
        self.template_dialog(ctx);
        self.export_dialog(ctx);
        self.help_dialog(ctx);
        self.confirm_dialog(ctx);
        self.recovery_dialog(ctx);
        self.file_dialog(ctx);
        if !self.suppress_history && document_changed(&before, &self.project) {
            self.dirty = true;
            if self.gesture_before.is_none() {
                self.history.checkpoint(&before);
            }
            if !same_calculation(&before.graph, &self.project.graph) {
                self.queue_compute();
            }
        }
        if !ctx.input(|i| i.pointer.any_down())
            && let Some(start) = self.gesture_before.take()
            && !self.suppress_history
            && document_changed(&start, &self.project)
        {
            self.history.checkpoint(&start);
        }
        if let Some((text, error, start)) = &self.toast
            && start.elapsed() < Duration::from_secs(if *error { 9 } else { 4 })
        {
            egui::Area::new(egui::Id::new("toast"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0., -45.])
                .order(egui::Order::Tooltip)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(if *error {
                            Color32::from_rgb(252, 231, 224)
                        } else {
                            Color32::from_rgb(32, 66, 53)
                        })
                        .corner_radius(9)
                        .inner_margin(egui::Margin::symmetric(18, 12))
                        .show(ui, |ui| {
                            ui.set_max_width(650.);
                            ui.label(RichText::new(text).size(12.).color(if *error {
                                theme::ERROR
                            } else {
                                Color32::WHITE
                            }));
                        });
                });
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        for dropped in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(path) = dropped.path {
                self.request(PendingAction::Open(path), ctx);
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_action = Some(PendingAction::Exit);
        }
        self.save_recovery();
        if self.dirty {
            ctx.request_repaint_after(Duration::from_secs(20));
        }
        if let Some(path) = self.screenshot.clone() {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    match image::save_buffer(
                        &path,
                        &bytes,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        Ok(()) => eprintln!("Screenshot saved: {}", path.display()),
                        Err(e) => eprintln!("Screenshot failed: {e}"),
                    }
                    self.screenshot = None;
                }
            }
            if !self.screenshot_requested
                && !self.stale()
                && self.started.elapsed() > Duration::from_secs(3)
            {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn collect_export_trees(
    graph: &Graph,
    evaluation: &Evaluation,
    selection: &std::collections::BTreeSet<u64>,
) -> Result<Vec<Tree>, String> {
    if evaluation.cancelled {
        return Err("計算が中断されています。更新が終わってから書き出してください。".into());
    }
    let outputs: Vec<_> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Output)
        .collect();
    let nodes: Vec<_> = if outputs.is_empty() {
        graph
            .nodes
            .iter()
            .filter(|n| selection.contains(&n.id))
            .collect()
    } else {
        outputs
    };
    if nodes.is_empty() {
        return Err("書き出す形状を選ぶか、「出力」ノードにつないでください。".into());
    }
    nodes
        .into_iter()
        .map(|node| match evaluation.outputs.get(&node.id) {
            Some(Ok(tree)) => Ok(tree.clone()),
            Some(Err(error)) => Err(format!(
                "「{}」を計算できません: {}。出力をすべて確認してから書き出してください。",
                node.name, error
            )),
            None => Err(format!("「{}」の計算結果がまだありません。", node.name)),
        })
        .collect()
}

fn document_changed(a: &Project, b: &Project) -> bool {
    a.name != b.name || a.graph != b.graph
}
fn same_calculation(a: &Graph, b: &Graph) -> bool {
    a.nodes.len() == b.nodes.len()
        && a.nodes.iter().zip(&b.nodes).all(|(x, y)| {
            x.id == y.id && x.kind == y.kind && x.params == y.params && x.inputs == y.inputs
        })
}
fn matches_search(kind: NodeKind, query: &str) -> bool {
    let q = query.trim().to_lowercase().replace('　', " ");
    if q.is_empty() {
        return true;
    }
    let s = kind.spec();
    let hay = format!("{} {} {}", s.label, s.keywords, s.category).to_lowercase();
    q.split_whitespace().all(|word| {
        let alias = match word {
            "まる" | "えん" => "円",
            "はこ" => "箱",
            "しかく" => "長方形",
            "はなびん" | "かびん" => "花器",
            "ならべる" => "配列",
            "おしだし" => "押し出し",
            "らせん" => "らせん",
            _ => word,
        };
        hay.contains(alias)
    })
}
fn category_rank(category: &str) -> usize {
    ["立体", "曲線", "変形", "配列", "数値", "データ", "出力"]
        .iter()
        .position(|s| *s == category)
        .unwrap_or(99)
}
fn safe_name(name: &str) -> String {
    let result: String = name
        .chars()
        .filter(|c| !"/\\:*?\"<>|".contains(*c) && !c.is_control())
        .take(70)
        .collect();
    if result.trim().is_empty() {
        "Morphlet".into()
    } else {
        result
    }
}
fn recovery_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("Morphlet").join("recovery.morph"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .map(|p| p.join("morphlet/recovery.morph"))
    }
}
fn value_summary(value: &Value) -> String {
    match value {
        Value::Number(v) => format!("{v:.4}"),
        Value::Point(p) => format!("点  ({:.2}, {:.2}, {:.2})", p[0], p[1], p[2]),
        Value::Curve(c) => format!(
            "曲線 · {} 点 · {}",
            c.points.len(),
            if c.closed { "閉じた" } else { "開いた" }
        ),
        Value::Mesh(m) => format!(
            "メッシュ · {} 頂点 / {} 面",
            m.vertices.len(),
            m.triangles.len()
        ),
    }
}
fn friendly_range(key: &str, min: f64, max: f64, value: f64) -> (f64, f64) {
    let (lo, hi) = match key {
        "radius" | "inner_radius" | "major" | "minor" => (min, 120.),
        "height" => (min, 240.),
        "width" | "depth" => (min, 180.),
        "thickness" => (min, 12.),
        "angle" => (min.max(-360.), 360.),
        "value" | "a" | "b" | "base" | "amp" | "start" | "end" => (min.max(-100.), 100.),
        "exponent" | "frequency" => (min.max(-10.), 10.),
        "sx" | "sy" | "sz" => (min, 5.),
        "divisions" => (min, 32.),
        "x" | "y" | "z" => (min.max(-200.), 200.),
        "count" => (min, 32.),
        "segments" => (min, 128.),
        _ => (min, max),
    };
    (lo.min(value).max(min), hi.max(value).min(max))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn japanese_and_english_search_find_real_nodes() {
        assert!(matches_search(NodeKind::Circle, "円"));
        assert!(matches_search(NodeKind::Twist, "twist"));
        assert!(matches_search(NodeKind::LinearArray, "並べる"));
        assert!(!matches_search(NodeKind::Circle, "box"));
    }
    #[test]
    fn layout_changes_do_not_trigger_geometry() {
        let a = templates::create(0);
        let mut b = a.clone();
        b.graph.nodes[0].position[0] += 20.;
        b.graph.nodes[0].name = "テスト".into();
        assert!(same_calculation(&a.graph, &b.graph));
        b.graph.nodes[0].params.insert("radius".into(), 40.);
        assert!(!same_calculation(&a.graph, &b.graph));
    }
    #[test]
    fn file_name_is_path_safe() {
        assert_eq!(safe_name("a/b:c*"), "abc");
        assert_eq!(safe_name(""), "Morphlet");
    }
    #[test]
    fn export_never_silently_drops_a_failed_output() {
        let mut p = templates::create(0);
        p.graph.add(NodeKind::Output, [0., 0.]).unwrap();
        let evaluation = Evaluator::default().evaluate(&p.graph, &|| false);
        let error = collect_export_trees(&p.graph, &evaluation, &Default::default()).unwrap_err();
        assert!(error.contains("出力をすべて確認"));
    }
    #[test]
    fn unrelated_errors_do_not_block_valid_export_outputs() {
        let mut p = templates::create(0);
        let id = p.graph.add(NodeKind::Divide, [0., 0.]).unwrap();
        p.graph.node_mut(id).unwrap().params.insert("b".into(), 0.);
        let evaluation = Evaluator::default().evaluate(&p.graph, &|| false);
        assert!(evaluation.outputs[&id].is_err());
        let trees = collect_export_trees(&p.graph, &evaluation, &Default::default()).unwrap();
        assert_eq!(persistence::mesh_stats(&trees).unwrap().mesh_count, 1);
    }
}
