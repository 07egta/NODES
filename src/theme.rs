use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextStyle,
    Visuals,
};
use std::sync::atomic::{AtomicU8, Ordering};

// Light-theme values. Files that do not read the mode (and the light branch of the
// `pick` helpers below) use these directly.
pub const INK: Color32 = Color32::from_rgb(35, 52, 57);
pub const MUTED: Color32 = Color32::from_rgb(83, 99, 104);
pub const TEAL: Color32 = Color32::from_rgb(21, 108, 94);
pub const TEAL_PALE: Color32 = Color32::from_rgb(220, 239, 231);
pub const LINE: Color32 = Color32::from_rgb(220, 226, 222);
pub const CANVAS: Color32 = Color32::from_rgb(240, 243, 245);
pub const ERROR: Color32 = Color32::from_rgb(159, 52, 49);

/// Visual themes. `Melt` (the Melt 3D look) is the default; the older dark and light
/// themes stay selectable in the 表示 menu and are kept in the settings file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Melt,
    Dark,
    Light,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Melt, Mode::Dark, Mode::Light];
    pub fn label(self) -> &'static str {
        match self {
            Mode::Melt => "Melt（標準）",
            Mode::Dark => "ダーク（従来）",
            Mode::Light => "ライト",
        }
    }
    /// The value after `theme=` in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Mode::Melt => "melt",
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }
    pub fn parse(text: &str) -> Option<Mode> {
        match text.trim().strip_prefix("theme=").unwrap_or(text.trim()) {
            "melt" => Some(Mode::Melt),
            "dark" => Some(Mode::Dark),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }
    fn code(self) -> u8 {
        match self {
            Mode::Melt => 0,
            Mode::Dark => 1,
            Mode::Light => 2,
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_mode(mode: Mode) {
    MODE.store(mode.code(), Ordering::Relaxed);
}
pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Dark,
        2 => Mode::Light,
        _ => Mode::Melt,
    }
}
/// True for both dark themes.
pub fn is_dark() -> bool {
    mode() != Mode::Light
}
pub fn is_melt() -> bool {
    mode() == Mode::Melt
}
fn pick(light: Color32, dark: Color32) -> Color32 {
    if is_dark() { dark } else { light }
}
/// Like `pick`, with a third value for the Melt theme.
fn pick3(light: Color32, dark: Color32, melt: Color32) -> Color32 {
    match mode() {
        Mode::Melt => melt,
        Mode::Dark => dark,
        Mode::Light => light,
    }
}
const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

// Melt 3D tokens.
pub const MELT_PANEL_DARK: Color32 = c(24, 24, 25);
pub const MELT_PANEL: Color32 = c(31, 31, 32);
pub const MELT_CARD: Color32 = c(44, 44, 46);
pub const MELT_HOVER: Color32 = c(62, 62, 65);
pub const MELT_BORDER: Color32 = c(72, 72, 72);
pub const MELT_TEXT: Color32 = c(230, 230, 230);
pub const MELT_DIM: Color32 = c(158, 158, 158);
pub const MELT_ACCENT: Color32 = c(52, 178, 120);
pub const MELT_ACCENT_SOFT: Color32 = c(34, 118, 82);
pub const MELT_ERROR: Color32 = c(244, 124, 132);
pub const MELT_SELECTION: Color32 = c(250, 200, 110);
pub const MELT_INFO: Color32 = c(74, 163, 216);
/// Button surface states (flat fill and a 1px edge).
pub const BTN_FILL: Color32 = c(42, 42, 44);
pub const BTN_EDGE: Color32 = c(48, 48, 50);
pub const BTN_HOT: Color32 = c(62, 62, 65);
pub const BTN_ON: Color32 = c(36, 132, 90);
pub const BTN_ON_EDGE: Color32 = c(70, 190, 134);
pub const BTN_DOWN: Color32 = c(36, 36, 38);

/// Body text on the current surfaces.
pub fn ink() -> Color32 {
    pick3(INK, c(218, 228, 224), MELT_TEXT)
}
pub fn muted() -> Color32 {
    pick3(MUTED, c(140, 158, 160), MELT_DIM)
}
pub fn teal() -> Color32 {
    pick3(TEAL, c(96, 200, 174), MELT_ACCENT)
}
pub fn line() -> Color32 {
    pick3(LINE, c(62, 72, 74), c(58, 58, 60))
}
pub fn canvas_bg() -> Color32 {
    pick3(CANVAS, c(22, 26, 28), MELT_PANEL_DARK)
}
pub fn grid_dot() -> Color32 {
    pick3(c(205, 214, 207), c(52, 60, 62), c(48, 48, 50))
}
pub fn panel_bg() -> Color32 {
    pick3(c(249, 251, 247), c(30, 35, 37), MELT_PANEL)
}
pub fn workspace_bg() -> Color32 {
    pick3(c(251, 253, 249), c(32, 37, 39), MELT_PANEL)
}
pub fn toolbar_bg() -> Color32 {
    pick3(c(253, 254, 250), c(36, 41, 43), MELT_PANEL)
}
pub fn status_bg() -> Color32 {
    pick3(c(237, 242, 234), c(24, 28, 30), MELT_PANEL_DARK)
}
pub fn card_bg() -> Color32 {
    pick3(c(253, 254, 251), c(41, 47, 49), MELT_CARD)
}
pub fn card_off_bg() -> Color32 {
    pick3(c(234, 236, 232), c(31, 35, 37), c(34, 34, 35))
}
pub fn card_border() -> Color32 {
    pick3(c(211, 220, 211), c(68, 78, 80), MELT_BORDER)
}
pub fn wire_idle() -> Color32 {
    pick3(c(156, 180, 169), c(108, 136, 126), c(104, 104, 110))
}
pub fn error() -> Color32 {
    pick3(ERROR, c(255, 153, 143), MELT_ERROR)
}
pub fn error_fill() -> Color32 {
    pick3(c(252, 231, 224), c(78, 46, 42), c(72, 38, 42))
}

pub fn configure(ctx: &egui::Context, mode: Mode) {
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
    set_mode(mode);
    apply_style(ctx);
}

/// Re-applies colours and spacing for the current mode. Called again when the toggle changes.
pub fn apply_style(ctx: &egui::Context) {
    if is_melt() {
        apply_melt(ctx);
        return;
    }
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
    style.spacing.item_spacing = egui::vec2(7.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 5.0);
    style.spacing.interact_size.y = 26.0;
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

/// The Melt 3D look: flat dark surfaces, one green accent, 1px edges, small radii.
fn apply_melt(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut v = Visuals::dark();
    v.panel_fill = MELT_PANEL;
    v.window_fill = c(34, 34, 36);
    v.extreme_bg_color = c(20, 20, 21);
    v.faint_bg_color = MELT_CARD;
    v.override_text_color = Some(MELT_TEXT);
    v.selection.bg_fill = MELT_ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0_f32, MELT_ACCENT);
    v.hyperlink_color = MELT_ACCENT;
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.menu_corner_radius = egui::CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(30));
    v.window_shadow = egui::Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    v.popup_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    let round = egui::CornerRadius::same(6);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = round;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(20));
    v.widgets.inactive.bg_fill = MELT_CARD;
    v.widgets.inactive.weak_bg_fill = BTN_FILL;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BTN_EDGE);
    v.widgets.hovered.bg_fill = MELT_HOVER;
    v.widgets.hovered.weak_bg_fill = BTN_HOT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(50));
    v.widgets.active.bg_fill = MELT_ACCENT_SOFT;
    v.widgets.active.weak_bg_fill = BTN_DOWN;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, MELT_ACCENT);
    v.widgets.open.bg_fill = MELT_HOVER;
    v.widgets.open.weak_bg_fill = MELT_HOVER;
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size = egui::vec2(40.0, 26.0);
    style.spacing.slider_width = 120.0;
    style.spacing.menu_margin = egui::Margin::same(6);
    style.animation_time = 0.12;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(18.0));
    ctx.set_style(style);
}

pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.0).color(muted())
}

/// Category colour, used for the card header tint, the pill and the library labels.
pub fn category_color(category: &str) -> Color32 {
    // (light, dark, melt): the Melt column is tuned to stay readable on #181819.
    let (light, dark, melt) = match category {
        "数値" => ((138, 91, 25), (226, 176, 91), (250, 200, 110)),
        "曲線" => ((40, 99, 166), (124, 185, 247), (120, 190, 255)),
        "精密曲面" => ((30, 106, 137), (113, 198, 230), (74, 163, 216)),
        "立体" => ((16, 113, 94), (92, 204, 173), (120, 214, 150)),
        "変形" => ((112, 73, 166), (195, 157, 244), (192, 152, 246)),
        "配列" => ((155, 83, 47), (246, 163, 116), (246, 150, 108)),
        "データ" => ((75, 96, 127), (166, 187, 216), (160, 184, 214)),
        _ => ((45, 104, 94), (133, 201, 184), (130, 206, 184)),
    };
    pick3(
        c(light.0, light.1, light.2),
        c(dark.0, dark.1, dark.2),
        c(melt.0, melt.1, melt.2),
    )
}

/// Port colors describe the data type, not the component category. The tooltip also
/// spells the type out, so connections never rely on color alone.
pub fn port_color(kind: nodes::model::DataType) -> Color32 {
    use nodes::model::DataType::*;
    let (light, dark, melt) = match kind {
        Number => ((149, 95, 26), (239, 186, 95), (250, 200, 110)),
        Point => ((158, 61, 112), (242, 154, 203), (244, 140, 196)),
        Curve => ((39, 99, 174), (135, 189, 255), (120, 190, 255)),
        Surface => ((30, 106, 137), (113, 198, 230), (74, 163, 216)),
        Solid => ((73, 99, 132), (155, 188, 227), (150, 176, 232)),
        Mesh => ((18, 112, 95), (93, 212, 177), (120, 214, 150)),
        Geometry | DisplayGeometry => ((109, 70, 161), (194, 159, 250), (192, 152, 246)),
        Any => ((82, 98, 113), (170, 187, 203), (170, 170, 176)),
    };
    pick3(
        c(light.0, light.1, light.2),
        c(dark.0, dark.1, dark.2),
        c(melt.0, melt.1, melt.2),
    )
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
