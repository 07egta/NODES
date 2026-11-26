//! A small, dependency-free CAD viewport built on egui's triangle painter.
//!
//! Geometry and smoothed normals are cached independently of the camera.
//! Solid surfaces are drawn by a software z-buffer (`raster.rs`) into one
//! texture, so intersecting parts are depth-correct. Meshes are capped at
//! `MAX_TRIANGLES`. Wireframe mode, or turning off 正確な奥行き, uses the
//! older back-to-front painter sort. See docs/ARCHITECTURE.md section 6.
//!
//! The 3D print helpers (build plate, fit check, volume, weight, overhang
//! colouring) are in `print_info.rs`.

use std::collections::hash_map::DefaultHasher;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Vec2,
};
use morphlet::model::{Curve, Mesh, Tree, Value};
use morphlet::persistence::{self, ExportReport};
use morphlet::print_info::{self, Printer};
use morphlet::raster::{Frame, Vertex as RasterVertex};

type V3 = [f64; 3];

/// Largest mesh the depth-correct z-buffer path handles. Above this the
/// preview is decimated and drawn with the same cap.
const MAX_TRIANGLES: usize = 200_000;
const MAX_SEGMENTS: usize = 18_000;
const MAX_POINTS: usize = 6_000;
const MAX_VALUES: usize = 50_000;
const MAX_BOUND_SAMPLES: usize = 200_000;
const MAX_COORDINATE: f64 = 1.0e14;
/// Pixel budget for the z-buffer texture. Above this the supersampling scale
/// is reduced so that large windows keep a usable frame rate.
const MAX_RASTER_PIXELS: f32 = 1_500_000.0;
/// Build-plate grid spacing in mm.
const BED_GRID_MM: f64 = 10.0;

pub struct PreviewItem {
    pub node_id: u64,
    pub tree: Tree,
    pub selected: bool,
}

pub struct Viewport {
    pub wireframe: bool,
    pub show_grid: bool,
    pub auto_rotate: bool,
    /// `true` is parallel (orthographic) projection, the default. `false` is perspective.
    pub orthographic: bool,
    /// Build plate drawn under the model. `Printer::Off` draws the normal grid.
    pub printer: Printer,
    /// Build volume used by `Printer::Custom`, in mm.
    pub custom_bed: V3,
    /// Colour faces steeper than `overhang_limit_deg` in orange.
    pub overhang_preview: bool,
    pub overhang_limit_deg: f64,
    /// Use the z-buffer for solid surfaces (on by default).
    pub accurate_depth: bool,
    yaw: f64,
    pitch: f64,
    target: V3,
    span: f64,
    last_size: Vec2,
    geometry_hash: Option<u64>,
    selection_hash: Option<u64>,
    scene: Scene,
    generation: u64,
    projected: Option<Projection>,
    first_fit: bool,
    fit_requested: bool,
    texture: Option<egui::TextureHandle>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            wireframe: false,
            show_grid: true,
            auto_rotate: false,
            orthographic: true,
            printer: Printer::Off,
            custom_bed: [200.0; 3],
            overhang_preview: false,
            overhang_limit_deg: 45.0,
            accurate_depth: true,
            yaw: -PI / 3.0,
            pitch: 0.48,
            target: [0.0; 3],
            span: 140.0,
            last_size: Vec2::new(700.0, 600.0),
            geometry_hash: None,
            selection_hash: None,
            scene: Scene::default(),
            generation: 0,
            projected: None,
            first_fit: true,
            fit_requested: false,
            texture: None,
        }
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    min: V3,
    max: V3,
}

impl Bounds {
    fn point(p: V3) -> Self {
        Self { min: p, max: p }
    }

    fn include(&mut self, p: V3) {
        for (i, value) in p.iter().enumerate() {
            self.min[i] = self.min[i].min(*value);
            self.max[i] = self.max[i].max(*value);
        }
    }

    fn center(self) -> V3 {
        mul(add(self.min, self.max), 0.5)
    }

    fn corners(self) -> [V3; 8] {
        std::array::from_fn(|i| {
            [
                if i & 1 == 0 { self.min[0] } else { self.max[0] },
                if i & 2 == 0 { self.min[1] } else { self.max[1] },
                if i & 4 == 0 { self.min[2] } else { self.max[2] },
            ]
        })
    }
}

struct WorldTriangle {
    p: [V3; 3],
    normals: [V3; 3],
    /// Unit face normal from the winding order, as used by the overhang node.
    face: V3,
    /// Lies on the mesh's lowest Z (the build plate); never an overhang.
    plate: bool,
    node_id: u64,
    selected: bool,
}

struct WorldSegment {
    a: V3,
    b: V3,
    node_id: u64,
    selected: bool,
}

struct WorldPoint {
    p: V3,
    node_id: u64,
    selected: bool,
}

#[derive(Default)]
struct Scene {
    triangles: Vec<WorldTriangle>,
    segments: Vec<WorldSegment>,
    points: Vec<WorldPoint>,
    bounds: Option<Bounds>,
    total_triangles: usize,
    reduced: bool,
    invalid: usize,
    /// Print check of the same trees (`persistence::mesh_stats`). Built only when
    /// the geometry changes, never per camera move.
    report: Option<ExportReport>,
    // Keep source allocations alive so a new Arc cannot reuse a cached pointer.
    mesh_sources: Vec<Arc<Mesh>>,
    curve_sources: Vec<Arc<Curve>>,
}

impl Scene {
    fn include(&mut self, p: V3) {
        if !valid(p) {
            return;
        }
        if let Some(bounds) = &mut self.bounds {
            bounds.include(p);
        } else {
            self.bounds = Some(Bounds::point(p));
        }
    }
}

struct PickTriangle {
    p: [Pos2; 3],
    node_id: u64,
    depth: f64,
}

struct PickSegment {
    a: Pos2,
    b: Pos2,
    node_id: u64,
    depth: f64,
}

struct PickPoint {
    p: Pos2,
    node_id: u64,
    depth: f64,
}

struct Projection {
    hash: u64,
    /// `true` when the solid surfaces are in `image`, not in `under`.
    raster: bool,
    image: Option<egui::ColorImage>,
    /// Drawn before the surfaces: background grid, bed, shadow, sorted surfaces.
    under: Vec<Shape>,
    /// Drawn after the surfaces: wires, curves, points, box edges.
    over: Vec<Shape>,
    triangles: Vec<PickTriangle>,
    segments: Vec<PickSegment>,
    points: Vec<PickPoint>,
}

