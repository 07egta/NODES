//! Melt 3D style widgets and small painters shared by the canvas and the panels.
use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use nodes::model::DataType;

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

#[cfg(test)]
mod tests {
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
