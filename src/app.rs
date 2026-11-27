use crate::file_picker::{FilePicker, FileTask, PickerEvent};
use crate::stl_export_ui::{self, StlExportUi, StlSources, UiRequest};
use crate::{
    canvas::GraphCanvas,
    data_view,
    gcode_ui::GcodeDialog,
    help::HelpWindow,
    node_icons, node_shelf::{self, ShelfAction},
    theme,
    ui_polish::{self, Recent},
    viewport::{PreviewItem, Viewport},
};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Stroke};
use morphlet::{
    annotations, cluster,
    engine::{Evaluation, Evaluator},
    gcode,
    latest_channel::{self, LatestReceiver, LatestSender},
    model::{DataType, Graph, InputSpec, Node, NodeId, NodeKind, Tree},
    persistence::{self, History, Project},
    templates,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
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
    sender: LatestSender<ComputeRequest>,
    receiver: LatestReceiver<ComputeResult>,
    latest: Arc<AtomicU64>,
}
impl Worker {
    fn new(ctx: egui::Context) -> Self {
        let (sender, requests) = latest_channel::channel::<ComputeRequest>();
        let (results, receiver) = latest_channel::channel::<ComputeResult>();
        let latest = Arc::new(AtomicU64::new(0));
        let revision = latest.clone();
        std::thread::spawn(move || {
            let mut evaluator = Evaluator::default();
            while let Ok(request) = requests.recv() {
                if revision.load(Ordering::Relaxed) != request.revision {
                    continue;
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
impl Drop for Worker {
    fn drop(&mut self) {
        // Cooperatively cancel the active evaluation when the UI is destroyed.
        self.latest.fetch_add(1, Ordering::Relaxed);
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
    palette_query: String,
    palette_ime: bool,
    category: String,
    library_filter: u8,
    favorites: BTreeSet<NodeKind>,
    show_library: bool,
    show_workspace: bool,
    clipboard: Vec<Node>,
    palette_open: bool,
    palette_focus: bool,
    palette_position: [f32; 2],
    template_open: bool,
    help_open: bool,
    /// Advanced: G-code (vase mode) from the その他の書き出し menu.
    gcode: GcodeDialog,
    stl_export: StlExportUi,
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
    /// Cluster definition waiting for a `.morphpart` file name.
    part_target: Option<u64>,
    /// Offline node dictionary (F1 / ?).
    help_window: HelpWindow,
    /// 値を見る window for the selected node.
    values_open: bool,
    /// ファイル読み込み node that the next picked mesh file is loaded into.
    import_target: Option<NodeId>,
    /// Node whose output wire was dropped on empty canvas; the search connects to it.
    wire_source: Option<NodeId>,
    /// Nodes added recently, shown first in the search popup.
    recent: Recent,
    /// Highlighted row of the search popup (↑ ↓ move it, Enter adds it).
    palette_cursor: usize,
    /// ショートカット一覧 window (?).
    shortcuts_open: bool,
    /// Dark theme is on (saved in the settings file).
    dark: bool,
    /// Node shelf: tabs, favourites, and the drag / Space state.
    shelf: node_shelf::Shelf,
    /// Favourites and recent nodes as last written to the settings file.
    saved_lists: (Vec<NodeKind>, Vec<NodeKind>),
}

impl MorphletApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        open_file: Option<PathBuf>,
        screenshot: Option<PathBuf>,
    ) -> Self {
        let dark = ui_polish::load_dark();
        theme::configure(&cc.egui_ctx, dark);
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
        let (mut favorites, saved_recent) = node_shelf::load_lists();
        let loaded_favorites = ui_polish::load_favorites();
        for kind in &loaded_favorites {
            if !favorites.contains(kind) {
                favorites.push(*kind);
            }
        }
        let recent = node_shelf::recent_from_saved(&saved_recent);
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
            palette_query: String::new(),
            palette_ime: false,
            library_filter: 0,
            favorites: favorites.iter().copied().collect(),
            show_library: true,
            show_workspace: true,
            clipboard: Vec::new(),
            category: "すべて".into(),
            palette_open: false,
            palette_focus: false,
            palette_position: [0., 0.],
            template_open: false,
            help_open: false,
            gcode: GcodeDialog::default(),
            stl_export: StlExportUi::new(),
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
            part_target: None,
            help_window: HelpWindow::default(),
            values_open: false,
            import_target: None,
            wire_source: None,
            recent,
            palette_cursor: 0,
            shortcuts_open: false,
            dark,
            shelf: node_shelf::Shelf::new(favorites.clone()),
            saved_lists: (favorites, saved_recent),
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
            self.clipboard.clear();
            self.wire_source = None;
            self.palette_open = false;
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
                    self.import_target = None;
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
                    FileTask::SavePart => {
                        let result = self
                            .part_target
                            .take()
                            .ok_or_else(|| "書き出す部品が選ばれていません。".to_string())
                            .and_then(|id| cluster::save_part(&path, &self.project.graph, id));
                        match result {
                            Ok(()) => self.notify("部品を書き出しました", false),
                            Err(e) => self.notify(e, true),
                        }
                    }
                    FileTask::LoadPart => {
                        match cluster::load_part(&path, &mut self.project.graph) {
                            Ok(def_id) => {
                                let at = self.canvas.center_for_add(&self.project);
                                match cluster::instantiate(&mut self.project.graph, def_id, at) {
                                    Ok(id) => {
                                        self.canvas.select_only(id);
                                        self.notify("部品を読み込みました", false);
                                    }
                                    Err(e) => self.notify(e, true),
                                }
                            }
                            Err(e) => self.notify(e, true),
                        }
                    }
                    FileTask::ImportMesh => {
                        let target = self.import_target.take();
                        self.set_import_file(&path, target, None);
                    }
                    FileTask::ExportGcode => {
                        let settings = self.gcode.settings().clone();
                        let result = self
                            .result_trees()
                            .and_then(|trees| gcode::export(&path, &trees, &settings));
                        match result {
                            Ok(r) => self.notify(
                                format!(
                                    "G-code を書き出しました（約 {:.0} 分・{:.1} m）",
                                    r.print_seconds / 60.0,
                                    r.filament_mm / 1000.0
                                ),
                                false,
                            ),
                            Err(e) => self.notify(e, true),
                        }
                    }
                    FileTask::ExportStl | FileTask::ExportObj | FileTask::ExportThreeMf => {
                        let sources = self.stl_sources();
                        self.stl_export.export_to(&sources, &path);
                        self.flush_stl_notice();
                    }
                    FileTask::PickSlicer => {
                        self.stl_export.set_slicer(path);
                        self.flush_stl_notice();
                    }
                },
            }
        }
    }
    /// Points an ファイル読み込み node at a mesh file. Without a valid target, a new node is
    /// added at `at` (or the canvas centre).
    fn set_import_file(&mut self, path: &Path, target: Option<NodeId>, at: Option<[f32; 2]>) {
        let Some(text) = path.to_str().map(str::to_owned) else {
            self.notify(
                "ファイル名を文字として読めませんでした。英数字の名前に変えてから選び直してください。",
                true,
            );
            return;
        };
        if text.len() > morphlet::script::MAX_SCRIPT_BYTES {
            self.notify(
                "ファイルのパスが長すぎます。フォルダーを近くへ移すか、名前を短くしてください。",
                true,
            );
            return;
        }
        let valid_target = target.filter(|id| {
            self.project
                .graph
                .node(*id)
                .is_some_and(|n| n.kind == NodeKind::ImportMesh)
        });
        let id = match valid_target {
            Some(id) => id,
            None => {
                let at = at.unwrap_or_else(|| self.canvas.center_for_add(&self.project));
                match self.project.graph.add(NodeKind::ImportMesh, at) {
                    Ok(id) => {
                        self.canvas.select_only(id);
                        id
                    }
                    Err(e) => {
                        self.notify(e, true);
                        return;
                    }
                }
            }
        };
        if let Some(node) = self.project.graph.node_mut(id) {
            node.script = text;
        }
        self.notify("「ファイル読み込み」にファイルを設定しました", false);
    }

    fn add_node(&mut self, kind: NodeKind, position: Option<[f32; 2]>) {
        // A wire dropped on the canvas takes precedence over the selection.
        let wire = self.wire_source.take();
        let selected = self.canvas.selected.iter().next_back().copied();
        let at = position.unwrap_or_else(|| {
            selected
                .and_then(|id| self.project.graph.node(id))
                .map(|n| [n.position[0] + 280., n.position[1] + 40.])
                .unwrap_or_else(|| self.canvas.center_for_add(&self.project))
        });
        match self.project.graph.add(kind, at) {
            Ok(id) => {
                if let Some(source) = wire {
                    let output = self
                        .project
                        .graph
                        .node(source)
                        .map(|n| n.kind.spec().output);
                    match output.and_then(|t| ui_polish::first_compatible_input(kind, t)) {
                        Some(key) => {
                            if let Err(e) = self.project.graph.connect(source, id, key) {
                                // Keep a wire-drop operation transactional: never leave a half-added node.
                                self.project.graph.remove(id);
                                self.notify(e, true);
                                self.palette_open = false;
                                return;
                            }
                        }
                        None => {
                            self.project.graph.remove(id);
                            self.notify(
                                "接続できる入力がありません。別のノードを選んでください。",
                                true,
                            );
                            self.palette_open = false;
                            return;
                        }
                    }
                } else if let Some(source) = selected {
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
                self.recent.push(kind);
                self.canvas.select_only(id);
                self.palette_open = false;
                self.palette_query.clear();
                self.notify(format!("「{}」を追加しました", kind.spec().label), false);
            }
            Err(e) => self.notify(e, true),
        }
    }
    /// Node shelf strip above the canvas (see `node_shelf.rs`).
    fn shelf_panel(&mut self, ctx: &egui::Context) {
        let selected = if self.canvas.selected.len() == 1 {
            self.canvas.selected.iter().next().copied()
        } else {
            None
        };
        let selected =
            selected.and_then(|id| self.project.graph.node(id).map(|n| (id, n.kind.spec().output)));
        let Some(action) = self
            .shelf
            .show(ctx, self.recent.kinds(), selected, self.canvas.last_rect)
        else {
            return;
        };
        match action {
            ShelfAction::Add(kind) => match selected {
                Some((source, output)) if node_shelf::first_input_accepts(kind, output) => {
                    self.add_suggestion(source, kind)
                }
                _ => self.add_node(kind, None),
            },
            ShelfAction::Suggest(source, kind) => self.add_suggestion(source, kind),
            ShelfAction::Place(kind, pos) => {
                if let Some(at) = self.canvas.world_at(&self.project, pos) {
                    self.add_node_at(kind, at);
                }
            }
        }
    }
    /// Adds a node after `source` and connects it to the first input (shelf and suggestions).
    fn add_suggestion(&mut self, source: NodeId, kind: NodeKind) {
        match node_shelf::add_suggested(&mut self.project.graph, source, kind) {
            Ok(id) => {
                self.recent.push(kind);
                self.canvas.select_only(id);
                self.notify(format!("「{}」を追加して線をつなぎました", kind.spec().label), false);
            }
            Err(e) => self.notify(e, true),
        }
    }
    /// Adds a dropped node exactly where it was released, without connecting it.
    fn add_node_at(&mut self, kind: NodeKind, at: [f32; 2]) {
        match self.project.graph.add(kind, at) {
            Ok(id) => {
                self.recent.push(kind);
                self.canvas.select_only(id);
                self.notify(format!("「{}」を配置しました", kind.spec().label), false);
            }
            Err(e) => self.notify(e, true),
        }
    }
    /// Where a keyboard-opened search puts its node: under the pointer on the canvas, else the centre.
    fn palette_anchor(&self, ctx: &egui::Context) -> [f32; 2] {
        node_shelf::pointer_in(ctx, self.canvas.last_rect)
            .and_then(|p| self.canvas.world_at(&self.project, p))
            .unwrap_or_else(|| self.canvas.center_for_add(&self.project))
    }
    /// Writes favourites and recent nodes to the settings file when they change.
    fn persist_shelf_lists(&mut self) {
        let now = (self.shelf.favorites().to_vec(), self.recent.kinds().to_vec());
        if now != self.saved_lists {
            if now.0 != self.saved_lists.0 {
                let set: BTreeSet<NodeKind> = now.0.iter().copied().collect();
                if set != self.favorites {
                    self.favorites = set;
                    if let Err(e) = ui_polish::save_favorites(&self.favorites) {
                        self.notify(e, true);
                    }
                }
            }
            if let Err(e) = node_shelf::save_lists(&now.0, &now.1) {
                self.notify(e, true);
            }
            self.saved_lists = now;
        }
    }
    fn group_selected(&mut self) {
        let ids: Vec<_> = self.canvas.selected.iter().copied().collect();
        let name = format!("部品 {}", self.project.graph.clusters.len() + 1);
        let result = cluster::group_nodes(&mut self.project.graph, &ids, &name);
        annotations::prune(&mut self.project.graph);
        match result {
            Ok(id) => {
                self.canvas.select_only(id);
                self.notify("選択したノードを 1 つの部品にまとめました", false);
            }
            Err(e) => self.notify(e, true),
        }
    }
    fn part_action(&mut self, action: crate::script_ui::PartAction) {
        use crate::script_ui::PartAction;
        match action {
            PartAction::Add(def_id) => {
                let at = self.canvas.center_for_add(&self.project);
                match cluster::instantiate(&mut self.project.graph, def_id, at) {
                    Ok(id) => self.canvas.select_only(id),
                    Err(e) => self.notify(e, true),
                }
            }
            PartAction::Export(def_id) => {
                let name = self
                    .project
                    .graph
                    .clusters
                    .iter()
                    .find(|d| d.id == def_id)
                    .map(|d| safe_name(&d.name))
                    .unwrap_or_else(|| "部品".into());
                self.part_target = Some(def_id);
                self.file_picker = Some(FilePicker::new(
                    FileTask::SavePart,
                    self.start_directory(),
                    format!("{name}.morphpart"),
                ));
            }
            PartAction::Import => {
                self.file_picker = Some(FilePicker::new(
                    FileTask::LoadPart,
                    self.start_directory(),
                    String::new(),
                ));
            }
        }
    }
    fn delete_selected(&mut self) {
        let ids: Vec<_> = self.canvas.selected.iter().copied().collect();
        for id in ids {
            self.project.graph.remove(id);
        }
        annotations::prune(&mut self.project.graph);
        self.canvas.selected.clear();
        self.canvas.connecting = None;
    }
    fn selected_nodes(&self) -> Vec<Node> {
        self.project
            .graph
            .nodes
            .iter()
            .filter(|n| self.canvas.selected.contains(&n.id))
            .cloned()
            .collect()
    }
    fn copy_selected(&mut self) {
        let nodes = self.selected_nodes();
        if !nodes.is_empty() {
            self.clipboard = nodes;
            self.notify(
                format!("{} ノードをコピーしました", self.clipboard.len()),
                false,
            );
        }
    }
    fn paste_selected(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let at = self.canvas.center_for_add(&self.project);
        let min_x = self
            .clipboard
            .iter()
            .map(|n| n.position[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = self
            .clipboard
            .iter()
            .map(|n| n.position[1])
            .fold(f32::INFINITY, f32::min);
        match insert_nodes(
            &mut self.project.graph,
            &self.clipboard,
            [at[0] - min_x, at[1] - min_y],
            false,
        ) {
            Ok(ids) => {
                self.canvas.selected = ids;
                self.notify(
                    format!("{} ノードを貼り付けました", self.canvas.selected.len()),
                    false,
                );
            }
            Err(e) => self.notify(e, true),
        }
    }
    fn duplicate_selected(&mut self) {
        let nodes = self.selected_nodes();
        if nodes.is_empty() {
            return;
        }
        match insert_nodes(&mut self.project.graph, &nodes, [35., 40.], true) {
            Ok(ids) => self.canvas.selected = ids,
            Err(e) => self.notify(e, true),
        }
    }
    fn open_palette(&mut self, at: [f32; 2], source: Option<NodeId>) {
        self.wire_source = source;
        self.palette_open = true;
        self.palette_focus = true;
        self.palette_position = at;
        self.palette_query.clear();
        self.palette_ime = false;
        self.palette_cursor = 0;
    }
    fn toggle_favorite(&mut self, kind: NodeKind) {
        if !self.favorites.remove(&kind) {
            self.favorites.insert(kind);
        }
        if let Err(e) = ui_polish::save_favorites(&self.favorites) {
            self.notify(e, true);
        }
        if self.shelf.is_favorite(kind) != self.favorites.contains(&kind) {
            self.shelf.toggle(kind);
        }
    }
    /// Toggle 無効 on the selection: if any selected node is on, turn all off; else on.
    fn toggle_off_selected(&mut self) {
        let ids: Vec<_> = self.canvas.selected.iter().copied().collect();
        if ids.is_empty() {
            self.notify("無効にするノードを選んでください", true);
            return;
        }
        let turn_off = ids
            .iter()
            .any(|id| self.project.graph.node(*id).is_some_and(|n| !n.off));
        for id in &ids {
            if let Some(n) = self.project.graph.node_mut(*id) {
                n.off = turn_off;
            }
        }
        self.notify(
            if turn_off {
                "選択したノードを無効にしました（出力なし）· Ctrl+E で戻せます"
            } else {
                "選択したノードを有効に戻しました"
            },
            false,
        );
    }
    fn make_group(&mut self) {
        let ids: Vec<_> = self.canvas.selected.iter().copied().collect();
        let name = format!("グループ {}", self.project.graph.groups.len() + 1);
        match annotations::add_group(&mut self.project.graph, &ids, &name) {
            Ok(_) => self.notify(
                "グループを作りました。見出しを右クリックで名前・色を変更できます",
                false,
            ),
            Err(e) => self.notify(e, true),
        }
    }
    fn add_note(&mut self, at: [f32; 2]) {
        match annotations::add_note(&mut self.project.graph, "", at) {
            Ok(index) => self.canvas.edit_note(index),
            Err(e) => self.notify(e, true),
        }
    }
    fn open_help(&mut self) {
        match self
            .canvas
            .selected
            .iter()
            .next_back()
            .and_then(|id| self.project.graph.node(*id))
        {
            Some(n) => self.help_window.open_for(n.kind),
            None => self.help_window.open_index(),
        }
    }
    fn toggle_snap(&mut self) {
        self.canvas.snap = !self.canvas.snap;
        let text = if self.canvas.snap {
            "格子に吸着: ON（ノードは離したときに格子点へ揃います）"
        } else {
            "格子に吸着: OFF"
        };
        self.notify(text, false);
    }
    fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.dark = !self.dark;
        theme::set_dark(self.dark);
        theme::apply_style(ctx);
        if let Err(e) = ui_polish::save_dark(self.dark) {
            self.notify(e, true);
        }
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
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::C)) {
                self.copy_selected();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::V)) {
                self.paste_selected();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::E)) {
                self.toggle_off_selected();
            }
            // Checked before Ctrl+G: a pattern without Shift also matches Ctrl+Shift+G.
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::G,
                )
            }) {
                self.toggle_snap();
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::G)) {
                self.make_group();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
                self.canvas.fit_requested = true;
            }
            let question = ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Text(t) if t == "?" || t == "？"))
            });
            if question {
                self.shortcuts_open = !self.shortcuts_open;
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                self.delete_selected();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
                self.canvas.selected = self.project.graph.nodes.iter().map(|n| n.id).collect();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F)) {
                self.canvas.fit_selection_requested = true;
            } else if ctx.input(|i| i.key_pressed(egui::Key::F)) {
                let items = self.previews();
                self.viewport.fit(&items);
                self.canvas.fit_requested = true;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Tab))
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K))
            {
                self.open_palette(self.palette_anchor(ctx), None);
            }
            // Space tapped over the canvas (Space + drag still pans; see node_shelf.rs).
            if self.shelf.space_opens_search(ctx, self.canvas.last_rect) {
                self.open_palette(self.palette_anchor(ctx), None);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            if self.help_window.open {
                self.help_window.open = false;
            } else {
                self.open_help();
            }
        }
    }
    fn topbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("topbar")
            .frame(
                egui::Frame::new()
                    .fill(theme::toolbar_bg())
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(22., 22.), Sense::hover());
                    let a = r.left_bottom() + egui::vec2(3., -4.);
                    let b = r.center_top() + egui::vec2(0., 4.);
                    let c = r.right_bottom() - egui::vec2(3., 4.);
                    for pair in [[a, b], [b, c], [a, c]] {
                        ui.painter()
                            .line_segment(pair, Stroke::new(1.8_f32, theme::teal()));
                    }
                    for p in [a, b, c] {
                        ui.painter().circle_filled(p, 2.6, theme::teal());
                    }
                    ui.label(
                        RichText::new("NODES")
                            .strong()
                            .size(18.)
                            .color(theme::ink()),
                    );
                    ui.separator();
                    ui.menu_button("ファイル", |ui| {
                        if ui.button("新規作品").clicked() {
                            self.request(PendingAction::New, ctx);
                            ui.close();
                        }
                        if ui.button("開く  Ctrl+O").clicked() {
                            self.open_dialog(ctx);
                            ui.close();
                        }
                        if ui.button("保存  Ctrl+S").clicked() {
                            self.save(false);
                            ui.close();
                        }
                        if ui.button("名前を付けて保存").clicked() {
                            self.save(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("STL / OBJ 書き出し").clicked() {
                            self.stl_export.open_dialog();
                            ui.close();
                        }
                        if ui.button("G-code（花瓶モード・未検証）").clicked() {
                            self.gcode.open();
                            ui.close();
                        }
                    });
                    ui.menu_button("編集", |ui| {
                        if ui
                            .add_enabled(
                                self.history.can_undo(),
                                egui::Button::new("元に戻す  Ctrl+Z"),
                            )
                            .clicked()
                        {
                            self.undo();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.history.can_redo(),
                                egui::Button::new("やり直す  Ctrl+Y"),
                            )
                            .clicked()
                        {
                            self.redo();
                            ui.close();
                        }
                        ui.separator();
                        let selected = !self.canvas.selected.is_empty();
                        if ui
                            .add_enabled(selected, egui::Button::new("コピー  Ctrl+C"))
                            .clicked()
                        {
                            self.copy_selected();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                !self.clipboard.is_empty(),
                                egui::Button::new("貼り付け  Ctrl+V"),
                            )
                            .clicked()
                        {
                            self.paste_selected();
                            ui.close();
                        }
                        if ui
                            .add_enabled(selected, egui::Button::new("複製  Ctrl+D"))
                            .clicked()
                        {
                            self.duplicate_selected();
                            ui.close();
                        }
                        if ui
                            .add_enabled(selected, egui::Button::new("グループ  Ctrl+G"))
                            .clicked()
                        {
                            self.make_group();
                            ui.close();
                        }
                        if ui
                            .add_enabled(selected, egui::Button::new("削除  Delete"))
                            .clicked()
                        {
                            self.delete_selected();
                            ui.close();
                        }
                    });
                    ui.menu_button("表示", |ui| {
                        ui.checkbox(&mut self.show_library, "ノードライブラリ");
                        ui.checkbox(&mut self.show_workspace, "3D と設定");
                        if ui.button("選択にズーム  Shift+F").clicked() {
                            self.canvas.fit_selection_requested = true;
                            ui.close();
                        }
                        if ui.button("全体表示  F").clicked() {
                            self.canvas.fit_requested = true;
                            let items = self.previews();
                            self.viewport.fit(&items);
                            ui.close();
                        }
                        if ui
                            .button(if self.dark {
                                "ライトテーマ"
                            } else {
                                "ダークテーマ"
                            })
                            .clicked()
                        {
                            self.toggle_theme(ctx);
                            ui.close();
                        }
                    });
                    ui.menu_button("作例・ヘルプ", |ui| {
                        if ui.button("作品例").clicked() {
                            self.template_open = true;
                            ui.close();
                        }
                        if ui.button("ノード辞典  F1").clicked() {
                            self.help_window.open_index();
                            ui.close();
                        }
                        if ui.button("使い方").clicked() {
                            self.help_open = true;
                            ui.close();
                        }
                        if ui.button("キー操作一覧").clicked() {
                            self.shortcuts_open = true;
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui
                        .small_button("＋ ノード")
                        .on_hover_text("Tab / Ctrl+K")
                        .clicked()
                    {
                        self.open_palette(self.canvas.center_for_add(&self.project), None);
                    }
                    if ui.small_button("保存").on_hover_text("Ctrl+S").clicked() {
                        self.save(false);
                    }
                    if ui.button("書き出し").on_hover_text("STL / OBJ").clicked() {
                        self.stl_export.open_dialog();
                    }
                    let name_width = ui.available_width().min(170.);
                    if name_width >= 100. {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.project.name)
                                .desired_width(name_width)
                                .frame(false),
                        )
                        .on_hover_text("作品名を編集");
                    } else {
                        ui.menu_button("作品名", |ui| {
                            ui.text_edit_singleline(&mut self.project.name);
                        });
                    }
                    if self.dirty {
                        ui.label(theme::small("未保存"));
                    }
                });
            });
    }
    fn library(&mut self, ctx: &egui::Context) {
        let width = ctx.available_rect().width();
        let maximum = (width * 0.33).clamp(140., 290.);
        egui::SidePanel::left("library")
            .default_width(205_f32.min(maximum))
            .min_width(140.)
            .max_width(maximum)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel_bg())
                    .inner_margin(egui::Margin::symmetric(9, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ライブラリ").strong().size(13.));
                    ui.label(theme::small(format!("{}", NodeKind::all().len())));
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("何を作る？")
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("日本語・英語で検索。例: 円を作る、Construct Point");
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.library_filter, 0, "全て");
                    ui.selectable_value(&mut self.library_filter, 1, "★");
                    ui.selectable_value(&mut self.library_filter, 2, "最近");
                });
                let mut categories: Vec<_> =
                    NodeKind::all().iter().map(|k| k.spec().category).collect();
                categories.sort_by_key(|c| (category_rank(c), *c));
                categories.dedup();
                egui::ComboBox::from_id_salt("category")
                    .selected_text(&self.category)
                    .width((ui.available_width() - 12.).max(70.))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.category, "すべて".into(), "すべて");
                        for category in categories {
                            ui.selectable_value(&mut self.category, category.into(), category);
                        }
                    });
                let mut base = ordered_kinds();
                if self.library_filter == 1 {
                    base.retain(|k| self.favorites.contains(k));
                }
                if self.library_filter == 2 {
                    base.retain(|k| self.recent.kinds().contains(k));
                }
                base.retain(|k| self.category == "すべて" || self.category == k.spec().category);
                let ordered = if self.search.trim().is_empty() && self.library_filter != 2 {
                    base
                } else {
                    ui_polish::rank(&base, &self.search, &self.recent, None)
                };
                let mut add = None;
                let mut favorite = None;
                ui.label(theme::small(format!("{} 件", ordered.len())));
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut last = "";
                        for kind in ordered {
                            let spec = kind.spec();
                            if self.search.trim().is_empty()
                                && self.library_filter != 2
                                && last != spec.category
                            {
                                ui.add_space(5.);
                                ui.label(theme::small(spec.category));
                                last = spec.category;
                            }
                            ui.horizontal(|ui| {
                                let starred = self.favorites.contains(&kind);
                                if ui
                                    .small_button(if starred { "★" } else { "☆" })
                                    .on_hover_text(if starred {
                                        "お気に入りから外す"
                                    } else {
                                        "お気に入りに追加"
                                    })
                                    .clicked()
                                {
                                    favorite = Some(kind);
                                }
                                let (r, _) =
                                    ui.allocate_exact_size(egui::vec2(6., 17.), Sense::hover());
                                ui.painter().rect_filled(
                                    r,
                                    2,
                                    theme::category_color(spec.category),
                                );
                                if ui
                                    .add(
                                        egui::Button::new(ui_polish::one_line(spec.label, 10))
                                            .frame(false),
                                    )
                                    .on_hover_text(format!("{}\n{}", spec.label, spec.description))
                                    .clicked()
                                {
                                    add = Some(kind);
                                }
                            });
                        }
                        if let Some(action) =
                            crate::script_ui::library_parts(ui, &self.project.graph)
                        {
                            self.part_action(action);
                        }
                        ui.separator();
                        if ui.button("作品例").clicked() {
                            self.template_open = true;
                        }
                        if ui.small_button("新規作品").clicked() {
                            self.request(PendingAction::New, ctx);
                        }
                    });
                if let Some(kind) = favorite {
                    self.toggle_favorite(kind);
                }
                if let Some(kind) = add
                    && !self.palette_open
                {
                    self.add_node(kind, None);
                }
            });
    }
    fn statusbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(30.)
            .frame(
                egui::Frame::new()
                    .fill(theme::status_bg())
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
                            ui.label(RichText::new("●").size(9.).color(theme::teal()));
                            ui.label(theme::small("更新済み"));
                        } else {
                            ui.label(
                                RichText::new(format!("!  {} 個の入力を確認", errors))
                                    .size(11.)
                                    .color(theme::error()),
                            );
                            if let Some(line) = self.error_line(e) {
                                let short = ui_polish::one_line(
                                    &line,
                                    if ui.available_width() > 700. { 42 } else { 12 },
                                );
                                ui.label(RichText::new(short).size(11.).color(theme::error()))
                                    .on_hover_text(line);
                            }
                        }
                        if ui.available_width() > 460. {
                            ui.label(theme::small(format!(
                                "{:.1} ms · 計算 {} / 再利用 {}",
                                e.elapsed_ms, e.computed, e.cached
                            )));
                        } else {
                            ui.label(theme::small(format!("{:.1} ms", e.elapsed_ms)));
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.available_width() > 260. {
                            ui.label(theme::small("mm / °"));
                            ui.separator();
                            ui.label(theme::small("オフライン"));
                            ui.separator();
                        }
                        ui.label(theme::small(format!(
                            "{} ノード",
                            self.project.graph.nodes.len()
                        )));
                    });
                });
            });
    }
    /// File line and the picker button for an ファイル読み込み node.
    fn import_panel(&mut self, ui: &mut egui::Ui, id: NodeId, file: &str) {
        ui.add_space(6.);
        ui.label(RichText::new("ファイル").strong());
        let shown = if file.is_empty() {
            "未選択".to_string()
        } else {
            Path::new(file)
                .file_name()
                .map_or_else(|| file.to_string(), |n| n.to_string_lossy().into_owned())
        };
        ui.label(theme::small(shown))
            .on_hover_text(if file.is_empty() {
                "STL または OBJ を選んでください"
            } else {
                file
            });
        if ui
            .button("ファイルを選ぶ")
            .on_hover_text("STL または OBJ を選びます。ウィンドウへのドラッグでも読み込めます")
            .clicked()
        {
            self.import_target = Some(id);
            self.file_picker = Some(FilePicker::new(
                FileTask::ImportMesh,
                self.start_directory(),
                String::new(),
            ));
        }
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
                if ui.small_button("?").on_hover_text("ヘルプ · F1").clicked() {
                    self.help_window.open_for(node.kind);
                }
            });
        });
        ui.add_space(3.);
        ui.label(theme::small(spec.description));
        if let Some(timing) = self
            .evaluation
            .as_ref()
            .and_then(|e| e.node_timings.get(&id))
        {
            let state = if self.stale() {
                "前回の評価"
            } else if node.off {
                "無効"
            } else if timing.cached {
                "キャッシュ"
            } else {
                "計算"
            };
            ui.label(theme::small(format!("{state} · {:.3} ms", timing.elapsed_ms)))
                .on_hover_text("このノードの入力準備・計算またはキャッシュ取得・出力検査にかかった時間です。クラスターは内部の処理を含みます。");
        }
        if node.off {
            ui.colored_label(
                theme::error(),
                "このノードは無効です（出力なし）。Ctrl+E または下の設定で戻せます。",
            );
        }
        if let Some(Err(message)) = self.evaluation.as_ref().and_then(|e| e.outputs.get(&id)) {
            egui::Frame::new()
                .fill(theme::error_fill())
                .corner_radius(6)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.label(RichText::new(message).size(12.).color(theme::error()));
                });
        }
        ui.add_space(9.);
        let mut disconnect = None;
        let shown = cluster::real_input_count(&self.project.graph, &node);
        for (index, input) in spec.inputs.iter().enumerate().take(shown) {
            let linked = node.inputs.get(input.key).copied();
            let label = cluster::input_label(&self.project.graph, &node, index, input);
            let needs_link = cluster::input_needs_link(&self.project.graph, &node, index, input);
            let optional = cluster::input_is_optional(&node, input);
            // Stable widget IDs preserve text editing even when an error appears
            // above this row while the user is still typing a multi-digit value.
            ui.scope_builder(
                egui::UiBuilder::new().id(("inspector_parameter", id, input.key)),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [100., 23.],
                            egui::Label::new(RichText::new(label).size(12.)),
                        )
                        .on_hover_text(format!(
                            "{} · {}\n{}",
                            input.kind.label(),
                            morphlet::node_access::for_input(node.kind, index)
                                .map_or("Custom", |a| a.label()),
                            morphlet::node_access::for_input(node.kind, index)
                                .map_or("", |a| a.description())
                        ));
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
                        } else if let (Some(default), false) = (input.default, needs_link) {
                            if let Some(n) = self.project.graph.node_mut(id) {
                                let value = n.params.entry(input.key.into()).or_insert(default);
                                ui.add(parameter_slider(value, input));
                            }
                        } else if optional {
                            ui.label(theme::small("つながなくても動きます（任意）"));
                        } else {
                            ui.label(
                                RichText::new(format!("{}を接続してください", input.kind.label()))
                                    .size(11.)
                                    .color(theme::error()),
                            );
                        }
                    });
                },
            );
        }
        if let Some(key) = disconnect {
            self.project.graph.disconnect(id, key);
        }
        if morphlet::data_tree_nodes::supports_transform(node.kind) {
            egui::CollapsingHeader::new("データツリー設定")
                .id_salt(("tree_settings", id))
                .show(ui, |ui| {
                    use morphlet::data_tree::{MATCH_PARAM, PORT_PREFIX};
                    if let Some(n) = self.project.graph.node_mut(id) {
                        if morphlet::data_tree_nodes::supports_matching(node.kind) {
                            let mut choice =
                                n.params.get(MATCH_PARAM).copied().unwrap_or(-1.) as i32;
                            let previous = choice;
                            let default_name =
                                if morphlet::node_access::uses_ordinal_matching(node.kind) {
                                    "既定（枝順・Longest）"
                                } else {
                                    "旧互換（同じパス）"
                                };
                            let names = [default_name, "Longest", "Shortest", "Cross"];
                            egui::ComboBox::from_id_salt(("matching", id))
                                .selected_text(names[(choice + 1).clamp(0, 3) as usize])
                                .show_ui(ui, |ui| {
                                    for (i, name) in names.iter().enumerate() {
                                        ui.selectable_value(&mut choice, i as i32 - 1, *name);
                                    }
                                });
                            if choice != previous {
                                if choice < 0 {
                                    n.params.remove(MATCH_PARAM);
                                } else {
                                    n.params.insert(MATCH_PARAM.into(), choice as f64);
                                }
                            }
                        }
                        for (index, input) in spec.inputs.iter().enumerate().take(shown) {
                            let key = format!("{PORT_PREFIX}{}", input.key);
                            let mut choice = n.params.get(&key).copied().unwrap_or(0.) as usize;
                            let previous = choice;
                            let names = ["そのまま", "Flatten", "Graft", "Simplify"];
                            ui.label(format!(
                                "{} · {} · {}",
                                input.label,
                                input.kind.label(),
                                morphlet::data_tree_nodes::access_label(node.kind, index)
                            ));
                            egui::ComboBox::from_id_salt(("tree_port", id, input.key))
                                .selected_text(names[choice.min(3)])
                                .show_ui(ui, |ui| {
                                    for (i, name) in names.iter().enumerate() {
                                        ui.selectable_value(&mut choice, i, *name);
                                    }
                                });
                            if choice != previous {
                                if choice == 0 {
                                    n.params.remove(&key);
                                } else {
                                    n.params.insert(key, choice as f64);
                                }
                            }
                        }
                    }
                });
        }
        if node.kind == NodeKind::PathMapper {
            ui.label(theme::small("例: {A;B}->{B;A}（変数・非負整数のみ）"));
            if let Some(n) = self.project.graph.node_mut(id) {
                ui.add(egui::TextEdit::singleline(&mut n.script).char_limit(1024));
            }
        }
        if node.kind == NodeKind::Script {
            crate::script_ui::script_editor(ui, &mut self.project.graph, id);
        }
        if node.kind == NodeKind::Expression {
            crate::script_ui::expression_editor(ui, &mut self.project.graph, id);
        }
        if node.kind == NodeKind::GraphMapper
            && let Some(n) = self.project.graph.node_mut(id)
        {
            crate::math_mapper_ui::show(ui, n);
        }
        if node.kind == NodeKind::ImportMesh {
            self.import_panel(ui, id, &node.script);
        }
        if node.kind == NodeKind::Cluster
            && let Some(action) = crate::script_ui::cluster_panel(ui, &mut self.project.graph, id)
        {
            self.part_action(action);
        }
        ui.add_space(9.);
        let stale = self.stale();
        let mut open_values = false;
        let evaluation = self.evaluation.as_ref();
        egui::CollapsingHeader::new("値を見る")
            .id_salt("data_inspector")
            .default_open(true)
            .show(ui, |ui| {
                if ui
                    .small_button("別ウィンドウで開く")
                    .on_hover_text("選択中のノードの値を、大きなウィンドウで表示します")
                    .clicked()
                {
                    open_values = true;
                }
                if stale {
                    ui.label(theme::small("更新を待っています…"));
                    return;
                }
                match evaluation.and_then(|e| e.outputs.get(&id)) {
                    Some(Ok(tree)) => data_view::show_tree(ui, tree, "inspector", 240.),
                    Some(Err(e)) => {
                        ui.colored_label(theme::error(), e);
                    }
                    None => {
                        ui.label(theme::small("未計算"));
                    }
                }
            });
        if open_values {
            self.values_open = true;
        }
        egui::CollapsingHeader::new("ノードの設定")
            .id_salt("node_settings")
            .show(ui, |ui| {
                if let Some(n) = self.project.graph.node_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("名前");
                        ui.text_edit_singleline(&mut n.name);
                    });
                    ui.checkbox(&mut n.preview, "「表示中」モードで表示");
                    ui.checkbox(&mut n.off, "無効にする（出力なし） · Ctrl+E");
                }
                ui.horizontal(|ui| {
                    if ui.button("複製").clicked() {
                        self.duplicate_selected();
                    }
                    if ui.button("削除").clicked() {
                        self.delete_selected();
                    }
                    if ui
                        .button("グループ")
                        .on_hover_text("選択中のノードを色つきの枠で囲みます · Ctrl+G")
                        .clicked()
                    {
                        self.make_group();
                    }
                    if ui
                        .button("まとめる")
                        .on_hover_text("選択中のノードを 1 つの部品（まとめノード）にします")
                        .clicked()
                    {
                        self.group_selected();
                    }
                });
            });
    }
    fn right_panel(&mut self, ctx: &egui::Context) {
        let remaining = ctx.available_rect().width();
        let maximum = (remaining - 220.).clamp(150., 850.);
        egui::SidePanel::right("workspace")
            .default_width((remaining * 0.42).clamp(150., maximum))
            .min_width(150.)
            .max_width(maximum)
            .resizable(true)
            .frame(egui::Frame::new().fill(theme::workspace_bg()))
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
                        ui.horizontal_wrapped(|ui| {
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
                let h = (ui.available_height() * 0.48).max(48.);
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
                            theme::muted(),
                        );
                    }
                });
                ui.add_space(2.);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new().inner_margin(8).show(ui, |ui| {
                            egui::CollapsingHeader::new("ビュー設定")
                                .default_open(true)
                                .show(ui, |ui| self.viewport.controls(ui));
                            ui.separator();
                            self.inspector(ui);
                        });
                    });
            });
    }
    fn center_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::canvas_bg()))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::toolbar_bg())
                    .inner_margin(egui::Margin::symmetric(15, 10))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("キャンバス").size(13.).strong());
                            if ui
                                .small_button("＋")
                                .on_hover_text("ノード追加 · Tab")
                                .clicked()
                            {
                                self.open_palette(self.canvas.center_for_add(&self.project), None);
                            }
                            if ui
                                .add_enabled(self.history.can_undo(), egui::Button::new("↩"))
                                .on_hover_text("元に戻す · Ctrl+Z")
                                .clicked()
                            {
                                self.undo();
                            }
                            if ui
                                .add_enabled(self.history.can_redo(), egui::Button::new("↪"))
                                .on_hover_text("やり直す · Ctrl+Y")
                                .clicked()
                            {
                                self.redo();
                            }
                            if ui.small_button("全体").clicked() {
                                self.canvas.fit_requested = true;
                            }
                            if ui
                                .small_button("選択")
                                .on_hover_text("選択にズーム · Shift+F")
                                .clicked()
                            {
                                self.canvas.fit_selection_requested = true;
                            }
                            ui.menu_button("操作", |ui| {
                                if ui.button("全体を自動整列").clicked() {
                                    self.canvas.auto_arrange(&mut self.project);
                                    ui.close();
                                }
                                ui.separator();
                                let n = self.canvas.selected.len();
                                if ui
                                    .add_enabled(n >= 2, egui::Button::new("選択：左端を揃える"))
                                    .clicked()
                                {
                                    self.canvas.align_selected(&mut self.project, 0);
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(n >= 2, egui::Button::new("選択：上端を揃える"))
                                    .clicked()
                                {
                                    self.canvas.align_selected(&mut self.project, 1);
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        n >= 3,
                                        egui::Button::new("選択：横方向の均等間隔"),
                                    )
                                    .clicked()
                                {
                                    self.canvas.distribute_selected(&mut self.project, 0);
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        n >= 3,
                                        egui::Button::new("選択：縦方向の均等間隔"),
                                    )
                                    .clicked()
                                {
                                    self.canvas.distribute_selected(&mut self.project, 1);
                                    ui.close();
                                }
                                ui.separator();
                                let mut minimap = !self.canvas.hide_minimap;
                                if ui
                                    .checkbox(&mut minimap, "ミニマップ（10ノード以上）")
                                    .changed()
                                {
                                    self.canvas.hide_minimap = !minimap;
                                }
                                ui.checkbox(&mut self.canvas.snap, "格子に吸着");
                                if ui.button("メモ").clicked() {
                                    let at = self.canvas.center_for_add(&self.project);
                                    self.add_note(at);
                                    ui.close();
                                }
                                if ui.button("値を見る").clicked() {
                                    self.values_open = !self.values_open;
                                    ui.close();
                                }
                            });
                            ui.label(theme::small(format!("{:.0}%", self.project.zoom * 100.)));
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
                if outcome.toggle_off {
                    self.toggle_off_selected();
                }
                if outcome.group {
                    self.make_group();
                }
                if outcome.view_values {
                    self.values_open = true;
                }
                if let Some(kind) = outcome.help {
                    self.help_window.open_for(kind);
                }
                if outcome.help_index {
                    self.help_window.open_index();
                }
                if let Some(at) = outcome.add_note_at {
                    self.add_note(at);
                }
                if let Some(at) = outcome.add_at {
                    self.open_palette(at, None);
                }
                if let Some((from, at)) = outcome.drop_wire {
                    // Released on empty canvas: search, then connect the chosen node to `from`.
                    self.open_palette(at, Some(from));
                }
            });
    }
    /// Node search popup (Tab / Ctrl+K / double-click / a wire dropped on empty canvas).
    /// Fuzzy ranking, recently used nodes first, ↑ ↓ to move and Enter to add.
    fn palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            self.wire_source = None;
            self.palette_ime = false;
            return;
        }
        let mut open = true;
        let mut add = None;
        // Right-click on a row toggles its favourite (applied after the window closes).
        let mut star: Option<NodeKind> = None;
        let ordered = ordered_kinds();
        let wire_source = self.wire_source.and_then(|id| self.project.graph.node(id));
        let wire_type: Option<DataType> = wire_source.map(|n| n.kind.spec().output);
        let wire_name: Option<String> = wire_source.map(|n| n.name.clone());
        let mut ime_event = false;
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Ime(event) = event {
                    ime_event = true;
                    match event {
                        egui::ImeEvent::Preedit(text) => self.palette_ime = !text.is_empty(),
                        egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => {
                            self.palette_ime = false
                        }
                        egui::ImeEvent::Enabled => {}
                    }
                }
            }
        });
        let composing = ime_event || self.palette_ime;
        let down = !composing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let up = !composing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let enter =
            !composing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let escape = !composing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let width = (ctx.content_rect().width() - 40.).clamp(180., 480.);
        let max_height = (ctx.content_rect().height() * 0.52).clamp(80., 390.);
        egui::Window::new("ノードを追加")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.set_width(width);
                let r = ui.add(egui::TextEdit::singleline(&mut self.palette_query)
                    .hint_text("円 / twist / 点を並べたい")
                    .desired_width(f32::INFINITY));
                let first = self.palette_focus;
                if first { r.request_focus(); self.palette_focus = false; }
                if r.changed() { self.palette_cursor = 0; }
                if let Some(name) = &wire_name {
                    ui.label(theme::small(format!("{} · {} と接続できる候補のみ", name, wire_type.map(DataType::label).unwrap_or("データ"))));
                }
                let matches = ui_polish::rank(&ordered, &self.palette_query, &self.recent, wire_type);
                let last = matches.len().saturating_sub(1);
                if down && !r.changed() { self.palette_cursor = (self.palette_cursor + 1).min(last); }
                if up && !r.changed() { self.palette_cursor = self.palette_cursor.saturating_sub(1); }
                self.palette_cursor = self.palette_cursor.min(last);
                if enter && !r.changed() { add = matches.get(self.palette_cursor).copied(); }
                ui.label(theme::small(format!("{} 件 · ↑↓ / Enter", matches.len())));
                let cursor = self.palette_cursor;
                egui::ScrollArea::vertical().max_height(max_height).show(ui, |ui| {
                    for (index, kind) in matches.iter().enumerate() {
                        let spec = kind.spec();
                        let favorite = self.favorites.contains(kind);
                        let response = ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(26., 26.), Sense::hover());
                            node_icons::paint(ui.painter(), r, *kind, spec.category, theme::category_color(spec.category));
                            let star = if favorite { "★ " } else { "" };
                            ui.selectable_label(index == cursor, format!("{star}{}  ·  {}", spec.label, spec.category))
                                .on_hover_text(spec.description)
                        }).inner;
                        response.context_menu(|ui| {
                            let text = if favorite { "★ お気に入りから外す" } else { "★ お気に入りに追加" };
                            if ui.button(text).clicked() {
                                star = Some(*kind);
                                ui.close();
                            }
                        });
                        if index == cursor && (first || up || down || r.changed()) { response.scroll_to_me(None); }
                        if response.clicked() { add = Some(*kind); }
                    }
                    if matches.is_empty() {
                        ui.label(if wire_type.is_some() { "接続できる候補がありません。検索語を変えるか Esc で中止してください。" }
                            else { "見つかりません。例: 円を作る、面を作る、花瓶を作る" });
                    }
                });
            });
        if let Some(kind) = star {
            self.toggle_favorite(kind);
        }
        if let Some(kind) = add {
            self.add_node(kind, Some(self.palette_position));
        } else {
            self.palette_open = open && !escape;
        }
    }
    /// ショートカット一覧 (?): every keyboard shortcut, in Japanese.
    fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        let mut open = true;
        egui::Window::new("キーボードショートカット")
            .open(&mut open)
            .collapsible(false)
            .default_width(520.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.label(theme::small(
                    "? キーでいつでも開けます。文字を入力中は一部の操作が無効になります。",
                ));
                ui.add_space(8.);
                egui::Grid::new("shortcut_grid")
                    .num_columns(2)
                    .spacing([28., 7.])
                    .show(ui, |ui| {
                        for (keys, what) in ui_polish::SHORTCUTS {
                            ui.label(RichText::new(*keys).size(12.).color(theme::teal()));
                            ui.label(RichText::new(*what).size(12.));
                            ui.end_row();
                        }
                    });
            });
        self.shortcuts_open = open;
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
                        .fill(theme::card_bg())
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
    fn stl_sources(&self) -> StlSources {
        StlSources {
            output: self.result_trees(),
            selected: self.selected_trees(),
            revision: self.result_revision,
            base_name: safe_name(&self.project.name),
            start_dir: self.start_directory(),
        }
    }
    /// Trees of the nodes selected on the canvas (選択ノード mode).
    fn selected_trees(&self) -> Result<Vec<Tree>, String> {
        if self.stale() {
            return Err("形状を更新中です。更新が終わってから書き出してください。".into());
        }
        let Some(e) = &self.evaluation else {
            return Err("形状の計算が完了していません。".into());
        };
        if self.canvas.selected.is_empty() {
            return Err("書き出すノードを選んでください。".into());
        }
        let mut trees = Vec::new();
        for id in &self.canvas.selected {
            match e.outputs.get(id) {
                Some(Ok(tree)) => trees.push(tree.clone()),
                Some(Err(error)) => {
                    return Err(format!("選んだノードを計算できません: {error}"));
                }
                None => return Err("選んだノードの計算結果がまだありません。".into()),
            }
        }
        Ok(trees)
    }
    fn stl_request(&mut self, request: Option<UiRequest>) {
        match request {
            Some(UiRequest::Pick {
                task,
                directory,
                file_name,
            }) => {
                self.file_picker = Some(FilePicker::new(task, directory, file_name));
            }
            Some(UiRequest::PickSlicer { directory }) => {
                self.file_picker = Some(FilePicker::new(
                    FileTask::PickSlicer,
                    directory,
                    String::new(),
                ));
            }
            None => {}
        }
    }
    fn flush_stl_notice(&mut self) {
        if let Some((text, error)) = self.stl_export.take_notice() {
            self.notify(text, error);
        }
    }
    fn stl_quick_shortcut(&mut self, ctx: &egui::Context) {
        if stl_export_ui::quick_export_pressed(ctx) {
            let sources = self.stl_sources();
            let request = self.stl_export.quick(&sources);
            self.stl_request(request);
            self.flush_stl_notice();
        }
    }
    fn stl_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.stl_export.is_open() {
            return;
        }
        let sources = self.stl_sources();
        let request = self.stl_export.show(ctx, &sources);
        self.stl_request(request);
        self.flush_stl_notice();
    }
    fn gcode_dialog(&mut self, ctx: &egui::Context) {
        if !self.gcode.is_open() {
            return;
        }
        let trees = self.result_trees();
        if self.gcode.show(ctx, &trees) {
            self.file_picker = Some(FilePicker::new(
                FileTask::ExportGcode,
                self.start_directory(),
                format!("{}.gcode", safe_name(&self.project.name)),
            ));
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
                for (a,b) in [("Ctrl+S / Ctrl+O","保存 / 開く"),("Ctrl+Z / Ctrl+Y","元に戻す / やり直す"),("Ctrl+D / Delete","選択ノードを複製 / 削除"),("Tab / Ctrl+K","ノードを検索"),("Shift+クリック / 範囲ドラッグ","複数のノードを選ぶ"),("F","グラフと形状を画面に収める"),("F1","選択中のノードのヘルプ（ノード辞典）"),("Ctrl+E","選択ノードを無効 / 有効"),("Ctrl+G","選択ノードをグループで囲む"),("右クリック（空き）","ここにノード / メモを追加"),("3D: 左ドラッグ / Shift+ドラッグ","回転 / 移動"),("ホイール","拡大・縮小")]{ui.label(RichText::new(a).size(11.).color(theme::TEAL));ui.label(RichText::new(b).size(11.));ui.end_row();}
            });
            ui.add_space(8.);if ui.button("ノード辞典を開く（全ノードの説明）").clicked(){self.help_window.open_index();}
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
                    self.clipboard.clear();
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
    /// 「名前」: message for the selected node's error, else the first error.
    fn error_line(&self, e: &Evaluation) -> Option<String> {
        let selected = self.canvas.selected.iter().next_back().copied();
        let pick = selected
            .and_then(|id| {
                e.outputs
                    .get(&id)
                    .and_then(|r| r.as_ref().err())
                    .map(|m| (id, m))
            })
            .or_else(|| {
                e.outputs
                    .iter()
                    .find_map(|(id, r)| r.as_ref().err().map(|m| (*id, m)))
            })?;
        let name = self
            .project
            .graph
            .node(pick.0)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        let message: String = pick.1.chars().take(90).collect();
        Some(format!("「{name}」: {message}"))
    }
    fn values_window(&mut self, ctx: &egui::Context) {
        if !self.values_open {
            return;
        }
        let mut open = true;
        let id = self.canvas.selected.iter().next_back().copied();
        let title = id
            .and_then(|id| self.project.graph.node(id))
            .map(|n| format!("値を見る · {}", n.name))
            .unwrap_or_else(|| "値を見る".into());
        egui::Window::new("値を見る")
            .id(egui::Id::new("values_window"))
            .open(&mut open)
            .resizable(true)
            .default_size([420., 460.])
            .show(ctx, |ui| {
                ui.label(RichText::new(title).strong());
                let Some(id) = id else {
                    ui.label(theme::small(
                        "キャンバスでノードを選ぶと、その出力が表示されます。",
                    ));
                    return;
                };
                if self.stale() {
                    ui.label(theme::small("更新を待っています…"));
                }
                match self.evaluation.as_ref().and_then(|e| e.outputs.get(&id)) {
                    Some(Ok(tree)) => {
                        let h = (ui.available_height() - 30.).max(160.);
                        data_view::show_tree(ui, tree, "window", h);
                    }
                    Some(Err(e)) => {
                        ui.colored_label(theme::error(), e);
                    }
                    None => {
                        ui.label(theme::small("未計算"));
                    }
                }
                ui.label(theme::small(format!(
                    "数値は小数 4 桁、最大 {} 行まで表示します。",
                    data_view::MAX_ROWS
                )));
            });
        self.values_open = open;
    }
    fn save_recovery(&mut self) {
        if !self.dirty || self.last_autosave.elapsed() < Duration::from_secs(20) {
            return;
        }
        let result = recovery_path()
            .ok_or_else(|| "自動保存先が見つかりません。".to_string())
            .and_then(|path| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("自動保存先を作成できませんでした: {e}"))?;
                }
                persistence::save(&path, &self.project)
            });
        if let Err(error) = result {
            self.notify(
                format!("自動保存に失敗しました。手動で保存してください: {error}"),
                true,
            );
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
            && !self.gcode.is_open()
            && self.file_picker.is_none()
        {
            self.stl_quick_shortcut(ctx);
            self.keyboard(ctx);
        }
        self.topbar(ctx);
        self.shelf_panel(ctx);
        self.statusbar(ctx);
        if self.show_library && (!self.show_workspace || ctx.available_rect().width() >= 850.) {
            self.library(ctx);
        }
        if self.show_workspace {
            self.right_panel(ctx);
        }
        self.center_panel(ctx);
        self.palette(ctx);
        self.persist_shelf_lists();
        self.shortcuts_dialog(ctx);
        self.template_dialog(ctx);
        self.gcode_dialog(ctx);
        self.stl_export_dialog(ctx);
        self.help_dialog(ctx);
        if let Some(kind) = self.help_window.show(ctx) {
            self.add_node(kind, None);
        }
        self.values_window(ctx);
        self.confirm_dialog(ctx);
        self.recovery_dialog(ctx);
        // Dropped files run before the change check below, so a node they add is
        // checkpointed and computed in this frame.
        for dropped in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(path) = dropped.path {
                if morphlet::import::format_of(&path).is_some() {
                    // The drop itself carries no position; the last pointer position is used.
                    let at = ctx
                        .input(|i| i.pointer.latest_pos())
                        .and_then(|p| self.canvas.world_at(&self.project, p));
                    self.set_import_file(&path, None, at);
                } else {
                    self.request(PendingAction::Open(path), ctx);
                }
            }
        }
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
                            theme::error_fill()
                        } else {
                            Color32::from_rgb(32, 66, 53)
                        })
                        .corner_radius(9)
                        .inner_margin(egui::Margin::symmetric(18, 12))
                        .show(ui, |ui| {
                            ui.set_max_width(650.);
                            ui.label(RichText::new(text).size(12.).color(if *error {
                                theme::error()
                            } else {
                                Color32::WHITE
                            }));
                        });
                });
            ctx.request_repaint_after(Duration::from_millis(200));
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

