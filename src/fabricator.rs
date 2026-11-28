//! Published machine envelopes and conservative checks for the G-code subset
//! emitted by NODES. A successful check is not a firmware or collision certificate.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    A1Mini,
    A1,
    P1S,
    X1Carbon,
    Creator5,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MachineEnvelope {
    /// Published nominal build volume, NOT the physical bed or full usable region.
    pub nominal_mm: [f64; 3],
    pub nozzle_max_c: f64,
    /// Conservative ceiling where published regional specifications differ.
    pub bed_ceiling_c: f64,
    pub travel_max_mm_s: f64,
    pub print_max_mm_s: f64,
    pub standard_nozzle_mm: f64,
}

impl Machine {
    pub const fn envelope(self) -> MachineEnvelope {
        match self {
            Self::A1Mini => envelope(180.0, 300.0, 80.0, 500.0, 500.0),
            Self::A1 => envelope(256.0, 300.0, 80.0, 500.0, 500.0),
            Self::P1S => envelope(256.0, 300.0, 100.0, 500.0, 500.0),
            Self::X1Carbon => envelope(256.0, 300.0, 110.0, 500.0, 500.0),
            Self::Creator5 => envelope(256.0, 320.0, 120.0, 600.0, 300.0),
        }
    }
}

const fn envelope(
    side: f64,
    nozzle: f64,
    bed: f64,
    travel: f64,
    print: f64,
) -> MachineEnvelope {
    MachineEnvelope {
        nominal_mm: [side; 3],
        nozzle_max_c: nozzle,
        bed_ceiling_c: bed,
        travel_max_mm_s: travel,
        print_max_mm_s: print,
        standard_nozzle_mm: 0.4,
    }
}

impl MachineEnvelope {
    pub fn check_bed(self, bed: [f64; 3]) -> Result<(), String> {
        if bed
            .iter()
            .zip(self.nominal_mm)
            .any(|(&v, max)| !v.is_finite() || v <= 0.0 || v > max)
        {
            return Err("指定範囲が機種の公称造形寸法を超えています。".into());
        }
        Ok(())
    }

    pub fn check_settings(
        self,
        nozzle: f64,
        bed: f64,
        print: f64,
        travel: f64,
    ) -> Result<(), String> {
        for (value, maximum) in [
            (nozzle, self.nozzle_max_c),
            (bed, self.bed_ceiling_c),
            (print, self.print_max_mm_s),
            (travel, self.travel_max_mm_s),
        ] {
            if !value.is_finite() || value < 0.0 || value > maximum {
                return Err("温度または速度が機種別の上限を超えています。".into());
            }
        }
        Ok(())
    }
}

/// Explicit limits for static inspection only. A generic machine has no verified
/// capabilities, so callers must not interpret this as approval for actual use.
#[derive(Clone, Copy, Debug)]
pub struct InspectionLimits {
    pub bed_mm: [f64; 3],
    pub nozzle_max_c: f64,
    pub bed_max_c: f64,
    pub travel_max_mm_s: f64,
    pub print_max_mm_s: f64,
}

