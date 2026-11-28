//! Direct editing of SubD / T-spline cages in the viewport (ケージ編集 node).
//!
//! The node keeps its edits as a JSON list in `Node::script` (`nodes::subd::EditOp`); this
//! module only appends to that list. While editing, the viewport draws the cage's control net
//! of the node's *output* and reports picks and Ctrl+drags; indices in new edits refer to that
//! output, so the list replays exactly.
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Response, RichText, Shape, Stroke};
use nodes::engine::Evaluation;
use nodes::model::{Graph, NodeId, NodeKind, Value};
use nodes::subd::{BrushAction, BrushEdit, Cage, EditOp, Falloff, edits_text, parse_edits};
use std::collections::BTreeSet;

type V3 = [f64; 3];

/// What the user picks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Vertex,
    Edge,
    Face,
}

/// The control net drawn over the viewport while editing.
#[derive(Clone, Debug)]
pub struct CageOverlay {
    pub points: Vec<V3>,
    pub edges: Vec<(u32, u32)>,
    pub faces: Vec<Vec<u32>>,
    pub mode: Mode,
    pub selected: BTreeSet<usize>,
    /// Points moved by the drag in progress (drawn displaced).
    pub moving: BTreeSet<usize>,
    pub offset: V3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CageEvent {
    Pick { index: usize, add: bool },
    Clear,
    Drag(V3),
    DragEnd,
}

/// Screen projection and the camera's screen axes (mm per pixel = 1 / scale).
pub struct View<'a> {
    pub project: &'a dyn Fn(V3) -> Pos2,
    pub right: V3,
    pub up: V3,
    pub scale: f64,
}

fn mid(a: V3, b: V3) -> V3 {
    [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2., (a[2] + b[2]) / 2.]
}

impl CageOverlay {
    fn point(&self, i: u32) -> V3 {
        let p = self.points.get(i as usize).copied().unwrap_or([0.; 3]);
        if self.moving.contains(&(i as usize)) {
            [
                p[0] + self.offset[0],
                p[1] + self.offset[1],
                p[2] + self.offset[2],
            ]
        } else {
            p
        }
    }

    fn handles(&self) -> Vec<V3> {
        match self.mode {
            Mode::Vertex => (0..self.points.len() as u32)
                .map(|i| self.point(i))
                .collect(),
            Mode::Edge => self
                .edges
                .iter()
                .map(|&(a, b)| mid(self.point(a), self.point(b)))
                .collect(),
            Mode::Face => self
                .faces
                .iter()
                .map(|f| {
                    let n = f.len().max(1) as f64;
                    let s = f.iter().fold([0.; 3], |s, &i| {
                        let p = self.point(i);
                        [s[0] + p[0], s[1] + p[1], s[2] + p[2]]
                    });
                    [s[0] / n, s[1] / n, s[2] / n]
                })
                .collect(),
        }
    }

    /// Draw the net and its handles.
    pub fn paint(&self, painter: &egui::Painter, view: &View, dark: bool) {
        let net = if dark {
            Color32::from_rgb(205, 170, 235)
        } else {
            Color32::from_rgb(120, 84, 150)
        };
        let hot = Color32::from_rgb(255, 170, 60);
        for (k, &(a, b)) in self.edges.iter().enumerate() {
            let sel = self.mode == Mode::Edge && self.selected.contains(&k);
            painter.line_segment(
                [(view.project)(self.point(a)), (view.project)(self.point(b))],
                Stroke::new(
                    if sel { 3.0_f32 } else { 1.2_f32 },
                    if sel { hot } else { net },
                ),
            );
        }
        if self.mode == Mode::Face {
            for k in &self.selected {
                if let Some(f) = self.faces.get(*k) {
                    let pts: Vec<Pos2> = f.iter().map(|&i| (view.project)(self.point(i))).collect();
                    painter.add(Shape::convex_polygon(
                        pts,
                        hot.gamma_multiply(0.35),
                        Stroke::new(2.0_f32, hot),
                    ));
                }
            }
        }
        for (k, p) in self.handles().into_iter().enumerate() {
            let sel = self.selected.contains(&k);
            painter.circle_filled(
                (view.project)(p),
                if sel { 5.0_f32 } else { 3.2_f32 },
                if sel { hot } else { net },
            );
        }
    }

