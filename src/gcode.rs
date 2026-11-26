//! Direct G-code for spiral vase mode (花瓶モード).
//!
//! One closed mesh is sliced with horizontal planes. Each layer keeps its largest
//! outer contour, resampled at about 1 mm and started at the point nearest to the
//! previous layer's start. The bottom layers are flat and filled with zigzag
//! infill. Above them each layer is one continuous spiral wall: Z rises linearly
//! along the loop and no retraction is used in the body.
//!
//! Status: not verified on any printer. The start and end G-code is a generic
//! example. Bambu Lab machines should use Bambu Studio's own .gcode.3mf export,
//! which is the official path. Check the start and end sequence against the
//! printer manual, and watch the first print.

use crate::model::{Mesh, Tree, Value};
use crate::persistence;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::io::Write;
use std::path::Path;

/// Upper bound on layers, so a tiny layer height cannot produce an unbounded file.
pub const MAX_LAYERS: usize = 3_000;
/// Upper bound on the generated text.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// Free margin kept on each side of the bed for the purge line and the nozzle.
pub const EDGE_MARGIN_MM: f64 = 10.0;
const SAMPLE_SPACING_MM: f64 = 1.0;
const MIN_RING_POINTS: usize = 8;
const MIN_LOOP_AREA_MM2: f64 = 1.0;
const PURGE_Y_MM: f64 = 4.0;
const PURGE_LENGTH_MM: f64 = 120.0;
const RETRACT_MM: f64 = 0.8;
const PLA_DENSITY_G_PER_CM3: f64 = 1.24;

/// 機種のプリセット。ベッドの大きさと注意書きを決める。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    BambuA1Mini,
    BambuA1,
    FlashForge,
    Marlin,
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::BambuA1Mini,
        Preset::BambuA1,
        Preset::FlashForge,
        Preset::Marlin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::BambuA1Mini => "Bambu Lab A1 mini",
            Preset::BambuA1 => "Bambu Lab A1",
            Preset::FlashForge => "FlashForge 汎用（Adventurer / Creator）",
            Preset::Marlin => "Marlin 汎用（ベッドは手入力）",
        }
    }

    /// Build volume in mm. Check against the printer's own specification.
    /// The FlashForge value is the smallest common size, so a model is never
    /// placed outside a real bed by accident.
    pub fn bed_mm(self) -> [f64; 3] {
        match self {
            Preset::BambuA1Mini => [180.0, 180.0, 180.0],
            Preset::BambuA1 => [256.0, 256.0, 256.0],
            Preset::FlashForge => [220.0, 220.0, 220.0],
            Preset::Marlin => [220.0, 220.0, 250.0],
        }
    }

    pub fn is_bambu(self) -> bool {
        matches!(self, Preset::BambuA1Mini | Preset::BambuA1)
    }
}

/// 花瓶モードの設定（書き出し前に画面で決める値）。
#[derive(Clone, Debug, PartialEq)]
pub struct GcodeSettings {
    /// 機種（プリセット）
    pub preset: Preset,
    /// ベッドの大きさ X・Y・Z（mm）。プリセットを選ぶと自動で入ります。
    pub bed_mm: [f64; 3],
    /// ノズル温度（℃）
    pub nozzle_c: f64,
    /// ベッド温度（℃）
    pub bed_c: f64,
    /// 層の高さ（mm）。螺旋の1周で上がる高さ。
    pub layer_height_mm: f64,
    /// 線幅（mm）
    pub line_width_mm: f64,
    /// 押し出しの速さ（mm/s）
    pub speed_mm_s: f64,
    /// 移動の速さ（mm/s）
    pub travel_mm_s: f64,
    /// ファン（0〜100 %）。最初の螺旋層から回します。
    pub fan_percent: f64,
    /// 底の塗りつぶし層数（最低1）
    pub bottom_layers: u32,
    /// フィラメントの直径（mm）
    pub filament_mm: f64,
}

