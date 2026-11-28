use crate::data_view::{self, DataShape};
use crate::melt;
use crate::theme;
use crate::ui_polish;
use eframe::egui::epaint::{RectShape, TessellationOptions, Tessellator};
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use nodes::{
    annotations::{self, GROUP_COLORS, MAX_GROUP_NAME_BYTES, MAX_NOTE_BYTES},
    engine::Evaluation,
    model::{InputSpec, Node, NodeId, NodeKind},
    persistence::Project,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const WIDTH: f32 = 218.0;
const HEADER: f32 = 43.0;
const ROW: f32 = 28.0;
/// Size of a reroute dot, which is drawn small instead of as a full card.
const DOT_W: f32 = 34.0;
const DOT_H: f32 = 26.0;
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
    /// Hide the faint dot grid.
    pub hide_grid: bool,
    dragging: Option<NodeId>,
    /// A wire was pulled out of an output port and the mouse button is still held.
    wire_drag: bool,
    box_start: Option<Pos2>,
    box_base: BTreeSet<NodeId>,
    /// Note being edited in place, and whether its text box still needs focus.
    editing_note: Option<usize>,
    note_focus: bool,
    /// World position of the last right-click on the empty canvas.
    menu_at: [f32; 2],
    node_indices: NodeIndices,
    minimap: Option<MinimapDrawing>,
}

#[derive(Default)]
struct NodeIndices {
    order: Vec<NodeId>,
    by_id: HashMap<NodeId, usize>,
}

impl NodeIndices {
    fn update(&mut self, nodes: &[Node]) {
        // The project can be replaced or reordered by undo/import outside the canvas.
        // Only IDs and their order matter for this index; numeric edits and pans reuse it.
        if self.order.len() == nodes.len()
            && self.order.iter().copied().eq(nodes.iter().map(|n| n.id))
        {
            return;
        }
        self.order.clear();
        self.by_id.clear();
        self.order.extend(nodes.iter().map(|node| node.id));
        self.by_id.extend(
            self.order
                .iter()
                .copied()
                .enumerate()
                .map(|(i, id)| (id, i)),
        );
    }

