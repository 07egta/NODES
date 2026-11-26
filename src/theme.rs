use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextStyle,
    Visuals,
};
use std::sync::atomic::{AtomicBool, Ordering};

// Light-theme values. Files that do not read the mode (and the light branch of the
// `pick` helpers below) use these directly.
pub const INK: Color32 = Color32::from_rgb(35, 52, 57);
pub const MUTED: Color32 = Color32::from_rgb(112, 129, 131);
pub const TEAL: Color32 = Color32::from_rgb(28, 133, 116);
pub const TEAL_PALE: Color32 = Color32::from_rgb(220, 239, 231);
pub const LINE: Color32 = Color32::from_rgb(220, 226, 222);
pub const CANVAS: Color32 = Color32::from_rgb(245, 246, 241);
pub const ERROR: Color32 = Color32::from_rgb(184, 75, 66);

/// Dark is the default. Set from the saved setting at start-up and by the topbar toggle.
static DARK: AtomicBool = AtomicBool::new(true);

pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}
pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}
fn pick(light: Color32, dark: Color32) -> Color32 {
    if is_dark() { dark } else { light }
}

/// Body text on the current surfaces.
pub fn ink() -> Color32 {
    pick(INK, Color32::from_rgb(218, 228, 224))
}
pub fn muted() -> Color32 {
    pick(MUTED, Color32::from_rgb(140, 158, 160))
}
pub fn teal() -> Color32 {
    pick(TEAL, Color32::from_rgb(96, 200, 174))
}
pub fn line() -> Color32 {
    pick(LINE, Color32::from_rgb(62, 72, 74))
}
pub fn canvas_bg() -> Color32 {
    pick(CANVAS, Color32::from_rgb(22, 26, 28))
}
pub fn grid_dot() -> Color32 {
    pick(
        Color32::from_rgb(205, 214, 207),
        Color32::from_rgb(52, 60, 62),
    )
}
pub fn panel_bg() -> Color32 {
    pick(
        Color32::from_rgb(249, 251, 247),
        Color32::from_rgb(30, 35, 37),
    )
}
pub fn workspace_bg() -> Color32 {
    pick(
        Color32::from_rgb(251, 253, 249),
        Color32::from_rgb(32, 37, 39),
    )
}
pub fn toolbar_bg() -> Color32 {
    pick(
        Color32::from_rgb(253, 254, 250),
        Color32::from_rgb(36, 41, 43),
    )
}
pub fn status_bg() -> Color32 {
    pick(
        Color32::from_rgb(237, 242, 234),
        Color32::from_rgb(24, 28, 30),
    )
}
pub fn card_bg() -> Color32 {
    pick(
        Color32::from_rgb(253, 254, 251),
        Color32::from_rgb(41, 47, 49),
    )
}
pub fn card_off_bg() -> Color32 {
    pick(
        Color32::from_rgb(234, 236, 232),
        Color32::from_rgb(31, 35, 37),
    )
}
pub fn card_border() -> Color32 {
    pick(
        Color32::from_rgb(211, 220, 211),
        Color32::from_rgb(68, 78, 80),
    )
}
pub fn wire_idle() -> Color32 {
    pick(
        Color32::from_rgb(156, 180, 169),
        Color32::from_rgb(108, 136, 126),
    )
}
pub fn error_fill() -> Color32 {
    pick(
        Color32::from_rgb(252, 231, 224),
        Color32::from_rgb(78, 46, 42),
    )
}

pub fn configure(ctx: &egui::Context, dark: bool) {
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
    set_dark(dark);
    apply_style(ctx);
}

/// Re-applies colours and spacing for the current mode. Called again when the toggle changes.
pub fn apply_style(ctx: &egui::Context) {
    let dark = is_dark();
    let mut style = (*ctx.style()).clone();
    style.visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    style.visuals.panel_fill = pick(
        Color32::from_rgb(253, 253, 250),
        Color32::from_rgb(28, 32, 34),
    );
    style.visuals.window_fill = pick(
        Color32::from_rgb(253, 253, 250),
        Color32::from_rgb(34, 39, 41),
    );
    style.visuals.extreme_bg_color = pick(
        Color32::from_rgb(241, 244, 239),
        Color32::from_rgb(20, 24, 26),
    );
    style.visuals.faint_bg_color = pick(
        Color32::from_rgb(243, 246, 241),
        Color32::from_rgb(30, 35, 37),
    );
    style.visuals.override_text_color = if dark { None } else { Some(INK) };
    style.visuals.selection.bg_fill = pick(TEAL_PALE, Color32::from_rgb(28, 76, 66));
    style.visuals.selection.stroke = Stroke::new(1.0_f32, teal());
    let idle = pick(
        Color32::from_rgb(240, 244, 239),
        Color32::from_rgb(45, 52, 54),
    );
    let hover = pick(TEAL_PALE, Color32::from_rgb(40, 68, 62));
    style.visuals.widgets.inactive.bg_fill = idle;
    style.visuals.widgets.inactive.weak_bg_fill = idle;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, line());
    style.visuals.widgets.hovered.bg_fill = hover;
    style.visuals.widgets.hovered.weak_bg_fill = hover;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, teal());
    style.visuals.widgets.active.bg_fill = hover;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, teal());
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
    RichText::new(text).size(11.0).color(muted())
}

/// Category colour, used for the card header tint, the pill and the library labels.
pub fn category_color(category: &str) -> Color32 {
    match category {
        "数値" => Color32::from_rgb(176, 128, 52),
        "曲線" => Color32::from_rgb(88, 142, 200),
        "立体" => Color32::from_rgb(34, 150, 128),
        "変形" => Color32::from_rgb(146, 116, 196),
        "配列" => Color32::from_rgb(208, 128, 92),
        "データ" => Color32::from_rgb(120, 138, 160),
        _ => Color32::from_rgb(66, 140, 120),
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
