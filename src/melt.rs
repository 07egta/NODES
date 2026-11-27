//! Melt 3D style widgets and small painters shared by the canvas and the panels.
use crate::{
    node_icons::{self, Part},
    theme,
};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui,
};
use nodes::model::{DataType, Graph, NodeId};

/// Port outline per data type, so the type is readable without colour alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Circle,
    Diamond,
    Pill,
    Square,
}

pub fn marker_of(kind: DataType) -> Marker {
    match kind {
        DataType::Number | DataType::Any => Marker::Circle,
        DataType::Point => Marker::Diamond,
        DataType::Curve => Marker::Pill,
        _ => Marker::Square,
    }
}

fn radius(r: f32) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

/// Draws a port marker centred on `center` with half-size `r`.
pub fn port_marker(
    painter: &Painter,
    center: Pos2,
    r: f32,
    kind: DataType,
    fill: Color32,
    color: Color32,
    width: f32,
) {
    let stroke = Stroke::new(width, color);
    match marker_of(kind) {
        Marker::Circle => {
            painter.circle_filled(center, r, fill);
            painter.circle_stroke(center, r, stroke);
        }
        Marker::Diamond => {
            let d = r * 1.3;
            painter.add(Shape::convex_polygon(
                vec![
                    center + egui::vec2(0.0, -d),
                    center + egui::vec2(d, 0.0),
                    center + egui::vec2(0.0, d),
                    center + egui::vec2(-d, 0.0),
                ],
                fill,
                stroke,
            ));
        }
        Marker::Pill => {
            let rect = Rect::from_center_size(center, egui::vec2(r * 2.8, r * 1.8));
            painter.rect(rect, radius(r * 0.9), fill, stroke, StrokeKind::Inside);
        }
        Marker::Square => {
            let rect = Rect::from_center_size(center, egui::vec2(r * 1.9, r * 1.9));
            painter.rect(rect, radius(r * 0.3), fill, stroke, StrokeKind::Inside);
        }
    }
}

/// Colour of a wire carrying `kind`, dimmed when idle and full when selected.
pub fn wire_color(kind: DataType, selected: bool) -> Color32 {
    let base = theme::port_color(kind);
    if selected {
        base
    } else {
        base.gamma_multiply(0.7)
    }
}

/// Wire width: thicker when selected.
pub fn wire_width(selected: bool) -> f32 {
    if selected { 3.2 } else { 1.8 }
}

/// Follows failed inputs upstream until a node whose own inputs are all fine: the root cause.
/// Bounded, so a malformed graph cannot loop.
pub fn root_cause(graph: &Graph, is_failed: &dyn Fn(NodeId) -> bool, start: NodeId) -> NodeId {
    let mut current = start;
    for _ in 0..256 {
        let Some(node) = graph.node(current) else {
            return current;
        };
        match node.inputs.values().copied().find(|id| is_failed(*id)) {
            Some(next) if next != current => current = next,
            _ => return current,
        }
    }
    current
}

/// Red dot at the top-right corner of a card.
pub fn error_badge(painter: &Painter, card: Rect, zoom: f32) -> Rect {
    let center = card.right_top() + egui::vec2(-3.0, 3.0) * zoom;
    let r = (5.5 * zoom).max(3.5);
    painter.circle_filled(center, r + 1.5, theme::canvas_bg());
    painter.circle_filled(center, r, theme::error());
    Rect::from_center_size(center, egui::vec2(r * 2.0 + 6.0, r * 2.0 + 6.0))
}

/// Toolbar glyphs, drawn from the same stroke tables as the node icons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Plus,
    Open,
    Save,
    Export,
    Library,
    Cube,
    Diag,
    Grid,
    Map,
    Fit,
}