/// Inspect the complete generated file, including custom header/footer.
/// Deliberately rejects commands outside the supported grammar. No printer I/O.
/// G28 is modeled as nominal zero; actual homing, keep-outs, offsets, temperature
/// readiness, acceleration and toolhead clearance require a real machine profile.
pub fn inspect_gcode(
    text: &str,
    limits: InspectionLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    if text.len() > 64 * 1024 * 1024
        || limits.bed_mm.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || [
            limits.nozzle_max_c,
            limits.bed_max_c,
            limits.travel_max_mm_s,
            limits.print_max_mm_s,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err("G-code または検査範囲が無効です。".into());
    }
    let mut xyz = [0.0_f64; 3];
    let mut homed = false;
    let mut e = 0.0_f64;
    let mut e_set = false;
    let mut xyz_relative = false;
    let mut e_relative = false;
    let mut mm = true;
    let mut feed = 0.0_f64;
    for (i, line) in text.lines().enumerate() {
        if i % 1024 == 0 && cancel() {
            return Err("計算を停止しました。".into());
        }
        let fail = |reason: &str| format!("G-code {} 行目: {reason}", i + 1);
        let mut words = line.split(';').next().unwrap_or("").split_ascii_whitespace();
        let Some(command) = words.next() else {
            continue;
        };
        let command = command.to_ascii_uppercase();
        let mut params = [None; 26];
        for word in words {
            let Some(&key) = word.as_bytes().first() else {
                return Err(fail("空のパラメータ"));
            };
            let key = key.to_ascii_uppercase();
            if !key.is_ascii_uppercase() {
                return Err(fail("未知のパラメータ"));
            }
            let slot = &mut params[usize::from(key - b'A')];
            let value = word
                .get(1..)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .ok_or_else(|| fail("非有限または不正な数値"))?;
            if slot.replace(value).is_some() {
                return Err(fail("重複パラメータ"));
            }
        }
        let param = |key: u8| params[usize::from(key - b'A')];
        let only = |allowed: &[u8]| {
            params.iter().enumerate().all(|(i, v)| {
                v.is_none() || allowed.contains(&(b'A' + i as u8))
            })
        };
        match command.as_str() {
            "G20" | "G21" | "G90" | "G91" | "M82" | "M83" | "G28" | "M84" | "M107"
                if only(b"") =>
            {
                match command.as_str() {
                    "G20" => mm = false,
                    "G21" => mm = true,
                    "G90" | "G91" => {
                        xyz_relative = command == "G91";
                        // Conservative Marlin-style reset; NODES sets M82/M83 after G90.
                        e_relative = xyz_relative;
                    }
                    "M82" => e_relative = false,
                    "M83" => e_relative = true,
                    "G28" => {
                        xyz = [0.0; 3];
                        homed = true;
                    }
                    "M84" => homed = false,
                    _ => {}
                }
            }
            "G92" if only(b"E") && param(b'E') == Some(0.0) => {
                e = 0.0;
                e_set = true;
            }
            "G0" | "G1" if only(b"XYZEF") => {
                if !mm || !homed {
                    return Err(fail("移動には mm 単位と全軸ホームが必要です"));
                }
                let prev = xyz;
                for (axis, key) in b"XYZ".iter().enumerate() {
                    if let Some(v) = param(*key) {
                        xyz[axis] = if xyz_relative { xyz[axis] + v } else { v };
                    }
                    if !(0.0..=limits.bed_mm[axis]).contains(&xyz[axis]) {
                        return Err(fail("移動が検査範囲外です"));
                    }
                }
                if let Some(v) = param(b'F') {
                    feed = v / 60.0;
                }
                let mut delta_e = 0.0;
                if let Some(v) = param(b'E') {
                    if !e_set {
                        return Err(fail("押出座標の初期化がありません"));
                    }
                    delta_e = if e_relative { v } else { v - e };
                    e += delta_e;
                    if !e.is_finite() || !delta_e.is_finite() || e.abs() > 1e9 {
                        return Err(fail("異常な押出座標"));
                    }
                    let distance = (xyz[0] - prev[0])
                        .hypot(xyz[1] - prev[1])
                        .hypot(xyz[2] - prev[2]);
                    // Loose sanity bound, not a material-specific flow guarantee.
                    if delta_e.abs() > 30.0 + 4.0 * distance {
                        return Err(fail("異常な単一移動の押出量"));
                    }
                }
                let maximum = if delta_e > 0.0 && xyz != prev {
                    limits.print_max_mm_s
                } else {
                    limits.travel_max_mm_s
                };
                if !(feed > 0.0 && feed <= maximum) {
                    return Err(fail("移動速度が範囲外です"));
                }
            }
            "M104" | "M109" | "M140" | "M190" if only(b"S") => {
                let max = if command == "M104" || command == "M109" {
                    limits.nozzle_max_c
                } else {
                    limits.bed_max_c
                };
                if !param(b'S').is_some_and(|v| (0.0..=max).contains(&v)) {
                    return Err(fail("温度が機種別の上限を超えています"));
                }
            }
            "M106" if only(b"S") && param(b'S').is_some_and(|v| (0.0..=255.0).contains(&v)) => {}
            "G4" if only(b"P") && param(b'P').is_some_and(|v| (0.0..=60_000.0).contains(&v)) => {}
            _ => return Err(fail("未対応・未確認の命令またはパラメータです")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> InspectionLimits {
        InspectionLimits {
            bed_mm: [180.0; 3],
            nozzle_max_c: 300.0,
            bed_max_c: 80.0,
            travel_max_mm_s: 500.0,
            print_max_mm_s: 500.0,
        }
    }

    #[test]
    fn published_envelopes_and_temperature_limits() {
        assert_eq!(Machine::Creator5.envelope().nominal_mm, [256.0; 3]);
        let a1 = Machine::A1Mini.envelope();
        assert!(a1.check_bed([181.0, 180.0, 180.0]).is_err());
        assert!(a1.check_settings(220.0, 81.0, 40.0, 150.0).is_err());
        assert!(a1.check_settings(220.0, 60.0, 40.0, 150.0).is_ok());
    }

    #[test]
    fn modal_moves_and_rejections() {
        let prefix = "G21\nG28\nG90\nM83\nG92 E0\nG1 X10 Y10 Z0.2 E0.2 F600\n";
        assert!(inspect_gcode(prefix, limits(), &|| false).is_ok());
        for suffix in [
            "G91\nG1 X171",
            "G1 Z-1",
            "G1 XNaN",
            "G1 X20 F30001",
            "G1 E9999",
            "M190 S81",
            "M104 S301",
            "M600",
            "m600",
            "M600X1",
            "M999",
            "G92 X0",
            "G20\nG0 X1",
        ] {
            assert!(
                inspect_gcode(&format!("{prefix}{suffix}"), limits(), &|| false).is_err(),
                "{suffix}"
            );
        }
        assert!(inspect_gcode(prefix, limits(), &|| true).is_err());
    }
}
