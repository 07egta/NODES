//! Visual inspector for the existing text-backed constrained sketch.
//! The script is the sole persistent owner. A visual operation builds and solves
//! a candidate script before replacing it; failed candidates never mutate the graph.
use crate::theme;
use eframe::egui::{self, RichText};
use nodes::model::{Graph, NodeId};
use nodes::sketch_nodes::{MAX_SKETCH_TEXT, parse, solve_sketch};
use solidcraft_geom::Vec2;
use solidcraft_sketch::{CurveKind, Sketch};
use std::sync::Arc;

const EXAMPLE: &str = "# 例: 60 × 40 の板に半径 8 の穴\n長方形 r 0 0 60 40\n固定 r.p0\n長さ r.b 60\n長さ r.l 40\n円 c 30 20 8\n半径 c 8\n";
// Keep synchronous UI work substantially smaller than the evaluator's text limit.
const VISUAL_LIMIT: usize = 128;
const COORD_LIMIT: f64 = 1e6;
const EPS: f64 = 1e-7;

type Preview = Result<(Sketch, usize), String>;

fn preview(text: &str) -> Preview {
    let commands = text
        .lines()
        .filter(|l| !l.split('#').next().unwrap_or("").trim().is_empty())
        .count();
    if text.len() > MAX_SKETCH_TEXT || commands > VISUAL_LIMIT {
        return Err(format!("直接編集は {VISUAL_LIMIT} 命令までです。文章編集は引き続き使えます。"));
    }
    let mut sk = parse(text)?;
    let dof = solve_sketch(&mut sk)?;
    Ok((sk, dof))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Point,
    Line,
    Rectangle,
    CenterRectangle,
    Circle,
}
impl Tool {
    const ALL: [Self; 5] = [
        Self::Point, Self::Line, Self::Rectangle, Self::CenterRectangle, Self::Circle,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Point => "点",
            Self::Line => "線",
            Self::Rectangle => "長方形",
            Self::CenterRectangle => "中心長方形",
            Self::Circle => "円",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SnapKind {
    Free,
    Point(String),
    Midpoint(String),
    Grid,
}
#[derive(Clone, Debug)]
struct Snap {
    pos: Vec2,
    kind: SnapKind,
}
impl Snap {
    fn free(pos: Vec2) -> Self {
        Self { pos, kind: SnapKind::Free }
    }
    fn label(&self) -> String {
        match &self.kind {
            SnapKind::Free => "自由点".into(),
            SnapKind::Point(id) => format!("点: {id}"),
            SnapKind::Midpoint(id) => format!("中点: {id}"),
            SnapKind::Grid => "グリッド".into(),
        }
    }
}

#[derive(Clone, Copy)]
struct View {
    center: Vec2,
    scale: f64, // logical egui points per mm, never physical pixels
}
impl Default for View {
    fn default() -> Self {
        Self { center: Vec2::new(0., 0.), scale: 3. }
    }
}
impl View {
    fn screen(self, p: Vec2, rect: egui::Rect) -> egui::Pos2 {
        rect.center() + egui::vec2(
            ((p.x - self.center.x) * self.scale) as f32,
            (-(p.y - self.center.y) * self.scale) as f32,
        )
    }
    fn world(self, p: egui::Pos2, rect: egui::Rect) -> Vec2 {
        Vec2::new(
            self.center.x + f64::from(p.x - rect.center().x) / self.scale,
            self.center.y - f64::from(p.y - rect.center().y) / self.scale,
        )
    }
    fn grid(self) -> f64 {
        // Adaptive 1/2/5 grid: avoid dense loops and keep the snap target visible.
        let desired = 22. / self.scale;
        let base = 10_f64.powf(desired.log10().floor());
        [base, 2. * base, 5. * base, 10. * base]
            .into_iter()
            .find(|s| *s >= desired)
            .unwrap_or(10. * base)
    }
    fn fit(sk: &Sketch, width: f32, height: f32) -> Self {
        let mut lo = Vec2::new(0., 0.);
        let mut hi = lo;
        for p in sk.points.iter().map(|p| p.pos).chain(
            (0..sk.curves.len()).flat_map(|i| sk.polyline(i)),
        ) {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        Self {
            center: Vec2::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            scale: (f64::from((width - 24.).max(1.)) / (hi.x - lo.x).max(20.))
                .min(f64::from((height - 24.).max(1.)) / (hi.y - lo.y).max(20.))
                .clamp(0.00001, 1000.),
        }
    }
}

fn candidates(sk: &Sketch, pos: Vec2, tolerance: f64, grid: Option<f64>) -> Vec<Snap> {
    let mut hits = Vec::new();
    let mut add = |p: Vec2, kind| {
        if p.dist(pos) <= tolerance {
            hits.push(Snap { pos: p, kind });
        }
    };
    // Includes endpoints, circle centres and origin. Keep coincident candidates
    // separate so that the user can choose the intended persistent reference.
    for p in &sk.points {
        add(p.pos, SnapKind::Point(p.id.clone()));
    }
    for c in &sk.curves {
        if let CurveKind::Line { a, b } = &c.kind
            && let (Some(a), Some(b)) = (sk.point(*a), sk.point(*b))
        {
            add((a + b) * 0.5, SnapKind::Midpoint(c.id.clone()));
        }
    }
    if let Some(step) = grid.filter(|s| s.is_finite() && *s > 0.) {
        add(
            Vec2::new((pos.x / step).round() * step, (pos.y / step).round() * step),
            SnapKind::Grid,
        );
    }
    // Stable sort: geometry precedes the grid for exactly overlapping candidates.
    hits.sort_by(|a, b| a.pos.dist(pos).total_cmp(&b.pos.dist(pos)));
    hits
}

fn next_id(sk: &Sketch) -> Result<String, String> {
    for i in 1..=VISUAL_LIMIT * 5 + 1 {
        let id = format!("s76_{i}");
        let prefix = format!("{id}.");
        if !sk.points.iter().any(|p| p.id == id || p.id.starts_with(&prefix))
            && !sk.curves.iter().any(|c| c.id == id || c.id.starts_with(&prefix))
        {
            return Ok(id);
        }
    }
    Err("空いている図形名がありません。".into())
}

fn reference(commands: &mut Vec<String>, target: &str, snap: &Snap) {
    match &snap.kind {
        SnapKind::Point(id) => commands.push(format!("coincident {target} {id}")),
        SnapKind::Midpoint(id) => commands.push(format!("midpoint {target} {id}")),
        SnapKind::Grid | SnapKind::Free => {}
    }
}

fn valid(p: Vec2) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() <= COORD_LIMIT && p.y.abs() <= COORD_LIMIT
}

fn make_commands(
    sk: &Sketch, tool: Tool, a: &Snap, b: Option<&Snap>, persistent: bool,
) -> Result<Vec<String>, String> {
    let id = next_id(sk)?;
    let p = a.pos;
    if !valid(p) {
        return Err("座標が範囲外です。".into());
    }
    if tool == Tool::Point {
        let mut out = vec![format!("point {id} {} {}", p.x, p.y)];
        if persistent { reference(&mut out, &id, a); }
        return Ok(out);
    }
    let b = b.ok_or("2 点目を指定してください。")?;
    let q = b.pos;
    if !valid(q) || p.dist(q) <= EPS {
        return Err("異なる有効な 2 点を指定してください。".into());
    }
    let mut out = Vec::new();
    match tool {
        Tool::Line => {
            out.push(format!("line {id} {} {} {} {}", p.x, p.y, q.x, q.y));
            if persistent {
                reference(&mut out, &format!("{id}.start"), a);
                reference(&mut out, &format!("{id}.end"), b);
            }
        }
        Tool::Rectangle | Tool::CenterRectangle => {
            let d = q - p;
            if d.x.abs() <= EPS || d.y.abs() <= EPS {
                return Err("長方形の幅と高さは 0 にできません。".into());
            }
            let (origin, size) = if tool == Tool::CenterRectangle {
                (p - d, d * 2.)
            } else {
                (p, d)
            };
            if !valid(origin) || !valid(origin + size)
                || size.x.abs() > COORD_LIMIT || size.y.abs() > COORD_LIMIT
            {
                return Err("長方形の寸法が範囲外です。".into());
            }
            out.push(format!("rect {id} {} {} {} {}", origin.x, origin.y, size.x, size.y));
            if persistent {
                if tool == Tool::Rectangle {
                    reference(&mut out, &format!("{id}.p0"), a);
                }
                reference(&mut out, &format!("{id}.p2"), b);
            }
        }
        Tool::Circle => {
            let radius = p.dist(q);
            if radius > COORD_LIMIT {
                return Err("半径が範囲外です。".into());
            }
            out.push(format!("circle {id} {} {} {radius}", p.x, p.y));
            if persistent { reference(&mut out, &format!("{id}.center"), a); }
        }
        Tool::Point => return Err("点の指定が不正です。".into()),
    }
    Ok(out)
}

fn appended(text: &str, commands: &[String]) -> String {
    let mut result = text.to_owned();
    if !result.is_empty() && !result.ends_with('\n') { result.push('\n'); }
    for command in commands {
        result.push_str(command);
        result.push('\n');
    }
    result
}

fn dimension_value(line: &str) -> Option<f64> {
    let words: Vec<_> = line.split('#').next()?.split_whitespace().collect();
    let expected = match *words.first()? {
        "長さ" | "length" | "半径" | "radius" | "直径" | "diameter" => 3,
        "距離" | "distance" | "距離x" | "dx" | "距離y" | "dy" | "角度" | "angle" => 4,
        _ => return None,
    };
    if words.len() != expected { return None; }
    words.last()?.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn constraint_line(line: &str) -> bool {
    let Some(cmd) = line.split('#').next().and_then(|s| s.split_whitespace().next()) else {
        return false;
    };
    matches!(cmd,
        "水平" | "horizontal" | "垂直" | "vertical" | "固定" | "fix"
        | "一致" | "coincident" | "点on線" | "on" | "中点" | "midpoint"
        | "平行" | "parallel" | "直交" | "perpendicular" | "接する" | "tangent"
        | "等しい" | "equal" | "同心" | "concentric" | "滑らか" | "smooth"
    ) || dimension_value(line).is_some()
}

fn edit_dimension(text: &str, index: usize, value: f64) -> Result<String, String> {
    if !value.is_finite() || value.abs() > COORD_LIMIT {
        return Err("寸法が範囲外です。".into());
    }
    let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
    let line = lines.get_mut(index).ok_or("寸法が見つかりません。")?;
    if dimension_value(line).is_none() { return Err("寸法の命令ではありません。".into()); }
    let (body, comment) = line.split_once('#').unwrap_or((line.as_str(), ""));
    let mut words: Vec<_> = body.split_whitespace().map(str::to_owned).collect();
    if let Some(last) = words.last_mut() { *last = value.to_string(); }
    let mut replacement = words.join(" ");
    if line.contains('#') { replacement.push_str(&format!(" #{comment}")); }
    *line = replacement;
    let mut result = lines.join("\n");
    if text.ends_with('\n') { result.push('\n'); }
    Ok(result)
}

#[derive(Clone)]
struct Editor {
    source: String,
    buffer: String,
    preview: Arc<Preview>,
    tool: Tool,
    first: Option<Snap>,
    snap_index: usize,
    grid: bool,
    persistent: bool,
    view: View,
    needs_fit: bool,
    numeric: [f64; 2],
    message: Option<String>,
    selected_constraint: Option<usize>,
    dimension: f64,
    selected_curve: String,
}
impl Editor {
    fn new(source: &str) -> Self {
        Self {
            source: source.into(), buffer: source.into(),
            preview: Arc::new(preview(source)), tool: Tool::Line, first: None,
            snap_index: 0, grid: true, persistent: false, view: View::default(),
            needs_fit: true, numeric: [0., 0.], message: None,
            selected_constraint: None, dimension: 10., selected_curve: String::new(),
        }
    }
    fn replace(&mut self, candidate: String, solved: Preview) {
        self.buffer = candidate.clone();
        self.source = candidate;
        self.preview = Arc::new(solved);
        self.first = None;
        self.snap_index = 0;
        self.selected_constraint = None;
        self.message = None;
    }
    fn commit(&mut self, candidate: String) -> bool {
        match preview(&candidate) {
            Ok(solved) => {
                self.replace(candidate, Ok(solved));
                true
            }
            Err(error) => {
                self.message = Some(error);
                false
            }
        }
    }
    fn pick(&mut self, snap: Snap) {
        if self.source != self.buffer { return; }
        if self.tool != Tool::Point && self.first.is_none() {
            self.first = Some(snap);
            self.message = None;
            return;
        }
        let snapshot = Arc::clone(&self.preview);
        let Ok((sk, _)) = snapshot.as_ref() else { return; };
        let first = self.first.as_ref().unwrap_or(&snap);
        match make_commands(sk, self.tool, first, self.first.as_ref().map(|_| &snap), self.persistent) {
            Ok(commands) => { self.commit(appended(&self.source, &commands)); }
            Err(error) => self.message = Some(error),
        }
    }
}

fn paint_grid(painter: &egui::Painter, rect: egui::Rect, view: View, grid: bool) {
    let lo = view.world(rect.left_bottom(), rect);
    let hi = view.world(rect.right_top(), rect);
    let stroke = egui::Stroke::new(0.5, theme::line());
    if grid {
        let step = view.grid();
        for k in 0..=((hi.x - lo.x) / step).ceil().min(256.) as usize {
            let x = (lo.x / step).ceil() * step + k as f64 * step;
            painter.line_segment([view.screen(Vec2::new(x, lo.y), rect), view.screen(Vec2::new(x, hi.y), rect)], stroke);
        }
        for k in 0..=((hi.y - lo.y) / step).ceil().min(256.) as usize {
            let y = (lo.y / step).ceil() * step + k as f64 * step;
            painter.line_segment([view.screen(Vec2::new(lo.x, y), rect), view.screen(Vec2::new(hi.x, y), rect)], stroke);
        }
    }
    let axes = egui::Stroke::new(1., theme::muted());
    painter.line_segment([view.screen(Vec2::new(lo.x, 0.), rect), view.screen(Vec2::new(hi.x, 0.), rect)], axes);
    painter.line_segment([view.screen(Vec2::new(0., lo.y), rect), view.screen(Vec2::new(0., hi.y), rect)], axes);
}

fn paint_draft(painter: &egui::Painter, rect: egui::Rect, view: View, tool: Tool, a: Vec2, b: Vec2) {
    let stroke = egui::Stroke::new(1.5, theme::teal());
    let p = view.screen(a, rect);
    let q = view.screen(b, rect);
    match tool {
        Tool::Line => { painter.line_segment([p, q], stroke); }
        Tool::Rectangle | Tool::CenterRectangle => {
            let other = if tool == Tool::CenterRectangle { view.screen(a - (b - a), rect) } else { p };
            painter.rect_stroke(egui::Rect::from_two_pos(other, q), 0., stroke, egui::StrokeKind::Inside);
        }
        Tool::Circle => { painter.circle_stroke(p, p.distance(q), stroke); }
        Tool::Point => {}
    }
}

fn visual(ui: &mut egui::Ui, state: &mut Editor) {
    let snapshot = Arc::clone(&state.preview);
    let (sk, dof) = match snapshot.as_ref() {
        Ok(v) => v,
        Err(error) => {
            ui.colored_label(theme::error(), error);
            ui.label(theme::small("文章欄から修正してください。"));
            return;
        }
    };
    ui.label(RichText::new(format!("自由度: {dof}  /  拘束: {}", sk.constraints.len()))
        .color(if *dof == 0 { theme::teal() } else { ui.visuals().text_color() }));
    if state.source != state.buffer {
        ui.label(theme::small("文章に未適用の変更があります。適用または破棄してから直接編集してください。"));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for tool in Tool::ALL {
            if ui.selectable_label(state.tool == tool, tool.label()).clicked() {
                state.tool = tool;
                state.first = None;
                state.snap_index = 0;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.grid, "グリッド");
        ui.checkbox(&mut state.persistent, "点・中点の拘束を追加")
            .on_hover_text("オフなら位置だけ合わせます。円の半径点・中心長方形の中心・グリッドは位置合わせのみ。");
    });
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("全体表示").clicked() { state.needs_fit = true; }
        if ui.small_button("＋").clicked() { state.view.scale = (state.view.scale * 1.4).min(1000.); }
        if ui.small_button("−").clicked() { state.view.scale = (state.view.scale / 1.4).max(0.00001); }
        if ui.small_button("入力取消").clicked() { state.first = None; state.message = None; }
    });
    let width = ui.available_width().max(1.);
    let height = 250.;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    if state.needs_fit {
        state.view = View::fit(sk, width, height);
        state.needs_fit = false;
    }
    if response.dragged_by(egui::PointerButton::Middle) {
        let delta = ui.input(|i| i.pointer.delta());
        state.view.center.x -= f64::from(delta.x) / state.view.scale;
        state.view.center.y += f64::from(delta.y) / state.view.scale;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4., theme::workspace_bg());
    paint_grid(&painter, rect, state.view, state.grid);
    for (i, curve) in sk.curves.iter().enumerate() {
        let poly = sk.polyline(i);
        if poly.len() >= 2 {
            let color = if curve.id == state.selected_curve || *dof == 0 {
                theme::teal()
            } else {
                ui.visuals().text_color()
            };
            painter.add(egui::Shape::line(
                poly.iter().map(|p| state.view.screen(*p, rect)).collect(),
                egui::Stroke::new(1.5, color),
            ));
        }
    }
    for p in &sk.points {
        painter.circle_filled(state.view.screen(p.pos, rect), 2.8, if p.fixed { theme::teal() } else { theme::muted() });
    }
    if response.hovered() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { state.first = None; }
        if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
            let world = state.view.world(pointer, rect);
            let hits = candidates(sk, world, 9. / state.view.scale, state.grid.then(|| state.view.grid()));
            if response.secondary_clicked() { state.snap_index = state.snap_index.wrapping_add(1); }
            let index = state.snap_index % hits.len().max(1);
            let snap = hits.get(index).cloned().unwrap_or_else(|| Snap::free(world));
            if let Some(first) = &state.first {
                paint_draft(&painter, rect, state.view, state.tool, first.pos, snap.pos);
            }
            painter.circle_stroke(state.view.screen(snap.pos, rect), 5., egui::Stroke::new(1.5, theme::teal()));
            ui.label(theme::small(format!("{} · {:.3}, {:.3} mm · {}/{}",
                snap.label(), snap.pos.x, snap.pos.y, if hits.is_empty() { 0 } else { index + 1 }, hits.len())));
            if response.clicked_by(egui::PointerButton::Primary) { state.pick(snap); }
        }
    } else {
        ui.label(theme::small("左クリック: 指定 / 右クリック: スナップ候補切替 / 中ボタン: 移動"));
    }
    ui.label(theme::small(if state.first.is_some() { "2 点目を指定" } else { "1 点目を指定（円は中心）" }));
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::DragValue::new(&mut state.numeric[0]).prefix("X ").range(-COORD_LIMIT..=COORD_LIMIT));
        ui.add(egui::DragValue::new(&mut state.numeric[1]).prefix("Y ").range(-COORD_LIMIT..=COORD_LIMIT));
        if ui.button("数値で指定").clicked() {
            state.pick(Snap::free(Vec2::new(state.numeric[0], state.numeric[1])));
        }
    });
    egui::CollapsingHeader::new("拘束・寸法").show(ui, |ui| {
        let rows: Vec<_> = state.source.lines().enumerate().filter(|(_, l)| constraint_line(l)).collect();
        // Make a copy before any candidate commit replaces the source.
        let rows: Vec<_> = rows.into_iter().map(|(i, l)| (i, l.to_owned())).collect();
        let current = state.selected_constraint;
        egui::ComboBox::from_id_salt("constraint_row").width(ui.available_width().min(220.))
            .selected_text(current.map_or_else(|| "既存の拘束を選択".into(), |i| format!("{} 行目", i + 1)))
            .show_ui(ui, |ui| {
                for (i, line) in &rows {
                    if ui.selectable_label(current == Some(*i), format!("{}: {line}", i + 1)).clicked() {
                        state.selected_constraint = Some(*i);
                        if let Some(v) = dimension_value(line) { state.dimension = v; }
                    }
                }
            });
        if let Some((index, line)) = rows.iter().find(|(i, _)| Some(*i) == state.selected_constraint) {
            ui.horizontal_wrapped(|ui| {
                if dimension_value(line).is_some() {
                    ui.add(egui::DragValue::new(&mut state.dimension).range(-COORD_LIMIT..=COORD_LIMIT));
                    if ui.button("寸法を更新").clicked() {
                        match edit_dimension(&state.source, *index, state.dimension) {
                            Ok(candidate) => { state.commit(candidate); }
                            Err(error) => state.message = Some(error),
                        }
                    }
                }
                if ui.button("この拘束を削除").clicked() {
                    let candidate = state.source.lines().enumerate().filter(|(i, _)| i != index)
                        .map(|(_, l)| l).collect::<Vec<_>>().join("\n");
                    state.commit(candidate);
                }
            });
        }
        egui::ComboBox::from_id_salt("constraint_curve")
            .selected_text(if state.selected_curve.is_empty() { "図形を選択" } else { &state.selected_curve })
            .show_ui(ui, |ui| {
                for c in &sk.curves {
                    ui.selectable_value(&mut state.selected_curve, c.id.clone(), &c.id);
                }
            });
        if let Some(curve) = sk.curves.iter().find(|c| c.id == state.selected_curve) {
            let mut command = None;
            ui.horizontal_wrapped(|ui| {
                match &curve.kind {
                    CurveKind::Line { .. } => {
                        if ui.button("水平").clicked() { command = Some(format!("horizontal {}", curve.id)); }
                        if ui.button("垂直").clicked() { command = Some(format!("vertical {}", curve.id)); }
                        ui.add(egui::DragValue::new(&mut state.dimension).range(EPS..=COORD_LIMIT));
                        if ui.button("長さを追加").clicked() { command = Some(format!("length {} {}", curve.id, state.dimension)); }
                    }
                    CurveKind::Circle { .. } | CurveKind::Arc { .. } => {
                        ui.add(egui::DragValue::new(&mut state.dimension).range(EPS..=COORD_LIMIT));
                        if ui.button("半径を追加").clicked() { command = Some(format!("radius {} {}", curve.id, state.dimension)); }
                    }
                    _ => {}
                }
            });
            if let Some(command) = command {
                state.commit(appended(&state.source, &[command]));
            }
        }
    });
    if let Some(error) = &state.message { ui.colored_label(theme::error(), error); }
}