/// Copies the chosen subgraph atomically. A duplicate retains upstream links;
/// a paste only retains links inside the copied selection. Cluster definitions remain
/// document-local, so a stale/missing definition causes validation to fail.
fn insert_nodes(
    graph: &mut Graph,
    nodes: &[Node],
    offset: [f32; 2],
    retain_external: bool,
) -> Result<BTreeSet<NodeId>, String> {
    if nodes.len() > morphlet::model::MAX_NODES.saturating_sub(graph.nodes.len()) {
        return Err("ノード数の上限を超えるため追加できません。".into());
    }
    let mut candidate = graph.clone();
    let mut mapping = BTreeMap::new();
    for old in nodes {
        let id = candidate.add(
            old.kind,
            [
                (old.position[0] + offset[0]).clamp(-1e6, 1e6),
                (old.position[1] + offset[1]).clamp(-1e6, 1e6),
            ],
        )?;
        mapping.insert(old.id, id);
        let n = candidate
            .node_mut(id)
            .ok_or("追加したノードが見つかりません。")?;
        n.name = old.name.clone();
        n.params = old.params.clone();
        n.preview = old.preview;
        n.script = old.script.clone();
        n.cluster = old.cluster;
        n.off = old.off;
    }
    for old in nodes {
        let new = candidate
            .node_mut(mapping[&old.id])
            .ok_or("追加したノードが見つかりません。")?;
        new.inputs = old
            .inputs
            .iter()
            .filter_map(|(key, source)| {
                mapping
                    .get(source)
                    .copied()
                    .or_else(|| retain_external.then_some(*source))
                    .map(|id| (key.clone(), id))
            })
            .collect();
    }
    candidate.validate()?;
    *graph = candidate;
    Ok(mapping.values().copied().collect())
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
        && a.clusters == b.clusters
        && a.nodes.iter().zip(&b.nodes).all(|(x, y)| {
            x.id == y.id
                && x.kind == y.kind
                && x.params == y.params
                && x.inputs == y.inputs
                && x.script == y.script
                && x.cluster == y.cluster
                && x.off == y.off
        })
}
#[cfg(test)]
fn matches_search(kind: NodeKind, query: &str) -> bool {
    ui_polish::query_score(kind, query).is_some()
}
/// Node kinds in library order: by category, then by the order in `NodeKind::all`.
fn ordered_kinds() -> Vec<NodeKind> {
    let mut ordered: Vec<_> = NodeKind::all().to_vec();
    ordered.sort_by_key(|kind| category_rank(kind.spec().category));
    ordered
}
fn category_rank(category: &str) -> usize {
    [
        "立体",
        "精密曲面",
        "曲線",
        "変形",
        "配列",
        "数値",
        "データ",
        "出力",
    ]
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
// Reading a parameter must not quantize it or change a loaded document. The
// metadata step is interaction speed, not an origin-offset grid of valid values.
fn parameter_binding<'a>(
    value: &'a mut f64,
    spec: InputSpec,
) -> impl FnMut(Option<f64>) -> f64 + 'a {
    let (min, max, integer) = (spec.min, spec.max, spec.integer);
    move |edited| {
        if let Some(edited) = edited.filter(|number| number.is_finite()) {
            let edited = if integer { edited.round() } else { edited };
            *value = edited.clamp(min, max);
        }
        *value
    }
}

