//! Print-preview helpers that do not depend on egui: build volumes, the fit
//! check, the PLA weight estimate and the overhang rule.
//!
//! The overhang rule is the one used by the 張り出しチェック node
//! (`gh_nodes::overhang`). A unit test below checks that both agree.

/// PLA density used for the weight estimate, in g/cm³. The estimate assumes a
/// solid part; infill and shells make the real print lighter.
pub const PLA_DENSITY_G_PER_CM3: f64 = 1.24;

/// Printers offered in the 3D preview. `Off` means no build plate is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Printer {
    #[default]
    Off,
    BambuA1,
    BambuA1Mini,
    Adventurer5M,
    Custom,
}

impl Printer {
    pub const ALL: [Printer; 5] = [
        Printer::Off,
        Printer::BambuA1,
        Printer::BambuA1Mini,
        Printer::Adventurer5M,
        Printer::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Printer::Off => "なし",
            Printer::BambuA1 => "Bambu A1",
            Printer::BambuA1Mini => "Bambu A1 mini",
            Printer::Adventurer5M => "FlashForge Adventurer 5M",
            Printer::Custom => "カスタム",
        }
    }

    /// Build volume `[X, Y, Z]` in mm, or `None` when no printer is chosen.
    pub fn volume(self, custom: [f64; 3]) -> Option<[f64; 3]> {
        match self {
            Printer::Off => None,
            Printer::BambuA1 => Some([256.0; 3]),
            Printer::BambuA1Mini => Some([180.0; 3]),
            Printer::Adventurer5M => Some([220.0; 3]),
            Printer::Custom => Some(custom),
        }
    }
}

/// Size of an axis-aligned box from its min and max corners.
pub fn size_of(min: [f64; 3], max: [f64; 3]) -> [f64; 3] {
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

/// Volume of a box in mm³ (the bounding box, not the part).
pub fn box_volume_mm3(size: [f64; 3]) -> f64 {
    size[0] * size[1] * size[2]
}

pub fn mm3_to_cm3(mm3: f64) -> f64 {
    mm3 / 1000.0
}

/// Estimated PLA weight in grams for a solid of `volume_cm3`.
pub fn pla_weight_g(volume_cm3: f64) -> f64 {
    volume_cm3 * PLA_DENSITY_G_PER_CM3
}

/// `true` when every axis of `size` fits inside `volume`. Orientation is not
/// changed, so a part only fits as it is placed.
pub fn fits_in_volume(size: [f64; 3], volume: [f64; 3]) -> bool {
    exceeding_axes(size, volume).iter().all(|over| !over)
}

/// Per-axis flags: `true` where `size` is larger than `volume`.
pub fn exceeding_axes(size: [f64; 3], volume: [f64; 3]) -> [bool; 3] {
    std::array::from_fn(|i| size[i] > volume[i] + 1e-9)
}

/// Overhang test for one face, using the same rule as `gh_nodes::overhang`.
///
/// `unit_normal` is the face normal from the winding order, normalised. A face
/// is an overhang when it faces downward and its angle `asin(-n.z)` is above
/// `limit_deg` (the limit is clamped to 0..=90). Faces lying on the build plate
/// must be excluded by the caller, as the node does.
pub fn is_overhang_face(unit_normal: [f64; 3], limit_deg: f64) -> bool {
    let limit = limit_deg.clamp(0.0, 90.0).to_radians().sin();
    let down = -unit_normal[2];
    down > 0.0 && down > limit + 1e-12
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gh_nodes;
    use crate::model::Mesh;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn cube_volume_is_side_cubed() {
        assert!(near(box_volume_mm3([2.0, 2.0, 2.0]), 8.0));
        assert!(near(box_volume_mm3([10.0, 10.0, 10.0]), 1000.0));
        assert!(near(mm3_to_cm3(box_volume_mm3([10.0, 10.0, 10.0])), 1.0));
    }

    #[test]
    fn pla_weight_uses_density() {
        assert!(near(pla_weight_g(1.0), 1.24));
        assert!(near(pla_weight_g(10.0), 12.4));
        assert!(near(pla_weight_g(0.0), 0.0));
    }

    #[test]
    fn size_comes_from_min_and_max() {
        assert_eq!(size_of([-1.0, 2.0, 0.0], [3.0, 2.5, 4.0]), [4.0, 0.5, 4.0]);
    }

    #[test]
    fn fit_checks_each_axis() {
        let a1 = Printer::BambuA1.volume([0.0; 3]).unwrap_or([0.0; 3]);
        assert!(fits_in_volume([256.0, 256.0, 256.0], a1));
        assert!(!fits_in_volume([256.5, 10.0, 10.0], a1));
        assert_eq!(
            exceeding_axes([10.0, 10.0, 300.0], a1),
            [false, false, true]
        );
        assert_eq!(Printer::Off.volume([1.0; 3]), None);
        assert_eq!(Printer::BambuA1Mini.volume([1.0; 3]), Some([180.0; 3]));
        assert_eq!(Printer::Adventurer5M.volume([1.0; 3]), Some([220.0; 3]));
        assert_eq!(
            Printer::Custom.volume([5.0, 6.0, 7.0]),
            Some([5.0, 6.0, 7.0])
        );
    }

    #[test]
    fn overhang_rule_follows_the_angle_from_horizontal() {
        // Straight down: always an overhang.
        assert!(is_overhang_face([0.0, 0.0, -1.0], 45.0));
        // Facing up or sideways: never.
        assert!(!is_overhang_face([0.0, 0.0, 1.0], 45.0));
        assert!(!is_overhang_face([1.0, 0.0, 0.0], 45.0));
        // asin(0.5) = 30 degrees: not an overhang at 45, an overhang at 20.
        assert!(!is_overhang_face([0.0, 0.0, -0.5], 45.0));
        assert!(is_overhang_face([0.0, 0.0, -0.5], 20.0));
        // asin(0.866) = 60 degrees: an overhang at 45.
        assert!(is_overhang_face(
            [0.0, 0.0, -(60f64.to_radians().sin())],
            45.0
        ));
    }

    #[test]
    fn overhang_rule_matches_the_overhang_node() {
        // Plate triangle at z = 0 (facing up) plus one downward face at z = 5.
        let mesh = Mesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 5.0],
                [0.0, 1.0, 5.0],
                [1.0, 0.0, 5.0],
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
        };
        let node = gh_nodes::overhang(&mesh, 45.0, &|| false);
        let count = node.map(|r| r[1]).unwrap_or(-1.0);
        assert!(near(count, 1.0));
        // The same face through the shared rule: normal of (3,4,5) is (0,0,-1).
        assert!(is_overhang_face([0.0, 0.0, -1.0], 45.0));
    }
}