pub fn editor(ui: &mut egui::Ui, graph: &mut Graph, id: NodeId) {
    let Some(node) = graph.node_mut(id) else { return; };
    let key = egui::Id::new(("sketch_editor_omega76", id));
    let mut state = ui.data_mut(|d| d.get_temp::<Editor>(key))
        .unwrap_or_else(|| Editor::new(&node.script));
    // Undo, redo, load and external script edits invalidate every pending pick.
    if state.source != node.script { state = Editor::new(&node.script); }
    ui.push_id(("sketch_editor", id), |ui| {
        ui.add_space(6.);
        ui.label(RichText::new("スケッチ · XY 平面").strong());
        visual(ui, &mut state);
        egui::CollapsingHeader::new("文章で編集").default_open(false).show(ui, |ui| {
            ui.label(theme::small("1 行に 1 命令。文章を適用するまで直接編集を停止します。"));
            ui.add(egui::TextEdit::multiline(&mut state.buffer)
                .code_editor().desired_rows(8).desired_width(f32::INFINITY)
                .char_limit(MAX_SKETCH_TEXT));
            ui.horizontal_wrapped(|ui| {
                if ui.button("文章を適用").clicked() && state.buffer != state.source {
                    // Preserve the existing text workflow even for large or invalid scripts.
                    // The evaluator reports errors; visual edits require a valid preview.
                    let candidate = state.buffer.clone();
                    let solved = preview(&candidate);
                    state.replace(candidate, solved);
                    state.needs_fit = true;
                }
                if ui.button("未適用の変更を破棄").clicked() { state.buffer = state.source.clone(); }
                if state.source.trim().is_empty() && state.buffer.trim().is_empty()
                    && ui.button("例を入れる").clicked()
                {
                    state.commit(EXAMPLE.into());
                    state.needs_fit = true;
                }
            });
        });
    });
    if node.script != state.source { node.script = state.source.clone(); }
    ui.data_mut(|d| d.insert_temp(key, state));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_rectangle_and_hole_use_existing_solver() {
        let mut s = Editor::new("");
        s.tool = Tool::Rectangle;
        s.pick(Snap::free(Vec2::new(0., 0.)));
        s.pick(Snap::free(Vec2::new(60., 40.)));
        s.tool = Tool::Circle;
        s.pick(Snap::free(Vec2::new(30., 20.)));
        s.pick(Snap::free(Vec2::new(38., 20.)));
        let (sk, _) = s.preview.as_ref().as_ref().unwrap();
        assert_eq!(sk.curves.len(), 5);
        let regions = solidcraft_sketch::find_profiles(sk);
        assert!(regions.iter().any(|p| !p.region.holes.is_empty()));
        assert!(nodes::sketch_nodes::run(&s.source, 0, &|| false).is_ok());
    }

    #[test]
    fn temporary_and_persistent_snap_are_distinct() {
        let (sk, _) = preview("line base 0 0 10 0").unwrap();
        let a = Snap { pos: Vec2::new(10., 0.), kind: SnapKind::Point("base.end".into()) };
        let b = Snap::free(Vec2::new(10., 5.));
        let loose = make_commands(&sk, Tool::Line, &a, Some(&b), false).unwrap();
        let tied = make_commands(&sk, Tool::Line, &a, Some(&b), true).unwrap();
        assert_eq!(loose.len(), 1);
        assert_eq!(tied.len(), 2);
        assert!(tied[1].ends_with("base.end"));
        assert!(preview(&appended("line base 0 0 10 0", &tied)).is_ok());
        let hits = candidates(&sk, Vec2::new(5., 0.), 0.01, Some(5.));
        assert!(hits.iter().any(|h| matches!(&h.kind, SnapKind::Midpoint(_))));
        assert!(hits.iter().any(|h| matches!(&h.kind, SnapKind::Grid)));
    }

    #[test]
    fn invalid_visual_edit_never_replaces_script() {
        let mut s = Editor::new("circle c 0 0 5\nradius c 5\n");
        let before = s.source.clone();
        assert!(!s.commit(appended(&before, &["radius missing 1".into()])));
        assert_eq!(s.source, before);
        assert_eq!(s.buffer, before);
        assert!(s.message.is_some());
        s.tool = Tool::Rectangle;
        s.pick(Snap::free(Vec2::new(1., 1.)));
        s.pick(Snap::free(Vec2::new(1., 2.)));
        assert_eq!(s.source, before);
    }

    #[test]
    fn dimensions_keep_comment_and_other_commands() {
        let before = "circle c 0 0 5\n半径 c 5 # hole\n";
        let after = edit_dimension(before, 1, 8.).unwrap();
        assert_eq!(after, "circle c 0 0 5\n半径 c 8 # hole\n");
        let (sk, _) = preview(&after).unwrap();
        assert!(matches!(&sk.curves[0].kind, CurveKind::Circle { r, .. } if (*r - 8.).abs() < 1e-5));
        assert!(edit_dimension(before, 0, 10.).is_err());
    }

    #[test]
    fn view_round_trip_and_bounded_preview() {
        let rect = egui::Rect::from_min_size(egui::pos2(20., 40.), egui::vec2(300., 250.));
        let view = View { center: Vec2::new(30., -12.), scale: 3. };
        let p = Vec2::new(12.5, 7.25);
        assert!(view.world(view.screen(p, rect), rect).dist(p) < 1e-5);
        let large = (0..=VISUAL_LIMIT).map(|i| format!("point p{i} {i} 0\n")).collect::<String>();
        assert!(preview(&large).is_err());
    }

    #[test]
    fn headless_egui_frame_does_not_mutate_source() {
        let ctx = egui::Context::default();
        let mut state = Editor::new(EXAMPLE);
        let before = state.source.clone();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| visual(ui, &mut state));
        });
        assert_eq!(state.source, before);
    }
}