/// Which colour a surface triangle gets.
#[derive(Clone, Copy)]
enum SurfaceKind {
    Normal,
    Selected,
    Overhang,
}

/// What the build plate and model box look like this frame.
#[derive(Clone, Copy, PartialEq)]
struct BedPlan {
    volume: V3,
    /// XY centre of the bed: the model's bounding-box centre.
    center: [f64; 2],
    /// Z of the bed surface: the model's lowest point.
    ground: f64,
    fits: bool,
    exceeding: [bool; 3],
}

#[derive(Clone, Copy)]
struct Camera {
    right: V3,
    up: V3,
    eye: V3,
    target: V3,
    scale: f64,
    center: Pos2,
    perspective: bool,
    /// Eye distance from the target, in mm. Used only when `perspective`.
    distance: f64,
}

impl Camera {
    /// Distance from the eye to `p`, clamped to stay positive.
    fn eye_distance(self, p: V3) -> f64 {
        (self.distance - self.depth(p)).max(self.distance * 0.05)
    }

    /// Screen scale for `p`: 1 in parallel projection, larger when closer.
    fn scale_at(self, p: V3) -> f64 {
        if self.perspective {
            self.scale * self.distance / self.eye_distance(p)
        } else {
            self.scale
        }
    }

    fn project(self, p: V3) -> Pos2 {
        let relative = sub(p, self.target);
        let scale = self.scale_at(p);
        Pos2::new(
            self.center.x + (dot(relative, self.right) * scale) as f32,
            self.center.y - (dot(relative, self.up) * scale) as f32,
        )
    }

    fn depth(self, p: V3) -> f64 {
        dot(sub(p, self.target), self.eye)
    }

    /// Larger is nearer. Linear in screen space for both projections, which
    /// the z-buffer needs (see raster.rs).
    fn key(self, p: V3) -> f64 {
        if self.perspective {
            1.0 / self.eye_distance(p)
        } else {
            self.depth(p)
        }
    }
}

impl Viewport {
    /// Fill the remaining UI area and return the picked geometry's source node.
    pub fn show(&mut self, ui: &mut egui::Ui, items: &[PreviewItem]) -> Option<u64> {
        let rect = ui.available_rect_before_wrap();
        if rect.width() < 2.0 || rect.height() < 2.0 {
            return None;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.last_size = rect.size();
        self.ensure_scene(items);
        if self.first_fit && self.scene.bounds.is_some() {
            self.fit_cached();
            self.first_fit = false;
        }
        if self.fit_requested {
            self.fit_cached();
            self.fit_requested = false;
        }

        let pointer = ui.input(|i| i.pointer.hover_pos());
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 {
                let old_camera = self.camera(rect);
                self.span = (self.span * (-f64::from(scroll.clamp(-500.0, 500.0)) * 0.0025).exp())
                    .clamp(1.0e-7, 1.0e12);
                let new_camera = self.camera(rect);
                if let Some(p) = pointer {
                    let x = f64::from(p.x - old_camera.center.x);
                    let y = -f64::from(p.y - old_camera.center.y);
                    self.target = add(
                        self.target,
                        add(
                            mul(
                                old_camera.right,
                                x / old_camera.scale - x / new_camera.scale,
                            ),
                            mul(old_camera.up, y / old_camera.scale - y / new_camera.scale),
                        ),
                    );
                }
            }
            if response.double_clicked() {
                self.fit_cached();
            }
        }

        let delta = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);
        if response.dragged_by(PointerButton::Middle)
            || (shift && response.dragged_by(PointerButton::Primary))
        {
            let camera = self.camera(rect);
            self.target = add(
                self.target,
                add(
                    mul(camera.right, -f64::from(delta.x) / camera.scale),
                    mul(camera.up, f64::from(delta.y) / camera.scale),
                ),
            );
        } else if response.dragged_by(PointerButton::Primary) {
            self.yaw -= f64::from(delta.x) * 0.008;
            self.pitch = (self.pitch + f64::from(delta.y) * 0.008).clamp(-1.54, 1.54);
        }

        if self.auto_rotate && !response.dragged() {
            let dt = ui.input(|i| i.stable_dt.min(0.05));
            self.yaw += f64::from(dt) * 0.24;
            ui.ctx().request_repaint();
        }

        let dark = ui.visuals().dark_mode;
        let painter = ui.painter_at(rect);
        painter.add(background(rect, dark));