impl Tool {
    fn glyph(self) -> &'static [Part] {
        match self {
            Tool::Plus => &[
                Part::Line(&[[0.5, 0.18], [0.5, 0.82]]),
                Part::Line(&[[0.18, 0.5], [0.82, 0.5]]),
            ],
            Tool::Open => &[Part::Line(&[
                [0.12, 0.8],
                [0.12, 0.22],
                [0.4, 0.22],
                [0.5, 0.34],
                [0.88, 0.34],
                [0.88, 0.8],
                [0.12, 0.8],
            ])],
            Tool::Save => &[
                Part::Line(&[
                    [0.15, 0.15],
                    [0.75, 0.15],
                    [0.85, 0.25],
                    [0.85, 0.85],
                    [0.15, 0.85],
                    [0.15, 0.15],
                ]),
                Part::Line(&[[0.3, 0.15], [0.3, 0.4], [0.7, 0.4], [0.7, 0.15]]),
                Part::Line(&[[0.3, 0.85], [0.3, 0.6], [0.7, 0.6], [0.7, 0.85]]),
            ],
            Tool::Export => node_icons::EXPORT,
            Tool::Library => &[
                Part::Line(&[
                    [0.15, 0.15],
                    [0.45, 0.15],
                    [0.45, 0.45],
                    [0.15, 0.45],
                    [0.15, 0.15],
                ]),
                Part::Line(&[
                    [0.55, 0.15],
                    [0.85, 0.15],
                    [0.85, 0.45],
                    [0.55, 0.45],
                    [0.55, 0.15],
                ]),
                Part::Line(&[
                    [0.15, 0.55],
                    [0.45, 0.55],
                    [0.45, 0.85],
                    [0.15, 0.85],
                    [0.15, 0.55],
                ]),
                Part::Line(&[
                    [0.55, 0.55],
                    [0.85, 0.55],
                    [0.85, 0.85],
                    [0.55, 0.85],
                    [0.55, 0.55],
                ]),
            ],
            Tool::Cube => node_icons::BOX,
            Tool::Diag => &[Part::Line(&[
                [0.08, 0.55],
                [0.3, 0.55],
                [0.4, 0.2],
                [0.58, 0.85],
                [0.68, 0.5],
                [0.92, 0.5],
            ])],
            Tool::Grid => &[
                Part::Dot([0.2, 0.2]),
                Part::Dot([0.5, 0.2]),
                Part::Dot([0.8, 0.2]),
                Part::Dot([0.2, 0.5]),
                Part::Dot([0.5, 0.5]),
                Part::Dot([0.8, 0.5]),
                Part::Dot([0.2, 0.8]),
                Part::Dot([0.5, 0.8]),
                Part::Dot([0.8, 0.8]),
            ],
            Tool::Map => &[
                Part::Line(&[[0.1, 0.2], [0.9, 0.2], [0.9, 0.8], [0.1, 0.8], [0.1, 0.2]]),
                Part::Line(&[
                    [0.5, 0.45],
                    [0.8, 0.45],
                    [0.8, 0.7],
                    [0.5, 0.7],
                    [0.5, 0.45],
                ]),
            ],
            Tool::Fit => &[
                Part::Line(&[[0.15, 0.38], [0.15, 0.15], [0.38, 0.15]]),
                Part::Line(&[[0.62, 0.15], [0.85, 0.15], [0.85, 0.38]]),
                Part::Line(&[[0.85, 0.62], [0.85, 0.85], [0.62, 0.85]]),
                Part::Line(&[[0.38, 0.85], [0.15, 0.85], [0.15, 0.62]]),
            ],
        }
    }
}

/// Fill and edge of a Melt button from its hover / chosen amounts (0..1) and press state.
pub fn button_colors(hot: f32, on: f32, pressed: bool) -> (Color32, Color32) {
    let hot = hot.clamp(0.0, 1.0);
    let on = on.clamp(0.0, 1.0);
    let base = if pressed {
        theme::BTN_DOWN
    } else {
        theme::BTN_FILL.lerp_to_gamma(theme::BTN_HOT, hot)
    };
    let fill = base.lerp_to_gamma(theme::BTN_ON, on);
    let edge = theme::BTN_EDGE
        .lerp_to_gamma(Color32::from_rgb(92, 92, 98), hot * 0.6)
        .lerp_to_gamma(theme::BTN_ON_EDGE, on);
    (fill, edge)
}