fn parse_parameter_number(text: &str) -> Option<f64> {
    let text: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '−' { '-' } else { c })
        .collect();
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}

pub(crate) fn parameter_slider<'a>(value: &'a mut f64, spec: &InputSpec) -> egui::Slider<'a> {
    let (low, high) = friendly_range(spec.key, spec.min, spec.max, *value);
    egui::Slider::from_get_set(low..=high, parameter_binding(value, *spec))
        // The track uses a convenient range. Text entry uses the full metadata
        // range enforced by the setter, including values beyond the track ends.
        .clamping(egui::SliderClamping::Never)
        .drag_value_speed(spec.step)
        .max_decimals_opt(spec.integer.then_some(0))
        // DragValue parses its displayed text on focus loss; a shortened decimal
        // display would silently round a precise value even without an edit.
        .custom_formatter(|number, _| number.to_string())
        .custom_parser(parse_parameter_number)
}

pub(crate) fn parameter_drag_value<'a>(
    value: &'a mut f64,
    spec: &InputSpec,
) -> egui::DragValue<'a> {
    egui::DragValue::from_get_set(parameter_binding(value, *spec))
        .speed(spec.step)
        .range(spec.min..=spec.max)
        .clamp_existing_to_range(false)
        .max_decimals_opt(spec.integer.then_some(0))
        .custom_formatter(|number, _| number.to_string())
        .custom_parser(parse_parameter_number)
}