        let camera = self.camera(rect);
        let scale = raster_scale(rect, ui.ctx().pixels_per_point());
        let projection_hash = self.projection_hash(rect, dark, scale);
        if self.projected.as_ref().map(|p| p.hash) != Some(projection_hash) {
            let mut projection = self.project(camera, rect, scale, dark, projection_hash);
            if let Some(image) = projection.image.take() {
                self.upload(ui.ctx(), image);
            }
            self.projected = Some(projection);
        }
        if let Some(projection) = &self.projected {
            painter.extend(projection.under.iter().cloned());
            if let Some(texture) = self.texture.as_ref().filter(|_| projection.raster) {
                painter.image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            painter.extend(projection.over.iter().cloned());
        }

        // Quiet context only. Controls live in the parent panel (`controls`).
        let muted = if dark {
            Color32::from_rgb(153, 176, 185)
        } else {
            Color32::from_rgb(96, 118, 129)
        };
        let ink = if dark {
            Color32::from_rgb(219, 234, 238)
        } else {
            Color32::from_rgb(47, 68, 78)
        };
        painter.text(
            rect.min + Vec2::new(14.0, 12.0),
            Align2::LEFT_TOP,
            if self.orthographic {
                "正投影  ·  mm"
            } else {
                "透視  ·  mm"
            },
            FontId::proportional(10.0),
            muted,
        );
        self.paint_print_info(&painter, rect, dark, ink, muted);

        if self.scene.bounds.is_none() {
            let center = rect.center();
            painter.circle_stroke(
                center - Vec2::new(0.0, 28.0),
                22.0,
                Stroke::new(1.2_f32, muted.gamma_multiply(0.35)),
            );
            painter.line_segment(
                [
                    center + Vec2::new(-9.0, -28.0),
                    center + Vec2::new(9.0, -28.0),
                ],
                Stroke::new(1.3_f32, muted),
            );
            painter.line_segment(
                [
                    center + Vec2::new(0.0, -37.0),
                    center + Vec2::new(0.0, -19.0),
                ],
                Stroke::new(1.3_f32, muted),
            );
            painter.text(
                center + Vec2::new(0.0, 13.0),
                Align2::CENTER_CENTER,
                "ここに形が現れます",
                FontId::proportional(16.0),
                ink,
            );
            painter.text(
                center + Vec2::new(0.0, 39.0),
                Align2::CENTER_CENTER,
                "形状ノードを追加し、出力をつないでみましょう",
                FontId::proportional(11.5),
                muted,
            );
        }

        let help = if rect.width() > 520.0 {
            "ドラッグで回転   ·   Shift＋ドラッグで移動   ·   ホイールで拡大"
        } else {
            "回転：ドラッグ   移動：Shift＋ドラッグ"
        };
        painter.text(
            Pos2::new(rect.min.x + 17.0, rect.max.y - 16.0),
            Align2::LEFT_BOTTOM,
            help,
            FontId::proportional(10.0),
            muted,
        );
        self.paint_axes(&painter, rect, camera, dark);

        if self.scene.reduced || self.scene.invalid > 0 {
            let message = if self.scene.reduced {
                "形が大きいためプレビューを簡略表示しています"
            } else {
                "表示できない面・座標を除外しました"
            };
            painter.text(
                Pos2::new(rect.min.x + 17.0, rect.max.y - 36.0),
                Align2::LEFT_BOTTOM,
                message,
                FontId::proportional(10.0),
                if dark {
                    Color32::from_rgb(235, 190, 116)
                } else {
                    Color32::from_rgb(156, 105, 36)
                },
            );
        }

        if response.clicked()
            && !response.double_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            return self.pick(p);
        }
        None
    }

    pub fn fit(&mut self, items: &[PreviewItem]) {
        self.ensure_scene(items);
        self.fit_cached();
        self.first_fit = self.scene.bounds.is_none();
    }

    pub fn set_view(&mut self, view: &str) {
        self.auto_rotate = false;
        match view {
            "front" => {
                self.yaw = -FRAC_PI_2;
                self.pitch = 0.0;
            }
            "top" => {
                self.yaw = -FRAC_PI_2;
                self.pitch = FRAC_PI_2;
            }
            "right" => {
                self.yaw = 0.0;
                self.pitch = 0.0;
            }
            _ => {
                self.yaw = -FRAC_PI_4;
                self.pitch = 0.615_479_7;
            }
        }
        self.projected = None;
    }

    /// Toolbar for the parent panel: display toggles, view presets, print options.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.wireframe, "ワイヤー");
            ui.checkbox(&mut self.show_grid, "グリッド");
            ui.checkbox(&mut self.orthographic, "正投影")
                .on_hover_text("外すと透視投影になります");
            ui.checkbox(&mut self.accurate_depth, "正確な奥行き")
                .on_hover_text("交差する面も前後を正しく描きます。ワイヤー表示中は従来の描画です");
        });
        ui.horizontal_wrapped(|ui| {
            for (label, view) in [
                ("上", "top"),
                ("前", "front"),
                ("横", "right"),
                ("斜め", "iso"),
            ] {
                if ui.small_button(label).clicked() {
                    self.set_view(view);
                }
            }
            if ui
                .small_button("全体")
                .on_hover_text("形を画面に収める · F")
                .clicked()
            {
                self.fit_requested = true;
            }
            ui.checkbox(&mut self.overhang_preview, "張り出し")
                .on_hover_text("この角度より下向きに急な面をオレンジで表示します");
            if self.overhang_preview {
                ui.add(
                    egui::DragValue::new(&mut self.overhang_limit_deg)
                        .range(0.0..=90.0)
                        .speed(0.5)
                        .suffix("°"),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("造形台");
            egui::ComboBox::from_id_salt("print-bed")
                .selected_text(self.printer.name())
                .show_ui(ui, |ui| {
                    for printer in Printer::ALL {
                        ui.selectable_value(&mut self.printer, printer, printer.name());
                    }
                });
            if self.printer == Printer::Custom {
                for (axis, value) in ["X", "Y", "Z"].into_iter().zip(self.custom_bed.iter_mut()) {
                    ui.label(axis);
                    ui.add(
                        egui::DragValue::new(value)
                            .range(10.0..=1000.0)
                            .speed(1.0)
                            .suffix(" mm"),
                    );
                }
            }
        });
    }

    fn upload(&mut self, ctx: &egui::Context, image: egui::ColorImage) {
        let handle = match self.texture.take() {
            Some(mut texture) => {
                texture.set(image, egui::TextureOptions::LINEAR);
                texture
            }
            None => ctx.load_texture("viewport-depth", image, egui::TextureOptions::LINEAR),
        };
        self.texture = Some(handle);
    }

    /// Bounding box of the model: the mesh check's bounds, else the scene's bounds.
    fn model_box(&self) -> Option<(V3, V3)> {
        self.scene
            .report
            .as_ref()
            .and_then(|report| report.bounds)
            .map(|[min, max]| (min, max))
            .or_else(|| self.scene.bounds.map(|b| (b.min, b.max)))
    }

    fn bed_plan(&self) -> Option<BedPlan> {
        let volume = self.printer.volume(self.custom_bed)?;
        match self.model_box() {
            Some((min, max)) => {
                let size = print_info::size_of(min, max);
                let exceeding = print_info::exceeding_axes(size, volume);
                Some(BedPlan {
                    volume,
                    center: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
                    ground: min[2],
                    fits: !exceeding.iter().any(|over| *over),
                    exceeding,
                })
            }
            // No model: show an empty bed centred on the origin.
            None => Some(BedPlan {
                volume,
                center: [0.0, 0.0],
                ground: 0.0,
                fits: true,
                exceeding: [false; 3],
            }),
        }
    }

    /// Size, volume, weight, mesh state and the build-plate fit, top left.
    fn paint_print_info(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        dark: bool,
        ink: Color32,
        muted: Color32,
    ) {
        let Some(report) = self.scene.report.as_ref() else {
            return;
        };
        let Some([min, max]) = report.bounds else {
            return;
        };
        let size = print_info::size_of(min, max);
        let volume_cm3 = print_info::mm3_to_cm3(report.signed_volume_mm3.abs());
        let weight = print_info::pla_weight_g(volume_cm3);
        let ok = if dark {
            Color32::from_rgb(132, 214, 186)
        } else {
            Color32::from_rgb(28, 120, 98)
        };
        let warn = if dark {
            Color32::from_rgb(255, 98, 86)
        } else {
            Color32::from_rgb(200, 36, 28)
        };
        let mut lines = vec![
            (
                format!(
                    "寸法  X {:.1} × Y {:.1} × Z {:.1} mm",
                    size[0], size[1], size[2]
                ),
                ink,
            ),
            (
                format!(
                    "体積  {:.2} cm³  ・  重さ（PLA 中実の目安）約 {:.1} g",
                    volume_cm3, weight
                ),
                ink,
            ),
            (
                format!("三角形 {} 枚  ・  {}", report.triangles, mesh_state(report)),
                ink,
            ),
        ];
        if let Some(plan) = self.bed_plan() {
            let [w, d, h] = plan.volume;
            let name = format!("{} {:.0}×{:.0}×{:.0} mm", self.printer.name(), w, d, h);
            if plan.fits {
                lines.push((format!("造形台（{name}）に収まります"), ok));
            } else {
                let axes: Vec<&str> = ["X", "Y", "Z"]
                    .into_iter()
                    .zip(plan.exceeding)
                    .filter(|(_, over)| *over)
                    .map(|(axis, _)| axis)
                    .collect();
                lines.push((
                    format!("造形台（{name}）より大きい軸: {}", axes.join("・")),
                    warn,
                ));
            }
        }
        if self.overhang_preview {
            lines.push((
                format!(
                    "張り出し: {:.0}° より下向きに急な面をオレンジで表示",
                    self.overhang_limit_deg
                ),
                muted,
            ));
        }
        let mut y = rect.min.y + 30.0;
        for (text, color) in lines {
            painter.text(
                Pos2::new(rect.min.x + 14.0, y),
                Align2::LEFT_TOP,
                text,
                FontId::proportional(11.0),
                color,
            );
            y += 16.0;
        }
    }

    fn basis(&self) -> (V3, V3, V3) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        (
            [-sy, cy, 0.0],
            [-sp * cy, -sp * sy, cp],
            [cp * cy, cp * sy, sp],
        )
    }

    fn camera(&self, rect: Rect) -> Camera {
        let (right, up, eye) = self.basis();
        let span = self.span.max(1.0e-7);
        Camera {
            right,
            up,
            eye,
            target: self.target,
            scale: f64::from(rect.width().min(rect.height()).max(1.0)) / span,
            center: rect.center() + Vec2::new(0.0, 6.0),
            perspective: !self.orthographic,
            // Twice the visible span: a moderate field of view.
            distance: span * 2.0,
        }
    }

    fn fit_cached(&mut self) {
        if let Some(bounds) = self.scene.bounds {
            self.target = bounds.center();
            let (right, up, _) = self.basis();
            let mut width: f64 = 0.0;
            let mut height: f64 = 0.0;
            for corner in bounds.corners() {
                let p = sub(corner, self.target);
                width = width.max(dot(p, right).abs() * 2.0);
                height = height.max(dot(p, up).abs() * 2.0);
            }
            let min_size = self.last_size.x.min(self.last_size.y).max(1.0);
            let extent = (width * f64::from(min_size / self.last_size.x.max(1.0)))
                .max(height * f64::from(min_size / self.last_size.y.max(1.0)));
            self.span = (extent * 1.52).clamp(1.0, 1.0e12);
        } else {
            self.target = [0.0; 3];
            self.span = 140.0;
        }
        self.projected = None;
    }

    fn ensure_scene(&mut self, items: &[PreviewItem]) {
        let mut hasher = DefaultHasher::new();
        let mut selection_hasher = DefaultHasher::new();
        let mut hashed_values = 0usize;
        items.len().hash(&mut hasher);
        'fingerprint: for item in items.iter().take(MAX_VALUES) {
            item.node_id.hash(&mut hasher);
            item.node_id.hash(&mut selection_hasher);
            item.selected.hash(&mut selection_hasher);
            for (path, values) in &item.tree.branches {
                path.hash(&mut hasher);
                values.len().hash(&mut hasher);
                for value in values.iter().take(MAX_VALUES) {
                    hashed_values += 1;
                    if hashed_values > MAX_VALUES {
                        break 'fingerprint;
                    }
                    match value {
                        Value::Number(n) => {
                            0u8.hash(&mut hasher);
                            n.to_bits().hash(&mut hasher);
                        }
                        Value::Point(p) => {
                            1u8.hash(&mut hasher);
                            for v in p {
                                v.to_bits().hash(&mut hasher);
                            }
                        }
                        Value::Curve(c) => {
                            2u8.hash(&mut hasher);
                            (Arc::as_ptr(c) as usize).hash(&mut hasher);
                            c.points.len().hash(&mut hasher);
                            c.closed.hash(&mut hasher);
                        }
                        Value::Mesh(m) => {
                            3u8.hash(&mut hasher);
                            (Arc::as_ptr(m) as usize).hash(&mut hasher);
                            m.vertices.len().hash(&mut hasher);
                            m.triangles.len().hash(&mut hasher);
                        }
                    }
                }
            }
        }
        let hash = hasher.finish();
        let selection_hash = selection_hasher.finish();
        if self.geometry_hash == Some(hash) {
            if self.selection_hash != Some(selection_hash) {
                let selected: std::collections::HashSet<_> = items
                    .iter()
                    .filter(|item| item.selected)
                    .map(|item| item.node_id)
                    .collect();
                for triangle in &mut self.scene.triangles {
                    triangle.selected = selected.contains(&triangle.node_id);
                }
                for segment in &mut self.scene.segments {
                    segment.selected = selected.contains(&segment.node_id);
                }
                for point in &mut self.scene.points {
                    point.selected = selected.contains(&point.node_id);
                }
                self.selection_hash = Some(selection_hash);
                self.generation = self.generation.wrapping_add(1);
                self.projected = None;
            }
            return;
        }

        let mut scene = Scene::default();
        let mut value_count = 0;
        'items: for item in items {
            for values in item.tree.branches.values() {
                for value in values {
                    value_count += 1;
                    if value_count > MAX_VALUES {
                        scene.reduced = true;
                        break 'items;
                    }
                    match value {
                        Value::Number(_) => {}
                        Value::Point(p) => {
                            if !valid(*p) {
                                scene.invalid += 1;
                                continue;
                            }
                            scene.include(*p);
                            if scene.points.len() < MAX_POINTS {
                                scene.points.push(WorldPoint {
                                    p: *p,
                                    node_id: item.node_id,
                                    selected: item.selected,
                                });
                            } else {
                                scene.reduced = true;
                            }
                        }
                        Value::Curve(curve) => {
                            add_curve(&mut scene, curve, item.node_id, item.selected);
                            scene.curve_sources.push(Arc::clone(curve));
                        }
                        Value::Mesh(mesh) => {
                            add_mesh(&mut scene, mesh, item.node_id, item.selected);
                            scene.mesh_sources.push(Arc::clone(mesh));
                        }
                    }
                }
            }
        }
        let trees: Vec<Tree> = items.iter().map(|item| item.tree.clone()).collect();
        scene.report = persistence::mesh_stats(&trees).ok();
        self.scene = scene;
        self.geometry_hash = Some(hash);
        self.selection_hash = Some(selection_hash);
        self.generation = self.generation.wrapping_add(1);
        self.projected = None;
    }

    fn projection_hash(&self, rect: Rect, dark: bool, scale: f32) -> u64 {
        let mut h = DefaultHasher::new();
        self.generation.hash(&mut h);
        self.yaw.to_bits().hash(&mut h);
        self.pitch.to_bits().hash(&mut h);
        self.span.to_bits().hash(&mut h);
        for v in self.target {
            v.to_bits().hash(&mut h);
        }
        for v in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
            v.to_bits().hash(&mut h);
        }
        self.wireframe.hash(&mut h);
        self.show_grid.hash(&mut h);
        dark.hash(&mut h);
        self.orthographic.hash(&mut h);
        self.printer.hash(&mut h);
        for v in self.custom_bed {
            v.to_bits().hash(&mut h);
        }
        self.overhang_preview.hash(&mut h);
        self.overhang_limit_deg.to_bits().hash(&mut h);
        self.accurate_depth.hash(&mut h);
        scale.to_bits().hash(&mut h);
        h.finish()
    }

    fn project(&self, camera: Camera, rect: Rect, scale: f32, dark: bool, hash: u64) -> Projection {
        let mut under = Vec::new();
        let mut over = Vec::new();
        let bed = self.bed_plan();
        let ground = bed.map_or_else(
            || self.scene.bounds.map_or(0.0, |b| b.min[2].min(0.0)),
            |plan| plan.ground,
        );
        match bed {
            Some(plan) => paint_bed(&mut under, &mut over, camera, plan, dark),
            None if self.show_grid => paint_grid(&mut under, camera, self.span, ground, dark),
            None => {}
        }
        if let Some(bounds) = self.scene.bounds {
            paint_shadow(&mut under, camera, bounds, ground, dark);
        }

        // Depth-correct path: one z-buffer texture. Wireframe keeps the painter.
        let raster_on = self.accurate_depth && !self.wireframe;
        let overhang_on = self.overhang_preview;
        let limit = self.overhang_limit_deg;

        let light = normalize(add(
            add(mul(camera.eye, 0.6), mul(camera.up, 0.9)),
            mul(camera.right, -0.65),
        ));
        let mut projected: Vec<_> = self
            .scene
            .triangles
            .iter()
            .filter_map(|triangle| {
                let p = triangle.p.map(|v| camera.project(v));
                if p.iter().any(|v| !v.x.is_finite() || !v.y.is_finite()) {
                    return None;
                }
                let bbox = Rect::from_points(&p);
                if !bbox.intersects(rect) {
                    return None;
                }
                let depth = triangle.p.iter().map(|v| camera.depth(*v)).sum::<f64>() / 3.0;
                Some((triangle, p, depth))
            })
            .collect();
        if !raster_on {
            projected.sort_unstable_by(|a, b| a.2.total_cmp(&b.2));
        }

        let mut frame = raster_on.then(|| {
            Frame::new(
                (rect.width() * scale).round().max(1.0) as usize,
                (rect.height() * scale).round().max(1.0) as usize,
            )
        });
        let mut surface = egui::Mesh::default();
        if !raster_on {
            surface.vertices.reserve(projected.len() * 3);
            surface.indices.reserve(projected.len() * 3);
        }
        let mut triangles = Vec::with_capacity(projected.len());
        let mut wires = Vec::new();
        for (triangle, p, depth) in projected {
            let kind = if overhang_on
                && !triangle.plate
                && print_info::is_overhang_face(triangle.face, limit)
            {
                SurfaceKind::Overhang
            } else if triangle.selected {
                SurfaceKind::Selected
            } else {
                SurfaceKind::Normal
            };
            let mut colors = [Color32::BLACK; 3];
            for (i, color) in colors.iter_mut().enumerate() {
                let mut normal = triangle.normals[i];
                if dot(normal, camera.eye) < 0.0 {
                    normal = mul(normal, -1.0);
                }
                let diffuse = dot(normal, light).max(0.0);
                let facing = dot(normal, camera.eye).abs().clamp(0.0, 1.0);
                let soft_specular = dot(normal, normalize(add(light, camera.eye)))
                    .max(0.0)
                    .powf(28.0)
                    * 0.12;
                let brightness = (0.42 + diffuse * 0.62 + soft_specular + (1.0 - facing) * 0.08)
                    .clamp(0.28, 1.15);
                *color = surface_color(kind, dark, brightness, self.wireframe);
            }
            match frame.as_mut() {
                Some(frame) => {
                    let vertices: [RasterVertex; 3] = std::array::from_fn(|i| RasterVertex {
                        x: (p[i].x - rect.min.x) * scale,
                        y: (p[i].y - rect.min.y) * scale,
                        key: camera.key(triangle.p[i]) as f32,
                        rgb: [
                            f32::from(colors[i].r()),
                            f32::from(colors[i].g()),
                            f32::from(colors[i].b()),
                        ],
                    });
                    frame.triangle(vertices);
                }
                None => {
                    let base = surface.vertices.len() as u32;
                    for (i, pos) in p.iter().enumerate() {
                        surface.vertices.push(egui::epaint::Vertex {
                            pos: *pos,
                            uv: egui::epaint::WHITE_UV,
                            color: colors[i],
                        });
                    }
                    surface
                        .indices
                        .extend_from_slice(&[base, base + 1, base + 2]);
                }
            }
            triangles.push(PickTriangle {
                p,
                node_id: triangle.node_id,
                depth,
            });
            if self.wireframe && wires.len() < 24_000 {
                let color = if triangle.selected {
                    Color32::from_rgba_unmultiplied(36, 137, 117, 115)
                } else if dark {
                    Color32::from_rgba_unmultiplied(175, 199, 211, 85)
                } else {
                    Color32::from_rgba_unmultiplied(73, 101, 115, 85)
                };
                for edge in [[p[0], p[1]], [p[1], p[2]], [p[2], p[0]]] {
                    wires.push(Shape::line_segment(edge, Stroke::new(0.65_f32, color)));
                }
            }
        }
        let image = frame.map(|frame| {
            let [width, height] = frame.size();
            egui::ColorImage::from_rgba_unmultiplied([width, height], &frame.into_rgba())
        });
        if !raster_on && !surface.vertices.is_empty() {
            under.push(Shape::mesh(surface));
        }
        over.extend(wires);

        let mut segments = Vec::with_capacity(self.scene.segments.len());
        for segment in &self.scene.segments {
            let a = camera.project(segment.a);
            let b = camera.project(segment.b);
            if !Rect::from_two_pos(a, b).expand(3.0).intersects(rect) {
                continue;
            }
            let color = if segment.selected {
                Color32::from_rgb(29, 156, 126)
            } else if dark {
                Color32::from_rgb(184, 211, 222)
            } else {
                Color32::from_rgb(69, 105, 123)
            };
            over.push(Shape::line_segment(
                [a, b],
                Stroke::new(if segment.selected { 2.2 } else { 1.65_f32 }, color),
            ));
            segments.push(PickSegment {
                a,
                b,
                node_id: segment.node_id,
                depth: (camera.depth(segment.a) + camera.depth(segment.b)) * 0.5,
            });
        }
        let mut points = Vec::with_capacity(self.scene.points.len());
        for point in &self.scene.points {
            let p = camera.project(point.p);
            if !rect.expand(5.0).contains(p) {
                continue;
            }
            let color = if point.selected {
                Color32::from_rgb(28, 158, 125)
            } else if dark {
                Color32::from_rgb(194, 211, 223)
            } else {
                Color32::from_rgb(61, 94, 115)
            };
            over.push(Shape::circle_filled(
                p,
                if point.selected { 4.2 } else { 3.0 },
                color,
            ));
            over.push(Shape::circle_stroke(
                p,
                if point.selected { 4.2 } else { 3.0 },
                Stroke::new(
                    1.0_f32,
                    if dark {
                        Color32::from_rgb(31, 49, 57)
                    } else {
                        Color32::WHITE
                    },
                ),
            ));
            points.push(PickPoint {
                p,
                node_id: point.node_id,
                depth: camera.depth(point.p),
            });
        }
        Projection {
            hash,
            raster: raster_on,
            image,
            under,
            over,
            triangles,
            segments,
            points,
        }
    }

    fn pick(&self, point: Pos2) -> Option<u64> {
        let projection = self.projected.as_ref()?;
        let mut closest: Option<(f64, u64)> = None;
        let mut consider = |depth: f64, node_id: u64| {
            if closest.is_none_or(|(old, _)| depth > old) {
                closest = Some((depth, node_id));
            }
        };
        for triangle in &projection.triangles {
            if in_triangle(point, triangle.p) {
                consider(triangle.depth, triangle.node_id);
            }
        }
        for segment in &projection.segments {
            if segment_distance(point, segment.a, segment.b) < 6.0 {
                consider(segment.depth, segment.node_id);
            }
        }
        for p in &projection.points {
            if p.p.distance(point) < 8.0 {
                consider(p.depth, p.node_id);
            }
        }
        closest.map(|(_, id)| id)
    }

    fn paint_axes(&self, painter: &egui::Painter, rect: Rect, camera: Camera, dark: bool) {
        if rect.width() < 250.0 || rect.height() < 160.0 {
            return;
        }
        let origin = Pos2::new(rect.max.x - 48.0, rect.max.y - 65.0);
        painter.circle_filled(
            origin,
            36.0,
            if dark {
                Color32::from_rgba_unmultiplied(25, 40, 48, 180)
            } else {
                Color32::from_rgba_unmultiplied(250, 253, 253, 175)
            },
        );
        let axes = [
            ([1.0, 0.0, 0.0], "X", Color32::from_rgb(202, 111, 103)),
            ([0.0, 1.0, 0.0], "Y", Color32::from_rgb(91, 156, 121)),
            ([0.0, 0.0, 1.0], "Z", Color32::from_rgb(96, 140, 194)),
        ];
        for (axis, label, color) in axes {
            let delta = Vec2::new(dot(axis, camera.right) as f32, -dot(axis, camera.up) as f32);
            let end = origin + delta * 22.0;
            painter.line_segment([origin, end], Stroke::new(1.8_f32, color));
            painter.circle_filled(end, 3.0, color);
            painter.text(
                origin + delta * 30.0,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.5),
                color,
            );
        }
        painter.circle_filled(
            origin,
            2.5,
            if dark {
                Color32::from_gray(180)
            } else {
                Color32::from_gray(125)
            },
        );
    }
}

