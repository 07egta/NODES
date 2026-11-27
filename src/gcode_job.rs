//! Pure data model, planning, checking and writing for the G-code node set.
//!
//! This file does not use the rest of the crate, so its maths and writer can be tested
//! on their own. `gcode_nodes.rs` adapts it to graph nodes and uses `gcode.rs` for the
//! slicing and infill that already exist for the vase-mode export.
//!
//! Design: one value, `GcodeJob`, flows through the Gコード nodes. Settings nodes change
//! its `Profile`; slicing fills `slices`; path nodes fill `paths`; the writer walks the
//! paths once and produces moves (print, travel, retract, hop, dwell). The estimate, the
//! safety check and the text writer all consume that same walk.
//!
//! Status: not verified on any printer. Start and end sequences are generic examples.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::fmt::Write as _;

/// Upper bound on stored path points (and slice points), so a graph cannot exhaust memory.
pub const MAX_POINTS: usize = 800_000;
/// Upper bound on the number of paths.
pub const MAX_PATHS: usize = 100_000;
/// Upper bound on the generated text.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// Upper bound on a start or end script typed by the user.
pub const MAX_SCRIPT_BYTES: usize = 20_000;
const STOPPED: &str = "計算を停止しました。";
const CANCEL_EVERY: usize = 1024;
const PLA_DENSITY_G_PER_CM3: f64 = 1.24;

// ---------------------------------------------------------------------------
// Printer
// ---------------------------------------------------------------------------