impl GcodeSettings {
    pub fn for_preset(preset: Preset) -> Self {
        Self {
            preset,
            bed_mm: preset.bed_mm(),
            nozzle_c: 220.0,
            bed_c: 60.0,
            layer_height_mm: 0.2,
            line_width_mm: 0.42,
            speed_mm_s: 40.0,
            travel_mm_s: 150.0,
            fan_percent: 100.0,
            bottom_layers: 3,
            filament_mm: 1.75,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.bed_mm.iter().all(|v| in_range(*v, 20.0, 500.0)) {
            return Err("ベッドの大きさは 20〜500 mm の範囲で指定してください。".into());
        }
        if !in_range(self.nozzle_c, 150.0, 300.0) {
            return Err("ノズル温度は 150〜300 ℃ の範囲で指定してください。".into());
        }
        if !in_range(self.bed_c, 0.0, 120.0) {
            return Err("ベッド温度は 0〜120 ℃ の範囲で指定してください。".into());
        }
        if !in_range(self.layer_height_mm, 0.05, 0.6) {
            return Err("層の高さは 0.05〜0.6 mm の範囲で指定してください。".into());
        }
        if !in_range(self.line_width_mm, 0.2, 1.2) {
            return Err("線幅は 0.2〜1.2 mm の範囲で指定してください。".into());
        }
        if self.layer_height_mm > self.line_width_mm {
            return Err("層の高さは線幅以下にしてください。".into());
        }
        if !in_range(self.speed_mm_s, 5.0, 300.0) {
            return Err("押し出し速度は 5〜300 mm/s の範囲で指定してください。".into());
        }
        if !in_range(self.travel_mm_s, 10.0, 500.0) {
            return Err("移動速度は 10〜500 mm/s の範囲で指定してください。".into());
        }
        if !in_range(self.fan_percent, 0.0, 100.0) {
            return Err("ファンは 0〜100 % の範囲で指定してください。".into());
        }
        if self.bottom_layers > 50 {
            return Err("底の塗りつぶし層数は 50 層までです。".into());
        }
        if !in_range(self.filament_mm, 1.0, 3.5) {
            return Err("フィラメントの直径は 1.0〜3.5 mm の範囲で指定してください。".into());
        }
        Ok(())
    }
}

impl Default for GcodeSettings {
    fn default() -> Self {
        Self::for_preset(Preset::default())
    }
}

fn in_range(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}

/// Estimates from the generated moves. Acceleration, heating waits and pauses
/// are not included.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GcodeReport {
    /// All layers: flat bottom layers plus spiral layers.
    pub layers: usize,
    pub flat_layers: usize,
    pub spiral_layers: usize,
    pub print_seconds: f64,
    /// Filament length, mm.
    pub filament_mm: f64,
    /// Filament mass for PLA at 1.24 g/cm³.
    pub filament_grams: f64,
    pub output_bytes: usize,
    /// Model size after placement, mm.
    pub size_mm: [f64; 3],
}

