use crate::data_view::{self, DataShape};
use crate::theme;
use crate::ui_polish;
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
/// Snap grid in world units (matches the dot grid drawn at zoom 1).
pub const GRID: f32 = 24.0;
/// How many numeric inputs of a card are drawn as sliders.
const CARD_SLIDERS: usize = 3;

#[derive(Default)]
pub struct GraphCanvas {
    pub selected: BTreeSet<NodeId>,
    pub connecting: Option<NodeId>,
    pub fit_requested: bool,
    pub fit_selection_requested: bool,
    pub last_rect: Option<Rect>,
    /// 格子に吸着: nodes snap to the grid when a drag ends.
    pub snap: bool,
    /// Hide the navigation overlay when editing dense graphs.
    pub hide_minimap: bool,
    dragging: Option<NodeId>,
    /// A wire was pulled out of an output port and the mouse button is still held.
    wire_drag: bool,
    reconnect_origin: Option<WireLink>,
    box_start: Option<Pos2>,
    box_base: BTreeSet<NodeId>,
    /// Note being edited in place, and whether its text box still needs focus.
    editing_note: Option<usize>,
    note_focus: bool,
    /// World position of the last right-click on the empty canvas.
    menu_at: [f32; 2],
    menu_wire: Option<WireLink>,
}
/// A particular edge, even when the source feeds multiple targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireLink {
    pub from: NodeId,
    pub to: NodeId,
    pub input: String,
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
    /// A wire was dropped on empty canvas: the source node and the world position.
    pub drop_wire: Option<(NodeId, [f32; 2])>,
    pub insert_wire: Option<(WireLink, [f32; 2])>,
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
    if theme::is_dark() {
        Color32::from_rgb(72, 66, 42)
    } else {
        Color32::from_rgb(255, 248, 214)
    }
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
    /// Moves every selected node onto the nearest grid point (used when 吸着 is on).
    fn snap_selected(&self, p: &mut Project) {
        for id in &self.selected {
            if let Some(m) = p.graph.node_mut(*id) {
                m.position = [
                    ui_polish::snap(m.position[0], GRID),
                    ui_polish::snap(m.position[1], GRID),
                ];
            }
        }
    }
    pub fn fit(&self, project: &mut Project, rect: Rect) {
        self.fit_nodes(project, rect, false);
    }
    fn fit_nodes(&self, project: &mut Project, rect: Rect, selected_only: bool) {
        if project.graph.nodes.is_empty() {
            project.pan = [40., 60.];
            project.zoom = 1.;
            return;
        }
        let mut bounds = Rect::NOTHING;
        for n in &project.graph.nodes {
            if selected_only && !self.selected.contains(&n.id) { continue; }
            bounds = bounds.union(Rect::from_min_size(
                egui::pos2(n.position[0], n.position[1]),
                egui::vec2(WIDTH, height(n)),
            ));
        }
        if bounds.is_positive() {
            (project.pan, project.zoom) = fit_transform(bounds, rect);
        }
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
    /// Align the selected cards along their left/top edge, without changing edges.
    pub fn align_selected(&self, p: &mut Project, axis: usize) {
        if axis > 1 || self.selected.len() < 2 { return; }
        let target = p.graph.nodes.iter().filter(|n| self.selected.contains(&n.id))
            .map(|n| n.position[axis]).reduce(f32::min);
        if let Some(target) = target {
            for node in &mut p.graph.nodes {
                if self.selected.contains(&node.id) { node.position[axis] = target; }
            }
        }
    }

    /// Equalize the gaps between selected card bounds, rather than their centers.
    pub fn distribute_selected(&self, p: &mut Project, axis: usize) {
        if axis > 1 { return; }
        let mut cards: Vec<_> = p.graph.nodes.iter().filter(|n| self.selected.contains(&n.id))
            .map(|n| (n.id, n.position[axis], if axis == 0 { WIDTH } else { height(n) })).collect();
        if cards.len() < 3 { return; }
        cards.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let start = cards[0].1;
        let last = cards[cards.len() - 1];
        let extent: f32 = cards.iter().map(|c| c.2).sum();
        let gap = ((last.1 + last.2 - start - extent) / (cards.len() - 1) as f32).max(0.);
        let mut at = start;
        for (id, _, size) in cards {
            if let Some(node) = p.graph.node_mut(id) { node.position[axis] = at.clamp(-1e6, 1e6); }
            at += size + gap;
        }
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
        if self.fit_selection_requested {
            self.fit_nodes(p, rect, true);
            self.fit_selection_requested = false;
            self.fit_requested = false;
        } else if self.fit_requested {
            self.fit(p, rect);
            self.fit_requested = false;
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let map_rect = (!self.hide_minimap && p.graph.nodes.len() >= 10
            && rect.width() >= 340. && rect.height() >= 220.
            && self.connecting.is_none() && self.dragging.is_none() && self.box_start.is_none())
            .then(|| Rect::from_min_size(rect.right_bottom() - egui::vec2(166., 138.), egui::vec2(154., 96.)));
        let over_map = pointer.is_some_and(|pos| map_rect.is_some_and(|r| r.contains(pos)));
        if !over_map && response.hovered() && !ui.ctx().wants_keyboard_input() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1
                && let Some(pos) = pointer
            {
                zoom_about(p, rect, pos, scroll);
            }
        }
        let pan_active = ui.input(|i| {
            i.pointer.middle_down() || (i.key_down(egui::Key::Space) && i.pointer.primary_down())
        });
        if !over_map && response.hovered() && pan_active {
            let d = ui.input(|i| i.pointer.delta());
            p.pan[0] = (p.pan[0] + d.x).clamp(-1.0e7, 1.0e7);
            p.pan[1] = (p.pan[1] + d.y).clamp(-1.0e7, 1.0e7);
            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, theme::canvas_bg());
        let zoom = p.zoom;
        let spacing = GRID * zoom;
        if spacing >= 8. {
            let sx = rect.left() + p.pan[0].rem_euclid(spacing);
            let sy = rect.top() + p.pan[1].rem_euclid(spacing);
            let mut x = sx;
            while x < rect.right() {
                let mut y = sy;
                while y < rect.bottom() {
                    painter.circle_filled(egui::pos2(x, y), 0.8, theme::grid_dot());
                    y += spacing;
                }
                x += spacing;
            }
        }
        let mut card_under = over_map;
        if let Some(pos) = pointer {
            card_under |= p.graph.nodes.iter().any(|n| {
                Rect::from_min_size(
                    to_screen(n.position, rect, p.pan, zoom),
                    egui::vec2(WIDTH, height(n)) * zoom,
                )
                .expand(12.)
                .contains(pos)
            });
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
            self.menu_wire = if card_under { None } else { nearest_wire(p, rect, pos) };
        }
        let menu_at = self.menu_at;
        let has_selection = !self.selected.is_empty();
        let menu_wire = self.menu_wire.clone();
        response.context_menu(|ui| {
            if let Some(link) = &menu_wire {
                ui.label(theme::small("接続線"));
                if ui.button("ワイヤーにノードを挿入").clicked() {
                    out.insert_wire = Some((link.clone(), menu_at));
                    ui.close();
                }
                if ui.button("ワイヤーを削除").clicked() {
                    if p.graph.node(link.to).and_then(|n| n.inputs.get(&link.input)) == Some(&link.from) {
                        p.graph.disconnect(link.to, &link.input);
                    }
                    ui.close();
                }
                ui.separator();
            }
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
            self.box_base = if ui.input(|i| i.modifiers.shift) { self.selected.clone() } else { BTreeSet::new() };
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
                    Stroke::new(1_f32, theme::teal()),
                    egui::StrokeKind::Inside,
                );
                self.selected.clone_from(&self.box_base);
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
        let mut reconnect_from: Option<WireLink> = None;
        // Index the endpoints once instead of searching the entire graph for each wire.
        let sources: BTreeMap<_, _> = p.graph.nodes.iter().map(|n| (n.id, n)).collect();
        for n in &p.graph.nodes {
            for (key, from) in &n.inputs {
                if let (Some(src), Some(index)) = (
                    sources.get(from).copied(),
                    n.kind.spec().inputs.iter().position(|s| s.key == key),
                ) {
                    let a = output_pos(src, rect, p.pan, p.zoom);
                    let b = input_pos(n, index, rect, p.pan, p.zoom);
                    if !wire_bounds(a, b).intersects(rect) {
                        continue;
                    }
                    let selected = self.selected.contains(&n.id) || self.selected.contains(from);
                    let color = if selected {
                        theme::teal()
                    } else {
                        theme::wire_idle()
                    };
                    let shape = evaluation
                        .and_then(|e| e.outputs.get(from))
                        .and_then(|r| r.as_ref().ok())
                        .map(data_view::shape_of);
                    wire(&painter, a, b, shape, color, selected, p.zoom);
                }
            }
        }
        // Avoid cloning invisible nodes, including potentially large script strings.
        let nodes: Vec<_> = p.graph.nodes.iter().filter(|n| {
            Rect::from_min_size(
                to_screen(n.position, rect, p.pan, p.zoom),
                egui::vec2(WIDTH, height(n)) * p.zoom,
            ).expand(28.).intersects(rect)
        }).cloned().collect();
        for n in &nodes {
            let pos = to_screen(n.position, rect, p.pan, p.zoom);
            let nr = Rect::from_min_size(pos, egui::vec2(WIDTH, height(n)) * p.zoom);
            if !rect.intersects(nr.expand(28.)) {
                continue;
            }
            let selected = self.selected.contains(&n.id);
            let col = theme::category_color(n.kind.spec().category);
            let overview = p.zoom < 0.38 && self.connecting.is_none();
            let error = evaluation.filter(|_| !stale && !n.off)
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
                    theme::card_off_bg()
                } else {
                    theme::card_bg()
                },
            );
            painter.rect_stroke(
                nr,
                9,
                Stroke::new(
                    if selected { 2. } else { 1_f32 },
                    if selected {
                        theme::teal()
                    } else if error.is_some() {
                        theme::error()
                    } else {
                        theme::card_border()
                    },
                ),
                egui::StrokeKind::Inside,
            );
            // Header tinted with the category colour, so categories are visible at a glance.
            let header_band = Rect::from_min_size(pos, egui::vec2(WIDTH, HEADER) * p.zoom);
            painter.rect_filled(
                header_band,
                egui::CornerRadius {
                    nw: 9,
                    ne: 9,
                    sw: 0,
                    se: 0,
                },
                col.gamma_multiply(if n.off { 0.06 } else { 0.18 }),
            );
            painter.line_segment(
                [
                    pos + egui::vec2(12., HEADER - 2.) * p.zoom,
                    pos + egui::vec2(WIDTH - 12., HEADER - 2.) * p.zoom,
                ],
                Stroke::new(1_f32, theme::line()),
            );
            let col = if n.off { Color32::from_gray(170) } else { col };
            painter.circle_filled(pos + egui::vec2(19., 21.) * p.zoom, 4.4 * p.zoom, col);
            if !overview {
            painter.with_clip_rect(nr).text(
                pos + egui::vec2(31., 19.) * p.zoom,
                Align2::LEFT_CENTER,
                ui_polish::one_line(&n.name, 10),
                FontId::proportional(13.5 * p.zoom),
                if n.off { theme::muted() } else { theme::ink() },
            );
            }
            let header = if overview { nr } else { Rect::from_min_size(pos, egui::vec2(WIDTH - 26., HEADER) * p.zoom) };
            let hr = ui.interact(
                header,
                Id::new(("node_header", n.id)),
                Sense::click_and_drag(),
            );
            if hr.clicked() && !over_map {
                if ui.input(|i| i.modifiers.shift) {
                    if !self.selected.remove(&n.id) {
                        self.selected.insert(n.id);
                    }
                } else {
                    self.select_only(n.id);
                }
            }
            if hr.drag_started() && !pan_active && !over_map {
                if !selected {
                    self.select_only(n.id);
                }
                self.dragging = Some(n.id);
            }
            if hr.dragged() && self.dragging == Some(n.id) && !pan_active && !over_map {
                let d = ui.input(|i| i.pointer.delta()) / p.zoom;
                for id in &self.selected {
                    if let Some(m) = p.graph.node_mut(*id) {
                        m.position[0] = (m.position[0] + d.x).clamp(-1e6, 1e6);
                        m.position[1] = (m.position[1] + d.y).clamp(-1e6, 1e6);
                    }
                }
            }
            if hr.drag_stopped() && self.dragging == Some(n.id) && self.snap {
                self.snap_selected(p);
            }
            hr.clone().on_hover_text(format!("{} · {}", n.name, n.kind.spec().description));
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
            if overview {
                // At overview zoom, keep card silhouettes, selection and wires, not
                // illegible labels/sliders. Zoom in or Shift+F to edit.
                continue;
            }
            if selected {
                let q = Rect::from_center_size(
                    pos + egui::vec2(WIDTH - 38., 20.) * p.zoom,
                    egui::vec2(18., 18.) * p.zoom,
                );
                painter.circle_stroke(q.center(), 7. * p.zoom, Stroke::new(1_f32, theme::muted()));
                painter.text(
                    q.center(),
                    Align2::CENTER_CENTER,
                    "?",
                    FontId::proportional(10.5 * p.zoom),
                    theme::muted(),
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
                Stroke::new(1_f32, if n.preview { col } else { theme::muted() }),
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
            let slider_keys: Vec<&str> = n
                .kind
                .spec()
                .inputs
                .iter()
                .filter(|s| s.default.is_some() && !n.inputs.contains_key(s.key))
                .take(CARD_SLIDERS)
                .map(|s| s.key)
                .collect();
            for (index, spec) in n.kind.spec().inputs.iter().enumerate() {
                let ip = input_pos(n, index, rect, p.pan, p.zoom);
                let linked = n.inputs.contains_key(spec.key);
                let can = self
                    .connecting
                    .and_then(|id| p.graph.node(id))
                    .map(|v| v.id != n.id && spec.kind.accepts(v.kind.spec().output));
                let port_col = if can == Some(false) {
                    theme::muted()
                } else if can == Some(true) {
                    theme::teal()
                } else {
                    theme::port_color(spec.kind)
                };
                painter.circle_filled(
                    ip,
                    5. * p.zoom,
                    if linked { port_col } else { theme::card_bg() },
                );
                painter.circle_stroke(ip, 5. * p.zoom, Stroke::new(1.6_f32, port_col));
                if can == Some(true) {
                    painter.circle_stroke(ip, 8. * p.zoom, Stroke::new(1.4, theme::teal()));
                }
                let pr = ui.interact(
                    Rect::from_center_size(ip, egui::vec2((22. * p.zoom).max(12.), (24. * p.zoom).max(12.))),
                    Id::new(("in", n.id, spec.key)),
                    Sense::click_and_drag(),
                );
                if !over_map && (pr.clicked() || pr.hovered() && ui.input(|i| i.pointer.primary_released()))
                    && self.connecting.is_some()
                {
                    to_connect = self.connecting.take().map(|from| (from, n.id, spec.key));
                }
                if !over_map && pr.drag_started_by(egui::PointerButton::Primary) && linked
                    && self.connecting.is_none()
                    && let Some(from) = n.inputs.get(spec.key)
                {
                    reconnect_from = Some(WireLink { from: *from, to: n.id, input: spec.key.into() });
                }
                if pr.secondary_clicked() && linked && !over_map {
                    to_disconnect = Some((n.id, spec.key));
                }
                pr.on_hover_text(format!(
                    "{} · {}{}{}",
                    spec.label,
                    spec.kind.label(),
                    if can == Some(false) { "\n現在の出力と型が合わないか、同じノードです。" } else if can == Some(true) { "\n接続候補（循環の有無は接続時に確認）" } else { "" },
                    if linked {
                        "\n右クリックで切断 · ドラッグで接続先を変更"
                    } else {
                        "\n出力からドラッグ、または出力→入力の順にクリック"
                    }
                ));
                painter.text(
                    ip + egui::vec2(13., 0.) * p.zoom,
                    Align2::LEFT_CENTER,
                    ui_polish::one_line(spec.label, 6),
                    FontId::proportional(11. * p.zoom),
                    if linked { theme::teal() } else { theme::muted() },
                );
                if p.zoom > 0.62
                    && let Some(default) = spec.default
                    && slider_keys.contains(&spec.key)
                    && !linked
                    && let Some(m) = p.graph.node_mut(n.id)
                {
                    // The first numeric inputs of a card are drawn as small sliders.
                    let value = m.params.entry(spec.key.into()).or_insert(default);
                    let (low, high) =
                        crate::app::friendly_range(spec.key, spec.min, spec.max, *value);
                    let track = Rect::from_min_size(
                        ip + egui::vec2(88., -9.) * p.zoom,
                        egui::vec2(60., 18.) * p.zoom,
                    );
                    let width = 60. * p.zoom;
                    let slider = egui::Slider::new(&mut *value, low..=high)
                        .show_value(false)
                        .trailing_fill(true)
                        .step_by(spec.step);
                    let response = ui
                        .scope(|ui| {
                            ui.spacing_mut().slider_width = width;
                            ui.put(track, slider)
                        })
                        .inner;
                    if response.changed() && spec.integer {
                        *value = value.round();
                    }
                    let shown = if spec.integer {
                        format!("{:.0}", *value)
                    } else {
                        format!("{:.2}", *value)
                    };
                    painter.text(
                        ip + egui::vec2(153., 0.) * p.zoom,
                        Align2::LEFT_CENTER,
                        shown,
                        FontId::proportional(10. * p.zoom),
                        theme::ink(),
                    );
                } else if p.zoom > 0.62
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
                            theme::teal(),
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
            painter.circle_filled(op, 6. * p.zoom, theme::port_color(n.kind.spec().output));
            painter.circle_filled(op, 2. * p.zoom, Color32::WHITE);
            let or = ui.interact(
                Rect::from_center_size(op, egui::vec2(24., 24.) * p.zoom),
                Id::new(("out", n.id)),
                Sense::click_and_drag(),
            );
            if !over_map && (or.clicked() || or.drag_started()) {
                self.reconnect_origin = None;
                self.connecting = Some(n.id);
            }
            if !over_map && or.drag_started() {
                self.wire_drag = true;
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
            let text = if stale {
                "更新中".to_owned()
            } else if n.off {
                "無効 · 出力なし".into()
            } else if let Some(message) = error {
                format!("! {}", short(message, 15))
            } else if let Some(Ok(tree)) = evaluation.and_then(|e| e.outputs.get(&n.id)) {
                format!("{} · {} 枝 / {} 個", n.kind.spec().output.label(), tree.branches.len(), tree.item_count())
            } else {
                "入力待ち".into()
            };
            painter.with_clip_rect(nr).text(
                nr.left_bottom() + egui::vec2(13., -13.) * p.zoom,
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(10. * p.zoom),
                if error.is_some() {
                    theme::error()
                } else {
                    theme::muted()
                },
            );
            if let Some(message) = error {
                ui.interact(
                    Rect::from_min_max(nr.left_bottom() - egui::vec2(0., 25. * p.zoom), nr.right_bottom()),
                    Id::new(("error", n.id)),
                    Sense::hover(),
                )
                .on_hover_text(message);
            }
        }
        if let Some(link) = reconnect_from {
            self.connecting = Some(link.from);
            self.reconnect_origin = Some(link);
            self.wire_drag = true;
        }
        if let Some((from, to, key)) = to_connect {
            let result = if let Some(old) = self.reconnect_origin.take() {
                reconnect_wire(&mut p.graph, &old, to, key)
            } else {
                p.graph.connect(from, to, key)
            };
            if let Err(e) = result { out.error = Some(e); }
        }
        if let Some((to, key)) = to_disconnect {
            p.graph.disconnect(to, key);
        }
        // A wire released on empty canvas (not on a port or a card) opens the node search,
        // and the chosen node gets connected to the wire's source.
        if self.wire_drag && !ui.input(|i| i.pointer.primary_down()) {
            self.wire_drag = false;
            let source = self.connecting.take();
            // A cancelled reconnect must preserve the original wire.
            let was_reconnect = self.reconnect_origin.take().is_some();
            if !was_reconnect && !card_under
                && let Some(at) = pointer.filter(|pos| rect.contains(*pos))
                && let Some(from) = source
            {
                out.drop_wire = Some((from, to_world(at, rect, p.pan, p.zoom)));
            }
        }
        if let (Some(id), Some(pointer)) = (self.connecting, pointer)
            && let Some(n) = p.graph.node(id)
        {
            bezier(
                &painter,
                output_pos(n, rect, p.pan, p.zoom),
                pointer,
                Stroke::new(2_f32, theme::teal()),
            );
            painter.text(pointer + egui::vec2(12., -10.), Align2::LEFT_BOTTOM,
                n.kind.spec().output.label(), FontId::proportional(12.), theme::port_color(n.kind.spec().output));
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.connecting = None;
            self.reconnect_origin = None;
            self.wire_drag = false;
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
                theme::ink(),
            );
            painter.text(
                rect.center() + egui::vec2(0., 15.),
                Align2::CENTER_CENTER,
                "左のライブラリから追加、またはダブルクリックで検索",
                FontId::proportional(12.),
                theme::muted(),
            );
        }
        let hint = if self.connecting.is_some() {
            "入力の丸をクリックして接続  ·  Esc で中止  ·  空いた所で離すと検索して追加"
        } else {
            "ドラッグで移動  ·  ホイールで拡大  ·  中ボタンでパン  ·  線: 細=1個  二重=リスト  点線=ツリー  ·  ? でショートカット一覧"
        };
        painter.rect_filled(
            Rect::from_min_max(
                rect.left_bottom() - egui::vec2(0., 29.),
                rect.right_bottom(),
            ),
            0,
            theme::canvas_bg().gamma_multiply(0.98),
        );
        painter.text(
            rect.left_bottom() + egui::vec2(16., -15.),
            Align2::LEFT_CENTER,
            if rect.width() < 520. {
                if self.connecting.is_some() { "入力へ接続 · Esc で中止" } else { "Tab: 追加 · ?: 操作一覧" }
            } else { hint },
            FontId::proportional(10.5),
            theme::muted(),
        );
        if let Some(map) = map_rect {
            self.draw_minimap(ui, &painter, p, rect, map);
        }
        out
    }

    fn draw_minimap(&self, ui: &mut egui::Ui, painter: &egui::Painter, p: &mut Project, canvas: Rect, map: Rect) {
        let bounds = p.graph.nodes.iter().fold(Rect::NOTHING, |b, n| {
            b.union(Rect::from_min_size(Pos2::new(n.position[0], n.position[1]), egui::vec2(WIDTH, height(n))))
        });
        if !bounds.is_positive() { return; }
        let inner = map.shrink(8.);
        let scale = (inner.width() / bounds.width()).min(inner.height() / bounds.height());
        let project = |w: Pos2| inner.center() + (w - bounds.center()) * scale;
        let response = ui.interact(map, Id::new("canvas_minimap"), Sense::click_and_drag())
            .on_hover_text("ミニマップ：クリック・ドラッグで表示位置を移動");
        if (response.clicked() || response.dragged_by(egui::PointerButton::Primary))
            && let Some(pos) = response.interact_pointer_pos()
        {
            let world = bounds.center() + (pos - inner.center()) / scale;
            let pan = canvas.size() / 2. - world.to_vec2() * p.zoom;
            p.pan = [pan.x.clamp(-1e7, 1e7), pan.y.clamp(-1e7, 1e7)];
            ui.ctx().request_repaint();
        }
        painter.rect_filled(map, 7, theme::panel_bg());
        painter.rect_stroke(map, 7, Stroke::new(1., theme::card_border()), egui::StrokeKind::Inside);
        let mini = painter.with_clip_rect(inner);
        for node in &p.graph.nodes {
            let start = project(Pos2::new(node.position[0], node.position[1]));
            let size = egui::vec2((WIDTH * scale).max(1.), (height(node) * scale).max(1.));
            mini.rect_filled(Rect::from_min_size(start, size), 1,
                if self.selected.contains(&node.id) { theme::teal() } else { theme::category_color(node.kind.spec().category).gamma_multiply(0.65) });
        }
        let a = to_world(canvas.min, canvas, p.pan, p.zoom);
        let b = to_world(canvas.max, canvas, p.pan, p.zoom);
        let view = Rect::from_two_pos(project(Pos2::new(a[0], a[1])), project(Pos2::new(b[0], b[1])));
        if view.intersects(inner) {
            mini.rect_stroke(view.intersect(inner), 2, Stroke::new(1.5, theme::teal()), egui::StrokeKind::Inside);
        }
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
                                    Stroke::new(1.5_f32, theme::ink()),
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
                ("ダブルクリックでメモを書く".to_owned(), theme::muted())
            } else {
                (note.text.clone(), theme::ink())
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

fn fit_transform(bounds: Rect, rect: Rect) -> ([f32; 2], f32) {
    let zoom = ((rect.width() - 48.).max(1.) / bounds.width().max(1.))
        .min((rect.height() - 64.).max(1.) / bounds.height().max(1.))
        .clamp(0.1, 1.15);
    let pan = rect.size() / 2. - bounds.center().to_vec2() * zoom;
    ([pan.x.clamp(-1.0e7, 1.0e7), pan.y.clamp(-1.0e7, 1.0e7)], zoom)
}

fn zoom_about(p: &mut Project, rect: Rect, pos: Pos2, scroll: f32) {
    let old = to_world(pos, rect, p.pan, p.zoom);
    p.zoom = (p.zoom * (scroll.clamp(-500., 500.) * 0.002).exp()).clamp(0.1, 2.5);
    let v = pos - rect.min - egui::vec2(old[0], old[1]) * p.zoom;
    p.pan = [v.x.clamp(-1.0e7, 1.0e7), v.y.clamp(-1.0e7, 1.0e7)];
}

// A cubic Bezier lies inside the bounding box of its four control points.
fn wire_bounds(a: Pos2, b: Pos2) -> Rect {
    Rect::from_two_pos(a, b).expand2(egui::vec2(165., 8.))
}

/// Hit testing uses the same Bezier as rendering, and picks the closest edge.
fn nearest_wire(p: &Project, rect: Rect, pointer: Pos2) -> Option<WireLink> {
    let sources: BTreeMap<_, _> = p.graph.nodes.iter().map(|n| (n.id, n)).collect();
    let mut nearest = None;
    let mut best = 7.0_f32;
    for node in &p.graph.nodes {
        for (input, from) in &node.inputs {
            let Some(source) = sources.get(from).copied() else { continue; };
            let Some(index) = node.kind.spec().inputs.iter().position(|s| s.key == input) else { continue; };
            let a = output_pos(source, rect, p.pan, p.zoom);
            let b = input_pos(node, index, rect, p.pan, p.zoom);
            if !wire_bounds(a,b).contains(pointer) { continue; }
            let points = cubic(a, b, 32);
            for pair in points.windows(2) {
                let d = pair[1] - pair[0];
                let t = ((pointer - pair[0]).dot(d) / d.length_sq().max(0.001)).clamp(0., 1.);
                let distance = pointer.distance(pair[0] + d * t);
                if distance < best {
                    best = distance;
                    nearest = Some(WireLink { from: *from, to: node.id, input: input.clone() });
                }
            }
        }
    }
    nearest
}

/// Rewire atomically, preserving the old edge if the new target is invalid.
fn reconnect_wire(graph: &mut morphlet::model::Graph, old: &WireLink, to: NodeId, input: &str) -> Result<(), String> {
    if graph.node(old.to).and_then(|n| n.inputs.get(&old.input)) != Some(&old.from) {
        return Err("元のワイヤーが変更されています。".into());
    }
    let mut candidate = graph.clone();
    candidate.disconnect(old.to, &old.input);
    candidate.connect(old.from, to, input)?;
    candidate.validate()?;
    *graph = candidate;
    Ok(())
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
            bezier(painter, a, b, Stroke::new(1.3_f32, theme::canvas_bg()));
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

#[cfg(test)]
mod premium_tests {
    use super::*;

    #[test]
    fn reconnect_preserves_edge_on_failure_and_moves_it_on_success() {
        let mut graph = morphlet::model::Graph::default();
        let source = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let a = graph.add(NodeKind::Add, [300., 0.]).unwrap();
        let b = graph.add(NodeKind::Add, [600., 0.]).unwrap();
        let key = NodeKind::Add.spec().inputs[0].key;
        graph.connect(source, a, key).unwrap();
        let old = WireLink { from: source, to: a, input: key.into() };
        assert!(reconnect_wire(&mut graph, &old, b, "missing").is_err());
        assert_eq!(graph.node(a).unwrap().inputs.get(key), Some(&source));
        reconnect_wire(&mut graph, &old, b, key).unwrap();
        assert!(graph.node(a).unwrap().inputs.get(key).is_none());
        assert_eq!(graph.node(b).unwrap().inputs.get(key), Some(&source));
    }

    #[test]
    fn logical_canvas_size_matrix_draws_without_panics() {
        for count in [10, 100, 1000] {
            let base = crate::ui_fixture::make(count).unwrap();
            for (w, h) in [(1280., 720.), (1920., 1080.), (2560., 1600.)] {
                for scale in [1.0, 1.5, 2.0] {
                    let mut project = base.clone();
                    let mut canvas = GraphCanvas { fit_requested: true, ..Default::default() };
                    let ctx = egui::Context::default();
                    let logical = Rect::from_min_size(Pos2::ZERO, egui::vec2(w / scale, h / scale));
                    let output = ctx.run(egui::RawInput { screen_rect: Some(logical), ..Default::default() }, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            canvas.show(ui, &mut project, None, false);
                        });
                    });
                    assert!(!output.shapes.is_empty(), "{count} nodes at {w}x{h}, scale {scale}");
                    assert_eq!(project.graph.nodes.len(), count);
                    project.validate().unwrap();
                }
            }
        }
    }

    #[test]
    fn selected_alignment_and_spacing_preserve_unselected_cards() {
        let mut project = Project::new("test", Default::default());
        let a = project.graph.add(NodeKind::Number, [0., 40.]).unwrap();
        let b = project.graph.add(NodeKind::Number, [270., 20.]).unwrap();
        let c = project.graph.add(NodeKind::Number, [900., 80.]).unwrap();
        let d = project.graph.add(NodeKind::Number, [1200., 120.]).unwrap();
        let canvas = GraphCanvas { selected: [a,b,c].into_iter().collect(), ..Default::default() };
        canvas.align_selected(&mut project, 1);
        canvas.distribute_selected(&mut project, 0);
        for id in [a,b,c] { assert_eq!(project.graph.node(id).unwrap().position[1], 20.); }
        assert_eq!(project.graph.node(b).unwrap().position[0], 450.);
        assert_eq!(project.graph.node(d).unwrap().position, [1200., 120.]);
    }

    #[test]
    fn fit_is_finite_for_narrow_and_large_graphs() {
        let bounds = Rect::from_min_size(egui::pos2(-200., -400.), egui::vec2(8000., 6000.));
        for size in [[640., 360.], [1280., 720.], [1920., 1080.], [2560., 1600.]] {
            let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(size[0], size[1]));
            let (pan, zoom) = fit_transform(bounds, rect);
            assert!((0.1..=1.15).contains(&zoom));
            assert!(pan.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn zoom_preserves_the_world_point_under_the_pointer() {
        let mut p = Project::new("test", Default::default());
        let r = Rect::from_min_size(egui::pos2(40., 20.), egui::vec2(900., 700.));
        let point = egui::pos2(300., 320.);
        let old = to_world(point, r, p.pan, p.zoom);
        zoom_about(&mut p, r, point, 123.);
        let new = to_world(point, r, p.pan, p.zoom);
        assert!((old[0] - new[0]).abs() < 0.001);
        assert!((old[1] - new[1]).abs() < 0.001);
    }

    #[test]
    fn wire_culling_preserves_reverse_and_crossing_curves() {
        for (a, b) in [(egui::pos2(500., 100.), egui::pos2(-500., 400.)), (egui::pos2(-900., -10.), egui::pos2(900., 10.))] {
            let bounds = wire_bounds(a, b);
            assert!(cubic(a, b, 80).iter().all(|p| bounds.contains(*p)));
        }
    }
}
