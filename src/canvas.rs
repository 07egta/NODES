use crate::data_view::{self, DataShape};
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use morphlet::{
    annotations::{self, GROUP_COLORS, MAX_GROUP_NAME_BYTES, MAX_NOTE_BYTES},
    engine::Evaluation,
    model::{Node, NodeId, NodeKind},
    persistence::Project,
};
use std::collections::{BTreeMap, BTreeSet};

const WIDTH: f32 = 218.0;
const HEADER: f32 = 43.0;
const ROW: f32 = 28.0;

#[derive(Default)]
pub struct GraphCanvas {
    pub selected: BTreeSet<NodeId>,
    pub connecting: Option<NodeId>,
    pub fit_requested: bool,
    pub last_rect: Option<Rect>,
    dragging: Option<NodeId>,
    box_start: Option<Pos2>,
    /// Note being edited in place, and whether its text box still needs focus.
    editing_note: Option<usize>,
    note_focus: bool,
    /// World position of the last right-click on the empty canvas.
    menu_at: [f32; 2],
}

#[derive(Default)]
pub struct CanvasOutcome {
    pub add_at: Option<[f32; 2]>,
    pub error: Option<String>,
    pub duplicate: bool,
    pub delete: bool,
    /// Open the node dictionary on this kind.
    pub help: Option<NodeKind>,
    /// Open the node dictionary index.
    pub help_index: bool,
    /// Toggle 無効 (bypass) on the selection.
    pub toggle_off: bool,
    /// Make a group from the selection.
    pub group: bool,
    /// Open the 値を見る window for the selection.
    pub view_values: bool,
    /// Add an empty note at this world position.
    pub add_note_at: Option<[f32; 2]>,
}

/// Screen rectangle of a group: the union of its live members plus a label band.
fn group_rect(p: &Project, members: &[NodeId], rect: Rect) -> Option<Rect> {
    let mut bounds = Rect::NOTHING;
    for id in members {
        if let Some(n) = p.graph.node(*id) {
            bounds = bounds.union(Rect::from_min_size(
                to_screen(n.position, rect, p.pan, p.zoom),
                egui::vec2(WIDTH, height(n)) * p.zoom,
            ));
        }
    }
    if !bounds.is_positive() {
        return None;
    }
    let pad = 14. * p.zoom;
    Some(Rect::from_min_max(
        bounds.min - egui::vec2(pad, pad + 24. * p.zoom),
        bounds.max + egui::vec2(pad, pad),
    ))
}
const NOTE_WIDTH: f32 = 210.0;
fn note_fill() -> Color32 {
    Color32::from_rgb(255, 248, 214)
}

fn height(node: &Node) -> f32 {
    HEADER + ROW * node.kind.spec().inputs.len() as f32 + 27.0
}
fn to_screen(world: [f32; 2], rect: Rect, pan: [f32; 2], zoom: f32) -> Pos2 {
    rect.min + egui::vec2(world[0] * zoom + pan[0], world[1] * zoom + pan[1])
}
fn to_world(screen: Pos2, rect: Rect, pan: [f32; 2], zoom: f32) -> [f32; 2] {
    let p = (screen - rect.min - egui::vec2(pan[0], pan[1])) / zoom;
    [p.x, p.y]
}
fn output_pos(n: &Node, rect: Rect, pan: [f32; 2], zoom: f32) -> Pos2 {
    to_screen(
        [n.position[0] + WIDTH, n.position[1] + HEADER + 14.],
        rect,
        pan,
        zoom,
    )
}
fn input_pos(n: &Node, index: usize, rect: Rect, pan: [f32; 2], zoom: f32) -> Pos2 {
    to_screen(
        [
            n.position[0],
            n.position[1] + HEADER + 14. + index as f32 * ROW,
        ],
        rect,
        pan,
        zoom,
    )
}