fn add_curve(scene: &mut Scene, curve: &Curve, node_id: u64, selected: bool) {
    let remaining = MAX_SEGMENTS.saturating_sub(scene.segments.len());
    if remaining == 0 {
        scene.reduced = true;
        return;
    }
    let n = curve.points.len();
    if n == 0 {
        return;
    }
    let stride = n.div_ceil(remaining.max(1)).max(1);
    if stride > 1 {
        scene.reduced = true;
    }
    let mut previous = None;
    let mut first = None;
    for i in (0..n).step_by(stride).chain(std::iter::once(n - 1)) {
        let p = curve.points[i];
        if !valid(p) {
            scene.invalid += 1;
            previous = None;
            continue;
        }
        scene.include(p);
        if first.is_none() {
            first = Some(p);
        }
        if let Some(a) = previous
            && a != p
            && scene.segments.len() < MAX_SEGMENTS
        {
            scene.segments.push(WorldSegment {
                a,
                b: p,
                node_id,
                selected,
            });
        }
        previous = Some(p);
    }
    if curve.closed
        && let (Some(a), Some(b)) = (previous, first)
        && a != b
        && scene.segments.len() < MAX_SEGMENTS
    {
        scene.segments.push(WorldSegment {
            a,
            b,
            node_id,
            selected,
        });
    }
    if n == 1 && valid(curve.points[0]) && scene.points.len() < MAX_POINTS {
        scene.points.push(WorldPoint {
            p: curve.points[0],
            node_id,
            selected,
        });
    }
}