    fn get(&self, id: NodeId) -> Option<usize> {
        self.by_id.get(&id).copied()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct MinimapNode {
    id: NodeId,
    position: [f32; 2],
    kind: NodeKind,
}

impl From<&Node> for MinimapNode {
    fn from(node: &Node) -> Self {
        Self {
            id: node.id,
            position: node.position,
            kind: node.kind,
        }
    }
}

struct MinimapDrawing {
    inner: Rect,
    dark: bool,
    pixels_per_point: f32,
    options: TessellationOptions,
    nodes: Vec<MinimapNode>,
    selected: BTreeSet<NodeId>,
    bounds: Rect,
    scale: f32,
    shape: Shape,
}

impl MinimapDrawing {
    fn project(&self, point: Pos2) -> Pos2 {
        self.inner.center() + (point - self.bounds.center()) * self.scale
    }

    fn new(
        nodes: &[Node],
        selected: &BTreeSet<NodeId>,
        inner: Rect,
        dark: bool,
        pixels_per_point: f32,
        options: TessellationOptions,
    ) -> Option<Self> {
        let bounds = nodes.iter().fold(Rect::NOTHING, |bounds, node| {
            bounds.union(Rect::from_min_size(node.position.into(), node_size(node)))
        });
        if !bounds.is_positive() {
            return None;
        }
        let scale = (inner.width() / bounds.width()).min(inner.height() / bounds.height());
        let mut drawing = Self {
            inner,
            dark,
            pixels_per_point,
            options,
            nodes: nodes.iter().map(MinimapNode::from).collect(),
            selected: selected.clone(),
            bounds,
            scale,
            shape: Shape::Noop,
        };
        // Cache the same rounded rectangles egui would tessellate on every pan.
        // These shapes only use the solid white texel, with no text or circle atlas.
        let mut tessellator = Tessellator::new(pixels_per_point, options, [1, 1], Vec::new());
        tessellator.set_clip_rect(inner);
        let mut mesh = egui::Mesh::default();
        for node in nodes {
            let start = drawing.project(node.position.into());
            let full = node_size(node);
            let size = egui::vec2((full.x * scale).max(1.), (full.y * scale).max(1.));
            let color = if selected.contains(&node.id) {
                theme::teal()
            } else {
                theme::category_color(node.kind.spec().category).gamma_multiply(0.65)
            };
            tessellator.tessellate_rect(
                &RectShape::filled(Rect::from_min_size(start, size), 1, color),
                &mut mesh,
            );
        }
        drawing.shape = Shape::mesh(mesh);
        Some(drawing)
    }
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
    /// 「サンプルを開く」 on the empty canvas.
    pub open_templates: bool,
}

/// Screen rectangle of a group: the union of its live members plus a label band.
fn group_rect(p: &Project, indices: &NodeIndices, members: &[NodeId], rect: Rect) -> Option<Rect> {
    let mut bounds = Rect::NOTHING;
    for id in members {
        if let Some(index) = indices.get(*id) {
            let n = &p.graph.nodes[index];
            bounds = bounds.union(Rect::from_min_size(
                to_screen(n.position, rect, p.pan, p.zoom),
                node_size(n) * p.zoom,
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
    if node.kind == NodeKind::Reroute {
        return DOT_H;
    }
    HEADER + ROW * node.kind.spec().inputs.len() as f32 + 27.0
}

fn node_size(node: &Node) -> Vec2 {
    if node.kind == NodeKind::Reroute {
        egui::vec2(DOT_W, DOT_H)
    } else {
        egui::vec2(WIDTH, height(node))
    }
}

fn parameter_number(
    ui: &mut egui::Ui,
    rect: Rect,
    node: NodeId,
    spec: &InputSpec,
    value: &mut f64,
    zoom: f32,
) -> egui::Response {
    let full_value = value.to_string();
    ui.scope_builder(
        egui::UiBuilder::new()
            // Auto IDs based on drawing order lose focus when cards are culled,
            // reordered, or a connection changes which inputs have sliders.
            .id(("canvas_parameter", node, spec.key))
            .max_rect(rect)
            .layout(egui::Layout::centered_and_justified(
                egui::Direction::TopDown,
            )),
        |ui| {
            // DragValue deliberately grows to fit its full precision string.
            // Clip painting AND hit testing so it cannot cover a card's ports.
            ui.set_clip_rect(ui.clip_rect().intersect(rect));
            ui.spacing_mut().interact_size = rect.size();
            ui.spacing_mut().button_padding = egui::vec2(2., 1.) * zoom;
            ui.style_mut().override_font_id = Some(FontId::proportional(10. * zoom));
            ui.style_mut().explanation_tooltips = false;
            ui.add(crate::app::parameter_drag_value(value, spec))
        },
    )
    .inner
    .on_hover_text(format!("{full_value}\nクリックで直接入力 · ドラッグで調整"))
}

fn to_screen(world: [f32; 2], rect: Rect, pan: [f32; 2], zoom: f32) -> Pos2 {
    rect.min + egui::vec2(world[0] * zoom + pan[0], world[1] * zoom + pan[1])
}
fn to_world(screen: Pos2, rect: Rect, pan: [f32; 2], zoom: f32) -> [f32; 2] {
    let p = (screen - rect.min - egui::vec2(pan[0], pan[1])) / zoom;
    [p.x, p.y]
}
fn output_pos(n: &Node, rect: Rect, pan: [f32; 2], zoom: f32) -> Pos2 {
    if n.kind == NodeKind::Reroute {
        return to_screen(
            [n.position[0] + DOT_W, n.position[1] + DOT_H / 2.],
            rect,
            pan,
            zoom,
        );
    }
    to_screen(
        [n.position[0] + WIDTH, n.position[1] + HEADER + 14.],
        rect,
        pan,
        zoom,
    )
}
fn input_pos(n: &Node, index: usize, rect: Rect, pan: [f32; 2], zoom: f32) -> Pos2 {
    if n.kind == NodeKind::Reroute {
        return to_screen([n.position[0], n.position[1] + DOT_H / 2.], rect, pan, zoom);
    }
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
            if let Some(index) = self.node_indices.get(*id) {
                let m = &mut p.graph.nodes[index];
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
            if selected_only && !self.selected.contains(&n.id) {
                continue;
            }
            bounds = bounds.union(Rect::from_min_size(
                egui::pos2(n.position[0], n.position[1]),
                node_size(n),
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
        if axis > 1 || self.selected.len() < 2 {
            return;
        }
        let target = p
            .graph
            .nodes
            .iter()
            .filter(|n| self.selected.contains(&n.id))
            .map(|n| n.position[axis])
            .reduce(f32::min);
        if let Some(target) = target {
            for node in &mut p.graph.nodes {
                if self.selected.contains(&node.id) {
                    node.position[axis] = target;
                }
            }
        }
    }

    /// Align the selected cards by `edge` (0 = left/top, 1 = centre, 2 = right/bottom) on `axis`
    /// (0 = horizontal, 1 = vertical), using each card's real size.
    pub fn align_selected_edge(&self, p: &mut Project, axis: usize, edge: u8) {
        if axis > 1 || edge > 2 || self.selected.len() < 2 {
            return;
        }
        let size_of = |n: &Node| if axis == 0 { WIDTH } else { height(n) };
        let cards: Vec<(NodeId, f32)> = p
            .graph
            .nodes
            .iter()
            .filter(|n| self.selected.contains(&n.id))
            .map(|n| (n.id, size_of(n)))
            .collect();
        if cards.len() < 2 {
            return;
        }
        let at = |p: &Project, id: NodeId| p.graph.node(id).map_or(0., |n| n.position[axis]);
        let target = match edge {
            0 => cards
                .iter()
                .map(|c| at(p, c.0))
                .fold(f32::INFINITY, f32::min),
            2 => cards
                .iter()
                .map(|c| at(p, c.0) + c.1)
                .fold(f32::NEG_INFINITY, f32::max),
            _ => cards.iter().map(|c| at(p, c.0) + c.1 / 2.).sum::<f32>() / cards.len() as f32,
        };
        if !target.is_finite() {
            return;
        }
        for (id, size) in cards {
            let shift = match edge {
                0 => 0.,
                2 => size,
                _ => size / 2.,
            };
            if let Some(node) = p.graph.node_mut(id) {
                node.position[axis] = (target - shift).clamp(-1e6, 1e6);
            }
        }
    }

    /// Equalize the gaps between selected card bounds, rather than their centers.
    pub fn distribute_selected(&self, p: &mut Project, axis: usize) {
        if axis > 1 {
            return;
        }
        let mut cards: Vec<_> = p
            .graph
            .nodes
            .iter()
            .filter(|n| self.selected.contains(&n.id))
            .map(|n| {
                (
                    n.id,
                    n.position[axis],
                    if axis == 0 { node_size(n).x } else { height(n) },
                )
            })
            .collect();
        if cards.len() < 3 {
            return;
        }
        cards.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let start = cards[0].1;
        let last = cards[cards.len() - 1];
        let extent: f32 = cards.iter().map(|c| c.2).sum();
        let gap = ((last.1 + last.2 - start - extent) / (cards.len() - 1) as f32).max(0.);
        let mut at = start;
        for (id, _, size) in cards {
            if let Some(node) = p.graph.node_mut(id) {
                node.position[axis] = at.clamp(-1e6, 1e6);
            }
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
        self.node_indices.update(&p.graph.nodes);
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
        let map_rect = (!self.hide_minimap
            && p.graph.nodes.len() >= 10
            && rect.width() >= 340.
            && rect.height() >= 220.
            && self.connecting.is_none()
            && self.dragging.is_none()
            && self.box_start.is_none())
        .then(|| {
            Rect::from_min_size(
                rect.right_bottom() - egui::vec2(166., 138.),
                egui::vec2(154., 96.),
            )
        });
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
        if !self.hide_grid && spacing >= 8. {
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
                    node_size(n) * zoom,
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
            self.box_base = if ui.input(|i| i.modifiers.shift) {
                self.selected.clone()
            } else {
                BTreeSet::new()
            };
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
                        node_size(n) * zoom,
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
        let mut to_insert: Option<(NodeId, NodeId, String, Pos2)> = None;
        for n in &p.graph.nodes {
            for (key, from) in &n.inputs {
                if let (Some(source_index), Some(index)) = (
                    self.node_indices.get(*from),
                    n.kind.spec().inputs.iter().position(|s| s.key == key),
                ) {
                    let src = &p.graph.nodes[source_index];
                    let a = output_pos(src, rect, p.pan, p.zoom);
                    let b = input_pos(n, index, rect, p.pan, p.zoom);
                    if !wire_bounds(a, b).intersects(rect) {
                        continue;
                    }
                    let selected = self.selected.contains(&n.id) || self.selected.contains(from);
                    // Per-input wire display (Grasshopper: Default / Faint / Hidden).
                    let display = n
                        .params
                        .get(&format!("{}{key}", nodes::data_tree::WIRE_PREFIX))
                        .copied()
                        .unwrap_or(0.);
                    if display == 2. && !selected {
                        continue;
                    }
                    let color = if theme::is_melt() {
                        melt::wire_color(src.kind.spec().output, selected)
                    } else if selected {
                        theme::teal()
                    } else {
                        theme::wire_idle()
                    };
                    let shape = evaluation
                        .and_then(|e| e.outputs.get(from))
                        .and_then(|r| r.as_ref().ok())
                        .map(data_view::shape_of);
                    let color = if display == 1. && !selected {
                        color.gamma_multiply(0.35)
                    } else {
                        color
                    };
                    wire(&painter, a, b, shape, color, selected, p.zoom);
                    // Hovering the middle of a wire offers a reroute dot at that spot.
                    if self.connecting.is_none()
                        && !over_map
                        && let Some(pos) = pointer
                    {
                        let mid = a.lerp(b, 0.5);
                        if pos.distance(mid) < 14. {
                            painter.circle_stroke(mid, 6., Stroke::new(1.4_f32, theme::teal()));
                            painter.circle_filled(mid, 2., theme::teal());
                            let hit = ui
                                .interact(
                                    Rect::from_center_size(mid, egui::vec2(18., 18.)),
                                    Id::new(("wire_dot", n.id, key.as_str())),
                                    Sense::click(),
                                )
                                .on_hover_text("クリックで中継点（リルート点）を置く");
                            if hit.clicked() {
                                to_insert = Some((*from, n.id, key.clone(), mid));
                            }
                        }
                    }
                }
            }
        }
        // Avoid cloning invisible nodes, including potentially large script strings.
        let cards: Vec<_> = p
            .graph
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, n)| {
                let bounds = Rect::from_min_size(
                    to_screen(n.position, rect, p.pan, p.zoom),
                    node_size(n) * p.zoom,
                );
                bounds
                    .expand(28.)
                    .intersects(rect)
                    .then(|| (index, n.clone(), bounds))
            })
            .collect();
        for (node_index, n, nr) in &cards {
            let pos = nr.min;
            let nr = *nr;
            let selected = self.selected.contains(&n.id);
            let col = theme::category_color(n.kind.spec().category);
            let lod = melt::lod(p.zoom, self.connecting.is_some());
            let overview = lod == melt::Lod::Shape;
            let error = evaluation
                .filter(|_| !stale && !n.off)
                .and_then(|e| e.outputs.get(&n.id))
                .and_then(|r| r.as_ref().err());
            if n.kind == NodeKind::Reroute {
                // A reroute dot: a small pill with an input on the left and an output on the right.
                let centre = nr.center();
                let ring = if selected {
                    theme::teal()
                } else if error.is_some() {
                    theme::error()
                } else {
                    theme::card_border()
                };
                painter.rect_filled(nr, 13, theme::card_bg());
                painter.rect_stroke(
                    nr,
                    13,
                    Stroke::new(if selected { 2. } else { 1_f32 }, ring),
                    egui::StrokeKind::Inside,
                );
                painter.circle_filled(centre, 4.4 * p.zoom, col);
                let body = ui.interact(
                    Rect::from_center_size(centre, egui::vec2(DOT_W - 14., DOT_H) * p.zoom),
                    Id::new(("node_header", n.id)),
                    Sense::click_and_drag(),
                );
                if body.clicked() && !over_map {
                    if ui.input(|i| i.modifiers.shift) {
                        if !self.selected.remove(&n.id) {
                            self.selected.insert(n.id);
                        }
                    } else {
                        self.select_only(n.id);
                    }
                }
                if body.drag_started() && !pan_active && !over_map {
                    if !selected {
                        self.select_only(n.id);
                    }
                    self.dragging = Some(n.id);
                }
                if body.dragged() && self.dragging == Some(n.id) && !pan_active && !over_map {
                    let d = ui.input(|i| i.pointer.delta()) / p.zoom;
                    for id in &self.selected {
                        if let Some(index) = self.node_indices.get(*id) {
                            let m = &mut p.graph.nodes[index];
                            m.position[0] = (m.position[0] + d.x).clamp(-1e6, 1e6);
                            m.position[1] = (m.position[1] + d.y).clamp(-1e6, 1e6);
                        }
                    }
                }
                if body.drag_stopped() && self.dragging == Some(n.id) && self.snap {
                    self.snap_selected(p);
                }
                body.on_hover_text("リルート点 · ドラッグで移動 · Delete で削除");
                let linked = n.inputs.contains_key("value");
                let ip = input_pos(n, 0, rect, p.pan, p.zoom);
                let port_col = theme::port_color(nodes::model::DataType::Any);
                melt::port_marker(
                    &painter,
                    ip,
                    4. * p.zoom,
                    nodes::model::DataType::Any,
                    if linked { port_col } else { theme::card_bg() },
                    port_col,
                    1.4,
                );
                let pr = ui.interact(
                    Rect::from_center_size(ip, egui::vec2(16., 24.) * p.zoom.max(0.6)),
                    Id::new(("in", n.id, "value")),
                    Sense::click_and_drag(),
                );
                if !over_map
                    && (pr.clicked() || pr.hovered() && ui.input(|i| i.pointer.primary_released()))
                    && self.connecting.is_some()
                {
                    to_connect = self.connecting.take().map(|from| (from, n.id, "value"));
                }
                if pr.secondary_clicked() && linked && !over_map {
                    to_disconnect = Some((n.id, "value"));
                }
                let op = output_pos(n, rect, p.pan, p.zoom);
                melt::port_marker(
                    &painter,
                    op,
                    4. * p.zoom,
                    nodes::model::DataType::Any,
                    port_col,
                    port_col,
                    1.0,
                );
                let or = ui.interact(
                    Rect::from_center_size(op, egui::vec2(16., 24.) * p.zoom.max(0.6)),
                    Id::new(("out", n.id)),
                    Sense::click_and_drag(),
                );
                if !over_map && (or.clicked() || or.drag_started()) {
                    self.connecting = Some(n.id);
                }
                if !over_map && or.drag_started() {
                    self.wire_drag = true;
                }
                continue;
            }
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
            let header = if overview {
                nr
            } else {
                Rect::from_min_size(pos, egui::vec2(WIDTH - 26., HEADER) * p.zoom)
            };
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
                    if let Some(index) = self.node_indices.get(*id) {
                        let m = &mut p.graph.nodes[index];
                        m.position[0] = (m.position[0] + d.x).clamp(-1e6, 1e6);
                        m.position[1] = (m.position[1] + d.y).clamp(-1e6, 1e6);
                    }
                }
            }
            if hr.drag_stopped() && self.dragging == Some(n.id) && self.snap {
                self.snap_selected(p);
            }
            hr.clone()
                .on_hover_text(format!("{} · {}", n.name, n.kind.spec().description));
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
            if let Some(message) = error {
                let badge = melt::error_badge(&painter, nr, p.zoom);
                let id = n.id;
                ui.interact(badge, Id::new(("error_badge", id)), Sense::hover())
                    .on_hover_ui(|ui| {
                        let failed = |x: NodeId| {
                            evaluation
                                .and_then(|e| e.outputs.get(&x))
                                .is_some_and(|r| r.is_err())
                        };
                        let root = melt::root_cause(&p.graph, &failed, id);
                        let root_message = evaluation
                            .and_then(|e| e.outputs.get(&root))
                            .and_then(|r| r.as_ref().err())
                            .map_or(message.as_str(), String::as_str);
                        let name = p.graph.node(root).map_or("?", |m| m.name.as_str());
                        ui.label(ui_polish::one_line(
                            &format!("原因: 「{name}」{root_message}"),
                            80,
                        ));
                    });
            }
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
            melt::eye(
                &painter,
                Rect::from_center_size(eye.center(), egui::vec2(16., 16.) * p.zoom),
                n.preview,
                if n.preview { col } else { theme::muted() },
            );
            if ui
                .interact(eye, Id::new(("node_preview", n.id)), Sense::click())
                .on_hover_text("表示対象を切替（「表示中」モード）")
                .clicked()
            {
                let m = &mut p.graph.nodes[*node_index];
                m.preview = !m.preview;
            }
            if lod == melt::Lod::Header {
                // Header only while zoomed out: no ports, rows or footer.
                continue;
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
                    .and_then(|id| self.node_indices.get(id))
                    .map(|i| &p.graph.nodes[i])
                    .map(|v| v.id != n.id && spec.kind.accepts(v.kind.spec().output));
                let port_col = if can == Some(false) {
                    theme::muted()
                } else if can == Some(true) {
                    theme::teal()
                } else {
                    theme::port_color(spec.kind)
                };
                melt::port_marker(
                    &painter,
                    ip,
                    5. * p.zoom,
                    spec.kind,
                    if linked { port_col } else { theme::card_bg() },
                    port_col,
                    1.6,
                );
                if can == Some(true) {
                    painter.circle_stroke(ip, 8. * p.zoom, Stroke::new(1.4_f32, theme::teal()));
                }
                let pr = ui.interact(
                    Rect::from_center_size(
                        ip,
                        egui::vec2((22. * p.zoom).max(12.), (24. * p.zoom).max(12.)),
                    ),
                    Id::new(("in", n.id, spec.key)),
                    Sense::click_and_drag(),
                );
                if !over_map
                    && (pr.clicked() || pr.hovered() && ui.input(|i| i.pointer.primary_released()))
                    && self.connecting.is_some()
                {
                    to_connect = self.connecting.take().map(|from| (from, n.id, spec.key));
                }
                if !over_map {
                    // Right-click menu, like Grasshopper's parameter menu.
                    let mut cut = false;
                    pr.context_menu(|ui| {
                        cut = port_menu(ui, &mut p.graph.nodes[*node_index], spec.key, linked);
                    });
                    if cut {
                        to_disconnect = Some((n.id, spec.key));
                    }
                }
                pr.on_hover_text(format!(
                    "{} · {}{}{}",
                    spec.label,
                    spec.kind.label(),
                    if can == Some(false) {
                        "\n現在の出力と型が合わないか、同じノードです。"
                    } else if can == Some(true) {
                        "\n接続候補（循環の有無は接続時に確認）"
                    } else {
                        ""
                    },
                    if linked {
                        "\n右クリックで切断"
                    } else {
                        "\n出力からドラッグ、または出力→入力の順にクリック"
                    }
                ));
                painter.text(
                    ip + egui::vec2(13., 0.) * p.zoom,
                    Align2::LEFT_CENTER,
                    ui_polish::one_line(spec.label, 6),
                    FontId::proportional(11. * p.zoom),
                    if linked {
                        theme::teal()
                    } else {
                        theme::muted()
                    },
                );
                if p.zoom > 0.62
                    && let Some(default) = spec.default
                    && slider_keys.contains(&spec.key)
                    && !linked
                {
                    // The first numeric inputs have a track and a direct-entry field.
                    let m = &mut p.graph.nodes[*node_index];
                    let value = m.params.entry(spec.key.into()).or_insert(default);
                    let track = Rect::from_min_size(
                        ip + egui::vec2(88., -9.) * p.zoom,
                        egui::vec2(60., 18.) * p.zoom,
                    );
                    let width = 60. * p.zoom;
                    let slider = crate::app::parameter_slider(value, spec)
                        .show_value(false)
                        .trailing_fill(true);
                    ui.scope_builder(
                        egui::UiBuilder::new().id(("canvas_parameter_track", n.id, spec.key)),
                        |ui| {
                            ui.spacing_mut().slider_width = width;
                            ui.put(track, slider)
                        },
                    );
                    let number = Rect::from_min_size(
                        ip + egui::vec2(153., -10.) * p.zoom,
                        egui::vec2(48., 22.) * p.zoom,
                    );
                    parameter_number(ui, number, n.id, spec, value, p.zoom);
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
                                            nodes::model::Value::Number(n) => {
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
                    } else {
                        let m = &mut p.graph.nodes[*node_index];
                        let value = m.params.entry(spec.key.into()).or_insert(default);
                        parameter_number(ui, vr, n.id, spec, value, p.zoom);
                    }
                }
            }
            let op = output_pos(n, rect, p.pan, p.zoom);
            let out_col = theme::port_color(n.kind.spec().output);
            melt::port_marker(
                &painter,
                op,
                6. * p.zoom,
                n.kind.spec().output,
                out_col,
                out_col,
                1.0,
            );
            painter.circle_filled(op, 2. * p.zoom, Color32::WHITE);
            let or = ui.interact(
                Rect::from_center_size(op, egui::vec2(24., 24.) * p.zoom),
                Id::new(("out", n.id)),
                Sense::click_and_drag(),
            );
            if !over_map && (or.clicked() || or.drag_started()) {
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
                format!(
                    "{} · {} 枝 / {} 個",
                    n.kind.spec().output.label(),
                    tree.branches.len(),
                    tree.item_count()
                )
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
                    Rect::from_min_max(
                        nr.left_bottom() - egui::vec2(0., 25. * p.zoom),
                        nr.right_bottom(),
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
        if let Some((from, to, key, at)) = to_insert {
            let world = to_world(at, rect, p.pan, p.zoom);
            let position = [world[0] - DOT_W / 2., world[1] - DOT_H / 2.];
            match p.graph.add(NodeKind::Reroute, position) {
                Ok(id) => {
                    let linked = p
                        .graph
                        .connect(from, id, "value")
                        .and_then(|()| p.graph.connect(id, to, &key));
                    match linked {
                        Ok(()) => self.select_only(id),
                        Err(e) => {
                            p.graph.nodes.retain(|m| m.id != id);
                            out.error = Some(e);
                        }
                    }
                }
                Err(e) => out.error = Some(e),
            }
        }
        // A wire released on empty canvas (not on a port or a card) opens the node search,
        // and the chosen node gets connected to the wire's source.
        if self.wire_drag && !ui.input(|i| i.pointer.primary_down()) {
            self.wire_drag = false;
            let source = self.connecting.take();
            if !card_under
                && let Some(at) = pointer.filter(|pos| rect.contains(*pos))
                && let Some(from) = source
            {
                out.drop_wire = Some((from, to_world(at, rect, p.pan, p.zoom)));
            }
        }
        if let (Some(id), Some(pointer)) = (self.connecting, pointer)
            && let Some(index) = self.node_indices.get(id)
        {
            let n = &p.graph.nodes[index];
            bezier(
                &painter,
                output_pos(n, rect, p.pan, p.zoom),
                pointer,
                Stroke::new(2_f32, theme::teal()),
            );
            painter.text(
                pointer + egui::vec2(12., -10.),
                Align2::LEFT_BOTTOM,
                n.kind.spec().output.label(),
                FontId::proportional(12.),
                theme::port_color(n.kind.spec().output),
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
                rect.center() - egui::vec2(0., 62.),
                Align2::CENTER_CENTER,
                "ここから、形をつくろう。",
                FontId::proportional(24.),
                theme::ink(),
            );
            let area = Rect::from_center_size(rect.center(), egui::vec2(360., 40.));
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.add_space(((area.width() - 304.) / 2.).max(0.));
                    if melt::tool_button(
                        ui,
                        melt::Tool::Plus,
                        "ノードを追加",
                        "検索してノードを置く  Tab / Ctrl+K",
                        true,
                        true,
                    )
                    .clicked()
                    {
                        out.add_at = Some(to_world(rect.center(), rect, p.pan, p.zoom));
                    }
                    if melt::tool_button(
                        ui,
                        melt::Tool::Open,
                        "サンプルを開く",
                        "作品例から始める",
                        false,
                        true,
                    )
                    .clicked()
                    {
                        out.open_templates = true;
                    }
                },
            );
            painter.text(
                rect.center() + egui::vec2(0., 44.),
                Align2::CENTER_CENTER,
                "Tab・ダブルクリック・? でノードを検索（例: 円 / circle / twist）",
                FontId::proportional(12.),
                theme::muted(),
            );
        }
        let hint = if self.connecting.is_some() {
            "入力の丸をクリックして接続  ·  Esc で中止  ·  空いた所で離すと検索して追加"
        } else {
            "ドラッグで移動  ·  ホイールで拡大  ·  中ボタンでパン  ·  線: 細=1個  二重=リスト  点線=ツリー  ·  ? でノード検索"
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
                if self.connecting.is_some() {
                    "入力へ接続 · Esc で中止"
                } else {
                    "Tab: 追加 · ?: 操作一覧"
                }
            } else {
                hint
            },
            FontId::proportional(10.5),
            theme::muted(),
        );
        if let Some(map) = map_rect {
            self.draw_minimap(ui, &painter, p, rect, map);
        }
        out
    }

    fn draw_minimap(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        p: &mut Project,
        canvas: Rect,
        map: Rect,
    ) {
        let inner = map.shrink(8.);
        let dark = theme::is_dark();
        let pixels_per_point = ui.ctx().pixels_per_point();
        let options = ui.ctx().tessellation_options(|options| *options);
        if self.minimap.as_ref().is_none_or(|drawing| {
            drawing.inner != inner
                || drawing.dark != dark
                || drawing.pixels_per_point != pixels_per_point
                || drawing.options != options
                || drawing.selected != self.selected
                || drawing.nodes.len() != p.graph.nodes.len()
                || !p
                    .graph
                    .nodes
                    .iter()
                    .map(MinimapNode::from)
                    .eq(drawing.nodes.iter().copied())
        }) {
            self.minimap = MinimapDrawing::new(
                &p.graph.nodes,
                &self.selected,
                inner,
                dark,
                pixels_per_point,
                options,
            );
        }
        let Some(drawing) = &self.minimap else {
            return;
        };
        let response = ui
            .interact(map, Id::new("canvas_minimap"), Sense::click_and_drag())
            .on_hover_text("ミニマップ：クリック・ドラッグで表示位置を移動");
        if (response.clicked() || response.dragged_by(egui::PointerButton::Primary))
            && let Some(pos) = response.interact_pointer_pos()
        {
            let world = drawing.bounds.center() + (pos - inner.center()) / drawing.scale;
            let pan = canvas.size() / 2. - world.to_vec2() * p.zoom;
            p.pan = [pan.x.clamp(-1e7, 1e7), pan.y.clamp(-1e7, 1e7)];
            ui.ctx().request_repaint();
        }
        painter.rect_filled(map, 8, theme::panel_bg());
        painter.rect_stroke(
            map,
            8,
            Stroke::new(1_f32, theme::card_border()),
            egui::StrokeKind::Inside,
        );
        let mini = painter.with_clip_rect(inner);
        mini.add(drawing.shape.clone());
        let a = to_world(canvas.min, canvas, p.pan, p.zoom);
        let b = to_world(canvas.max, canvas, p.pan, p.zoom);
        let view = Rect::from_two_pos(drawing.project(a.into()), drawing.project(b.into()));
        if view.intersects(inner) {
            mini.rect_filled(view.intersect(inner), 2, theme::teal().gamma_multiply(0.14));
            mini.rect_stroke(
                view.intersect(inner),
                2,
                Stroke::new(1.5_f32, theme::teal()),
                egui::StrokeKind::Inside,
            );
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
            let Some(gr) = group_rect(p, &self.node_indices, &group.nodes, rect) else {
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
                    .filter(|id| self.node_indices.get(*id).is_some())
                    .collect();
            }
            if lr.dragged() {
                let d = ui.input(|i| i.pointer.delta()) / p.zoom;
                for id in &group.nodes {
                    if let Some(index) = self.node_indices.get(*id) {
                        let m = &mut p.graph.nodes[index];
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
    (
        [pan.x.clamp(-1.0e7, 1.0e7), pan.y.clamp(-1.0e7, 1.0e7)],
        zoom,
    )
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

/// Parametric interval of a line segment inside the clip rectangle.
fn clip_segment(start: Pos2, end: Pos2, clip: Rect) -> Option<[f64; 2]> {
    let mut enter = 0.0_f64;
    let mut exit = 1.0_f64;
    for (a, b, min, max) in [
        (start.x, end.x, clip.min.x, clip.max.x),
        (start.y, end.y, clip.min.y, clip.max.y),
    ] {
        let a = f64::from(a);
        let delta = f64::from(b) - a;
        if delta == 0.0 {
            if a < f64::from(min) || a > f64::from(max) {
                return None;
            }
            continue;
        }
        let t0 = (f64::from(min) - a) / delta;
        let t1 = (f64::from(max) - a) / delta;
        enter = enter.max(t0.min(t1));
        exit = exit.min(t0.max(t1));
        if enter >= exit {
            return None;
        }
    }
    Some([enter, exit])
}

fn dashed_wire_shapes(
    points: &[Pos2],
    clip: Rect,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) -> Vec<Shape> {
    // Clip outside the painter's rectangle so stroke ends and antialiasing are hidden.
    let clip = clip.expand(stroke.width + 2.0);
    if points.iter().all(|point| clip.contains(*point)) {
        return Shape::dashed_line(points, stroke, dash, gap);
    }
    let dash = f64::from(dash);
    let period = dash + f64::from(gap);
    let mut phase = 0.0_f64;
    let mut shapes = Vec::new();
    for segment in points.windows(2) {
        let start = segment[0];
        let end = segment[1];
        let dx = f64::from(end.x) - f64::from(start.x);
        let dy = f64::from(end.y) - f64::from(start.y);
        let length = (dx * dx + dy * dy).sqrt();
        if length == 0.0 || !length.is_finite() {
            continue;
        }
        if let Some([enter, exit]) = clip_segment(start, end, clip) {
            let first = enter * length;
            let last = exit * length;
            // Skip every whole offscreen dash period in one operation, retaining phase
            // along the full polyline so the pattern follows the wire when panning.
            let mut at = first - (first + phase).rem_euclid(period);
            let point_at = |distance| {
                let t = distance / length;
                egui::pos2(
                    (f64::from(start.x) + dx * t) as f32,
                    (f64::from(start.y) + dy * t) as f32,
                )
            };
            while at < last {
                let from = at.max(first);
                let to = (at + dash).min(last);
                if to > from {
                    shapes.push(Shape::line_segment([point_at(from), point_at(to)], stroke));
                }
                at += period;
            }
        }
        phase = (phase + length).rem_euclid(period);
    }
    shapes
}

/// Right-click menu of an input port: how the data is handled (Flatten, Graft,
/// Simplify, Reverse) and how the wire is drawn. Returns `true` to disconnect.
fn port_menu(ui: &mut egui::Ui, node: &mut Node, key: &str, linked: bool) -> bool {
    use nodes::data_tree::{PORT_PREFIX, WIRE_PREFIX};
    let mut cut = false;
    if nodes::data_tree_nodes::supports_transform(node.kind) {
        ui.label(theme::small("データの扱い"));
        let tkey = format!("{PORT_PREFIX}{key}");
        let current = node.params.get(&tkey).copied().unwrap_or(0.);
        for (value, label) in [
            (0., "そのまま"),
            (1., "Flatten（1本のリストにする）"),
            (2., "Graft（1個ずつ枝にする）"),
            (3., "Simplify（共通の枝番号を省く）"),
            (4., "Reverse（各リストを逆順）"),
        ] {
            if ui.selectable_label(current == value, label).clicked() {
                if value == 0. {
                    node.params.remove(&tkey);
                } else {
                    node.params.insert(tkey.clone(), value);
                }
                ui.close();
            }
        }
        ui.separator();
    }
    ui.label(theme::small("ワイヤの表示"));
    let wkey = format!("{WIRE_PREFIX}{key}");
    let current = node.params.get(&wkey).copied().unwrap_or(0.);
    for (value, label) in [
        (0., "標準"),
        (1., "うっすら（選ぶと濃くなる）"),
        (2., "非表示（選ぶと見える）"),
    ] {
        if ui.selectable_label(current == value, label).clicked() {
            if value == 0. {
                node.params.remove(&wkey);
            } else {
                node.params.insert(wkey.clone(), value);
            }
            ui.close();
        }
    }
    if linked {
        ui.separator();
        if ui.button("切断").clicked() {
            cut = true;
            ui.close();
        }
    }
    cut
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
    let w = if theme::is_melt() {
        melt::wire_width(selected)
    } else if selected {
        2.4
    } else {
        1.7_f32
    };
    match shape {
        Some(DataShape::List(_)) => {
            bezier(painter, a, b, Stroke::new(w + 2.8, color));
            bezier(painter, a, b, Stroke::new(1.3_f32, theme::canvas_bg()));
        }
        Some(DataShape::Tree { .. }) => {
            let points = cubic(a, b, 40);
            painter.extend(dashed_wire_shapes(
                &points,
                painter.clip_rect(),
                Stroke::new(w + 0.6, color),
                8. * zoom.max(0.5),
                5. * zoom.max(0.5),
            ));
        }
        Some(DataShape::Empty) => {
            let points = cubic(a, b, 40);
            painter.extend(dashed_wire_shapes(
                &points,
                painter.clip_rect(),
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
        for (a, b) in [
            (egui::pos2(500., 100.), egui::pos2(-500., 400.)),
            (egui::pos2(-900., -10.), egui::pos2(900., 10.)),
        ] {
            let bounds = wire_bounds(a, b);
            assert!(cubic(a, b, 80).iter().all(|p| bounds.contains(*p)));
        }
    }

    #[test]
    fn dashed_wire_keeps_the_pattern_when_crossing_the_clip() {
        let stroke = Stroke::new(2.0_f32, Color32::WHITE);
        let clip = Rect::from_min_max(egui::pos2(-40., -40.), egui::pos2(220., 120.));
        let mut path = cubic(egui::pos2(-550., -90.), egui::pos2(580., 210.), 40);
        for _ in 0..2 {
            let visible_inside = |shape| match shape {
                Shape::LineSegment { points, .. } if points.iter().all(|p| clip.contains(*p)) => {
                    Some(points)
                }
                _ => None,
            };
            let expected: Vec<_> = Shape::dashed_line(&path, stroke, 8., 5.)
                .into_iter()
                .filter_map(visible_inside)
                .collect();
            let actual: Vec<_> = dashed_wire_shapes(&path, clip, stroke, 8., 5.)
                .into_iter()
                .filter_map(visible_inside)
                .collect();
            assert!(expected.len() > 10);
            assert_eq!(expected.len(), actual.len());
            for ([a, b], [c, d]) in expected.into_iter().zip(actual) {
                assert!(a.distance(c) < 0.001 && b.distance(d) < 0.001);
            }
            path.reverse();
        }
    }

    #[test]
    fn dashed_wire_shape_count_depends_on_visible_length() {
        let stroke = Stroke::new(1.2_f32, Color32::WHITE);
        let clip = Rect::from_min_max(Pos2::ZERO, egui::pos2(800., 600.));
        let path = cubic(egui::pos2(-1e6, 300.), egui::pos2(1e6, 300.), 40);
        let shapes = dashed_wire_shapes(&path, clip, stroke, 3., 4.);
        assert!(
            (110..120).contains(&shapes.len()),
            "{} shapes",
            shapes.len()
        );
        let offscreen = cubic(egui::pos2(-1e6, -30.), egui::pos2(1e6, -30.), 40);
        assert!(dashed_wire_shapes(&offscreen, clip, stroke, 3., 4.).is_empty());
    }

    #[test]
    fn canvas_node_lookup_follows_reordered_and_replaced_graphs() {
        let mut graph = nodes::model::Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = graph.add(NodeKind::Add, [300., 0.]).unwrap();
        let mut indices = NodeIndices::default();
        indices.update(&graph.nodes);
        assert_eq!(indices.get(b), Some(1));
        graph.nodes.reverse();
        indices.update(&graph.nodes);
        assert_eq!(indices.get(b), Some(0));
        assert_eq!(indices.get(a), Some(1));
        graph.remove(a);
        let c = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        indices.update(&graph.nodes);
        assert_eq!(indices.get(a), None);
        assert_eq!(indices.get(c), Some(1));
    }

    #[test]
    fn canvas_slider_still_edits_its_node_and_panning_reuses_the_minimap() {
        let ctx = egui::Context::default();
        let mut canvas = GraphCanvas::default();
        let mut graph = nodes::model::Graph::default();
        let id = graph.add(NodeKind::Number, [40., 40.]).unwrap();
        for row in 0..11 {
            graph
                .add(NodeKind::Number, [1_000., row as f32 * 185.])
                .unwrap();
        }
        let mut project = Project::new("test", graph);
        project.pan = [0., 0.];
        project.zoom = 1.;
        let frame = |canvas: &mut GraphCanvas, project: &mut Project, events| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900., 700.))),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    canvas.show(ui, project, None, false);
                });
            });
        };
        let minimap_mesh = |canvas: &GraphCanvas| match &canvas.minimap.as_ref().unwrap().shape {
            Shape::Mesh(mesh) => mesh.clone(),
            _ => panic!("minimap should be a mesh"),
        };
        frame(&mut canvas, &mut project, vec![]);
        let original = minimap_mesh(&canvas);
        project.pan = [28., 10.];
        frame(&mut canvas, &mut project, vec![]);
        assert!(std::sync::Arc::ptr_eq(&original, &minimap_mesh(&canvas)));

        project.graph.nodes.reverse();
        frame(&mut canvas, &mut project, vec![]);
        let reordered = minimap_mesh(&canvas);
        assert!(!std::sync::Arc::ptr_eq(&original, &reordered));
        let node = project.graph.node(id).unwrap();
        let key = node.kind.spec().inputs[0].key;
        let before = node.params[key];
        let point = input_pos(
            node,
            0,
            canvas.last_rect.unwrap(),
            project.pan,
            project.zoom,
        ) + egui::vec2(141., 0.);
        for pressed in [true, false] {
            frame(
                &mut canvas,
                &mut project,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_ne!(project.graph.node(id).unwrap().params[key], before);
        assert!(
            project
                .graph
                .nodes
                .iter()
                .filter(|node| node.id != id)
                .all(|node| node.params[key] == before)
        );
        assert!(std::sync::Arc::ptr_eq(&reordered, &minimap_mesh(&canvas)));

        project.graph.node_mut(id).unwrap().position[0] += 200.;
        frame(&mut canvas, &mut project, vec![]);
        let moved = minimap_mesh(&canvas);
        assert!(!std::sync::Arc::ptr_eq(&reordered, &moved));
        canvas.select_only(id);
        frame(&mut canvas, &mut project, vec![]);
        assert!(!std::sync::Arc::ptr_eq(&moved, &minimap_mesh(&canvas)));
    }
}

#[cfg(test)]
#[path = "canvas_perf.rs"]
mod perf;

#[cfg(test)]
#[path = "canvas_numeric_tests.rs"]
mod numeric_tests;