    /// The handle nearest the pointer, within 14 px.
    pub fn pick(&self, at: Pos2, view: &View) -> Option<usize> {
        self.handles()
            .into_iter()
            .enumerate()
            .map(|(k, p)| (k, (view.project)(p).distance(at)))
            .filter(|(_, d)| *d <= 14.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    }

    /// Picks, clicks on empty space and Ctrl+drags as events. Returns true when the input was
    /// used by the cage (the viewport then neither rotates nor picks a node).
    pub fn interact(
        &self,
        response: &Response,
        ctrl: bool,
        shift: bool,
        view: &View,
        events: &mut Vec<CageEvent>,
    ) -> bool {
        let mut used = false;
        if ctrl && response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            let (dx, dy) = (f64::from(d.x) / view.scale, -f64::from(d.y) / view.scale);
            let mut w = [
                view.right[0] * dx + view.up[0] * dy,
                view.right[1] * dx + view.up[1] * dy,
                view.right[2] * dx + view.up[2] * dy,
            ];
            // Hold X, Y or Z while dragging to move along that axis only.
            let axis = response.ctx.input(|i| {
                [egui::Key::X, egui::Key::Y, egui::Key::Z]
                    .iter()
                    .position(|k| i.key_down(*k))
            });
            if let Some(a) = axis {
                for (k, x) in w.iter_mut().enumerate() {
                    if k != a {
                        *x = 0.;
                    }
                }
            }
            events.push(CageEvent::Drag(w));
            used = true;
        }
        if response.drag_stopped() && !self.moving.is_empty() {
            events.push(CageEvent::DragEnd);
            used = true;
        }
        if response.clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            match self.pick(at, view) {
                Some(index) => events.push(CageEvent::Pick {
                    index,
                    add: shift || ctrl,
                }),
                None => events.push(CageEvent::Clear),
            }
            used = true;
        }
        used
    }
}

/// Editing state kept by the app.
#[derive(Default)]
pub struct CageEdit {
    node: Option<NodeId>,
    mode: Mode,
    selected: BTreeSet<usize>,
    symmetry: Option<u8>,
    offset: V3,
    dragging: bool,
    move_by: V3,
    brush_radius: f64,
    brush_falloff: Falloff,
    brush_strength: f64,
    extrude: f64,
    scale: f64,
    message: Option<String>,
}

fn output_cage(evaluation: Option<&Evaluation>, id: NodeId) -> Option<Cage> {
    let tree = evaluation?.outputs.get(&id)?.as_ref().ok()?;
    tree.branches.values().flatten().find_map(|v| match v {
        Value::Cage(c) => Some(c.as_ref().clone()),
        _ => None,
    })
}

impl CageEdit {
    /// The overlay for the viewport, when a ケージ編集 node is being edited and has a result.
    pub fn overlay(
        &mut self,
        graph: &Graph,
        evaluation: Option<&Evaluation>,
    ) -> Option<CageOverlay> {
        let id = self.node?;
        if graph
            .node(id)
            .is_none_or(|n| n.kind != NodeKind::SubdCageEdit)
        {
            self.node = None;
            return None;
        }
        let cage = output_cage(evaluation, id)?;
        let moving = if self.dragging {
            self.moving_points(&cage)
        } else {
            BTreeSet::new()
        };
        Some(CageOverlay {
            points: cage.vertices().to_vec(),
            edges: cage.edges(),
            faces: cage.faces().to_vec(),
            mode: self.mode,
            selected: self.selected.clone(),
            moving,
            offset: self.offset,
        })
    }

    fn moving_points(&self, cage: &Cage) -> BTreeSet<usize> {
        match self.mode {
            Mode::Vertex => self.selected.clone(),
            Mode::Edge => {
                let edges = cage.edges();
                self.selected
                    .iter()
                    .filter_map(|k| edges.get(*k))
                    .flat_map(|&(a, b)| [a as usize, b as usize])
                    .collect()
            }
            Mode::Face => self
                .selected
                .iter()
                .filter_map(|k| cage.faces().get(*k))
                .flatten()
                .map(|&i| i as usize)
                .collect(),
        }
    }