impl GraphCanvas {
    pub fn center_for_add(&self, p: &Project) -> [f32; 2] {
        self.last_rect
            .map(|r| to_world(r.center(), r, p.pan, p.zoom))
            .unwrap_or([50., 50.])
    }
    /// Graph position under a screen point (used to place a dropped file's node).
    pub fn world_at(&self, p: &Project, screen: Pos2) -> Option<[f32; 2]> {
        self.last_rect.map(|r| to_world(screen, r, p.pan, p.zoom))
    }
    pub fn select_only(&mut self, id: NodeId) {
        self.selected.clear();
        self.selected.insert(id);
    }
    pub fn fit(&self, project: &mut Project, rect: Rect) {
        if project.graph.nodes.is_empty() {
            project.pan = [40., 60.];
            project.zoom = 1.;
            return;
        }
        let mut bounds = Rect::NOTHING;
        for n in &project.graph.nodes {
            bounds = bounds.union(Rect::from_min_size(
                egui::pos2(n.position[0], n.position[1]),
                egui::vec2(WIDTH, height(n)),
            ));
        }
        project.zoom = ((rect.width() - 80.) / bounds.width().max(1.))
            .min((rect.height() - 110.) / bounds.height().max(1.))
            .clamp(0.35, 1.15);
        let p = rect.size() / 2. - bounds.center().to_vec2() * project.zoom;
        project.pan = [p.x, p.y];
    }
    pub fn auto_arrange(&mut self, p: &mut Project) {
        let Ok(order) = p.graph.topological_order() else {
            return;
        };
        let mut depth = BTreeMap::<NodeId, usize>::new();
        let mut rows = BTreeMap::<usize, f32>::new();
        for id in order {
            let d = p
                .graph
                .node(id)
                .map(|n| {
                    n.inputs
                        .values()
                        .filter_map(|s| depth.get(s))
                        .copied()
                        .max()
                        .map_or(0, |d| d + 1)
                })
                .unwrap_or(0);
            depth.insert(id, d);
            let y = rows.entry(d).or_insert(30.);
            if let Some(n) = p.graph.node_mut(id) {
                n.position = [30. + d as f32 * 285., *y];
                *y += height(n) + 45.;
            }
        }
        self.fit_requested = true;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        p: &mut Project,
        evaluation: Option<&Evaluation>,
        stale: bool,
    ) -> CanvasOutcome {
        let mut out = CanvasOutcome::default();
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.last_rect = Some(rect);
        if self.fit_requested {
            self.fit(p, rect);
            self.fit_requested = false;
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, theme::CANVAS);
        let zoom = p.zoom;
        let spacing = 24. * zoom;
        if spacing >= 8. {
            let sx = rect.left() + p.pan[0].rem_euclid(spacing);
            let sy = rect.top() + p.pan[1].rem_euclid(spacing);
            let mut x = sx;
            while x < rect.right() {
                let mut y = sy;
                while y < rect.bottom() {
                    painter.circle_filled(egui::pos2(x, y), 0.8, Color32::from_rgb(205, 214, 207));
                    y += spacing;
                }
                x += spacing;
            }
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut card_under = false;
        if let Some(pos) = pointer {
            card_under = p.graph.nodes.iter().any(|n| {
                Rect::from_min_size(
                    to_screen(n.position, rect, p.pan, zoom),
                    egui::vec2(WIDTH, height(n)) * zoom,
                )
                .expand(12.)
                .contains(pos)
            });
        }
        if response.hovered() && !ui.ctx().wants_keyboard_input() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1
                && let Some(pos) = pointer
            {
                let old = to_world(pos, rect, p.pan, p.zoom);
                p.zoom = (p.zoom * (scroll * 0.002).exp()).clamp(0.3, 1.65);
                let v = pos - rect.min - egui::vec2(old[0], old[1]) * p.zoom;
                p.pan = [v.x, v.y];
            }
        }
        let pan_active = ui.input(|i| {
            i.pointer.middle_down() || (i.key_down(egui::Key::Space) && i.pointer.primary_down())
        });
        if response.hovered() && pan_active {
            let d = ui.input(|i| i.pointer.delta());
            p.pan[0] += d.x;
            p.pan[1] += d.y;
            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
        }
        if response.double_clicked()
            && !card_under
            && let Some(pos) = pointer
        {
            out.add_at = Some(to_world(pos, rect, p.pan, p.zoom));
        }
        if response.secondary_clicked()
            && let Some(pos) = pointer
        {
            self.menu_at = to_world(pos, rect, p.pan, p.zoom);
        }
        let menu_at = self.menu_at;
        let has_selection = !self.selected.is_empty();
        response.context_menu(|ui| {
            if ui.button("ここにノードを追加").clicked() {
                out.add_at = Some(menu_at);
                ui.close();
            }
            if ui.button("ここにメモを追加").clicked() {
                out.add_note_at = Some(menu_at);
                ui.close();
            }
            if has_selection && ui.button("選択をグループにする  Ctrl+G").clicked() {
                out.group = true;
                ui.close();
            }
            if ui.button("ノード辞典  F1").clicked() {
                out.help_index = true;
                ui.close();
            }
        });
        self.draw_groups(ui, &painter, p, rect);
        self.draw_notes(ui, &painter, p, rect);
        if response.clicked() && !card_under {
            if !ui.input(|i| i.modifiers.shift) {
                self.selected.clear();
            }
            if self.connecting.take().is_none() {
                self.box_start = None;
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary) && !card_under && !pan_active {
            self.box_start = pointer;
        }
        if let (Some(a), Some(b)) = (self.box_start, pointer) {
            if ui.input(|i| i.pointer.primary_down()) {
                let box_rect = Rect::from_two_pos(a, b);
                painter.rect_filled(
                    box_rect,
                    2,
                    Color32::from_rgba_unmultiplied(30, 130, 105, 20),
                );
                painter.rect_stroke(
                    box_rect,
                    2,
                    Stroke::new(1_f32, theme::TEAL),
                    egui::StrokeKind::Inside,
                );
                if !ui.input(|i| i.modifiers.shift) {
                    self.selected.clear();
                }
                for n in &p.graph.nodes {
                    let nr = Rect::from_min_size(
                        to_screen(n.position, rect, p.pan, zoom),
                        egui::vec2(WIDTH, height(n)) * zoom,
                    );
                    if box_rect.intersects(nr) {
                        self.selected.insert(n.id);
                    }
                }
            } else {
                self.box_start = None;
            }
        }
        let mut to_connect = None;
        let mut to_disconnect = None;
        // Curves stay behind the cards. Their shape is independent of graph evaluation.
        for n in &p.graph.nodes {
            for (key, from) in &n.inputs {
                if let (Some(src), Some(index)) = (
                    p.graph.node(*from),
                    n.kind.spec().inputs.iter().position(|s| s.key == key),
                ) {
                    let a = output_pos(src, rect, p.pan, p.zoom);
                    let b = input_pos(n, index, rect, p.pan, p.zoom);
                    let selected = self.selected.contains(&n.id) || self.selected.contains(from);
                    let color = if selected {
                        theme::TEAL
                    } else {
                        Color32::from_rgb(156, 180, 169)
                    };
                    let shape = evaluation
                        .and_then(|e| e.outputs.get(from))
                        .and_then(|r| r.as_ref().ok())
                        .map(data_view::shape_of);
                    wire(&painter, a, b, shape, color, selected, p.zoom);
                }
            }
        }
        let nodes = p.graph.nodes.clone();
        for n in &nodes {
            let pos = to_screen(n.position, rect, p.pan, p.zoom);
            let nr = Rect::from_min_size(pos, egui::vec2(WIDTH, height(n)) * p.zoom);
            if !rect.intersects(nr.expand(15.)) {
                continue;
            }
            let selected = self.selected.contains(&n.id);
            let col = theme::category_color(n.kind.spec().category);
            let error = evaluation
                .and_then(|e| e.outputs.get(&n.id))
                .and_then(|r| r.as_ref().err());
            painter.rect_filled(
                nr.translate(egui::vec2(0., 3.)),
                9,
                Color32::from_rgba_unmultiplied(39, 60, 48, 15),
            );
            painter.rect_filled(
                nr,
                9,
                if n.off {
                    Color32::from_rgb(234, 236, 232)
                } else {
                    Color32::from_rgb(253, 254, 251)
                },
            );
            painter.rect_stroke(
                nr,
                9,
                Stroke::new(
                    if selected { 2. } else { 1_f32 },
                    if selected {
                        theme::TEAL
                    } else if error.is_some() {
                        theme::ERROR
                    } else {
                        Color32::from_rgb(211, 220, 211)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    pos + egui::vec2(12., HEADER - 2.) * p.zoom,
                    pos + egui::vec2(WIDTH - 12., HEADER - 2.) * p.zoom,
                ],
                Stroke::new(1_f32, theme::LINE),
            );
            let col = if n.off { Color32::from_gray(170) } else { col };
            painter.circle_filled(pos + egui::vec2(19., 21.) * p.zoom, 4.4 * p.zoom, col);
            painter.text(
                pos + egui::vec2(31., 19.) * p.zoom,
                Align2::LEFT_CENTER,
                if n.off {
                    format!("{}（無効）", n.name)
                } else {
                    n.name.clone()
                },
                FontId::proportional(13.5 * p.zoom),
                if n.off { theme::MUTED } else { theme::INK },
            );
            let header = Rect::from_min_size(pos, egui::vec2(WIDTH - 26., HEADER) * p.zoom);
            let hr = ui.interact(
                header,
                Id::new(("node_header", n.id)),
                Sense::click_and_drag(),
            );
            if hr.clicked() {
                if ui.input(|i| i.modifiers.shift) {
                    if !self.selected.remove(&n.id) {
                        self.selected.insert(n.id);
                    }
                } else {
                    self.select_only(n.id);
                }
            }
            if hr.drag_started() && !pan_active {
                if !selected {
                    self.select_only(n.id);
                }
                self.dragging = Some(n.id);
            }
            if hr.dragged() && self.dragging == Some(n.id) && !pan_active {
                let d = ui.input(|i| i.pointer.delta()) / p.zoom;
                for id in &self.selected {
                    if let Some(m) = p.graph.node_mut(*id) {
                        m.position[0] = (m.position[0] + d.x).clamp(-1e6, 1e6);
                        m.position[1] = (m.position[1] + d.y).clamp(-1e6, 1e6);
                    }
                }
            }
            hr.context_menu(|ui| {
                if ui.button("このノードを選択").clicked() {
                    self.select_only(n.id);
                    ui.close();
                }
                if ui.button("複製  Ctrl+D").clicked() {
                    self.select_only(n.id);
                    out.duplicate = true;
                    ui.close();
                }
                if ui.button("削除  Delete").clicked() {
                    self.select_only(n.id);
                    out.delete = true;
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(if n.off {
                        "有効にする  Ctrl+E"
                    } else {
                        "無効にする（出力なし）  Ctrl+E"
                    })
                    .clicked()
                {
                    if !self.selected.contains(&n.id) {
                        self.select_only(n.id);
                    }
                    out.toggle_off = true;
                    ui.close();
                }
                if ui.button("値を見る").clicked() {
                    self.select_only(n.id);
                    out.view_values = true;
                    ui.close();
                }
                if ui.button("グループにする  Ctrl+G").clicked() {
                    if !self.selected.contains(&n.id) {
                        self.select_only(n.id);
                    }
                    out.group = true;
                    ui.close();
                }
                if ui.button("ヘルプ  F1").clicked() {
                    out.help = Some(n.kind);
                    ui.close();
                }
            });
            if selected {
                let q = Rect::from_center_size(
                    pos + egui::vec2(WIDTH - 38., 20.) * p.zoom,
                    egui::vec2(18., 18.) * p.zoom,
                );
                painter.circle_stroke(q.center(), 7. * p.zoom, Stroke::new(1_f32, theme::MUTED));
                painter.text(
                    q.center(),
                    Align2::CENTER_CENTER,
                    "?",
                    FontId::proportional(10.5 * p.zoom),
                    theme::MUTED,
                );
                if ui
                    .interact(q, Id::new(("node_help", n.id)), Sense::click())
                    .on_hover_text("このノードのヘルプ · F1")
                    .clicked()
                {
                    out.help = Some(n.kind);
                }
            }
            let eye = Rect::from_center_size(
                pos + egui::vec2(WIDTH - 15., 20.) * p.zoom,
                egui::vec2(20., 22.) * p.zoom,
            );
            painter.circle_stroke(
                eye.center(),
                4.5 * p.zoom,
                Stroke::new(1_f32, if n.preview { col } else { theme::MUTED }),
            );
            if n.preview {
                painter.circle_filled(eye.center(), 2. * p.zoom, col);
            }
            if ui
                .interact(eye, Id::new(("node_preview", n.id)), Sense::click())
                .on_hover_text("表示対象を切替（「表示中」モード）")
                .clicked()
                && let Some(m) = p.graph.node_mut(n.id)
            {
                m.preview = !m.preview;
            }
            for (index, spec) in n.kind.spec().inputs.iter().enumerate() {
                let ip = input_pos(n, index, rect, p.pan, p.zoom);
                let linked = n.inputs.contains_key(spec.key);
                let can = self
                    .connecting
                    .and_then(|id| p.graph.node(id))
                    .map(|v| spec.kind.accepts(v.kind.spec().output));
                let port_col = if can == Some(false) {
                    Color32::from_gray(195)
                } else if can == Some(true) {
                    theme::TEAL
                } else {
                    col
                };
                painter.circle_filled(
                    ip,
                    5. * p.zoom,
                    if linked { port_col } else { Color32::WHITE },
                );
                painter.circle_stroke(ip, 5. * p.zoom, Stroke::new(1.6_f32, port_col));
                let pr = ui.interact(
                    Rect::from_center_size(ip, egui::vec2(22., 24.) * p.zoom),
                    Id::new(("in", n.id, spec.key)),
                    Sense::click_and_drag(),
                );
                if (pr.clicked() || pr.hovered() && ui.input(|i| i.pointer.any_released()))
                    && self.connecting.is_some()
                {
                    to_connect = self.connecting.take().map(|from| (from, n.id, spec.key));
                }
                if pr.secondary_clicked() && linked {
                    to_disconnect = Some((n.id, spec.key));
                }
                pr.on_hover_text(format!(
                    "{} · {}{}",
                    spec.label,
                    spec.kind.label(),
                    if linked {
                        "\n右クリックで切断"
                    } else {
                        "\n出力からドラッグ、または出力→入力の順にクリック"
                    }
                ));
                painter.text(
                    ip + egui::vec2(13., 0.) * p.zoom,
                    Align2::LEFT_CENTER,
                    spec.label,
                    FontId::proportional(11. * p.zoom),
                    if linked { theme::TEAL } else { theme::MUTED },
                );
                if p.zoom > 0.62
                    && let Some(default) = spec.default
                {
                    let vr = Rect::from_min_size(
                        ip + egui::vec2(122., -10.) * p.zoom,
                        egui::vec2(77., 22.) * p.zoom,
                    );
                    if linked {
                        let value = n
                            .inputs
                            .get(spec.key)
                            .and_then(|id| evaluation.and_then(|e| e.outputs.get(id)))
                            .and_then(|r| r.as_ref().ok())
                            .map(|t| {
                                if t.item_count() == 1 {
                                    t.branches
                                        .values()
                                        .flatten()
                                        .next()
                                        .map(|v| match v {
                                            morphlet::model::Value::Number(n) => {
                                                format!("{n:.2}")
                                            }
                                            _ => "接続済み".into(),
                                        })
                                        .unwrap_or_default()
                                } else {
                                    format!("{} 個", t.item_count())
                                }
                            })
                            .unwrap_or_else(|| "接続済み".into());
                        painter.text(
                            vr.center(),
                            Align2::CENTER_CENTER,
                            value,
                            FontId::proportional(10. * p.zoom),
                            theme::TEAL,
                        );
                    } else if let Some(m) = p.graph.node_mut(n.id) {
                        let value = m.params.entry(spec.key.into()).or_insert(default);
                        let drag = egui::DragValue::new(value)
                            .speed(spec.step)
                            .range(spec.min..=spec.max)
                            .max_decimals(if spec.integer { 0 } else { 3 });
                        if ui.put(vr, drag).changed() && spec.integer {
                            *value = value.round();
                        }
                    }
                }
            }
            let op = output_pos(n, rect, p.pan, p.zoom);
            painter.circle_filled(op, 6. * p.zoom, col);
            painter.circle_filled(op, 2. * p.zoom, Color32::WHITE);
            let or = ui.interact(
                Rect::from_center_size(op, egui::vec2(24., 24.) * p.zoom),
                Id::new(("out", n.id)),
                Sense::click_and_drag(),
            );
            if or.clicked() || or.drag_started() {
                self.connecting = Some(n.id);
            }
            let shape = evaluation
                .and_then(|e| e.outputs.get(&n.id))
                .and_then(|r| r.as_ref().ok())
                .map(data_view::shape_of);
            or.on_hover_text(format!(
                "{}の出力 · {}{}",
                n.name,
                n.kind.spec().output.label(),
                shape
                    .map(|s| format!("\n形: {}", s.describe()))
                    .unwrap_or_default()
            ));
            if let Some(shape) = shape {
                let galley = painter.layout_no_wrap(
                    shape.badge(),
                    FontId::proportional(9.5 * p.zoom),
                    Color32::WHITE,
                );
                let badge = Rect::from_min_size(
                    op + egui::vec2(5., -20.) * p.zoom,
                    galley.size() + egui::vec2(8., 3.) * p.zoom,
                );
                painter.rect_filled(
                    badge,
                    4,
                    match shape {
                        DataShape::Empty => Color32::from_gray(165),
                        DataShape::Item => col,
                        DataShape::List(_) => col.gamma_multiply(0.85),
                        DataShape::Tree { .. } => Color32::from_rgb(70, 84, 104),
                    },
                );
                painter.galley(
                    badge.min + egui::vec2(4., 1.5) * p.zoom,
                    galley,
                    Color32::WHITE,
                );
            }
            let text = if stale {
                "更新中".to_owned()
            } else if n.off {
                "無効 · 出力なし".into()
            } else if error.is_some() {
                "! 入力を確認".into()
            } else if let Some(Ok(tree)) = evaluation.and_then(|e| e.outputs.get(&n.id)) {
                format!("{} 枝  /  {} 個", tree.branches.len(), tree.item_count())
            } else {
                "入力待ち".into()
            };
            painter.text(
                nr.left_bottom() + egui::vec2(13., -13.) * p.zoom,
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(10. * p.zoom),
                if error.is_some() {
                    theme::ERROR
                } else {
                    theme::MUTED
                },
            );
            if let Some(message) = error {
                let strip = Rect::from_min_size(
                    nr.left_bottom() + egui::vec2(0., 4. * p.zoom),
                    egui::vec2(WIDTH, 22.) * p.zoom,
                );
                painter.rect_filled(strip, 6, Color32::from_rgb(252, 231, 224));
                painter.text(
                    strip.left_center() + egui::vec2(8. * p.zoom, 0.),
                    Align2::LEFT_CENTER,
                    short(message, 22),
                    FontId::proportional(10. * p.zoom),
                    theme::ERROR,
                );
                ui.interact(
                    Rect::from_min_max(
                        nr.left_bottom() - egui::vec2(0., 25. * p.zoom),
                        strip.right_bottom(),
                    ),
                    Id::new(("error", n.id)),
                    Sense::hover(),
                )
                .on_hover_text(message);
            }
        }
        if let Some((from, to, key)) = to_connect
            && let Err(e) = p.graph.connect(from, to, key)
        {
            out.error = Some(e);
        }
        if let Some((to, key)) = to_disconnect {
            p.graph.disconnect(to, key);
        }
        if let (Some(id), Some(pointer)) = (self.connecting, pointer)
            && let Some(n) = p.graph.node(id)
        {
            bezier(
                &painter,
                output_pos(n, rect, p.pan, p.zoom),
                pointer,
                Stroke::new(2_f32, theme::TEAL),
            );
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.connecting = None;
            self.box_start = None;
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.dragging = None;
        }
        if p.graph.nodes.is_empty() {
            painter.text(
                rect.center() - egui::vec2(0., 25.),
                Align2::CENTER_CENTER,
                "ここから、形をつくろう。",
                FontId::proportional(24.),
                theme::INK,
            );
            painter.text(
                rect.center() + egui::vec2(0., 15.),
                Align2::CENTER_CENTER,
                "左のライブラリから追加、またはダブルクリックで検索",
                FontId::proportional(12.),
                theme::MUTED,
            );
        }
        let hint = if self.connecting.is_some() {
            "入力の丸をクリックして接続  ·  Esc で中止"
        } else {
            "ドラッグで移動  ·  ホイールで拡大  ·  中ボタンでパン  ·  線: 細=1個  二重=リスト  点線=ツリー  ·  右クリックでメモ"
        };
        painter.rect_filled(
            Rect::from_min_max(
                rect.left_bottom() - egui::vec2(0., 29.),
                rect.right_bottom(),
            ),
            0,
            theme::CANVAS.gamma_multiply(0.98),
        );
        painter.text(
            rect.left_bottom() + egui::vec2(16., -15.),
            Align2::LEFT_CENTER,
            hint,
            FontId::proportional(10.5),
            theme::MUTED,
        );
        out
    }
}

impl GraphCanvas {
    /// Coloured rectangles behind member nodes. The label band selects (click), moves
    /// (drag) and edits (right-click: rename, colour, ungroup) the group.
    fn draw_groups(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        p: &mut Project,
        rect: Rect,
    ) {
        let mut remove = None;
        for gi in 0..p.graph.groups.len() {
            let Some(group) = p.graph.groups.get(gi).cloned() else {
                continue;
            };
            let Some(gr) = group_rect(p, &group.nodes, rect) else {
                continue;
            };
            if !rect.intersects(gr) {
                continue;
            }
            let [r, g, b] = group.color;
            let color = Color32::from_rgb(r, g, b);
            painter.rect_filled(gr, 12, Color32::from_rgba_unmultiplied(r, g, b, 30));
            painter.rect_stroke(
                gr,
                12,
                Stroke::new(1_f32, color.gamma_multiply(0.7)),
                egui::StrokeKind::Inside,
            );
            let band = Rect::from_min_size(gr.min, egui::vec2(gr.width(), 24. * p.zoom));
            painter.text(
                band.left_center() + egui::vec2(10. * p.zoom, 0.),
                Align2::LEFT_CENTER,
                if group.name.is_empty() {
                    "グループ"
                } else {
                    group.name.as_str()
                },
                FontId::proportional(12. * p.zoom),
                color,
            );
            let lr = ui
                .interact(band, Id::new(("group_band", gi)), Sense::click_and_drag())
                .on_hover_text(
                    "クリックで中のノードを選択 · ドラッグで移動 · 右クリックで名前・色・解除",
                );
            if lr.clicked() || lr.drag_started() {
                self.selected = group
                    .nodes
                    .iter()
                    .copied()
                    .filter(|id| p.graph.node(*id).is_some())
                    .collect();
            }
            if lr.dragged() {
                let d = ui.input(|i| i.pointer.delta()) / p.zoom;
                for id in &group.nodes {
                    if let Some(m) = p.graph.node_mut(*id) {
                        m.position[0] = (m.position[0] + d.x).clamp(-1e6, 1e6);
                        m.position[1] = (m.position[1] + d.y).clamp(-1e6, 1e6);
                    }
                }
            }
            lr.context_menu(|ui| {
                if let Some(g) = p.graph.groups.get_mut(gi) {
                    ui.label(theme::small("グループ名"));
                    if ui.text_edit_singleline(&mut g.name).changed()
                        && g.name.len() > MAX_GROUP_NAME_BYTES
                    {
                        g.name = annotations::truncate_bytes(&g.name, MAX_GROUP_NAME_BYTES);
                    }
                    ui.horizontal(|ui| {
                        for c in GROUP_COLORS {
                            let (sw, resp) =
                                ui.allocate_exact_size(egui::vec2(18., 18.), Sense::click());
                            ui.painter().circle_filled(
                                sw.center(),
                                7.,
                                Color32::from_rgb(c[0], c[1], c[2]),
                            );
                            if g.color == c {
                                ui.painter().circle_stroke(
                                    sw.center(),
                                    8.5,
                                    Stroke::new(1.5_f32, theme::INK),
                                );
                            }
                            if resp.clicked() {
                                g.color = c;
                            }
                        }
                    });
                }
                if ui.button("グループを解除（ノードは残す）").clicked() {
                    remove = Some(gi);
                    ui.close();
                }
            });
        }
        if let Some(gi) = remove
            && gi < p.graph.groups.len()
        {
            p.graph.groups.remove(gi);
        }
    }

    /// Sticky notes: drag to move, double-click to edit, right-click to delete.
    fn draw_notes(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        p: &mut Project,
        rect: Rect,
    ) {
        let mut remove = None;
        let width = NOTE_WIDTH * p.zoom;
        for ni in 0..p.graph.notes.len() {
            let Some(note) = p.graph.notes.get(ni).cloned() else {
                continue;
            };
            let pos = to_screen(note.position, rect, p.pan, p.zoom);
            let editing = self.editing_note == Some(ni);
            let (text, color) = if note.text.is_empty() {
                ("ダブルクリックでメモを書く".to_owned(), theme::MUTED)
            } else {
                (note.text.clone(), theme::INK)
            };
            let galley = painter.layout(
                text,
                FontId::proportional(12. * p.zoom),
                color,
                width - 20. * p.zoom,
            );
            let h =
                (galley.size().y + 20. * p.zoom).max((if editing { 90. } else { 40. }) * p.zoom);
            let nr = Rect::from_min_size(pos, egui::vec2(width, h));
            if !rect.intersects(nr.expand(10.)) {
                continue;
            }
            painter.rect_filled(
                nr.translate(egui::vec2(0., 2.)),
                6,
                Color32::from_black_alpha(14),
            );
            painter.rect_filled(nr, 6, note_fill());
            painter.rect_stroke(
                nr,
                6,
                Stroke::new(1_f32, Color32::from_rgb(226, 206, 140)),
                egui::StrokeKind::Inside,
            );
            if editing {
                if let Some(n) = p.graph.notes.get_mut(ni) {
                    let te = ui.put(
                        nr.shrink(6. * p.zoom),
                        egui::TextEdit::multiline(&mut n.text)
                            .font(FontId::proportional(12. * p.zoom))
                            .frame(false)
                            .desired_width(width - 12. * p.zoom),
                    );
                    if n.text.len() > MAX_NOTE_BYTES {
                        n.text = annotations::truncate_bytes(&n.text, MAX_NOTE_BYTES);
                    }
                    if self.note_focus {
                        te.request_focus();
                        self.note_focus = false;
                    } else if te.lost_focus() {
                        self.editing_note = None;
                    }
                }
                continue;
            }
            painter.galley(nr.min + egui::vec2(10., 10.) * p.zoom, galley, color);
            let r = ui
                .interact(nr, Id::new(("note", ni)), Sense::click_and_drag())
                .on_hover_text("ドラッグで移動 · ダブルクリックで編集 · 右クリックで削除");
            if r.double_clicked() {
                self.editing_note = Some(ni);
                self.note_focus = true;
            }
            if r.dragged() {
                let d = ui.input(|i| i.pointer.delta()) / p.zoom;
                if let Some(n) = p.graph.notes.get_mut(ni) {
                    n.position[0] = (n.position[0] + d.x).clamp(-1e6, 1e6);
                    n.position[1] = (n.position[1] + d.y).clamp(-1e6, 1e6);
                }
            }
            r.context_menu(|ui| {
                if ui.button("編集").clicked() {
                    self.editing_note = Some(ni);
                    self.note_focus = true;
                    ui.close();
                }
                if ui.button("メモを削除").clicked() {
                    remove = Some(ni);
                    ui.close();
                }
            });
        }
        if let Some(ni) = remove
            && ni < p.graph.notes.len()
        {
            p.graph.notes.remove(ni);
            self.editing_note = None;
        }
        if self.editing_note.is_some_and(|i| i >= p.graph.notes.len()) {
            self.editing_note = None;
        }
    }
    /// Start editing a note that was just created.
    pub fn edit_note(&mut self, index: usize) {
        self.editing_note = Some(index);
        self.note_focus = true;
    }
}

/// First `max` characters of `text`, with an ellipsis when cut.
fn short(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().nth(max).is_some() {
        out.push('…');
    }
    out
}

fn cubic(a: Pos2, b: Pos2, samples: usize) -> Vec<Pos2> {
    let handle = ((b.x - a.x).abs() * 0.5).clamp(40., 160.);
    let (p0, p1, p2, p3) = (a, a + Vec2::new(handle, 0.), b - Vec2::new(handle, 0.), b);
    (0..=samples)
        .map(|i| {
            let t = i as f32 / samples.max(1) as f32;
            let u = 1. - t;
            let v = p0.to_vec2() * (u * u * u)
                + p1.to_vec2() * (3. * u * u * t)
                + p2.to_vec2() * (3. * u * t * t)
                + p3.to_vec2() * (t * t * t);
            v.to_pos2()
        })
        .collect()
}

/// A wire styled by the shape of the data flowing through it:
/// one item = thin solid, list = double line, tree = dashed, empty = thin grey dots.
fn wire(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    shape: Option<DataShape>,
    color: Color32,
    selected: bool,
    zoom: f32,
) {
    let w = if selected { 2.4 } else { 1.7_f32 };
    match shape {
        Some(DataShape::List(_)) => {
            bezier(painter, a, b, Stroke::new(w + 2.8, color));
            bezier(painter, a, b, Stroke::new(1.3_f32, theme::CANVAS));
        }
        Some(DataShape::Tree { .. }) => {
            let points = cubic(a, b, 40);
            painter.extend(Shape::dashed_line(
                &points,
                Stroke::new(w + 0.6, color),
                8. * zoom.max(0.5),
                5. * zoom.max(0.5),
            ));
        }
        Some(DataShape::Empty) => {
            let points = cubic(a, b, 40);
            painter.extend(Shape::dashed_line(
                &points,
                Stroke::new(1.2_f32, Color32::from_gray(175)),
                3.,
                4.,
            ));
        }
        _ => bezier(painter, a, b, Stroke::new(w, color)),
    }
}

fn bezier(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let handle = ((b.x - a.x).abs() * 0.5).clamp(40., 160.);
    painter.add(Shape::CubicBezier(
        egui::epaint::CubicBezierShape::from_points_stroke(
            [a, a + Vec2::new(handle, 0.), b - Vec2::new(handle, 0.), b],
            false,
            Color32::TRANSPARENT,
            stroke,
        ),
    ));
}