fn add_mesh(scene: &mut Scene, mesh: &Mesh, node_id: u64, selected: bool) {
    scene.total_triangles = scene.total_triangles.saturating_add(mesh.triangles.len());
    let remaining = MAX_TRIANGLES.saturating_sub(scene.triangles.len());
    if remaining == 0 {
        scene.reduced = true;
        return;
    }
    let bound_stride = mesh.vertices.len().div_ceil(MAX_BOUND_SAMPLES).max(1);
    for p in mesh.vertices.iter().step_by(bound_stride) {
        scene.include(*p);
    }
    let stride = mesh.triangles.len().div_ceil(remaining).max(1);
    if stride > 1 {
        scene.reduced = true;
    }

    // Bounded, per-vertex adjacency allows smooth faces while preserving hard
    // creases. Normals are only built when the source Arc changes.
    let mut normals: Vec<(V3, [u32; 3], [V3; 3])> = Vec::new();
    let mut adjacency = std::collections::HashMap::<u32, Vec<usize>>::new();
    for indices in mesh.triangles.iter().step_by(stride).take(remaining) {
        let Some(&a) = mesh.vertices.get(indices[0] as usize) else {
            scene.invalid += 1;
            continue;
        };
        let Some(&b) = mesh.vertices.get(indices[1] as usize) else {
            scene.invalid += 1;
            continue;
        };
        let Some(&c) = mesh.vertices.get(indices[2] as usize) else {
            scene.invalid += 1;
            continue;
        };
        if !valid(a) || !valid(b) || !valid(c) {
            scene.invalid += 1;
            continue;
        }
        scene.include(a);
        scene.include(b);
        scene.include(c);
        let normal = cross(sub(b, a), sub(c, a));
        let length = dot(normal, normal).sqrt();
        if !length.is_finite() || length < 1.0e-24 {
            scene.invalid += 1;
            continue;
        }
        let index = normals.len();
        normals.push((mul(normal, 1.0 / length), *indices, [a, b, c]));
        for vertex in indices {
            let list = adjacency.entry(*vertex).or_default();
            if list.len() < 48 {
                list.push(index);
            }
        }
    }
    // Same plate rule as `gh_nodes::overhang`: the mesh's lowest Z.
    let lowest = mesh
        .vertices
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    for (face, indices, p) in &normals {
        let plate = p.iter().all(|v| (v[2] - lowest).abs() < 1.0e-6);
        let corner_normals = std::array::from_fn(|i| {
            let mut sum = [0.0; 3];
            if let Some(neighbors) = adjacency.get(&indices[i]) {
                for index in neighbors {
                    let n = normals[*index].0;
                    if dot(n, *face) > 0.70 {
                        sum = add(sum, n);
                    }
                }
            }
            if dot(sum, sum) < 1.0e-20 {
                *face
            } else {
                normalize(sum)
            }
        });
        scene.triangles.push(WorldTriangle {
            p: *p,
            normals: corner_normals,
            face: *face,
            plate,
            node_id,
            selected,
        });
    }
}

