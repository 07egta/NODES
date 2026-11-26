use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use morphlet::{
    engine::Evaluation,
    model::{Node, NodeId},
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
}

#[derive(Default)]
pub struct CanvasOutcome {
    pub add_at: Option<[f32; 2]>,
    pub error: Option<String>,
    pub duplicate: bool,
    pub delete: bool,
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
                    bezier(
                        &painter,
                        a,
                        b,
                        Stroke::new(if selected { 2.4 } else { 1.7_f32 }, color),
                    );
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
            painter.rect_filled(nr, 9, Color32::from_rgb(253, 254, 251));
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
            painter.circle_filled(pos + egui::vec2(19., 21.) * p.zoom, 4.4 * p.zoom, col);
            painter.text(
                pos + egui::vec2(31., 19.) * p.zoom,
                Align2::LEFT_CENTER,
                &n.name,
                FontId::proportional(13.5 * p.zoom),
                theme::INK,
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
            });
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
            or.on_hover_text(format!(
                "{}の出力 · {}",
                n.name,
                n.kind.spec().output.label()
            ));
            let text = if stale {
                "更新中".to_owned()
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
            "ドラッグで移動  ·  ホイールで拡大  ·  中ボタンでパン"
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