/// Generate the G-code text and its estimates. Nothing is written to disk.
pub fn generate(trees: &[Tree], settings: &GcodeSettings) -> Result<(String, GcodeReport), String> {
    settings.validate()?;
    let mesh = single_mesh(trees)?;
    let stats = persistence::mesh_stats(trees)?;
    if !stats.is_closed() {
        return Err(
            "閉じた形状ではないため、花瓶モードでは書き出せません。「出力」の形状を確認してください。"
                .into(),
        );
    }
    let (low, high) = bounds(&mesh.vertices)?;
    let size = [high[0] - low[0], high[1] - low[1], high[2] - low[2]];
    let bed = settings.bed_mm;
    let usable_x = bed[0] - 2.0 * EDGE_MARGIN_MM;
    let usable_y = bed[1] - 2.0 * EDGE_MARGIN_MM;
    if size[0] > usable_x || size[1] > usable_y || size[2] > bed[2] {
        return Err(format!(
            "形状（{:.1} × {:.1} × {:.1} mm）がベッドに入りません。使える範囲は X {:.0} × Y {:.0} mm（外周 {:.0} mm を除く）、高さ {:.0} mm です。",
            size[0], size[1], size[2], usable_x, usable_y, EDGE_MARGIN_MM, bed[2]
        ));
    }

    // Centre the footprint on the bed and put the bottom on Z = 0.
    let offset = [
        bed[0] / 2.0 - (low[0] + high[0]) / 2.0,
        bed[1] / 2.0 - (low[1] + high[1]) / 2.0,
        -low[2],
    ];
    let verts: Vec<[f64; 3]> = mesh
        .vertices
        .iter()
        .map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
        .collect();

    let h = settings.layer_height_mm;
    let layer_count = planned_layers(size[2], h)?;
    let flat = (settings.bottom_layers.max(1) as usize).min(layer_count);
    let filament_area = PI * (settings.filament_mm / 2.0).powi(2);
    let fan_value = (settings.fan_percent * 2.55).round();

    let mut writer = Writer::new(settings, filament_area);
    write_start(&mut writer, settings)?;

    let mut prev_start: Option<[f64; 2]> = None;
    let mut fan_started = false;
    for k in 0..layer_count {
        let plane = (k as f64 + 0.5) * h;
        let loops: Vec<Vec<[f64; 2]>> = slice_plane(&verts, &mesh.triangles, plane)
            .into_iter()
            .filter(|l| signed_area(l.as_slice()).abs() >= MIN_LOOP_AREA_MM2)
            .collect();
        let Some(outer) = largest(&loops) else {
            return Err(format!(
                "高さ {plane:.2} mm で断面が得られません。形状が閉じているか確認してください。"
            ));
        };
        let Some(mut ring) = resample_ring(outer, SAMPLE_SPACING_MM) else {
            return Err("断面の輪郭が小さすぎるため、書き出せません。".into());
        };
        if let Some(target) = prev_start {
            rotate_to_nearest(&mut ring, target);
        }

        let base = k as f64 * h;
        let top = base + h;
        if k < flat {
            // Flat bottom layer: outer wall, then zigzag infill clipped by every loop
            // of the slice (even-odd), so a cavity that starts low is not filled.
            writer.line(&format!("; 層 {} 底（塗りつぶし）", k + 1))?;
            let start = emit_ring(&mut writer, &ring, top, top)?;
            for (a, b) in infill(&loops, settings.line_width_mm) {
                writer.travel([a[0], a[1], top])?;
                writer.print([b[0], b[1], top])?;
            }
            prev_start = Some(start);
        } else {
            if !fan_started {
                writer.line(&format!("M106 S{fan_value:.0} ; ファン開始"))?;
                fan_started = true;
            }
            writer.line(&format!("; 層 {} 螺旋", k + 1))?;
            // Z rises from the layer bottom to its top along the loop.
            let start = emit_ring(&mut writer, &ring, base, top)?;
            prev_start = Some(start);
        }
    }

    let top_mm = layer_count as f64 * h;
    write_end(&mut writer, settings, top_mm)?;
    let text = writer.out;
    let report = GcodeReport {
        layers: layer_count,
        flat_layers: flat,
        spiral_layers: layer_count - flat,
        print_seconds: writer.seconds,
        filament_mm: writer.e_total,
        filament_grams: writer.e_total * filament_area * PLA_DENSITY_G_PER_CM3 / 1000.0,
        output_bytes: text.len(),
        size_mm: size,
    };
    Ok((text, report))
}

/// Generate and write atomically. The old file is kept if anything fails.
pub fn export(
    path: &Path,
    trees: &[Tree],
    settings: &GcodeSettings,
) -> Result<GcodeReport, String> {
    let (text, report) = generate(trees, settings)?;
    persistence::atomic_write(path, |file| file.write_all(text.as_bytes()))?;
    Ok(report)
}