/// Where machine coordinates (0, 0) is relative to the design coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    /// Design (0, 0) is the bed's front-left corner.
    #[default]
    Corner,
    /// Design (0, 0) is the bed centre, like G-coordinator (machine 90, 90 on a 180 mm bed).
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    BambuA1MiniCorner,
    BambuA1MiniCenter,
    FlashForgeCreator5,
    TronxyMoore1Clay,
    GenericMarlin,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::BambuA1MiniCorner,
        Preset::BambuA1MiniCenter,
        Preset::FlashForgeCreator5,
        Preset::TronxyMoore1Clay,
        Preset::GenericMarlin,
    ];

    pub fn from_index(index: i64) -> Option<Self> {
        usize::try_from(index)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::BambuA1MiniCorner => "Bambu Lab A1 mini（原点は角）",
            Preset::BambuA1MiniCenter => "Bambu Lab A1 mini（原点は中心 90,90）",
            Preset::FlashForgeCreator5 => "FlashForge Creator 5",
            Preset::TronxyMoore1Clay => "TRONXY Moore 1（粘土）",
            Preset::GenericMarlin => "Marlin 汎用",
        }
    }

    /// Build volume in mm. These are starting values: check them against the printer's
    /// own specification. The FlashForge value is the smallest common size.
    pub fn bed(self) -> [f64; 3] {
        match self {
            Preset::BambuA1MiniCorner | Preset::BambuA1MiniCenter => [180.0, 180.0, 180.0],
            Preset::FlashForgeCreator5 => [220.0, 220.0, 220.0],
            Preset::TronxyMoore1Clay => [180.0, 180.0, 180.0],
            Preset::GenericMarlin => [220.0, 220.0, 250.0],
        }
    }

    pub fn origin(self) -> Origin {
        match self {
            Preset::BambuA1MiniCenter => Origin::Center,
            _ => Origin::Corner,
        }
    }

    pub fn is_clay(self) -> bool {
        matches!(self, Preset::TronxyMoore1Clay)
    }

    pub fn is_bambu(self) -> bool {
        matches!(self, Preset::BambuA1MiniCorner | Preset::BambuA1MiniCenter)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Printer {
    pub preset: Preset,
    /// Bed size X, Y and the usable height Z, mm.
    pub bed: [f64; 3],
    pub origin: Origin,
    /// False for clay printers: no heater, fan or purge line is written.
    pub heated: bool,
    /// Filament diameter, or the cartridge diameter of a clay printer, mm.
    pub filament_d: f64,
    pub nozzle_d: f64,
}

impl Printer {
    pub fn from_preset(preset: Preset) -> Self {
        let clay = preset.is_clay();
        Self {
            preset,
            bed: preset.bed(),
            origin: preset.origin(),
            heated: !clay,
            filament_d: if clay { 30.0 } else { 1.75 },
            nozzle_d: if clay { 2.0 } else { 0.4 },
        }
    }

    /// Machine X, Y = design X, Y plus this offset.
    pub fn offset(&self) -> [f64; 2] {
        match self.origin {
            Origin::Corner => [0.0, 0.0],
            Origin::Center => [self.bed[0] / 2.0, self.bed[1] / 2.0],
        }
    }

    /// Where the middle of the bed is, in design coordinates.
    pub fn bed_center_design(&self) -> [f64; 2] {
        let o = self.offset();
        [self.bed[0] / 2.0 - o[0], self.bed[1] / 2.0 - o[1]]
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.bed.iter().all(|v| in_range(*v, 20.0, 1000.0)) {
            return Err("ベッドの大きさは 20〜1000 mm の範囲にしてください。".into());
        }
        if !in_range(self.filament_d, 1.0, 100.0) {
            return Err("フィラメント（押出筒）の直径は 1〜100 mm にしてください。".into());
        }
        if !in_range(self.nozzle_d, 0.1, 20.0) {
            return Err("ノズル径は 0.1〜20 mm にしてください。".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile (settings changed by the setting nodes)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub layer_h: f64,
    pub width: f64,
    /// Flow multiplier in percent.
    pub flow_pct: f64,
    /// True: M83 (relative E). False: M82 (absolute E).
    pub relative_e: bool,
    pub speed_outer: f64,
    pub speed_inner: f64,
    pub speed_travel: f64,
    pub speed_first: f64,
    pub first_layers: u32,
    pub nozzle_c: f64,
    pub bed_c: f64,
    pub fan_pct: f64,
    pub fan_from_layer: u32,
    pub retract_mm: f64,
    pub retract_speed: f64,
    pub retract_min_travel: f64,
    pub hop_mm: f64,
    pub hop_min_travel: f64,
    /// Clay: dwell after a path so the pressure can drop, ms.
    pub pressure_dwell_ms: u32,
    /// Clay: dwell before a path so the clay is flowing, ms.
    pub preflow_dwell_ms: u32,
    /// Safety limit for the volumetric flow, mm³/s. 0 = not checked.
    pub max_flow_mm3_s: f64,
}

impl Profile {
    pub fn for_printer(printer: &Printer) -> Self {
        if printer.preset.is_clay() {
            Self {
                layer_h: 1.0,
                width: 2.4,
                flow_pct: 100.0,
                relative_e: true,
                speed_outer: 20.0,
                speed_inner: 20.0,
                speed_travel: 60.0,
                speed_first: 15.0,
                first_layers: 1,
                nozzle_c: 0.0,
                bed_c: 0.0,
                fan_pct: 0.0,
                fan_from_layer: 0,
                retract_mm: 2.0,
                retract_speed: 10.0,
                retract_min_travel: 5.0,
                hop_mm: 3.0,
                hop_min_travel: 3.0,
                pressure_dwell_ms: 0,
                preflow_dwell_ms: 0,
                max_flow_mm3_s: 0.0,
            }
        } else {
            Self {
                layer_h: 0.2,
                width: 0.42,
                flow_pct: 100.0,
                relative_e: false,
                speed_outer: 40.0,
                speed_inner: 60.0,
                speed_travel: 150.0,
                speed_first: 20.0,
                first_layers: 1,
                nozzle_c: 220.0,
                bed_c: 60.0,
                fan_pct: 100.0,
                fan_from_layer: 2,
                retract_mm: 0.8,
                retract_speed: 30.0,
                retract_min_travel: 2.0,
                hop_mm: 0.0,
                hop_min_travel: 2.0,
                pressure_dwell_ms: 0,
                preflow_dwell_ms: 0,
                max_flow_mm3_s: 15.0,
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !in_range(self.layer_h, 0.02, 10.0) {
            return Err("層の高さは 0.02〜10 mm にしてください。".into());
        }
        if !in_range(self.width, 0.1, 20.0) {
            return Err("線幅は 0.1〜20 mm にしてください。".into());
        }
        if !in_range(self.flow_pct, 1.0, 400.0) {
            return Err("流量は 1〜400 % にしてください。".into());
        }
        for (v, name) in [
            (self.speed_outer, "外壁の速度"),
            (self.speed_inner, "内側の速度"),
            (self.speed_first, "1層目の速度"),
        ] {
            if !in_range(v, 1.0, 500.0) {
                return Err(format!("{name}は 1〜500 mm/s にしてください。"));
            }
        }
        if !in_range(self.speed_travel, 5.0, 1000.0) {
            return Err("移動の速度は 5〜1000 mm/s にしてください。".into());
        }
        if !in_range(self.nozzle_c, 0.0, 400.0) || !in_range(self.bed_c, 0.0, 200.0) {
            return Err("温度が範囲外です（ノズル 0〜400 ℃、ベッド 0〜200 ℃）。".into());
        }
        if !in_range(self.fan_pct, 0.0, 100.0) {
            return Err("ファンは 0〜100 % にしてください。".into());
        }
        if !in_range(self.retract_mm, 0.0, 30.0)
            || !in_range(self.retract_speed, 1.0, 200.0)
            || !in_range(self.retract_min_travel, 0.0, 500.0)
        {
            return Err("リトラクトの値が範囲外です。".into());
        }
        if !in_range(self.hop_mm, 0.0, 50.0) || !in_range(self.hop_min_travel, 0.0, 500.0) {
            return Err("Zホップの値が範囲外です。".into());
        }
        if !self.max_flow_mm3_s.is_finite() || self.max_flow_mm3_s < 0.0 {
            return Err("最大押出流量は有限の非負値にしてください。".into());
        }
        if self.pressure_dwell_ms > 60_000 || self.preflow_dwell_ms > 60_000 {
            return Err("待ち時間は 60000 ms 以下にしてください。".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    /// Outermost wall of a loop.
    Outer,
    /// Further walls.
    Inner,
    Infill,
    /// One continuous spiral (vase mode).
    Spiral,
}

impl PathKind {
    pub fn label(self) -> &'static str {
        match self {
            PathKind::Outer => "外壁",
            PathKind::Inner => "内壁",
            PathKind::Infill => "充填",
            PathKind::Spiral => "らせん",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathSeg {
    pub pts: Vec<[f64; 3]>,
    pub kind: PathKind,
    pub layer: u32,
    /// Order inside a layer: lower ranks print first. Sorting never crosses ranks.
    pub rank: u8,
    /// A closed path returns to its first point.
    pub closed: bool,
}

impl PathSeg {
    pub fn edge_count(&self) -> usize {
        match self.pts.len() {
            0 | 1 => 0,
            n if self.closed && n > 2 => n,
            n => n - 1,
        }
    }

    pub fn length(&self) -> f64 {
        let mut total = 0.0;
        for i in 0..self.edge_count() {
            if let (Some(a), Some(b)) = (self.pts.get(i), self.pts.get((i + 1) % self.pts.len())) {
                total += dist3(*a, *b);
            }
        }
        total
    }
}

/// One slice of a mesh: closed loops in XY at the top height of the layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    pub z_top: f64,
    pub loops: Vec<Vec<[f64; 2]>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScriptMode {
    /// Standard sequence of the printer.
    #[default]
    Auto,
    /// Standard sequence followed by the user's text.
    AutoPlus,
    /// Only the user's text.
    Custom,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub mode: ScriptMode,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorBy {
    #[default]
    Speed,
    Extrusion,
    Layer,
    Kind,
}

impl ColorBy {
    pub fn from_index(i: i64) -> Option<Self> {
        match i {
            0 => Some(Self::Speed),
            1 => Some(Self::Extrusion),
            2 => Some(Self::Layer),
            3 => Some(Self::Kind),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preview {
    pub color_by: ColorBy,
    pub show_travel: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub error: bool,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GcodeJob {
    pub printer: Printer,
    pub profile: Profile,
    pub slices: Vec<Slice>,
    pub paths: Vec<PathSeg>,
    pub start: Script,
    pub end: Script,
    pub preview: Option<Preview>,
    /// Result of the last 安全チェック node.
    pub notes: Vec<Issue>,
    /// Layer indices (0 = first layer) where the writer emits M600 (filament change).
    /// Sorted, no duplicates, never contains 0.
    pub color_changes: Vec<u32>,
}

/// Most filament changes one job may hold.
pub const MAX_COLOR_CHANGES: usize = 256;

impl GcodeJob {
    pub fn new(printer: Printer) -> Self {
        let profile = Profile::for_printer(&printer);
        Self {
            printer,
            profile,
            slices: Vec::new(),
            paths: Vec::new(),
            start: Script::default(),
            end: Script::default(),
            preview: None,
            notes: Vec::new(),
            color_changes: Vec::new(),
        }
    }

    /// Adds filament changes at the start of the given layers. Layer 0 is ignored
    /// (a change before anything is printed is just a loading step).
    pub fn add_color_changes(&mut self, layers: &[u32]) -> Result<(), String> {
        self.color_changes
            .extend(layers.iter().copied().filter(|&l| l > 0));
        self.color_changes.sort_unstable();
        self.color_changes.dedup();
        if self.color_changes.len() > MAX_COLOR_CHANGES {
            return Err(format!(
                "色替えは {MAX_COLOR_CHANGES} 回までです。回数を減らしてください。"
            ));
        }
        Ok(())
    }

    pub fn point_count(&self) -> usize {
        let slice_points: usize = self
            .slices
            .iter()
            .map(|s| s.loops.iter().map(Vec::len).sum::<usize>())
            .sum();
        self.paths
            .iter()
            .map(|p| p.pts.len())
            .sum::<usize>()
            .saturating_add(slice_points)
    }

    pub fn weight(&self) -> usize {
        self.point_count()
            .saturating_add(self.paths.len())
            .saturating_add(1)
    }

    pub fn check_limits(&self) -> Result<(), String> {
        if self.paths.len() > MAX_PATHS {
            return Err(format!(
                "経路の数が上限の {MAX_PATHS} 本を超えました。分割の長さを大きくするか、形状を小さくしてください。"
            ));
        }
        if self.point_count() > MAX_POINTS {
            return Err(format!(
                "経路の点が上限の {MAX_POINTS} 点を超えました。層の高さを大きくするか、サンプル間隔を広げてください。"
            ));
        }
        Ok(())
    }

    pub fn layer_count(&self) -> u32 {
        self.paths
            .iter()
            .map(|p| p.layer)
            .max()
            .map_or(0, |m| m.saturating_add(1))
    }

    fn feed_for(&self, kind: PathKind, layer: u32) -> f64 {
        let p = &self.profile;
        let base = match kind {
            PathKind::Outer | PathKind::Spiral => p.speed_outer,
            PathKind::Inner | PathKind::Infill => p.speed_inner,
        };
        if layer < p.first_layers {
            base.min(p.speed_first)
        } else {
            base
        }
    }

    fn e_per_mm(&self) -> f64 {
        extrusion_e(
            1.0,
            self.profile.width,
            self.profile.layer_h,
            self.printer.filament_d,
        ) * self.profile.flow_pct
            / 100.0
    }

    /// Visit every move of the job in print order. This is the one place that decides
    /// where retractions, hops and dwells go.
    pub fn walk(
        &self,
        cancel: &dyn Fn() -> bool,
        visit: &mut dyn FnMut(&Move) -> Result<(), String>,
    ) -> Result<(), String> {
        let p = &self.profile;
        let e_mm = self.e_per_mm();
        let mut cur: Option<[f64; 3]> = None;
        let mut retracted = false;
        let mut last_layer: Option<u32> = None;
        let mut steps = 0usize;
        for path in &self.paths {
            let Some(&first) = path.pts.first() else {
                continue;
            };
            if path.pts.len() < 2 {
                continue;
            }
            steps += 1;
            if steps.is_multiple_of(CANCEL_EVERY) && cancel() {
                return Err(STOPPED.into());
            }
            let kind = path.kind;
            let layer = path.layer;
            let mk = |op: Op, from: [f64; 3], to: [f64; 3], feed: f64, e: f64, hop: bool| Move {
                op,
                from,
                to,
                feed,
                e,
                kind,
                layer,
                hop,
            };
            if last_layer != Some(layer) {
                visit(&mk(Op::Layer, first, first, 0.0, 0.0, false))?;
                last_layer = Some(layer);
            }
            match cur {
                None => {
                    let above = [first[0], first[1], first[2] + 2.0];
                    visit(&mk(Op::Travel, above, above, p.speed_travel, 0.0, true))?;
                    visit(&mk(Op::Travel, above, first, p.speed_travel, 0.0, false))?;
                }
                Some(c) => {
                    let d = dist3(c, first);
                    if d > 1e-6 {
                        let long = d >= p.retract_min_travel;
                        if p.retract_mm > 0.0 && long && !retracted {
                            visit(&mk(
                                Op::Retract,
                                c,
                                c,
                                p.retract_speed,
                                -p.retract_mm,
                                false,
                            ))?;
                            retracted = true;
                        }
                        if p.hop_mm > 0.0 && d >= p.hop_min_travel {
                            let top = c[2].max(first[2]) + p.hop_mm;
                            let up = [c[0], c[1], top];
                            let over = [first[0], first[1], top];
                            visit(&mk(Op::Travel, c, up, p.speed_travel, 0.0, true))?;
                            visit(&mk(Op::Travel, up, over, p.speed_travel, 0.0, true))?;
                            visit(&mk(Op::Travel, over, first, p.speed_travel, 0.0, true))?;
                        } else {
                            visit(&mk(Op::Travel, c, first, p.speed_travel, 0.0, false))?;
                        }
                    }
                }
            }
            if retracted {
                visit(&mk(
                    Op::Unretract,
                    first,
                    first,
                    p.retract_speed,
                    p.retract_mm,
                    false,
                ))?;
                retracted = false;
            }
            if p.preflow_dwell_ms > 0 {
                visit(&mk(
                    Op::Dwell(p.preflow_dwell_ms),
                    first,
                    first,
                    0.0,
                    0.0,
                    false,
                ))?;
            }
            let feed = self.feed_for(kind, layer);
            let n = path.pts.len();
            let mut at = first;
            for i in 0..path.edge_count() {
                let Some(&to) = path.pts.get((i + 1) % n) else {
                    break;
                };
                let len = dist3(at, to);
                if len > 1e-9 {
                    visit(&mk(Op::Print, at, to, feed, len * e_mm, false))?;
                }
                at = to;
                steps += 1;
                if steps.is_multiple_of(CANCEL_EVERY) && cancel() {
                    return Err(STOPPED.into());
                }
            }
            cur = Some(at);
            if p.pressure_dwell_ms > 0 {
                visit(&mk(Op::Dwell(p.pressure_dwell_ms), at, at, 0.0, 0.0, false))?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Print,
    Travel,
    Retract,
    Unretract,
    Dwell(u32),
    /// Start of a new layer (a comment, and the fan switch).
    Layer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub op: Op,
    pub from: [f64; 3],
    pub to: [f64; 3],
    /// mm/s
    pub feed: f64,
    /// Filament length, mm. Negative for a retraction.
    pub e: f64,
    pub kind: PathKind,
    pub layer: u32,
    /// A travel at lifted height (Z hop).
    pub hop: bool,
}

// ---------------------------------------------------------------------------
// Extrusion
// ---------------------------------------------------------------------------

/// Filament length for a line of `len` mm: E = len·w·h / (π(d/2)²).
pub fn extrusion_e(len: f64, width: f64, height: f64, filament_d: f64) -> f64 {
    let area = PI * (filament_d / 2.0).powi(2);
    if area > 0.0 {
        len * width * height / area
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Estimate
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub seconds: f64,
    pub filament_mm: f64,
    /// Material volume, cm³.
    pub volume_cm3: f64,
    /// Mass for PLA at 1.24 g/cm³; meaningless for clay.
    pub grams: f64,
    pub print_mm: f64,
    pub travel_mm: f64,
    pub layers: u32,
    pub retractions: usize,
    /// Number of M600 filament changes. The time they take (heating, manual swap) is not counted.
    pub color_changes: usize,
    pub low: [f64; 3],
    pub high: [f64; 3],
}

impl GcodeJob {
    pub fn stats(&self, cancel: &dyn Fn() -> bool) -> Result<Stats, String> {
        let mut s = Stats {
            low: [f64::INFINITY; 3],
            high: [f64::NEG_INFINITY; 3],
            layers: self.layer_count(),
            color_changes: self.color_changes.len(),
            ..Stats::default()
        };
        self.walk(cancel, &mut |m| {
            match m.op {
                Op::Print => {
                    let len = dist3(m.from, m.to);
                    s.print_mm += len;
                    s.filament_mm += m.e;
                    if m.feed > 0.0 {
                        s.seconds += len / m.feed;
                    }
                    for axis in 0..3 {
                        s.low[axis] = s.low[axis].min(m.to[axis]).min(m.from[axis]);
                        s.high[axis] = s.high[axis].max(m.to[axis]).max(m.from[axis]);
                    }
                }
                Op::Travel => {
                    let len = dist3(m.from, m.to);
                    s.travel_mm += len;
                    if m.feed > 0.0 {
                        s.seconds += len / m.feed;
                    }
                }
                Op::Retract | Op::Unretract => {
                    s.retractions += usize::from(m.op == Op::Retract);
                    if m.feed > 0.0 {
                        s.seconds += m.e.abs() / m.feed;
                    }
                }
                Op::Dwell(ms) => s.seconds += f64::from(ms) / 1000.0,
                Op::Layer => {}
            }
            Ok(())
        })?;
        let area = PI * (self.printer.filament_d / 2.0).powi(2);
        s.volume_cm3 = s.filament_mm * area / 1000.0;
        s.grams = s.volume_cm3 * PLA_DENSITY_G_PER_CM3;
        if !s.low.iter().chain(s.high.iter()).all(|v| v.is_finite()) {
            s.low = [0.0; 3];
            s.high = [0.0; 3];
        }
        Ok(s)
    }
}

/// Evenly spaced layers for a colour gradient: `count` changes, the first at `from`
/// and the last at `to` (0-based layer indices). One change goes at `from`.
pub fn gradient_layers(from: u32, to: u32, count: u32) -> Result<Vec<u32>, String> {
    if count == 0 || count as usize > MAX_COLOR_CHANGES {
        return Err(format!(
            "色替えの回数は 1〜{MAX_COLOR_CHANGES} にしてください。"
        ));
    }
    if to < from {
        return Err("終わりの層は、はじめの層以上にしてください。".into());
    }
    if count == 1 {
        return Ok(vec![from]);
    }
    let span = f64::from(to - from);
    let steps = f64::from(count - 1);
    let mut out: Vec<u32> = (0..count)
        .map(|k| from + (span * f64::from(k) / steps).round() as u32)
        .collect();
    out.dedup();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Safety check
// ---------------------------------------------------------------------------

const GRID_CELL_MM: f64 = 2.0;
const MAX_GRID_CELLS: usize = 2_000_000;

fn cell(x: f64, y: f64) -> (i32, i32) {
    (
        (x / GRID_CELL_MM).floor().clamp(-1e9, 1e9) as i32,
        (y / GRID_CELL_MM).floor().clamp(-1e9, 1e9) as i32,
    )
}

impl GcodeJob {
    /// Look for things that can damage a print or the printer. Messages are Japanese.
    pub fn check(&self, cancel: &dyn Fn() -> bool) -> Result<Vec<Issue>, String> {
        let mut out: Vec<Issue> = Vec::new();
        let mut push = |error: bool, text: String| out.push(Issue { error, text });
        let pr = &self.printer;
        let pf = &self.profile;
        if cancel() {
            return Err(STOPPED.into());
        }
        self.check_limits()?;
        if self.paths.is_empty() {
            push(
                true,
                "経路がありません。輪切り → 等高線パス（またはらせん連続Z）をつないでください。"
                    .into(),
            );
            return Ok(out);
        }
        if let Err(e) = pr.validate().and_then(|()| pf.validate()) {
            push(true, e);
            return Ok(out);
        }

        if !self.color_changes.is_empty() {
            if !pr.heated {
                push(
                    false,
                    "この機種は加熱しないので、色替え（M600）は書き出されません。".into(),
                );
            } else if self.color_changes.iter().any(|&l| l >= self.layer_count()) {
                push(
                    false,
                    format!(
                        "色替えの層が全 {} 層より後にあります。その色替えは書き出されません。",
                        self.layer_count()
                    ),
                );
            }
        }

        // Non-finite or absurd coordinates.
        let mut bad = 0usize;
        let mut first_bad: Option<(u32, usize)> = None;
        let off = pr.offset();
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for (pi, path) in self.paths.iter().enumerate() {
            if pi % 256 == 0 && cancel() {
                return Err(STOPPED.into());
            }
            for p in &path.pts {
                if !p.iter().all(|v| v.is_finite() && v.abs() < 1e7) {
                    bad += 1;
                    first_bad.get_or_insert((path.layer, pi));
                    continue;
                }
                let m = [p[0] + off[0], p[1] + off[1], p[2]];
                for axis in 0..3 {
                    low[axis] = low[axis].min(m[axis]);
                    high[axis] = high[axis].max(m[axis]);
                }
            }
        }
        if first_bad.is_none() && !low[0].is_finite() {
            push(true, "経路に点がありません。".into());
            return Ok(out);
        }
        if let Some((layer, index)) = first_bad {
            push(
                true,
                format!(
                    "数値でない座標または極端に大きい座標が {bad} 点あります（層 {} の経路 {} が最初）。",
                    u64::from(layer) + 1,
                    index + 1
                ),
            );
            return Ok(out);
        }

        // Outside the bed (machine coordinates).
        let axes = ["X", "Y", "Z"];
        for axis in 0..3 {
            let (lo, hi) = (low[axis], high[axis]);
            let size = pr.bed[axis];
            if lo < -0.001 || hi > size + 0.001 {
                push(
                    true,
                    format!(
                        "{} が {:.1}〜{:.1} mm で、ベッド（0〜{:.0} mm）の外に出ます。配置を見直してください。",
                        axes[axis], lo, hi, size
                    ),
                );
            }
        }

        // Over-extrusion and flow.
        if pf.width > pr.nozzle_d * 2.5 {
            push(
                true,
                format!(
                    "線幅 {:.2} mm がノズル径 {:.2} mm の 2.5 倍を超えています（過押出）。",
                    pf.width, pr.nozzle_d
                ),
            );
        } else if pf.width < pr.nozzle_d * 0.5 {
            push(
                false,
                format!(
                    "線幅 {:.2} mm がノズル径 {:.2} mm の半分未満です（押出不足で線が切れることがあります）。",
                    pf.width, pr.nozzle_d
                ),
            );
        }
        if pf.layer_h > pr.nozzle_d * 0.8 {
            push(
                true,
                format!(
                    "層の高さ {:.2} mm がノズル径 {:.2} mm の 80 % を超えています（過押出）。",
                    pf.layer_h, pr.nozzle_d
                ),
            );
        }
        if pf.flow_pct > 150.0 {
            push(
                true,
                format!(
                    "流量 {:.0} % は多すぎます（150 % 以下にしてください）。",
                    pf.flow_pct
                ),
            );
        } else if pf.flow_pct < 50.0 {
            push(false, format!("流量 {:.0} % は少なめです。", pf.flow_pct));
        }
        if pf.max_flow_mm3_s > 0.0 {
            let fastest = pf.speed_outer.max(pf.speed_inner);
            let flow = pf.width * pf.layer_h * fastest * pf.flow_pct / 100.0;
            if flow > pf.max_flow_mm3_s * 1.5 {
                push(
                    true,
                    format!(
                        "最大の押出流量が {flow:.1} mm³/s で、目安の上限 {:.0} mm³/s を大きく超えます。速度を下げてください。",
                        pf.max_flow_mm3_s
                    ),
                );
            } else if flow > pf.max_flow_mm3_s {
                push(
                    false,
                    format!(
                        "最大の押出流量が {flow:.1} mm³/s で、目安の上限 {:.0} mm³/s を超えます。",
                        pf.max_flow_mm3_s
                    ),
                );
            }
        }
        if pr.heated && (pf.nozzle_c > 300.0 || pf.bed_c > 120.0) {
            push(
                true,
                "温度が高すぎます（ノズル 300 ℃・ベッド 120 ℃ まで）。".into(),
            );
        }
        if pr.heated && pf.nozzle_c < 150.0 {
            push(true, "ノズル温度が 150 ℃ 未満では押し出せません。".into());
        }

        // Travel through printed area, and steep slopes.
        let mut printed: HashMap<(i32, i32), f64> = HashMap::new();
        let mut cells_full = false;
        let mut crossings: HashMap<u32, usize> = HashMap::new();
        let mut steep = 0usize;
        let mut first_steep: Option<(u32, [f64; 3])> = None;
        let mut has_print = false;
        let mut invalid_move = false;
        self.walk(cancel, &mut |m| {
            if matches!(m.op, Op::Travel | Op::Print) {
                for p in [m.from, m.to] {
                    let machine = [p[0] + off[0], p[1] + off[1], p[2]];
                    if machine
                        .iter()
                        .zip(pr.bed)
                        .any(|(v, size)| !in_range(*v, 0.0, size))
                    {
                        invalid_move = true;
                    }
                }
            }
            match m.op {
                Op::Print => {
                    has_print = true;
                    let dxy = (m.to[0] - m.from[0]).hypot(m.to[1] - m.from[1]);
                    let dz = (m.to[2] - m.from[2]).abs();
                    if dxy > 0.01 && dz / dxy > 1.0 {
                        steep += 1;
                        first_steep.get_or_insert((m.layer, m.to));
                    }
                    if !cells_full {
                        for t in [0.0, 0.5, 1.0] {
                            let x = m.from[0] + (m.to[0] - m.from[0]) * t;
                            let y = m.from[1] + (m.to[1] - m.from[1]) * t;
                            let z = m.from[2] + (m.to[2] - m.from[2]) * t;
                            let entry = printed.entry(cell(x, y)).or_insert(f64::NEG_INFINITY);
                            if z > *entry {
                                *entry = z;
                            }
                        }
                        if printed.len() > MAX_GRID_CELLS {
                            cells_full = true;
                        }
                    }
                }
                Op::Travel if !m.hop => {
                    let len = dist3(m.from, m.to);
                    if len > 2.0 * GRID_CELL_MM {
                        let steps = (len / 1.0).ceil() as usize;
                        let mut hit = false;
                        for i in 1..steps {
                            let t = i as f64 / steps as f64;
                            let x = m.from[0] + (m.to[0] - m.from[0]) * t;
                            let y = m.from[1] + (m.to[1] - m.from[1]) * t;
                            let z = m.from[2] + (m.to[2] - m.from[2]) * t;
                            let near_end = (x - m.to[0]).hypot(y - m.to[1]) < 1.5 * GRID_CELL_MM
                                || (x - m.from[0]).hypot(y - m.from[1]) < 1.5 * GRID_CELL_MM;
                            if near_end {
                                continue;
                            }
                            if printed.get(&cell(x, y)).is_some_and(|top| *top >= z - 0.05) {
                                hit = true;
                                break;
                            }
                        }
                        if hit {
                            *crossings.entry(m.layer).or_insert(0) += 1;
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        if invalid_move {
            push(
                true,
                "生成された移動（Zホップを含む）がベッド範囲外です。".into(),
            );
        }
        if !has_print {
            push(true, "有効な押出経路がありません。".into());
        }
        if !crossings.is_empty() {
            let total: usize = crossings.values().sum();
            let first = crossings.keys().min().copied().unwrap_or(0);
            push(
                false,
                format!(
                    "印刷済みの部分を横切る移動が {total} 回あります（最初は層 {}）。Zホップを入れるか、経路の並び替えを見直してください。",
                    u64::from(first) + 1
                ),
            );
        }
        if cells_full {
            push(
                false,
                "経路が多いため、横切り移動の確認を一部省略しました。".into(),
            );
        }
        if let Some((layer, at)) = first_steep {
            push(
                false,
                format!(
                    "傾斜が 45° を超える部分が {steep} か所あります（層 {}、X {:.1} Y {:.1}）。ノズルが印刷物に当たる恐れがあります。",
                    u64::from(layer) + 1,
                    at[0],
                    at[1]
                ),
            );
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

struct Text {
    out: String,
}

impl Text {
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
}

/// Reject user script text that could break the file. Only printable text, bounded.
pub fn validate_script(text: &str) -> Result<(), String> {
    if text.len() > MAX_SCRIPT_BYTES {
        return Err(format!(
            "Gコードの文章が長すぎます（{MAX_SCRIPT_BYTES} バイトまで）。"
        ));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r')
    {
        return Err("Gコードの文章に使えない制御文字があります。".into());
    }
    if text.lines().any(|l| l.len() > 300) {
        return Err("Gコードの1行が長すぎます（300 バイトまで）。".into());
    }
    Ok(())
}

fn push_script(t: &mut Text, text: &str) -> Result<(), String> {
    for line in text.lines() {
        t.line(line.trim_end())?;
    }
    Ok(())
}

impl GcodeJob {
    /// Text of the whole file and its estimate. Refuses a job the safety check calls an error.
    pub fn render(&self, cancel: &dyn Fn() -> bool) -> Result<(String, Stats), String> {
        let issues = self.check(cancel)?;
        let errors: Vec<&str> = issues
            .iter()
            .filter(|i| i.error)
            .map(|i| i.text.as_str())
            .collect();
        if !errors.is_empty() {
            return Err(format!(
                "安全チェックで問題が見つかったため書き出せません。\n{}",
                errors.join("\n")
            ));
        }
        validate_script(&self.start.text)?;
        validate_script(&self.end.text)?;
        let stats = self.stats(cancel)?;
        let pr = &self.printer;
        let pf = &self.profile;
        let off = pr.offset();
        let mut t = Text {
            out: String::with_capacity(self.point_count().saturating_mul(48).min(MAX_OUTPUT_BYTES)),
        };

        // Start.
        t.line("; NODES で作成した G-code（実機では未検証です）")?;
        t.line(&format!("; 機種: {}", pr.preset.label()))?;
        t.line("; 開始・終了の G-code は一般的な例です。機種の説明書と照らして確認し、初回は印刷を見守ってください。")?;
        if pr.preset.is_bambu() {
            t.line("; Bambu Lab 機種は Bambu Studio の .gcode.3mf が公式の方法です。この生の G-code は動作未確認です。")?;
        }
        if self.start.mode != ScriptMode::Custom {
            t.line("G21 ; 単位は mm")?;
            t.line("G90 ; 座標は絶対値")?;
            if pr.heated {
                t.line(&format!("M140 S{:.0} ; ベッドを加熱", pf.bed_c))?;
                t.line(&format!("M104 S{:.0} ; ノズルを加熱", pf.nozzle_c))?;
                t.line("M107 ; ファン停止")?;
                t.line("G28 ; 全軸をホーム")?;
                t.line(&format!("M190 S{:.0} ; ベッドの温度を待つ", pf.bed_c))?;
                t.line(&format!("M109 S{:.0} ; ノズルの温度を待つ", pf.nozzle_c))?;
            } else {
                t.line("G28 ; 全軸をホーム")?;
            }
        }
        if self.start.mode != ScriptMode::Auto {
            t.line("; ---- ユーザーの開始 G-code ----")?;
            push_script(&mut t, &self.start.text)?;
            t.line("; ---- ここまで ----")?;
        }
        // The extruder mode is always set here, whatever the script did.
        t.line(if pf.relative_e {
            "M83 ; 押し出し量は相対値"
        } else {
            "M82 ; 押し出し量は絶対値"
        })?;
        t.line("G92 E0 ; 押し出し量を 0 に")?;
        if self.start.mode == ScriptMode::Auto && pr.heated {
            let x0 = 10.0_f64.min(pr.bed[0] / 2.0);
            let x1 = (10.0 + 120.0_f64).min(pr.bed[0] - 10.0).max(x0);
            let e = extrusion_e(x1 - x0, pf.width, pf.layer_h, pr.filament_d);
            t.line("; 捨て打ち線（手前側）")?;
            t.line(&format!(
                "G0 X{x0:.3} Y4.000 Z{:.3} F{:.0}",
                pf.layer_h,
                pf.speed_travel * 60.0
            ))?;
            let e_text = if pf.relative_e { e } else { e.max(0.0) };
            t.line(&format!(
                "G1 X{x1:.3} Y4.000 E{e_text:.5} F{:.0}",
                pf.speed_first * 60.0
            ))?;
            t.line("G92 E0 ; 押し出し量を 0 に")?;
        }
        t.line(";BODY-START")?;

        // Body.
        let mut e_abs = 0.0_f64;
        let mut feed_last = f64::NAN;
        let mut fan_on = false;
        let mut top_z = 0.0_f64;
        let mut last = [0.0_f64, 0.0, 0.0];
        let mut next_change = 0usize;
        let fan_value = (pf.fan_pct * 2.55).round();
        let relative = pf.relative_e;
        self.walk(cancel, &mut |m| {
            let mx = m.to[0] + off[0];
            let my = m.to[1] + off[1];
            let mz = m.to[2];
            let mut feed_text = |feed_mm_s: f64| -> String {
                let f = (feed_mm_s * 60.0).round();
                if f != feed_last && f > 0.0 {
                    feed_last = f;
                    format!(" F{f:.0}")
                } else {
                    String::new()
                }
            };
            match m.op {
                Op::Layer => {
                    let mut change_here = false;
                    while self
                        .color_changes
                        .get(next_change)
                        .is_some_and(|&l| l <= m.layer)
                    {
                        next_change += 1;
                        change_here = true;
                    }
                    if change_here && pr.heated {
                        t.line(&format!("M600 ; 色替え（層 {} の前）", m.layer + 1))?;
                    }
                    t.line(&format!(";LAYER:{}", m.layer))?;
                    t.line(&format!(";Z:{:.3}", m.to[2]))?;
                    if pr.heated
                        && pf.fan_pct > 0.0
                        && !fan_on
                        && (m.layer >= pf.fan_from_layer || m.kind == PathKind::Spiral)
                    {
                        t.line(&format!("M106 S{fan_value:.0} ; ファン開始"))?;
                        fan_on = true;
                    }
                }
                Op::Travel => {
                    let f = feed_text(m.feed);
                    t.line(&format!("G0 X{mx:.3} Y{my:.3} Z{mz:.4}{f}"))?;
                    last = [mx, my, mz];
                }
                Op::Print => {
                    top_z = top_z.max(mz);
                    e_abs += m.e;
                    let e = if relative { m.e } else { e_abs };
                    let f = feed_text(m.feed);
                    t.line(&format!("G1 X{mx:.3} Y{my:.3} Z{mz:.4} E{e:.5}{f}"))?;
                    last = [mx, my, mz];
                }
                Op::Retract | Op::Unretract => {
                    e_abs += m.e;
                    let e = if relative { m.e } else { e_abs };
                    let f = feed_text(m.feed);
                    t.line(&format!("G1 E{e:.5}{f}"))?;
                }
                Op::Dwell(ms) => t.line(&format!("G4 P{ms}"))?,
            }
            Ok(())
        })?;

        // End.
        t.line(";BODY-END")?;
        if self.end.mode != ScriptMode::Custom {
            if pf.retract_mm > 0.0 {
                let e = if relative {
                    -pf.retract_mm
                } else {
                    e_abs - pf.retract_mm
                };
                t.line(&format!(
                    "G1 E{e:.5} F{:.0} ; 引き戻し",
                    pf.retract_speed * 60.0
                ))?;
            }
            let lift = (top_z + 10.0).min(pr.bed[2]).max(last[2]);
            t.line(&format!(
                "G0 Z{lift:.3} F{:.0} ; 上へ逃がす",
                pf.speed_travel * 60.0
            ))?;
            t.line(&format!(
                "G0 X0.000 Y{:.3} F{:.0} ; 奥へ移動",
                pr.bed[1],
                pf.speed_travel * 60.0
            ))?;
            if pr.heated {
                t.line("M104 S0 ; ノズル加熱を停止")?;
                t.line("M140 S0 ; ベッド加熱を停止")?;
                t.line("M107 ; ファン停止")?;
            }
            t.line("M84 ; モーター停止")?;
        }
        if self.end.mode != ScriptMode::Auto {
            t.line("; ---- ユーザーの終了 G-code ----")?;
            push_script(&mut t, &self.end.text)?;
            t.line("; ---- ここまで ----")?;
        }
        Ok((t.out, stats))
    }
}

// ---------------------------------------------------------------------------
// Preview segments (for the viewport)
// ---------------------------------------------------------------------------

pub type PreviewSegment = ([f64; 3], [f64; 3], [u8; 3]);

fn ramp(t: f64) -> [u8; 3] {
    // Blue (low) to green to yellow to red (high).
    let t = t.clamp(0.0, 1.0);
    let stops: [[f64; 3]; 4] = [
        [45.0, 90.0, 210.0],
        [40.0, 180.0, 130.0],
        [235.0, 205.0, 60.0],
        [215.0, 60.0, 50.0],
    ];
    let scaled = t * 3.0;
    let i = (scaled.floor() as usize).min(2);
    let f = scaled - i as f64;
    let (Some(a), Some(b)) = (stops.get(i), stops.get(i + 1)) else {
        return [128, 128, 128];
    };
    [
        (a[0] + (b[0] - a[0]) * f).round() as u8,
        (a[1] + (b[1] - a[1]) * f).round() as u8,
        (a[2] + (b[2] - a[2]) * f).round() as u8,
    ]
}

fn kind_color(kind: PathKind) -> [u8; 3] {
    match kind {
        PathKind::Outer => [45, 130, 200],
        PathKind::Inner => [60, 170, 120],
        PathKind::Infill => [225, 160, 50],
        PathKind::Spiral => [190, 80, 170],
    }
}

impl GcodeJob {
    /// Coloured line segments of the extrusion paths, at most about `max` of them.
    /// Travels are shown in grey when `show_travel` is set.
    pub fn preview_segments(&self, max: usize) -> Vec<PreviewSegment> {
        let view = self.preview.unwrap_or(Preview {
            color_by: ColorBy::Kind,
            show_travel: false,
        });
        let total: usize = self.paths.iter().map(PathSeg::edge_count).sum();
        if total == 0 || max == 0 {
            return Vec::new();
        }
        let stride = total.div_ceil(max).max(1);
        let layers = self.layer_count().max(1);
        let (mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in &self.paths {
            let f = self.feed_for(p.kind, p.layer);
            vmin = vmin.min(f);
            vmax = vmax.max(f);
        }
        let mut out = Vec::new();
        let mut index = 0usize;
        let mut done = 0.0_f64;
        let mut cur: Option<[f64; 3]> = None;
        for path in &self.paths {
            let n = path.pts.len();
            if n < 2 {
                continue;
            }
            if view.show_travel
                && let (Some(c), Some(&first)) = (cur, path.pts.first())
                && dist3(c, first) > 0.5
                && out.len() < max
            {
                out.push((c, first, [150, 150, 150]));
            }
            for i in 0..path.edge_count() {
                let (Some(&a), Some(&b)) = (path.pts.get(i), path.pts.get((i + 1) % n)) else {
                    break;
                };
                let len = dist3(a, b);
                done += len;
                if index.is_multiple_of(stride) {
                    let color = match view.color_by {
                        ColorBy::Speed => {
                            let f = self.feed_for(path.kind, path.layer);
                            if vmax > vmin {
                                ramp((f - vmin) / (vmax - vmin))
                            } else {
                                ramp(0.5)
                            }
                        }
                        ColorBy::Extrusion => ramp(done / self.total_print_len().max(1e-9)),
                        ColorBy::Layer => ramp(f64::from(path.layer) / f64::from(layers)),
                        ColorBy::Kind => kind_color(path.kind),
                    };
                    out.push((a, b, color));
                }
                index += 1;
            }
            cur = path.pts.get(if path.closed { 0 } else { n - 1 }).copied();
        }
        out
    }

    fn total_print_len(&self) -> f64 {
        self.paths.iter().map(PathSeg::length).sum()
    }

    /// One line for the data inspector.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "Gコード · {} · 経路 {} 本 · {} 点",
            self.printer.preset.label(),
            self.paths.len(),
            self.paths.iter().map(|p| p.pts.len()).sum::<usize>()
        );
        if self.paths.is_empty() && !self.slices.is_empty() {
            let _ = write!(
                text,
                " · 輪切り {} 層（経路はまだありません）",
                self.slices.len()
            );
        }
        let layers = self.layer_count();
        if layers > 0 {
            let _ = write!(text, " · {layers} 層");
        }
        if !self.notes.is_empty() {
            for note in self.notes.iter().take(8) {
                let _ = write!(
                    text,
                    "\n{} {}",
                    if note.error {
                        "[エラー]"
                    } else {
                        "[注意]"
                    },
                    note.text
                );
            }
            if self.notes.len() > 8 {
                let _ = write!(text, "\n…ほか {} 件", self.notes.len() - 8);
            }
        }
        text
    }
}

// ---------------------------------------------------------------------------
// Path transforms
// ---------------------------------------------------------------------------

/// Insert points so no edge is longer than `step`. Fails when the result would be too big.
pub fn subdivide(
    pts: &[[f64; 3]],
    closed: bool,
    step: f64,
    budget: &mut usize,
) -> Result<Vec<[f64; 3]>, String> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    let edges = if closed && n > 2 {
        n
    } else {
        n.saturating_sub(1)
    };
    for i in 0..n {
        let Some(&a) = pts.get(i) else { break };
        out.push(a);
        if i >= edges {
            continue;
        }
        let Some(&b) = pts.get((i + 1) % n) else {
            break;
        };
        let pieces = (dist3(a, b) / step).ceil();
        if !pieces.is_finite() {
            return Err("経路の長さが無効です。".into());
        }
        let pieces = pieces as usize;
        if pieces > 1 {
            if pieces > *budget {
                return Err("細かく分けた結果が大きすぎます。間隔を広げてください。".into());
            }
            for k in 1..pieces {
                let t = k as f64 / pieces as f64;
                out.push([
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ]);
            }
            *budget -= pieces - 1;
        }
    }
    if out.len() > *budget + n && *budget == 0 {
        return Err("細かく分けた結果が大きすぎます。間隔を広げてください。".into());
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nonplanar {
    /// 0 = wave along X, 1 = along Y, 2 = concentric, 3 = egg crate.
    pub mode: u8,
    pub amplitude: f64,
    pub wavelength: f64,
    pub phase_deg: f64,
    pub start_layer: u32,
    pub fade_layers: u32,
}

fn wave_value(mode: u8, x: f64, y: f64, c: [f64; 2], wl: f64, phase: f64) -> f64 {
    let k = 2.0 * PI / wl;
    match mode {
        0 => (k * x + phase).sin(),
        1 => (k * y + phase).sin(),
        2 => (k * (x - c[0]).hypot(y - c[1]) + phase).sin(),
        _ => (k * x + phase).sin() * (k * y + phase).sin(),
    }
}

pub fn bounds_xy(paths: &[PathSeg]) -> Option<([f64; 2], [f64; 2])> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for p in paths.iter().flat_map(|p| p.pts.iter()) {
        for axis in 0..2 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    }
    (low.iter().chain(high.iter()).all(|v| v.is_finite())).then_some((low, high))
}

impl GcodeJob {
    pub fn apply_nonplanar(
        &mut self,
        np: Nonplanar,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if !(np.amplitude.is_finite() && np.wavelength.is_finite() && np.wavelength > 0.5) {
            return Err("ノンプラナーの振幅または波長が無効です。".into());
        }
        let (low, high) = bounds_xy(&self.paths).ok_or("経路がありません。")?;
        let centre = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
        let step = (np.wavelength / 8.0).clamp(0.2, 1.0);
        let mut budget = MAX_POINTS.saturating_sub(self.point_count());
        let phase = np.phase_deg.to_radians();
        for (i, path) in self.paths.iter_mut().enumerate() {
            if i % 64 == 0 && cancel() {
                return Err(STOPPED.into());
            }
            if path.layer < np.start_layer {
                continue;
            }
            let fade = f64::from(np.fade_layers.max(1));
            let weight = (f64::from(path.layer - np.start_layer + 1) / fade).min(1.0);
            let subdivided = subdivide(&path.pts, path.closed, step, &mut budget)?;
            path.pts = subdivided;
            for p in &mut path.pts {
                p[2] += np.amplitude
                    * weight
                    * wave_value(np.mode, p[0], p[1], centre, np.wavelength, phase);
            }
        }
        self.check_limits()
    }

    pub fn apply_split(
        &mut self,
        max_len: f64,
        corner_deg: f64,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if !(max_len.is_finite() && max_len >= 1.0) {
            return Err("分割の長さは 1 mm 以上にしてください。".into());
        }
        let cos_limit = corner_deg.to_radians().cos();
        let mut out: Vec<PathSeg> = Vec::with_capacity(self.paths.len());
        let mut total = 0usize;
        for (index, path) in self.paths.drain(..).enumerate() {
            if index % 64 == 0 && cancel() {
                return Err(STOPPED.into());
            }
            if path.kind == PathKind::Spiral && corner_deg <= 0.0 && max_len >= 1e6 {
                out.push(path);
                continue;
            }
            // Walk the path (closed paths return to the start) and cut where needed.
            let mut pts = path.pts.clone();
            if path.closed
                && pts.len() > 2
                && let Some(&f) = pts.first()
            {
                pts.push(f);
            }
            let mut budget = MAX_POINTS.saturating_sub(total);
            let pts = subdivide(&pts, false, max_len, &mut budget)?;
            let mut cur: Vec<[f64; 3]> = Vec::new();
            let mut acc = 0.0;
            for (i, &p) in pts.iter().enumerate() {
                if let Some(&prev) = cur.last() {
                    let d = dist3(prev, p);
                    if cur.len() >= 2 && acc + d > max_len + 1e-9 {
                        total += cur.len();
                        out.push(PathSeg {
                            pts: std::mem::take(&mut cur),
                            kind: path.kind,
                            layer: path.layer,
                            rank: path.rank,
                            closed: false,
                        });
                        cur.push(prev);
                        acc = 0.0;
                    }
                    acc += d;
                }
                cur.push(p);
                let mut cut = false;
                if corner_deg > 0.0
                    && i >= 1
                    && i + 1 < pts.len()
                    && let (Some(&a), Some(&c)) = (pts.get(i - 1), pts.get(i + 1))
                    && let (Some(u), Some(v)) = (norm3(sub3(p, a)), norm3(sub3(c, p)))
                {
                    cut = dot3(u, v) < cos_limit;
                }
                if cut && cur.len() >= 2 {
                    total += cur.len();
                    out.push(PathSeg {
                        pts: std::mem::take(&mut cur),
                        kind: path.kind,
                        layer: path.layer,
                        rank: path.rank,
                        closed: false,
                    });
                    cur.push(p);
                    acc = 0.0;
                }
            }
            if cur.len() >= 2 {
                total += cur.len();
                out.push(PathSeg {
                    pts: cur,
                    kind: path.kind,
                    layer: path.layer,
                    rank: path.rank,
                    closed: false,
                });
            }
            if out.len() > MAX_PATHS || total > MAX_POINTS {
                return Err("分割した結果が大きすぎます。分割の長さを大きくしてください。".into());
            }
        }
        self.paths = out;
        Ok(())
    }

    /// Nearest-neighbour order inside each (layer, rank) group. Closed paths are rotated
    /// to start at the point nearest to the head; open paths may be reversed.
    pub fn sort_nearest(&mut self, cancel: &dyn Fn() -> bool) -> Result<(), String> {
        let mut keys: Vec<(u32, u8)> = self.paths.iter().map(|p| (p.layer, p.rank)).collect();
        keys.dedup();
        let mut groups: HashMap<(u32, u8), Vec<usize>> = HashMap::new();
        for (i, p) in self.paths.iter().enumerate() {
            groups.entry((p.layer, p.rank)).or_default().push(i);
        }
        if groups.values().any(|g| g.len() > 20_000) {
            return Err(
                "1つの層の経路が多すぎて並び替えできません。経路の分割を減らしてください。".into(),
            );
        }
        let mut order_keys: Vec<(u32, u8)> = groups.keys().copied().collect();
        order_keys.sort_unstable();
        let mut taken: Vec<Option<PathSeg>> = self.paths.drain(..).map(Some).collect();
        let mut result: Vec<PathSeg> = Vec::with_capacity(taken.len());
        let mut head: Option<[f64; 3]> = None;
        let mut spent = 0usize;
        for key in order_keys {
            let Some(members) = groups.get(&key) else {
                continue;
            };
            let mut left: Vec<usize> = members.clone();
            while !left.is_empty() {
                spent += left.len();
                if spent > 2_000_000 {
                    spent = 0;
                    if cancel() {
                        return Err(STOPPED.into());
                    }
                }
                let target = head;
                let mut best = 0usize;
                let mut best_d = f64::INFINITY;
                let mut best_rev = false;
                for (slot, &idx) in left.iter().enumerate() {
                    let Some(Some(p)) = taken.get(idx) else {
                        continue;
                    };
                    let Some(t) = target else {
                        best = slot;
                        break;
                    };
                    if let Some(&f) = p.pts.first() {
                        let d = dist3(t, f);
                        if d < best_d {
                            best_d = d;
                            best = slot;
                            best_rev = false;
                        }
                    }
                    if !p.closed
                        && let Some(&l) = p.pts.last()
                    {
                        let d = dist3(t, l);
                        if d < best_d {
                            best_d = d;
                            best = slot;
                            best_rev = true;
                        }
                    }
                    if p.closed && p.kind != PathKind::Spiral {
                        for &q in &p.pts {
                            let d = dist3(t, q);
                            if d < best_d {
                                best_d = d;
                                best = slot;
                                best_rev = false;
                            }
                        }
                    }
                }
                let idx = left.swap_remove(best);
                let Some(mut p) = taken.get_mut(idx).and_then(Option::take) else {
                    continue;
                };
                if best_rev {
                    p.pts.reverse();
                } else if p.closed
                    && p.kind != PathKind::Spiral
                    && let Some(t) = target
                {
                    let mut at = 0usize;
                    let mut d_min = f64::INFINITY;
                    for (i, q) in p.pts.iter().enumerate() {
                        let d = dist3(t, *q);
                        if d < d_min {
                            d_min = d;
                            at = i;
                        }
                    }
                    p.pts.rotate_left(at);
                }
                head = if p.closed {
                    p.pts.first().copied()
                } else {
                    p.pts.last().copied()
                };
                result.push(p);
            }
        }
        self.paths = result;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Texture {
    /// 0 = sine, 1 = triangle, 2 = soft square.
    pub mode: u8,
    pub amplitude: f64,
    pub wavelength: f64,
    /// Vertical wavelength in mm. 0 = no vertical variation.
    pub z_wavelength: f64,
    /// Extra phase per layer, degrees.
    pub twist_deg: f64,
}

fn shape(mode: u8, phase: f64) -> f64 {
    let t = phase / (2.0 * PI);
    let frac = t - t.floor();
    match mode {
        0 => phase.sin(),
        1 => 1.0 - 4.0 * (frac - 0.5).abs(),
        _ => (3.0 * phase.sin()).tanh(),
    }
}

impl GcodeJob {
    /// Move wall points sideways (away from the material) by a periodic amount.
    /// Works on outer, inner and spiral paths. Contour paths must be oriented
    /// counter-clockwise for outer loops (the contour node does that).
    pub fn apply_texture(&mut self, tx: Texture, cancel: &dyn Fn() -> bool) -> Result<(), String> {
        if !(tx.amplitude.is_finite() && tx.wavelength.is_finite() && tx.wavelength >= 0.5) {
            return Err("テクスチャの振幅または波長が無効です。".into());
        }
        let step = (tx.wavelength / 10.0).clamp(0.2, 1.0);
        let mut budget = MAX_POINTS.saturating_sub(self.point_count());
        for (i, path) in self.paths.iter_mut().enumerate() {
            if i % 32 == 0 && cancel() {
                return Err(STOPPED.into());
            }
            if path.kind == PathKind::Infill {
                continue;
            }
            let pts = subdivide(&path.pts, path.closed, step, &mut budget)?;
            let n = pts.len();
            if n < 3 {
                path.pts = pts;
                continue;
            }
            let mut s = Vec::with_capacity(n);
            let mut acc = 0.0;
            for k in 0..n {
                if let (Some(a), Some(b)) = (pts.get(k.saturating_sub(1)), pts.get(k)) {
                    acc += dist3(*a, *b);
                }
                s.push(acc);
            }
            let perimeter = if path.closed {
                acc + match (pts.last(), pts.first()) {
                    (Some(a), Some(b)) => dist3(*a, *b),
                    _ => 0.0,
                }
            } else {
                acc
            };
            // A closed wall gets a whole number of waves so the seam matches.
            let wl = if path.closed {
                let waves = (perimeter / tx.wavelength).round().max(1.0);
                perimeter / waves
            } else {
                tx.wavelength
            };
            let layer_phase = f64::from(path.layer) * tx.twist_deg.to_radians();
            let mut moved = Vec::with_capacity(n);
            for k in 0..n {
                let (Some(&p), Some(&sk)) = (pts.get(k), s.get(k)) else {
                    break;
                };
                let prev = if k == 0 {
                    if path.closed {
                        pts.get(n - 1)
                    } else {
                        pts.first()
                    }
                } else {
                    pts.get(k - 1)
                };
                let next = if k + 1 == n {
                    if path.closed {
                        pts.first()
                    } else {
                        pts.get(n - 1)
                    }
                } else {
                    pts.get(k + 1)
                };
                let (Some(&a), Some(&b)) = (prev, next) else {
                    break;
                };
                let tx_ = b[0] - a[0];
                let ty = b[1] - a[1];
                let len = tx_.hypot(ty);
                if len < 1e-9 {
                    moved.push(p);
                    continue;
                }
                // Right-hand normal of a counter-clockwise loop points out of the material.
                let nx = ty / len;
                let ny = -tx_ / len;
                let mut phase = 2.0 * PI * sk / wl + layer_phase;
                if tx.z_wavelength > 0.0 {
                    phase += 2.0 * PI * p[2] / tx.z_wavelength;
                }
                let d = tx.amplitude * shape(tx.mode, phase);
                moved.push([p[0] + nx * d, p[1] + ny * d, p[2]]);
            }
            path.pts = moved;
        }
        self.check_limits()
    }
}

// ---------------------------------------------------------------------------
// Small maths helpers
// ---------------------------------------------------------------------------

pub fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm3(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = dot3(a, a).sqrt();
    (l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

fn in_range(v: f64, lo: f64, hi: f64) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> bool {
        false
    }

    fn square(z: f64, layer: u32, kind: PathKind) -> PathSeg {
        PathSeg {
            pts: vec![
                [20.0, 20.0, z],
                [40.0, 20.0, z],
                [40.0, 40.0, z],
                [20.0, 40.0, z],
            ],
            kind,
            layer,
            rank: 0,
            closed: true,
        }
    }

    fn job_with(paths: Vec<PathSeg>) -> GcodeJob {
        let mut job = GcodeJob::new(Printer::from_preset(Preset::BambuA1MiniCorner));
        job.paths = paths;
        job
    }

    #[test]
    fn gradient_layers_are_even_and_include_both_ends() {
        assert_eq!(
            gradient_layers(10, 50, 5).unwrap(),
            vec![10, 20, 30, 40, 50]
        );
        assert_eq!(gradient_layers(7, 7, 1).unwrap(), vec![7]);
        // Too many changes for the span collapse onto the same layers.
        assert_eq!(gradient_layers(3, 5, 6).unwrap(), vec![3, 4, 5]);
        assert!(gradient_layers(5, 3, 2).is_err());
        assert!(gradient_layers(0, 10, 0).is_err());
        assert!(gradient_layers(0, 10, 100_000).is_err());
    }

    #[test]
    fn color_change_is_written_once_before_its_layer() {
        let mut job = job_with(vec![
            square(0.2, 0, PathKind::Outer),
            square(0.4, 1, PathKind::Outer),
            square(0.6, 2, PathKind::Outer),
        ]);
        job.add_color_changes(&[0, 2, 2]).unwrap();
        assert_eq!(job.color_changes, vec![2]);
        let (text, stats) = job.render(&never).unwrap();
        assert_eq!(text.matches("M600").count(), 1);
        let m600 = text.find("M600").unwrap();
        assert!(m600 > text.find(";LAYER:1").unwrap());
        assert!(m600 < text.find(";LAYER:2").unwrap());
        assert_eq!(stats.color_changes, 1);
        assert!(
            job.check(&never)
                .unwrap()
                .iter()
                .all(|n| !(n.error && n.text.contains("色替え")))
        );
    }

    #[test]
    fn color_change_past_the_last_layer_is_warned_not_written() {
        let mut job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        job.add_color_changes(&[5]).unwrap();
        let (text, _) = job.render(&never).unwrap();
        assert!(!text.contains("M600"));
        assert!(
            job.check(&never)
                .unwrap()
                .iter()
                .any(|n| !n.error && n.text.contains("色替え"))
        );
    }

    #[test]
    fn extrusion_formula_matches_hand_calculation() {
        // 10 mm line, 0.4 wide, 0.2 high, 1.75 filament: 10*0.08/(pi*0.875^2)
        let e = extrusion_e(10.0, 0.4, 0.2, 1.75);
        assert!((e - 0.332_6).abs() < 1e-3, "{e}");
        assert_eq!(extrusion_e(10.0, 0.4, 0.2, 0.0), 0.0);
    }

    #[test]
    fn walk_prints_a_closed_square_with_four_edges() {
        let job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        let mut prints = 0;
        let mut length = 0.0;
        job.walk(&never, &mut |m| {
            if m.op == Op::Print {
                prints += 1;
                length += dist3(m.from, m.to);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(prints, 4);
        assert!((length - 80.0).abs() < 1e-9);
    }

    #[test]
    fn stats_filament_equals_formula() {
        let job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        let s = job.stats(&never).unwrap();
        let expected = extrusion_e(80.0, 0.42, 0.2, 1.75);
        assert!((s.filament_mm - expected).abs() < 1e-9);
        assert!(s.seconds > 0.0);
        assert!((s.print_mm - 80.0).abs() < 1e-9);
    }

    #[test]
    fn render_always_sets_extruder_mode_and_resets_e() {
        for relative in [false, true] {
            for mode in [ScriptMode::Auto, ScriptMode::AutoPlus, ScriptMode::Custom] {
                let mut job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
                job.profile.relative_e = relative;
                job.start = Script {
                    mode,
                    text: "M83\nM82\nG28".into(),
                };
                let (text, _) = job.render(&never).unwrap();
                let wanted = if relative { "M83" } else { "M82" };
                let body = text.find(";BODY-START").unwrap();
                let head = &text[..body];
                let last_mode = head
                    .lines()
                    .rfind(|l| l.starts_with("M82") || l.starts_with("M83"))
                    .unwrap();
                assert!(
                    last_mode.starts_with(wanted),
                    "{mode:?} {relative}: {last_mode}"
                );
                assert!(head.contains("G92 E0"));
            }
        }
    }

    #[test]
    fn absolute_e_increases_and_relative_e_is_per_move() {
        let mut job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        let (abs, _) = job.render(&never).unwrap();
        let body = abs.split(";BODY-START").nth(1).unwrap();
        let es: Vec<f64> = body
            .lines()
            .filter(|l| l.starts_with("G1 X"))
            .filter_map(|l| l.split(" E").nth(1)?.split(' ').next()?.parse().ok())
            .collect();
        assert!(es.windows(2).all(|w| w[1] >= w[0]));
        job.profile.relative_e = true;
        let (rel, _) = job.render(&never).unwrap();
        assert!(rel.contains("M83"));
        assert!(!rel.contains("\nM82"));
    }

    #[test]
    fn origin_center_adds_half_the_bed() {
        let mut job = job_with(vec![PathSeg {
            pts: vec![[-10.0, -10.0, 0.2], [10.0, -10.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        }]);
        job.printer = Printer::from_preset(Preset::BambuA1MiniCenter);
        let (text, _) = job.render(&never).unwrap();
        assert!(text.contains("X80.000 Y80.000"), "{text}");
        assert!(text.contains("X100.000 Y80.000"));
    }

    #[test]
    fn safety_flags_outside_bed_nan_and_overextrusion() {
        let mut job = job_with(vec![PathSeg {
            pts: vec![[-5.0, 10.0, 0.2], [400.0, 10.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        }]);
        let issues = job.check(&never).unwrap();
        assert!(issues.iter().any(|i| i.error && i.text.contains("ベッド")));
        assert!(job.render(&never).is_err());

        job.paths[0].pts[1][0] = f64::NAN;
        let issues = job.check(&never).unwrap();
        assert!(
            issues
                .iter()
                .any(|i| i.error && i.text.contains("数値でない"))
        );

        let mut over = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        over.profile.width = 2.0;
        assert!(
            over.check(&never)
                .unwrap()
                .iter()
                .any(|i| i.error && i.text.contains("過押出"))
        );
    }

    #[test]
    fn safety_flags_travel_across_printed_wall_without_hop() {
        // A wall along y = 40, then a short path below it and one above it.
        let a = PathSeg {
            pts: vec![[20.0, 40.0, 0.2], [60.0, 40.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let b = PathSeg {
            pts: vec![[40.0, 10.0, 0.2], [40.0, 20.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let c = PathSeg {
            pts: vec![[40.0, 60.0, 0.2], [40.0, 70.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let mut job = job_with(vec![a, b, c]);
        let issues = job.check(&never).unwrap();
        assert!(
            issues.iter().any(|i| i.text.contains("横切る")),
            "{issues:?}"
        );
        job.profile.hop_mm = 1.0;
        let issues = job.check(&never).unwrap();
        assert!(
            !issues.iter().any(|i| i.text.contains("横切る")),
            "{issues:?}"
        );
    }

    #[test]
    fn retract_and_hop_are_written_for_long_travel() {
        let mut job = job_with(vec![
            square(0.2, 0, PathKind::Outer),
            PathSeg {
                pts: vec![[100.0, 100.0, 0.2], [110.0, 100.0, 0.2]],
                kind: PathKind::Outer,
                layer: 0,
                rank: 0,
                closed: false,
            },
        ]);
        job.profile.hop_mm = 0.4;
        let (text, stats) = job.render(&never).unwrap();
        assert!(text.contains("Z0.6000"));
        assert_eq!(stats.retractions, 1);
        assert!(
            text.lines()
                .any(|l| l.starts_with("G1 E") && l.contains("F1800"))
        );
    }

    #[test]
    fn split_limits_length_and_keeps_total_length() {
        let mut job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        job.apply_split(15.0, 0.0, &never).unwrap();
        assert!(job.paths.len() >= 6);
        let total: f64 = job.paths.iter().map(PathSeg::length).sum();
        assert!((total - 80.0).abs() < 1e-6, "{total}");
        assert!(job.paths.iter().all(|p| p.length() <= 15.0 + 1e-6));
        assert!(job.apply_split(0.1, 0.0, &never).is_err());
    }

    #[test]
    fn sort_nearest_starts_near_head_and_never_crosses_layers() {
        let far = PathSeg {
            pts: vec![[100.0, 100.0, 0.2], [110.0, 100.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let near = PathSeg {
            pts: vec![[10.0, 10.0, 0.2], [20.0, 10.0, 0.2]],
            kind: PathKind::Outer,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let upper = PathSeg {
            pts: vec![[0.0, 0.0, 0.4], [1.0, 0.0, 0.4]],
            kind: PathKind::Outer,
            layer: 1,
            rank: 0,
            closed: false,
        };
        let mut job = job_with(vec![far, upper.clone(), near]);
        job.sort_nearest(&never).unwrap();
        assert_eq!(job.paths.len(), 3);
        assert_eq!(job.paths[2].layer, upper.layer);
        // Layer 0 is two paths in some order; the second begins at the closest end.
        assert_eq!(job.paths[0].layer, 0);
        assert_eq!(job.paths[1].layer, 0);
    }

    #[test]
    fn sort_reverses_open_path_when_the_end_is_nearer() {
        let a = PathSeg {
            pts: vec![[0.0, 0.0, 0.2], [10.0, 0.0, 0.2]],
            kind: PathKind::Infill,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let b = PathSeg {
            pts: vec![[50.0, 0.0, 0.2], [11.0, 0.0, 0.2]],
            kind: PathKind::Infill,
            layer: 0,
            rank: 0,
            closed: false,
        };
        let mut job = job_with(vec![a, b]);
        job.sort_nearest(&never).unwrap();
        assert_eq!(job.paths[1].pts.first().map(|p| p[0]), Some(11.0));
    }

    #[test]
    fn nonplanar_changes_z_after_start_layer_only() {
        let mut job = job_with(vec![
            square(0.2, 0, PathKind::Outer),
            square(0.4, 3, PathKind::Outer),
        ]);
        job.apply_nonplanar(
            Nonplanar {
                mode: 0,
                amplitude: 2.0,
                wavelength: 20.0,
                phase_deg: 0.0,
                start_layer: 3,
                fade_layers: 1,
            },
            &never,
        )
        .unwrap();
        assert!(job.paths[0].pts.iter().all(|p| (p[2] - 0.2).abs() < 1e-12));
        let zs: Vec<f64> = job.paths[1].pts.iter().map(|p| p[2]).collect();
        assert!(zs.iter().any(|z| (*z - 0.4).abs() > 0.5));
        assert!(zs.iter().all(|z| (*z - 0.4).abs() <= 2.0 + 1e-9));
    }

    #[test]
    fn texture_keeps_closed_wall_seamless_and_bounded() {
        let mut job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        job.apply_texture(
            Texture {
                mode: 0,
                amplitude: 1.0,
                wavelength: 10.0,
                z_wavelength: 0.0,
                twist_deg: 0.0,
            },
            &never,
        )
        .unwrap();
        let path = &job.paths[0];
        assert!(path.pts.len() > 40);
        // Outward only by at most the amplitude (plus corner slack).
        for p in &path.pts {
            assert!(p[0] > 20.0 - 2.5 && p[0] < 40.0 + 2.5);
        }
        assert!(path.pts.iter().all(|p| p.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn script_validation_rejects_control_characters_and_long_text() {
        assert!(validate_script("G28\nM104 S200").is_ok());
        assert!(validate_script("G28\u{0}").is_err());
        assert!(validate_script(&"G".repeat(MAX_SCRIPT_BYTES + 1)).is_err());
        assert!(validate_script(&"G".repeat(400)).is_err());
    }

    #[test]
    fn preview_colours_by_speed_and_respects_the_limit() {
        let mut job = job_with(vec![
            square(0.2, 0, PathKind::Outer),
            square(0.4, 1, PathKind::Infill),
        ]);
        job.preview = Some(Preview {
            color_by: ColorBy::Speed,
            show_travel: true,
        });
        let all = job.preview_segments(1000);
        assert!(all.len() >= 8);
        assert!(
            all.iter()
                .map(|s| s.2)
                .collect::<std::collections::HashSet<_>>()
                .len()
                >= 2
        );
        let few = job.preview_segments(3);
        assert!(few.len() <= 5);
    }

    #[test]
    fn clay_job_has_no_heaters_or_fan() {
        let mut job = GcodeJob::new(Printer::from_preset(Preset::TronxyMoore1Clay));
        job.paths = vec![square(1.0, 0, PathKind::Outer)];
        let (text, _) = job.render(&never).unwrap();
        assert!(!text.contains("M104") && !text.contains("M140") && !text.contains("M106"));
        assert!(text.contains("M83") && text.contains("G92 E0"));
    }

    #[test]
    fn hostile_job_never_panics() {
        let mut job = job_with(vec![PathSeg {
            pts: vec![],
            kind: PathKind::Outer,
            layer: u32::MAX,
            rank: 0,
            closed: true,
        }]);
        assert!(job.check(&never).is_ok());
        assert!(job.stats(&never).is_ok());
        assert!(job.render(&never).is_err());
        job.paths.push(PathSeg {
            pts: vec![[1e300, 0.0, 0.0], [0.0, f64::INFINITY, 0.0]],
            kind: PathKind::Spiral,
            layer: 0,
            rank: 0,
            closed: true,
        });
        assert!(job.render(&never).is_err());
        let _ = job.preview_segments(10);
        let _ = job.summary();
    }

    #[test]
    fn preflight_rejects_generated_out_of_bounds_travel() {
        let job = job_with(vec![square(179.0, 0, PathKind::Outer)]);
        assert!(job.render(&never).is_err());

        let mut job = job_with(vec![
            square(0.2, 0, PathKind::Outer),
            square(179.0, 1, PathKind::Outer),
        ]);
        job.profile.hop_mm = 3.0;
        assert!(job.render(&never).is_err());
        job.profile.hop_mm = 0.0;
        assert!(!job.check(&never).unwrap().iter().any(|i| i.error));
    }

    #[test]
    fn preflight_rejects_degenerate_paths_and_bad_flow_limits() {
        let mut path = square(0.2, 0, PathKind::Outer);
        path.pts.fill([20.0, 20.0, 0.2]);
        let job = job_with(vec![path]);
        assert!(job.render(&never).is_err());

        let mut job = job_with(vec![square(0.2, 0, PathKind::Outer)]);
        for invalid in [f64::NAN, f64::INFINITY, -1.0] {
            job.profile.max_flow_mm3_s = invalid;
            assert!(job.render(&never).is_err());
        }
        job.profile.max_flow_mm3_s = 0.0;
        assert!(job.render(&never).is_ok());
        assert!(job.check(&|| true).is_err());
    }

    #[test]
    fn preflight_diagnostic_handles_max_layer() {
        let mut job = job_with(vec![square(0.2, u32::MAX, PathKind::Outer)]);
        job.paths[0].pts[0][0] = f64::NAN;
        let issues = job.check(&never).unwrap();
        assert!(
            issues
                .iter()
                .any(|i| i.error && i.text.contains("4294967296"))
        );
    }

    #[test]
    fn cancel_stops_the_walk() {
        let job = job_with(vec![square(0.2, 0, PathKind::Outer); 5000]);
        assert!(job.stats(&|| true).is_err());
    }
}