fn paint_grid(shapes: &mut Vec<Shape>, camera: Camera, span: f64, z: f64, dark: bool) {
    let desired = (span / 13.0).max(1.0e-9);
    let power = 10.0_f64.powf(desired.log10().floor());
    let fraction = desired / power;
    let step = power
        * if fraction < 2.0 {
            1.0
        } else if fraction < 5.0 {
            2.0
        } else {
            5.0
        };
    let cx = (camera.target[0] / step).round() * step;
    let cy = (camera.target[1] / step).round() * step;
    let radius = 26.0 * step;
    for i in -26_i32..=26 {
        let offset = f64::from(i) * step;
        let alpha = ((1.0 - f64::from(i.abs()) / 34.0) * if dark { 27.0 } else { 37.0 }) as u8;
        let color = if dark {
            Color32::from_rgba_unmultiplied(166, 193, 203, alpha)
        } else {
            Color32::from_rgba_unmultiplied(102, 132, 145, alpha)
        };
        let width = if i % 5 == 0 { 0.85 } else { 0.55_f32 };
        shapes.push(Shape::line_segment(
            [
                camera.project([cx + offset, cy - radius, z]),
                camera.project([cx + offset, cy + radius, z]),
            ],
            Stroke::new(width, color),
        ));
        shapes.push(Shape::line_segment(
            [
                camera.project([cx - radius, cy + offset, z]),
                camera.project([cx + radius, cy + offset, z]),
            ],
            Stroke::new(width, color),
        ));
    }
    let axis_alpha = if dark { 66 } else { 83 };
    shapes.push(Shape::line_segment(
        [
            camera.project([cx - radius, 0.0, z]),
            camera.project([cx + radius, 0.0, z]),
        ],
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(190, 105, 96, axis_alpha),
        ),
    ));
    shapes.push(Shape::line_segment(
        [
            camera.project([0.0, cy - radius, z]),
            camera.project([0.0, cy + radius, z]),
        ],
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(85, 149, 121, axis_alpha),
        ),
    ));
}

