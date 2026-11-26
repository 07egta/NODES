use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextStyle,
};
pub const INK: Color32 = Color32::from_rgb(35, 52, 57);
pub const MUTED: Color32 = Color32::from_rgb(112, 129, 131);
pub const TEAL: Color32 = Color32::from_rgb(28, 133, 116);
pub const TEAL_PALE: Color32 = Color32::from_rgb(220, 239, 231);
pub const LINE: Color32 = Color32::from_rgb(220, 226, 222);
pub const CANVAS: Color32 = Color32::from_rgb(245, 246, 241);
pub const ERROR: Color32 = Color32::from_rgb(184, 75, 66);

pub fn configure(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "morphlet-jp".into(),
        FontData::from_static(include_bytes!("../assets/MorphletJP.ttf")).into(),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("morphlet-jp".into());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = Color32::from_rgb(253, 253, 250);
    style.visuals.window_fill = Color32::from_rgb(253, 253, 250);
    style.visuals.extreme_bg_color = Color32::from_rgb(241, 244, 239);
    style.visuals.faint_bg_color = Color32::from_rgb(243, 246, 241);
    style.visuals.override_text_color = Some(INK);
    style.visuals.selection.bg_fill = TEAL_PALE;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, TEAL);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(240, 244, 239);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(240, 244, 239);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.hovered.bg_fill = TEAL_PALE;
    style.visuals.widgets.hovered.weak_bg_fill = TEAL_PALE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, TEAL);
    style.visuals.widgets.active.bg_fill = TEAL_PALE;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, TEAL);
    style.visuals.window_corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.slider_width = 130.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(12.5));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(20.0));
    ctx.set_style(style);
}

pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.0).color(MUTED)
}
pub fn category_color(category: &str) -> Color32 {
    match category {
        "数値" => Color32::from_rgb(148, 101, 34),
        "曲線" => Color32::from_rgb(75, 127, 183),
        "立体" => TEAL,
        "変形" => Color32::from_rgb(133, 104, 178),
        "配列" => Color32::from_rgb(196, 119, 85),
        "データ" => Color32::from_rgb(107, 124, 145),
        _ => Color32::from_rgb(61, 130, 112),
    }
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.13))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(color));
        });
}