    fn push(&mut self, graph: &mut Graph, op: EditOp) {
        let Some(node) = self.node.and_then(|id| graph.node_mut(id)) else {
            return;
        };
        match parse_edits(&node.script) {
            Ok(mut ops) => {
                ops.push(op);
                node.script = edits_text(&ops);
                self.message = None;
            }
            Err(e) => self.message = Some(e),
        }
    }

    fn push_move(&mut self, graph: &mut Graph, evaluation: Option<&Evaluation>, d: V3) {
        let Some(cage) = self.node.and_then(|id| output_cage(evaluation, id)) else {
            return;
        };
        let verts: Vec<usize> = self.moving_points(&cage).into_iter().collect();
        if verts.is_empty() || d.iter().all(|x| x.abs() < 1e-12) {
            return;
        }
        self.push(
            graph,
            EditOp::Move {
                verts,
                d,
                sym: self.symmetry,
            },
        );
    }

    /// Apply the viewport's events.
    pub fn handle(
        &mut self,
        events: Vec<CageEvent>,
        graph: &mut Graph,
        evaluation: Option<&Evaluation>,
    ) {
        for e in events {
            match e {
                CageEvent::Pick { index, add } => {
                    if !add {
                        self.selected.clear();
                        self.selected.insert(index);
                    } else if !self.selected.remove(&index) {
                        self.selected.insert(index);
                    }
                }
                CageEvent::Clear => self.selected.clear(),
                CageEvent::Drag(d) => {
                    if !self.selected.is_empty() {
                        self.dragging = true;
                        for (o, x) in self.offset.iter_mut().zip(d) {
                            *o += x;
                        }
                    }
                }
                CageEvent::DragEnd => {
                    if self.dragging {
                        let d = self.offset;
                        self.push_move(graph, evaluation, d);
                    }
                    self.dragging = false;
                    self.offset = [0.; 3];
                }
            }
        }
    }