fn paint_shadow(shapes: &mut Vec<Shape>, camera: Camera, bounds: Bounds, z: f64, dark: bool) {
    if camera.eye[2] <= 0.04 {
        return;
    }
    let center = bounds.center();
    let rx = ((bounds.max[0] - bounds.min[0]) * 0.58).max(0.2);
    let ry = ((bounds.max[1] - bounds.min[1]) * 0.58).max(0.2);
    for layer in (0..7).rev() {
        let scale = 1.0 + f64::from(layer) * 0.10;
        let points: Vec<_> = (0..48)
            .map(|i| {
                let t = f64::from(i) / 48.0 * TAU;
                camera.project([
                    center[0] + t.cos() * rx * scale,
                    center[1] + t.sin() * ry * scale,
                    z,
                ])
            })
            .collect();
        shapes.push(Shape::convex_polygon(
            points,
            Color32::from_black_alpha(if dark { 7 } else { 4 }),
            Stroke::NONE,
        ));
    }
}

/// Build plate: a tinted floor with a 10 mm grid and an outline, plus the
/// build volume's edges. Red when the model is larger than the volume.
fn paint_bed(
    under: &mut Vec<Shape>,
    over: &mut Vec<Shape>,
    camera: Camera,
    plan: BedPlan,
    dark: bool,
) {
    let [w, d, h] = plan.volume;
    let [cx, cy] = plan.center;
    let z = plan.ground;
    let (x0, x1) = (cx - w * 0.5, cx + w * 0.5);
    let (y0, y1) = (cy - d * 0.5, cy + d * 0.5);
    let accent = match (plan.fits, dark) {
        (true, true) => Color32::from_rgb(120, 205, 180),
        (true, false) => Color32::from_rgb(28, 128, 106),
        (false, _) => Color32::from_rgb(222, 58, 48),
    };
    let fill = if plan.fits {
        Color32::from_rgba_unmultiplied(28, 158, 125, if dark { 16 } else { 12 })
    } else {
        Color32::from_rgba_unmultiplied(222, 58, 48, 30)
    };
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|[x, y]| camera.project([x, y, z]));
    under.push(Shape::convex_polygon(corners.to_vec(), fill, Stroke::NONE));

    let grid = accent.gamma_multiply(0.35);
    let lines_x = ((w / BED_GRID_MM).floor() as usize).min(200);
    for i in 0..=lines_x {
        let x = x0 + i as f64 * BED_GRID_MM;
        let width: f32 = if i % 5 == 0 { 0.9 } else { 0.5 };
        segment(
            under,
            camera,
            [x, y0, z],
            [x, y1, z],
            Stroke::new(width, grid),
        );
    }
    let lines_y = ((d / BED_GRID_MM).floor() as usize).min(200);
    for i in 0..=lines_y {
        let y = y0 + i as f64 * BED_GRID_MM;
        let width: f32 = if i % 5 == 0 { 0.9 } else { 0.5 };
        segment(
            under,
            camera,
            [x0, y, z],
            [x1, y, z],
            Stroke::new(width, grid),
        );
    }
    let outline = Stroke::new(1.4_f32, accent);
    for (a, b) in [
        ([x0, y0], [x1, y0]),
        ([x1, y0], [x1, y1]),
        ([x1, y1], [x0, y1]),
        ([x0, y1], [x0, y0]),
    ] {
        segment(under, camera, [a[0], a[1], z], [b[0], b[1], z], outline);
    }

    let edge = Stroke::new(0.9_f32, accent.gamma_multiply(0.6));
    for [x, y] in [[x0, y0], [x1, y0], [x1, y1], [x0, y1]] {
        segment(over, camera, [x, y, z], [x, y, z + h], edge);
    }
    for (a, b) in [
        ([x0, y0], [x1, y0]),
        ([x1, y0], [x1, y1]),
        ([x1, y1], [x0, y1]),
        ([x0, y1], [x0, y0]),
    ] {
        segment(over, camera, [a[0], a[1], z + h], [b[0], b[1], z + h], edge);
    }
}