fn single_mesh(trees: &[Tree]) -> Result<&Mesh, String> {
    let mut found: Vec<&Mesh> = Vec::new();
    for value in trees.iter().flat_map(|t| t.branches.values()).flatten() {
        if let Value::Mesh(mesh) = value {
            found.push(mesh.as_ref());
        }
    }
    match found.as_slice() {
        [mesh] => Ok(*mesh),
        [] => {
            Err("書き出せる立体がありません。「出力」ノードに閉じた立体をつないでください。".into())
        }
        _ => Err(
            "花瓶モードで書き出せるのは形状1つだけです。複数の形状は別々に書き出してください。"
                .into(),
        ),
    }
}

fn bounds(points: &[[f64; 3]]) -> Result<([f64; 3], [f64; 3]), String> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in points {
        for axis in 0..3 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    }
    if low.iter().chain(high.iter()).all(|v| v.is_finite()) {
        Ok((low, high))
    } else {
        Err("形状の座標を確認できません。".into())
    }
}

fn planned_layers(height: f64, layer: f64) -> Result<usize, String> {
    let mut count = 0usize;
    while (count as f64 + 0.5) * layer < height {
        count += 1;
        if count > MAX_LAYERS {
            return Err(format!(
                "層数が上限の {MAX_LAYERS} 層を超えます。層の高さを大きくしてください。"
            ));
        }
    }
    if count == 0 {
        Err("形状の高さが層の高さより小さすぎます。".into())
    } else {
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Slicing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Segment {
    a_key: (u32, u32),
    a: [f64; 2],
    b_key: (u32, u32),
    b: [f64; 2],
}

fn corner(verts: &[[f64; 3]], index: u32) -> Option<[f64; 3]> {
    verts.get(index as usize).copied()
}

/// Cross-section of the mesh at height `z`, as closed loops in XY.
/// A crossing point is keyed by its mesh edge, so neighbouring triangles agree
/// exactly and the loops chain without a tolerance.
fn slice_plane(verts: &[[f64; 3]], triangles: &[[u32; 3]], z: f64) -> Vec<Vec<[f64; 2]>> {
    let mut segments: Vec<Segment> = Vec::new();
    'triangles: for tri in triangles {
        let (Some(p0), Some(p1), Some(p2)) = (
            corner(verts, tri[0]),
            corner(verts, tri[1]),
            corner(verts, tri[2]),
        ) else {
            continue 'triangles;
        };
        let points = [p0, p1, p2];
        let dist = points.map(|p| p[2] - z);
        let above = dist.map(|d| d >= 0.0);
        if above[0] == above[1] && above[1] == above[2] {
            continue;
        }
        let mut crossings: Vec<((u32, u32), [f64; 2])> = Vec::with_capacity(2);
        for (a, b) in [(0usize, 1usize), (1, 2), (2, 0)] {
            if above[a] == above[b] {
                continue;
            }
            let (lo, hi) = if tri[a] < tri[b] { (a, b) } else { (b, a) };
            let t = dist[lo] / (dist[lo] - dist[hi]);
            let point = [
                points[lo][0] + (points[hi][0] - points[lo][0]) * t,
                points[lo][1] + (points[hi][1] - points[lo][1]) * t,
            ];
            crossings.push(((tri[lo], tri[hi]), point));
        }
        if let [first, second] = crossings.as_slice() {
            segments.push(Segment {
                a_key: first.0,
                a: first.1,
                b_key: second.0,
                b: second.1,
            });
        }
    }
    chain_loops(&segments)
}

fn chain_loops(segments: &[Segment]) -> Vec<Vec<[f64; 2]>> {
    let mut by_key: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (index, seg) in segments.iter().enumerate() {
        by_key.entry(seg.a_key).or_default().push(index);
        by_key.entry(seg.b_key).or_default().push(index);
    }
    let mut used = vec![false; segments.len()];
    let mut loops = Vec::new();
    for first in 0..segments.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let Some(seed) = segments.get(first) else {
            continue;
        };
        let origin = seed.a_key;
        let mut points = vec![seed.a, seed.b];
        let mut current = seed.b_key;
        let mut closed = false;
        loop {
            if current == origin {
                closed = true;
                break;
            }
            let next = by_key
                .get(&current)
                .and_then(|list| list.iter().copied().find(|&j| !used[j]));
            let Some(j) = next else {
                break;
            };
            used[j] = true;
            let Some(seg) = segments.get(j) else {
                break;
            };
            if seg.a_key == current {
                points.push(seg.b);
                current = seg.b_key;
            } else {
                points.push(seg.a);
                current = seg.a_key;
            }
        }
        // Open chains (non-manifold input) are dropped; the last point repeats the first.
        if closed {
            points.pop();
            loops.push(points);
        }
    }
    loops
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    let twice: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    twice * 0.5
}