/// Icon plus short label button in the Melt style: flat fill, 1px edge, hot / on / down states.
/// `on` marks a toggle that is switched on. With `show_label` false only the icon is drawn.
/// Other themes get a standard egui button.
pub fn tool_button(
    ui: &mut Ui,
    tool: Tool,
    label: &str,
    tip: &str,
    on: bool,
    show_label: bool,
) -> Response {
    if !theme::is_melt() {
        let text = if show_label { label } else { "·" };
        return ui.selectable_label(on, text).on_hover_text(tip);
    }
    let enabled = ui.is_enabled();
    let galley = show_label.then(|| {
        ui.painter().layout_no_wrap(
            label.to_owned(),
            FontId::proportional(13.0),
            Color32::PLACEHOLDER,
        )
    });
    let icon_size = 16.0;
    let width = match &galley {
        Some(g) => 10.0 + icon_size + 6.0 + g.size().x + 10.0,
        None => 28.0,
    };
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width, 28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let ctx = ui.ctx();
    let hot = ctx.animate_value_with_time(
        resp.id.with("hot"),
        if resp.hovered() && enabled { 1.0 } else { 0.0 },
        0.10,
    );
    let on_t = ctx.animate_value_with_time(resp.id.with("on"), if on { 1.0 } else { 0.0 }, 0.12);
    let pressed = enabled && resp.is_pointer_button_down_on();
    let (fill, edge) = button_colors(hot, on_t, pressed);
    ui.painter()
        .rect(rect, 6, fill, Stroke::new(1.0, edge), StrokeKind::Inside);
    let ink = if !enabled {
        theme::MELT_DIM.gamma_multiply(0.5)
    } else if on {
        Color32::WHITE
    } else {
        theme::MELT_TEXT.lerp_to_gamma(Color32::WHITE, hot * 0.6)
    };
    let icon_rect = if galley.is_some() {
        Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.center().y - icon_size / 2.0),
            egui::vec2(icon_size, icon_size),
        )
    } else {
        Rect::from_center_size(rect.center(), egui::vec2(icon_size, icon_size))
    };
    node_icons::draw(
        ui.painter(),
        icon_rect.expand(icon_size * 0.18),
        tool.glyph(),
        ink,
    );
    if let Some(g) = galley {
        let pos = egui::pos2(icon_rect.right() + 6.0, rect.center().y - g.size().y / 2.0);
        ui.painter().galley(pos, g, ink);
    }
    resp.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_states_follow_the_melt_tokens() {
        let (idle, idle_edge) = button_colors(0.0, 0.0, false);
        assert_eq!(idle, theme::BTN_FILL);
        assert_eq!(idle_edge, theme::BTN_EDGE);
        let (hot, _) = button_colors(1.0, 0.0, false);
        assert_eq!(hot, theme::BTN_HOT);
        let (on, on_edge) = button_colors(0.0, 1.0, false);
        assert_eq!(on, theme::BTN_ON);
        assert_eq!(on_edge, theme::BTN_ON_EDGE);
        let (down, _) = button_colors(1.0, 0.0, true);
        assert_eq!(down, theme::BTN_DOWN);
    }

    use super::*;
    use nodes::model::NodeKind;

    #[test]
    fn root_cause_walks_to_the_first_failed_node() {
        let mut graph = Graph::default();
        let kind = NodeKind::all()[0];
        let a = graph.add(kind, [0.0, 0.0]).expect("add");
        let b = graph.add(kind, [10.0, 0.0]).expect("add");
        let c = graph.add(kind, [20.0, 0.0]).expect("add");
        graph.nodes[1].inputs.insert("x".into(), a);
        graph.nodes[2].inputs.insert("x".into(), b);
        let failed = |id: NodeId| id == a || id == b || id == c;
        assert_eq!(root_cause(&graph, &failed, c), a);
        let only_c = |id: NodeId| id == c;
        assert_eq!(root_cause(&graph, &only_c, c), c);
    }

    use super::*;

    #[test]
    fn every_data_type_has_a_marker_and_shapes_differ() {
        assert_eq!(marker_of(DataType::Number), Marker::Circle);
        assert_eq!(marker_of(DataType::Point), Marker::Diamond);
        assert_eq!(marker_of(DataType::Curve), Marker::Pill);
        for kind in [DataType::Surface, DataType::Solid, DataType::Mesh] {
            assert_eq!(marker_of(kind), Marker::Square);
        }
    }

    #[test]
    fn selected_wires_are_thicker() {
        assert!(wire_width(true) > wire_width(false));
    }
}