fn segment(shapes: &mut Vec<Shape>, camera: Camera, a: V3, b: V3, stroke: Stroke) {
    shapes.push(Shape::line_segment(
        [camera.project(a), camera.project(b)],
        stroke,
    ));
}

/// Supersampling scale for the z-buffer: the screen's pixel density, lowered
/// when the window is so large that the pixel budget would be exceeded.
fn raster_scale(rect: Rect, pixels_per_point: f32) -> f32 {
    let ppp = pixels_per_point.max(1.0);
    let pixels = rect.width() * rect.height() * ppp * ppp;
    if pixels <= MAX_RASTER_PIXELS {
        ppp
    } else {
        (ppp * (MAX_RASTER_PIXELS / pixels).sqrt()).max(0.5)
    }
}

fn mesh_state(report: &ExportReport) -> String {
    if report.is_closed() {
        "閉じたメッシュ".to_owned()
    } else {
        format!(
            "開いた箇所あり（境界の辺 {} ・ 不正な辺 {}）",
            report.open_edges,
            report.non_manifold_edges + report.inconsistent_edges
        )
    }
}

fn background(rect: Rect, dark: bool) -> Shape {
    let top = if dark {
        Color32::from_rgb(29, 40, 47)
    } else {
        Color32::from_rgb(242, 246, 246)
    };
    let bottom = if dark {
        Color32::from_rgb(19, 29, 36)
    } else {
        Color32::from_rgb(224, 233, 235)
    };
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [
        (rect.left_top(), top),
        (rect.right_top(), top),
        (rect.right_bottom(), bottom),
        (rect.left_bottom(), bottom),
    ] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    Shape::mesh(mesh)
}

fn surface_color(kind: SurfaceKind, dark: bool, brightness: f64, wireframe: bool) -> Color32 {
    let base = match kind {
        SurfaceKind::Overhang => [236.0, 138.0, 52.0],
        SurfaceKind::Selected => [110.0, 195.0, 169.0],
        SurfaceKind::Normal if dark => [157.0, 185.0, 199.0],
        SurfaceKind::Normal => [170.0, 191.0, 202.0],
    };
    let rgb = base.map(|c: f64| (c * brightness).clamp(0.0, 255.0) as u8);
    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], if wireframe { 100 } else { 255 })
}

fn in_triangle(p: Pos2, v: [Pos2; 3]) -> bool {
    let cross2 = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let area = cross2(v[1] - v[0], v[2] - v[0]);
    if area.abs() < 0.01 {
        return false;
    }
    let a = cross2(v[1] - v[0], p - v[0]);
    let b = cross2(v[2] - v[1], p - v[1]);
    let c = cross2(v[0] - v[2], p - v[2]);
    (a >= -0.01 && b >= -0.01 && c >= -0.01) || (a <= 0.01 && b <= 0.01 && c <= 0.01)
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length = ab.length_sq();
    if length < 1.0e-12 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / length).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn valid(p: V3) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE)
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1.0e-20 {
        mul(a, 1.0 / l)
    } else {
        [0.0, 0.0, 1.0]
    }
}