pub(crate) fn friendly_range(key: &str, min: f64, max: f64, value: f64) -> (f64, f64) {
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
#[path = "numeric_editor_tests.rs"]
mod numeric_editor_tests;

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
    fn copy_paste_remaps_internal_edges_and_drops_external_links() {
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = graph.add(NodeKind::Add, [300., 0.]).unwrap();
        let c = graph.add(NodeKind::Multiply, [600., 0.]).unwrap();
        graph.connect(a, b, "a").unwrap();
        graph.connect(b, c, "a").unwrap();
        let nodes = vec![
            graph.node(b).unwrap().clone(),
            graph.node(c).unwrap().clone(),
        ];
        let ids = insert_nodes(&mut graph, &nodes, [0., 300.], false).unwrap();
        let pasted: Vec<_> = ids.iter().copied().collect();
        assert!(graph.node(pasted[0]).unwrap().inputs.is_empty());
        assert_eq!(graph.node(pasted[1]).unwrap().inputs["a"], pasted[0]);
        let duplicates: Vec<_> = insert_nodes(&mut graph, &nodes, [0., 600.], true)
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(graph.node(duplicates[0]).unwrap().inputs["a"], a);
        assert_eq!(
            graph.node(duplicates[1]).unwrap().inputs["a"],
            duplicates[0]
        );
        graph.validate().unwrap();
    }

    #[test]
    fn failed_paste_is_atomic() {
        let mut graph = Graph::default();
        let id = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let mut node = graph.node(id).unwrap().clone();
        node.params.insert("value".into(), f64::NAN);
        let before = graph.clone();
        assert!(insert_nodes(&mut graph, &[node], [0., 0.], false).is_err());
        assert_eq!(graph, before);
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