    /// Inspector panel for a ケージ編集 node.
    pub fn panel(
        &mut self,
        ui: &mut egui::Ui,
        graph: &mut Graph,
        id: NodeId,
        evaluation: Option<&Evaluation>,
    ) {
        ui.add_space(6.);
        ui.label(RichText::new("ケージの直接編集").strong());
        let editing = self.node == Some(id);
        if !editing {
            if ui.button("ビューポートで編集を始める").clicked() {
                *self = CageEdit {
                    node: Some(id),
                    extrude: 10.,
                    scale: 1.2,
                    brush_radius: 20.,
                    brush_strength: 0.35,
                    ..CageEdit::default()
                };
            }
            ui.label(theme::small(
                "編集中は、クリックで選択（Shift で追加）、Ctrl＋ドラッグで移動します（X・Y・Z キーを押しながらだとその軸だけ）。",
            ));
            return;
        }
        if self.extrude == 0. {
            self.extrude = 10.;
        }
        if self.scale == 0. {
            self.scale = 1.2;
        }
        ui.horizontal(|ui| {
            for (m, label) in [(Mode::Vertex, "点"), (Mode::Edge, "辺"), (Mode::Face, "面")] {
                if ui.selectable_label(self.mode == m, label).clicked() && self.mode != m {
                    self.mode = m;
                    self.selected.clear();
                }
            }
            if ui.button("編集を終える").clicked() {
                self.node = None;
            }
        });
        ui.horizontal(|ui| {
            ui.label("対称");
            for (v, label) in [
                (None, "なし"),
                (Some(0u8), "X"),
                (Some(1), "Y"),
                (Some(2), "Z"),
            ] {
                if ui.selectable_label(self.symmetry == v, label).clicked() {
                    self.symmetry = v;
                }
            }
        });
        ui.label(theme::small(format!(
            "選択: {} 個。クリックで選択、Shift＋クリックで追加、Ctrl＋ドラッグで移動。",
            self.selected.len()
        )));
        ui.horizontal(|ui| {
            for k in 0..3 {
                ui.add(
                    egui::DragValue::new(&mut self.move_by[k])
                        .speed(0.5)
                        .prefix(["X ", "Y ", "Z "][k]),
                );
            }
            if ui.button("移動").clicked() {
                let d = self.move_by;
                self.push_move(graph, evaluation, d);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.scale)
                    .speed(0.01)
                    .range(0.01..=100.0)
                    .prefix("倍率 "),
            );
            if ui.button("拡大縮小").clicked()
                && let Some(cage) = output_cage(evaluation, id)
            {
                let verts: Vec<usize> = self.moving_points(&cage).into_iter().collect();
                if !verts.is_empty() {
                    let s = self.scale;
                    self.push(graph, EditOp::Scale { verts, s });
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.extrude)
                    .speed(0.5)
                    .prefix("距離 "),
            );
            if ui
                .add_enabled(
                    self.mode == Mode::Face && !self.selected.is_empty(),
                    egui::Button::new("面を押し出す"),
                )
                .clicked()
            {
                let faces: Vec<usize> = self.selected.iter().copied().collect();
                let d = self.extrude;
                self.push(graph, EditOp::Extrude { faces, d });
                self.selected.clear();
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.mode == Mode::Edge && !self.selected.is_empty(),
                    egui::Button::new("折り目"),
                )
                .clicked()
            {
                let edges: Vec<usize> = self.selected.iter().copied().collect();
                self.push(graph, EditOp::Crease { edges });
            }
            if ui.button("折り目を消す").clicked() {
                self.push(graph, EditOp::Uncrease);
            }
            if ui
                .add_enabled(
                    self.mode == Mode::Face && !self.selected.is_empty(),
                    egui::Button::new("局所細分"),
                )
                .on_hover_text("T-spline パッチケージで、選んだ面だけ点を増やします")
                .clicked()
            {
                let faces: Vec<usize> = self.selected.iter().copied().collect();
                self.push(graph, EditOp::Refine { faces });
                self.selected.clear();
            }
            if ui.button("細分").clicked() {
                self.push(graph, EditOp::Subdivide);
                self.selected.clear();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("1 つ戻す").clicked()
                && let Some(node) = graph.node_mut(id)
                && let Ok(mut ops) = parse_edits(&node.script)
            {
                ops.pop();
                node.script = edits_text(&ops);
                self.selected.clear();
            }
            if ui.button("編集をリセット").clicked()
                && let Some(node) = graph.node_mut(id)
            {
                node.script.clear();
                self.selected.clear();
            }
        });
        if let Some(cage) = output_cage(evaluation, id) {
            ui.separator();
            ui.label(RichText::new("ソフト選択ブラシ").strong());
            if cage.tspline().is_some() {
                ui.label(theme::small("通常の SubD・格子ケージ専用です。"));
            } else {
                ui.add(egui::DragValue::new(&mut self.brush_radius)
                    .range(0.001..=1e6).speed(0.5).prefix("半径 mm "));
                ui.horizontal(|ui| {
                    for (f, label) in [(Falloff::Linear, "直線"), (Falloff::Smooth, "滑らか"), (Falloff::Gaussian, "ガウス")] {
                        ui.selectable_value(&mut self.brush_falloff, f, label);
                    }
                });
                ui.add(egui::DragValue::new(&mut self.brush_strength)
                    .range(0.0..=1.0).speed(0.01).prefix("緩和強度 "));
                let seeds: Vec<_> = self.moving_points(&cage).into_iter().collect();
                let enabled = !seeds.is_empty() && self.symmetry.is_none();
                let mut action = None;
                ui.horizontal(|ui| {
                    if ui.add_enabled(enabled, egui::Button::new("ソフト移動")).clicked() {
                        action = Some(BrushAction::Move { delta: self.move_by });
                    }
                    if ui.add_enabled(enabled, egui::Button::new("緩和")).clicked() {
                        action = Some(BrushAction::Relax { strength: self.brush_strength });
                    }
                });
                if self.symmetry.is_some() {
                    ui.label(theme::small("ブラシを使うには対称編集を解除してください。"));
                }
                if let Some(action) = action {
                    self.push(graph, EditOp::Brush { brush: BrushEdit {
                        topology: cage.brush_topology_key(),
                        seeds,
                        pinned: Vec::new(),
                        radius: self.brush_radius,
                        falloff: self.brush_falloff,
                        action,
                    }});
                }
                ui.label(theme::small("選択点を中心に、上の XYZ 移動量または緩和を適用します。"));
            }
        }
        if let Some(node) = graph.node(id) {
            let count = parse_edits(&node.script).map(|o| o.len()).unwrap_or(0);
            ui.label(theme::small(format!("記録された編集: {count} 件")));
        }
        if let Some(m) = &self.message {
            ui.colored_label(theme::error(), m);
        }
    }
}