fn largest(loops: &[Vec<[f64; 2]>]) -> Option<&[[f64; 2]]> {
    loops
        .iter()
        .max_by(|a, b| {
            signed_area(a.as_slice())
                .abs()
                .total_cmp(&signed_area(b.as_slice()).abs())
        })
        .map(|l| l.as_slice())
}

/// Resample a closed loop at equal arc length, counter-clockwise, about `spacing` apart.
fn resample_ring(points: &[[f64; 2]], spacing: f64) -> Option<Vec<[f64; 2]>> {
    let mut source: Vec<[f64; 2]> = points.to_vec();
    if signed_area(&source) < 0.0 {
        source.reverse();
    }
    let n = source.len();
    if n < 3 {
        return None;
    }
    let edges: Vec<f64> = source
        .iter()
        .zip(source.iter().cycle().skip(1))
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .collect();
    let perimeter: f64 = edges.iter().sum();
    if !(perimeter.is_finite() && perimeter > 0.0) {
        return None;
    }
    let count = ((perimeter / spacing).round() as usize).max(MIN_RING_POINTS);
    let step = perimeter / count as f64;
    let mut out = Vec::with_capacity(count);
    let mut edge = 0usize;
    let mut edge_start = 0.0f64;
    for k in 0..count {
        let target = step * k as f64;
        while edge + 1 < n && edge_start + edges.get(edge).copied().unwrap_or(0.0) <= target {
            edge_start += edges.get(edge).copied().unwrap_or(0.0);
            edge += 1;
        }
        let a = *source.get(edge)?;
        let b = *source.get((edge + 1) % n)?;
        let len = edges.get(edge).copied().unwrap_or(0.0);
        let t = if len > 0.0 {
            ((target - edge_start) / len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
    }
    Some(out)
}

/// Start the ring at the point nearest to `target` (the previous layer's start),
/// so consecutive loops join without a visible seam or a long travel.
fn rotate_to_nearest(ring: &mut [[f64; 2]], target: [f64; 2]) {
    let mut best = 0usize;
    let mut best_distance = f64::INFINITY;
    for (index, p) in ring.iter().enumerate() {
        let d = (p[0] - target[0]).powi(2) + (p[1] - target[1]).powi(2);
        if d < best_distance {
            best_distance = d;
            best = index;
        }
    }
    ring.rotate_left(best);
}

/// Zigzag scan lines inside all loops (even-odd rule), inset half a line width from the walls.
fn infill(loops: &[Vec<[f64; 2]>], spacing: f64) -> Vec<([f64; 2], [f64; 2])> {
    let mut segments = Vec::new();
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for lp in loops {
        for p in lp {
            low = low.min(p[1]);
            high = high.max(p[1]);
        }
    }
    if !(low.is_finite() && high.is_finite()) || high - low <= spacing {
        return segments;
    }
    let inset = spacing / 2.0;
    let mut y = low + inset;
    let mut row = 0usize;
    while y <= high - inset {
        let mut crossings: Vec<f64> = Vec::new();
        for lp in loops {
            for (a, b) in lp.iter().zip(lp.iter().cycle().skip(1)) {
                if (a[1] <= y && y < b[1]) || (b[1] <= y && y < a[1]) {
                    crossings.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        let mut spans: Vec<(f64, f64)> = crossings
            .chunks_exact(2)
            .map(|c| (c[0] + inset, c[1] - inset))
            .filter(|(a, b)| b > a)
            .collect();
        if row % 2 == 1 {
            spans.reverse();
            for (a, b) in spans {
                segments.push(([b, y], [a, y]));
            }
        } else {
            for (a, b) in spans {
                segments.push(([a, y], [b, y]));
            }
        }
        y += spacing;
        row += 1;
    }
    segments
}

// ---------------------------------------------------------------------------
// G-code writing
// ---------------------------------------------------------------------------

struct Writer {
    out: String,
    /// Last written position. NaN until the first move.
    pos: [f64; 3],
    /// Absolute filament position, mm (M82).
    e_total: f64,
    /// Filament length per mm of wall: line width × layer height ÷ filament area.
    e_per_mm: f64,
    print_mm_s: f64,
    travel_mm_s: f64,
    last_feed_mm_min: f64,
    seconds: f64,
}

impl Writer {
    fn new(settings: &GcodeSettings, filament_area: f64) -> Self {
        Self {
            out: String::new(),
            pos: [f64::NAN; 3],
            e_total: 0.0,
            e_per_mm: settings.line_width_mm * settings.layer_height_mm / filament_area,
            print_mm_s: settings.speed_mm_s,
            travel_mm_s: settings.travel_mm_s,
            last_feed_mm_min: 0.0,
            seconds: 0.0,
        }
    }

    fn line(&mut self, text: &str) -> Result<(), String> {
        self.out.push_str(text);
        self.out.push('\n');
        if self.out.len() > MAX_OUTPUT_BYTES {
            return Err(
                "G-code が上限の 64 MiB を超えました。層の高さを大きくするか、形状を小さくしてください。"
                    .into(),
            );
        }
        Ok(())
    }

    fn travel(&mut self, to: [f64; 3]) -> Result<(), String> {
        self.move_to(to, false)
    }

    fn print(&mut self, to: [f64; 3]) -> Result<(), String> {
        self.move_to(to, true)
    }

    fn move_to(&mut self, to: [f64; 3], extrude: bool) -> Result<(), String> {
        let known = self.pos.iter().all(|v| v.is_finite());
        if extrude && !known {
            return Err("内部エラー: 押し出しの開始位置が不明です。".into());
        }
        let (dx, dy, dz) = if known {
            (
                to[0] - self.pos[0],
                to[1] - self.pos[1],
                to[2] - self.pos[2],
            )
        } else {
            (0.0, 0.0, 0.0)
        };
        let spatial = dx * dx + dy * dy + dz * dz;
        if known && spatial < 1e-12 {
            return Ok(());
        }
        let speed = if extrude {
            self.print_mm_s
        } else {
            self.travel_mm_s
        };
        let mut text = String::from(if extrude { "G1" } else { "G0" });
        text.push_str(&format!(" X{:.3} Y{:.3} Z{:.4}", to[0], to[1], to[2]));
        if extrude {
            self.e_total += (dx * dx + dy * dy).sqrt() * self.e_per_mm;
            text.push_str(&format!(" E{:.5}", self.e_total));
        }
        let feed = speed * 60.0;
        if feed != self.last_feed_mm_min {
            self.last_feed_mm_min = feed;
            text.push_str(&format!(" F{feed:.0}"));
        }
        if known {
            self.seconds += spatial.sqrt() / speed;
        }
        self.pos = to;
        self.line(&text)
    }
}

/// One closed ring at constant or rising Z. Returns the start point.
/// `z0` is the Z at the start of the loop, `z1` the Z at its end.
fn emit_ring(w: &mut Writer, ring: &[[f64; 2]], z0: f64, z1: f64) -> Result<[f64; 2], String> {
    let n = ring.len();
    let Some(&start) = ring.first() else {
        return Err("内部エラー: 空の輪郭です。".into());
    };
    w.travel([start[0], start[1], z0])?;
    for i in 1..=n {
        let p = ring.get(i % n).copied().unwrap_or(start);
        let z = z0 + (z1 - z0) * i as f64 / n as f64;
        w.print([p[0], p[1], z])?;
    }
    Ok(start)
}

fn write_start(w: &mut Writer, s: &GcodeSettings) -> Result<(), String> {
    let h = s.layer_height_mm;
    w.line("; NODES 花瓶モード G-code（未検証）")?;
    w.line(&format!("; 機種: {}", s.preset.label()))?;
    w.line("; 開始・終了の G-code は一般的な例です。機種の説明書と照らして確認してください。")?;
    if s.preset.is_bambu() {
        w.line("; Bambu Lab 機種は Bambu Studio の .gcode.3mf 書き出しが公式の方法です。この生の G-code は動作未確認です。")?;
    }
    w.line(&format!("M140 S{:.0} ; ベッドを加熱", s.bed_c))?;
    w.line(&format!("M104 S{:.0} ; ノズルを加熱", s.nozzle_c))?;
    w.line("G21 ; 単位は mm")?;
    w.line("G90 ; 座標は絶対値")?;
    w.line("M82 ; 押し出し量は絶対値")?;
    w.line("M107 ; ファン停止")?;
    w.line("G28 ; 全軸をホーム")?;
    w.line(&format!("M190 S{:.0} ; ベッドの温度を待つ", s.bed_c))?;
    w.line(&format!("M109 S{:.0} ; ノズルの温度を待つ", s.nozzle_c))?;
    w.line("G92 E0 ; 押し出し量を 0 に")?;
    // Purge line on the front edge, outside the model's margin.
    let x_end = (EDGE_MARGIN_MM + PURGE_LENGTH_MM).min(s.bed_mm[0] - EDGE_MARGIN_MM);
    w.line("; 捨て打ち線（造形物の外、手前側）")?;
    w.travel([EDGE_MARGIN_MM, PURGE_Y_MM, h])?;
    w.print([x_end, PURGE_Y_MM, h])?;
    w.line(";BODY-START")
}

fn write_end(w: &mut Writer, s: &GcodeSettings, top_mm: f64) -> Result<(), String> {
    w.line(";BODY-END")?;
    let retract_to = w.e_total - RETRACT_MM;
    let lift = (top_mm + 10.0).min(s.bed_mm[2]);
    let travel_feed = s.travel_mm_s * 60.0;
    w.line(&format!("G1 E{retract_to:.5} F2400 ; 引き戻し"))?;
    w.line(&format!("G0 Z{lift:.3} F{travel_feed:.0} ; 上へ逃がす"))?;
    w.line(&format!(
        "G0 X0.000 Y{:.3} F{travel_feed:.0} ; 奥へ移動",
        s.bed_mm[1]
    ))?;
    w.line("M104 S0 ; ノズル加熱を停止")?;
    w.line("M140 S0 ; ベッド加熱を停止")?;
    w.line("M107 ; ファン停止")?;
    w.line("M84 ; モーター停止")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_valid() {
        assert!(GcodeSettings::default().validate().is_ok());
    }

    #[test]
    fn layer_height_above_line_width_is_rejected() {
        let s = GcodeSettings {
            layer_height_mm: 0.5,
            line_width_mm: 0.4,
            ..GcodeSettings::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn resample_keeps_perimeter_and_ccw() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let ring = resample_ring(&square, 1.0).unwrap_or_default();
        assert_eq!(ring.len(), 40);
        assert!(signed_area(&ring) > 99.0);
    }
}
