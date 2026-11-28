//! Vector icons for node kinds, drawn with the egui painter (no image files).
//!
//! Each kind has a few strokes in a 0..1 square (y grows downward), kept in a table.
//! Kinds without an entry use their category's icon, and unknown categories use the
//! generic one, so node kinds added later still get an icon without code changes here.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use nodes::model::NodeKind;

/// One piece of an icon, in a 0..1 square.
#[derive(Clone, Copy)]
pub enum Part {
    /// Polyline through these points.
    Line(&'static [[f32; 2]]),
    /// Filled dot at this point.
    Dot([f32; 2]),
    /// Circle outline: centre x, centre y, radius.
    Ring([f32; 3]),
}

type Glyph = &'static [Part];

const GENERIC: Glyph = &[Part::Ring([0.5, 0.5, 0.34]), Part::Dot([0.5, 0.5])];

pub(crate) const BOX: Glyph = &[
    Part::Line(&[
        [0.5, 0.12],
        [0.85, 0.3],
        [0.5, 0.48],
        [0.15, 0.3],
        [0.5, 0.12],
    ]),
    Part::Line(&[[0.15, 0.3], [0.15, 0.7], [0.5, 0.88], [0.5, 0.48]]),
    Part::Line(&[[0.85, 0.3], [0.85, 0.7], [0.5, 0.88]]),
];

const MOVE: Glyph = &[
    Part::Line(&[[0.12, 0.5], [0.88, 0.5]]),
    Part::Line(&[[0.65, 0.3], [0.88, 0.5], [0.65, 0.7]]),
];

pub(crate) const EXPORT: Glyph = &[
    Part::Line(&[[0.15, 0.5], [0.15, 0.85], [0.85, 0.85], [0.85, 0.5]]),
    Part::Line(&[[0.5, 0.12], [0.5, 0.65]]),
    Part::Line(&[[0.35, 0.27], [0.5, 0.12], [0.65, 0.27]]),
];

const IMPORT: Glyph = &[
    Part::Line(&[[0.15, 0.5], [0.15, 0.85], [0.85, 0.85], [0.85, 0.5]]),
    Part::Line(&[[0.5, 0.12], [0.5, 0.65]]),
    Part::Line(&[[0.35, 0.5], [0.5, 0.65], [0.65, 0.5]]),
];

/// Fallback icon per category (used when a kind has no icon of its own).
const CATEGORY_GLYPHS: &[(&str, Glyph)] = &[
    (
        "数値",
        &[
            Part::Line(&[[0.1, 0.5], [0.9, 0.5]]),
            Part::Dot([0.62, 0.5]),
        ],
    ),
    (
        "データ",
        &[
            Part::Line(&[[0.2, 0.85], [0.2, 0.6]]),
            Part::Line(&[[0.5, 0.85], [0.5, 0.4]]),
            Part::Line(&[[0.8, 0.85], [0.8, 0.2]]),
        ],
    ),
    (
        "曲線",
        &[Part::Line(&[[0.1, 0.8], [0.45, 0.25], [0.9, 0.7]])],
    ),
    ("立体", BOX),
    ("変形", MOVE),
    (
        "配列",
        &[
            Part::Dot([0.15, 0.5]),
            Part::Dot([0.5, 0.5]),
            Part::Dot([0.85, 0.5]),
        ],
    ),
    ("出力", EXPORT),
];

/// One icon per node kind. Kinds not listed here fall back to their category.
const KIND_GLYPHS: &[(NodeKind, Glyph)] = &[
    (
        NodeKind::ExactTorus,
        &[Part::Ring([0.5, 0.5, 0.4]), Part::Ring([0.5, 0.5, 0.18])],
    ),
    (
        NodeKind::PhysSpringRelax,
        &[
            Part::Line(&[
                [0.1, 0.5],
                [0.28, 0.5],
                [0.36, 0.25],
                [0.5, 0.75],
                [0.64, 0.25],
                [0.72, 0.5],
                [0.9, 0.5],
            ]),
            Part::Dot([0.1, 0.5]),
            Part::Dot([0.9, 0.5]),
        ],
    ),
    (
        NodeKind::PhysHang,
        &[
            Part::Line(&[
                [0.12, 0.2],
                [0.25, 0.55],
                [0.4, 0.75],
                [0.5, 0.78],
                [0.6, 0.75],
                [0.75, 0.55],
                [0.88, 0.2],
            ]),
            Part::Dot([0.12, 0.2]),
            Part::Dot([0.88, 0.2]),
        ],
    ),
    (
        NodeKind::PhysSoapFilm,
        &[
            Part::Ring([0.5, 0.5, 0.38]),
            Part::Line(&[[0.2, 0.62], [0.4, 0.45], [0.6, 0.55], [0.8, 0.38]]),
            Part::Line(&[[0.3, 0.3], [0.45, 0.48], [0.6, 0.62], [0.72, 0.75]]),
        ],
    ),
    (
        NodeKind::PhysDrape,
        &[
            Part::Ring([0.5, 0.62, 0.22]),
            Part::Line(&[
                [0.1, 0.35],
                [0.3, 0.3],
                [0.5, 0.25],
                [0.7, 0.3],
                [0.9, 0.35],
            ]),
            Part::Line(&[[0.15, 0.88], [0.85, 0.88]]),
        ],
    ),
    (
        NodeKind::PhysMinimalSurface,
        &[
            Part::Line(&[[0.12, 0.3], [0.5, 0.5], [0.88, 0.3]]),
            Part::Line(&[[0.12, 0.7], [0.5, 0.5], [0.88, 0.7]]),
            Part::Line(&[[0.12, 0.3], [0.12, 0.7]]),
            Part::Line(&[[0.88, 0.3], [0.88, 0.7]]),
        ],
    ),
    (
        NodeKind::MeshLoop,
        &[
            Part::Line(&[[0.5, 0.1], [0.9, 0.85], [0.1, 0.85], [0.5, 0.1]]),
            Part::Line(&[[0.3, 0.475], [0.7, 0.475], [0.5, 0.85], [0.3, 0.475]]),
        ],
    ),
    (
        NodeKind::MeshSplitFaces,
        &[
            Part::Line(&[[0.5, 0.1], [0.9, 0.85], [0.1, 0.85], [0.5, 0.1]]),
            Part::Line(&[[0.5, 0.1], [0.5, 0.6], [0.9, 0.85]]),
            Part::Line(&[[0.5, 0.6], [0.1, 0.85]]),
        ],
    ),
    (
        NodeKind::MeshTaubin,
        &[
            Part::Line(&[
                [0.1, 0.55],
                [0.2, 0.35],
                [0.3, 0.65],
                [0.4, 0.35],
                [0.5, 0.65],
            ]),
            Part::Line(&[[0.5, 0.65], [0.7, 0.45], [0.9, 0.45]]),
        ],
    ),
    (
        NodeKind::MeshRemesh,
        &[
            Part::Line(&[
                [0.5, 0.1],
                [0.88, 0.3],
                [0.88, 0.7],
                [0.5, 0.9],
                [0.12, 0.7],
                [0.12, 0.3],
                [0.5, 0.1],
            ]),
            Part::Line(&[[0.12, 0.3], [0.5, 0.5], [0.88, 0.3]]),
            Part::Line(&[[0.5, 0.5], [0.5, 0.9]]),
            Part::Line(&[[0.12, 0.7], [0.5, 0.5], [0.88, 0.7]]),
        ],
    ),
    (
        NodeKind::MeshFillHoles,
        &[
            Part::Line(&[
                [0.12, 0.12],
                [0.88, 0.12],
                [0.88, 0.88],
                [0.12, 0.88],
                [0.12, 0.12],
            ]),
            Part::Line(&[[0.12, 0.12], [0.88, 0.88]]),
            Part::Line(&[[0.88, 0.12], [0.12, 0.88]]),
            Part::Dot([0.5, 0.5]),
        ],
    ),
    (
        NodeKind::MeshThicken,
        &[
            Part::Line(&[[0.1, 0.45], [0.35, 0.2], [0.65, 0.2], [0.9, 0.45]]),
            Part::Line(&[[0.1, 0.8], [0.35, 0.55], [0.65, 0.55], [0.9, 0.8]]),
            Part::Line(&[[0.1, 0.45], [0.1, 0.8]]),
            Part::Line(&[[0.9, 0.45], [0.9, 0.8]]),
        ],
    ),
    (
        NodeKind::BioSpaceColonization,
        &[
            Part::Line(&[[0.5, 0.9], [0.5, 0.6], [0.32, 0.42], [0.16, 0.2]]),
            Part::Line(&[[0.5, 0.6], [0.72, 0.38], [0.82, 0.16]]),
            Part::Line(&[[0.32, 0.42], [0.35, 0.16]]),
            Part::Line(&[[0.72, 0.38], [0.92, 0.32]]),
        ],
    ),
    (
        NodeKind::BioCoiledShell,
        &[
            Part::Line(&[
                [0.87, 0.67],
                [0.83, 0.29],
                [0.58, 0.13],
                [0.28, 0.18],
                [0.13, 0.42],
                [0.22, 0.66],
                [0.46, 0.73],
                [0.66, 0.58],
                [0.63, 0.38],
                [0.46, 0.32],
                [0.35, 0.43],
                [0.43, 0.53],
            ]),
            Part::Line(&[[0.87, 0.67], [0.69, 0.87], [0.46, 0.73]]),
        ],
    ),
    (
        NodeKind::MeshOffset,
        &[
            Part::Line(&[[0.1, 0.8], [0.35, 0.5], [0.65, 0.5], [0.9, 0.8]]),
            Part::Line(&[[0.1, 0.5], [0.35, 0.2], [0.65, 0.2], [0.9, 0.5]]),
            Part::Line(&[[0.5, 0.45], [0.5, 0.25]]),
            Part::Line(&[[0.42, 0.33], [0.5, 0.25], [0.58, 0.33]]),
        ],
    ),
    (
        NodeKind::Number,
        &[
            Part::Line(&[[0.1, 0.5], [0.9, 0.5]]),
            Part::Dot([0.62, 0.5]),
        ],
    ),
    (
        NodeKind::Add,
        &[
            Part::Line(&[[0.5, 0.15], [0.5, 0.85]]),
            Part::Line(&[[0.15, 0.5], [0.85, 0.5]]),
        ],
    ),
    (
        NodeKind::Multiply,
        &[
            Part::Line(&[[0.2, 0.2], [0.8, 0.8]]),
            Part::Line(&[[0.8, 0.2], [0.2, 0.8]]),
        ],
    ),
    (
        NodeKind::Divide,
        &[
            Part::Line(&[[0.15, 0.5], [0.85, 0.5]]),
            Part::Dot([0.5, 0.25]),
            Part::Dot([0.5, 0.75]),
        ],
    ),
    (
        NodeKind::Subtract,
        &[Part::Line(&[[0.15, 0.5], [0.85, 0.5]])],
    ),
    (
        NodeKind::Power,
        &[Part::Line(&[[0.15, 0.85], [0.5, 0.75], [0.85, 0.2]])],
    ),
    (
        NodeKind::Abs,
        &[
            Part::Line(&[[0.3, 0.12], [0.3, 0.88]]),
            Part::Line(&[[0.7, 0.12], [0.7, 0.88]]),
        ],
    ),
    (
        NodeKind::Min,
        &[
            Part::Line(&[[0.15, 0.2], [0.5, 0.5], [0.85, 0.2]]),
            Part::Line(&[[0.15, 0.85], [0.85, 0.85]]),
        ],
    ),
    (
        NodeKind::Max,
        &[
            Part::Line(&[[0.15, 0.8], [0.5, 0.5], [0.85, 0.8]]),
            Part::Line(&[[0.15, 0.15], [0.85, 0.15]]),
        ],
    ),
    (
        NodeKind::Modulo,
        &[Part::Line(&[
            [0.1, 0.7],
            [0.3, 0.3],
            [0.3, 0.7],
            [0.5, 0.3],
            [0.5, 0.7],
            [0.7, 0.3],
            [0.7, 0.7],
            [0.9, 0.3],
        ])],
    ),
    (
        NodeKind::Sine,
        &[Part::Line(&[
            [0.08, 0.5],
            [0.25, 0.15],
            [0.42, 0.5],
            [0.58, 0.85],
            [0.75, 0.5],
            [0.92, 0.15],
        ])],
    ),
    (
        NodeKind::Series,
        &[
            Part::Line(&[[0.2, 0.85], [0.2, 0.65]]),
            Part::Line(&[[0.4, 0.85], [0.4, 0.5]]),
            Part::Line(&[[0.6, 0.85], [0.6, 0.35]]),
            Part::Line(&[[0.8, 0.85], [0.8, 0.2]]),
        ],
    ),
    (
        NodeKind::Range,
        &[
            Part::Line(&[[0.12, 0.5], [0.88, 0.5]]),
            Part::Line(&[[0.12, 0.3], [0.12, 0.7]]),
            Part::Line(&[[0.88, 0.3], [0.88, 0.7]]),
            Part::Dot([0.5, 0.5]),
        ],
    ),
    (
        NodeKind::Point,
        &[Part::Dot([0.5, 0.5]), Part::Ring([0.5, 0.5, 0.25])],
    ),
    (
        NodeKind::Circle,
        &[Part::Ring([0.5, 0.5, 0.36]), Part::Dot([0.5, 0.5])],
    ),
    (
        NodeKind::Polygon,
        &[Part::Line(&[
            [0.5, 0.1],
            [0.85, 0.3],
            [0.85, 0.7],
            [0.5, 0.9],
            [0.15, 0.7],
            [0.15, 0.3],
            [0.5, 0.1],
        ])],
    ),
    (
        NodeKind::Star,
        &[Part::Line(&[
            [0.5, 0.1],
            [0.606, 0.374],
            [0.9, 0.39],
            [0.671, 0.576],
            [0.747, 0.86],
            [0.5, 0.7],
            [0.253, 0.86],
            [0.329, 0.576],
            [0.1, 0.39],
            [0.394, 0.374],
            [0.5, 0.1],
        ])],
    ),
    (
        NodeKind::Rectangle,
        &[Part::Line(&[
            [0.2, 0.25],
            [0.8, 0.25],
            [0.8, 0.75],
            [0.2, 0.75],
            [0.2, 0.25],
        ])],
    ),
    (
        NodeKind::Helix,
        &[Part::Line(&[
            [0.2, 0.1],
            [0.8, 0.25],
            [0.2, 0.45],
            [0.8, 0.65],
            [0.2, 0.85],
        ])],
    ),
    (
        NodeKind::Extrude,
        &[
            Part::Line(&[[0.5, 0.08], [0.5, 0.5]]),
            Part::Line(&[[0.35, 0.23], [0.5, 0.08], [0.65, 0.23]]),
            Part::Line(&[
                [0.5, 0.45],
                [0.82, 0.62],
                [0.5, 0.8],
                [0.18, 0.62],
                [0.5, 0.45],
            ]),
        ],
    ),
    (
        NodeKind::Loft,
        &[Part::Line(&[
            [0.3, 0.15],
            [0.7, 0.15],
            [0.9, 0.85],
            [0.1, 0.85],
            [0.3, 0.15],
        ])],
    ),
    (
        NodeKind::Revolve,
        &[
            Part::Line(&[[0.5, 0.08], [0.5, 0.92]]),
            Part::Line(&[[0.3, 0.2], [0.7, 0.35], [0.7, 0.65], [0.3, 0.8]]),
        ],
    ),
    (
        NodeKind::Vase,
        &[Part::Line(&[
            [0.35, 0.1],
            [0.65, 0.1],
            [0.6, 0.35],
            [0.8, 0.55],
            [0.75, 0.88],
            [0.25, 0.88],
            [0.2, 0.55],
            [0.4, 0.35],
            [0.35, 0.1],
        ])],
    ),
    (
        NodeKind::Sphere,
        &[
            Part::Ring([0.5, 0.5, 0.36]),
            Part::Line(&[[0.14, 0.5], [0.5, 0.6], [0.86, 0.5]]),
        ],
    ),
    (NodeKind::Box, BOX),
    (
        NodeKind::GenesisScopes,
        &[
            Part::Line(&[[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9], [0.1, 0.1]]),
            Part::Line(&[[0.45, 0.1], [0.45, 0.9]]),
            Part::Line(&[[0.45, 0.58], [0.9, 0.58]]),
            Part::Line(&[[0.1, 0.38], [0.45, 0.38]]),
        ],
    ),
    (
        NodeKind::GenesisScopeIds,
        &[
            Part::Line(&[[0.5, 0.15], [0.25, 0.48], [0.12, 0.82]]),
            Part::Line(&[[0.25, 0.48], [0.38, 0.82]]),
            Part::Line(&[[0.5, 0.15], [0.75, 0.48], [0.62, 0.82]]),
            Part::Line(&[[0.75, 0.48], [0.88, 0.82]]),
            Part::Dot([0.5, 0.15]),
        ],
    ),
    (NodeKind::GenesisRelief, BOX),
    (
        NodeKind::Cylinder,
        &[
            Part::Line(&[
                [0.2, 0.25],
                [0.2, 0.75],
                [0.5, 0.9],
                [0.8, 0.75],
                [0.8, 0.25],
            ]),
            Part::Line(&[
                [0.2, 0.25],
                [0.5, 0.4],
                [0.8, 0.25],
                [0.5, 0.1],
                [0.2, 0.25],
            ]),
        ],
    ),
    (
        NodeKind::Cone,
        &[
            Part::Line(&[[0.5, 0.1], [0.82, 0.85], [0.18, 0.85], [0.5, 0.1]]),
            Part::Line(&[[0.18, 0.85], [0.5, 0.97], [0.82, 0.85]]),
        ],
    ),
    (
        NodeKind::Torus,
        &[Part::Ring([0.5, 0.5, 0.4]), Part::Ring([0.5, 0.5, 0.18])],
    ),
    (
        NodeKind::Plane,
        &[Part::Line(&[
            [0.12, 0.3],
            [0.8, 0.2],
            [0.88, 0.7],
            [0.2, 0.8],
            [0.12, 0.3],
        ])],
    ),
    (NodeKind::Move, MOVE),
    (
        NodeKind::Rotate,
        &[
            Part::Ring([0.5, 0.5, 0.34]),
            Part::Line(&[[0.76, 0.22], [0.86, 0.42], [0.66, 0.44]]),
        ],
    ),
    (
        NodeKind::Scale,
        &[
            Part::Line(&[[0.18, 0.82], [0.82, 0.18]]),
            Part::Line(&[[0.5, 0.18], [0.82, 0.18], [0.82, 0.5]]),
        ],
    ),
    (
        NodeKind::ScaleXyz,
        &[
            Part::Line(&[
                [0.15, 0.25],
                [0.85, 0.25],
                [0.85, 0.75],
                [0.15, 0.75],
                [0.15, 0.25],
            ]),
            Part::Line(&[[0.15, 0.9], [0.85, 0.9]]),
            Part::Line(&[[0.92, 0.25], [0.92, 0.75]]),
        ],
    ),
    (
        NodeKind::Mirror,
        &[
            Part::Line(&[[0.5, 0.08], [0.5, 0.92]]),
            Part::Line(&[[0.12, 0.25], [0.4, 0.5], [0.12, 0.75], [0.12, 0.25]]),
            Part::Line(&[[0.88, 0.25], [0.6, 0.5], [0.88, 0.75], [0.88, 0.25]]),
        ],
    ),
    (
        NodeKind::Twist,
        &[
            Part::Line(&[[0.2, 0.1], [0.8, 0.35], [0.2, 0.6], [0.8, 0.85]]),
            Part::Line(&[[0.8, 0.1], [0.2, 0.35], [0.8, 0.6], [0.2, 0.85]]),
        ],
    ),
    (
        NodeKind::Wave,
        &[
            Part::Line(&[[0.1, 0.3], [0.3, 0.2], [0.5, 0.3], [0.7, 0.4], [0.9, 0.3]]),
            Part::Line(&[[0.1, 0.7], [0.3, 0.6], [0.5, 0.7], [0.7, 0.8], [0.9, 0.7]]),
        ],
    ),
    (
        NodeKind::LinearArray,
        &[
            Part::Dot([0.15, 0.5]),
            Part::Dot([0.5, 0.5]),
            Part::Dot([0.85, 0.5]),
        ],
    ),
    (
        NodeKind::RadialArray,
        &[
            Part::Dot([0.5, 0.14]),
            Part::Dot([0.81, 0.32]),
            Part::Dot([0.81, 0.68]),
            Part::Dot([0.5, 0.86]),
            Part::Dot([0.19, 0.68]),
            Part::Dot([0.19, 0.32]),
        ],
    ),
    (
        NodeKind::Merge,
        &[
            Part::Line(&[[0.15, 0.15], [0.5, 0.5], [0.15, 0.85]]),
            Part::Line(&[[0.5, 0.5], [0.9, 0.5]]),
        ],
    ),
    (
        NodeKind::Flatten,
        &[
            Part::Line(&[[0.15, 0.25], [0.85, 0.25]]),
            Part::Line(&[[0.15, 0.5], [0.85, 0.5]]),
            Part::Line(&[[0.15, 0.75], [0.85, 0.75]]),
        ],
    ),
    (
        NodeKind::Graft,
        &[
            Part::Line(&[[0.5, 0.15], [0.5, 0.45]]),
            Part::Line(&[[0.5, 0.45], [0.2, 0.8]]),
            Part::Line(&[[0.5, 0.45], [0.8, 0.8]]),
        ],
    ),
    (NodeKind::Output, EXPORT),
    (
        NodeKind::Script,
        &[
            Part::Line(&[[0.35, 0.2], [0.2, 0.5], [0.35, 0.8]]),
            Part::Line(&[[0.65, 0.2], [0.8, 0.5], [0.65, 0.8]]),
        ],
    ),
    (
        NodeKind::ListItem,
        &[
            Part::Line(&[[0.3, 0.25], [0.85, 0.25]]),
            Part::Line(&[[0.3, 0.5], [0.85, 0.5]]),
            Part::Line(&[[0.3, 0.75], [0.85, 0.75]]),
            Part::Dot([0.15, 0.5]),
        ],
    ),
    (
        NodeKind::ListLength,
        &[
            Part::Line(&[[0.3, 0.25], [0.85, 0.25]]),
            Part::Line(&[[0.3, 0.5], [0.85, 0.5]]),
            Part::Line(&[[0.3, 0.75], [0.85, 0.75]]),
            Part::Line(&[[0.12, 0.2], [0.12, 0.8]]),
        ],
    ),
    (
        NodeKind::ReverseList,
        &[
            Part::Line(&[[0.15, 0.35], [0.85, 0.35]]),
            Part::Line(&[[0.7, 0.2], [0.85, 0.35], [0.7, 0.5]]),
            Part::Line(&[[0.85, 0.65], [0.15, 0.65]]),
            Part::Line(&[[0.3, 0.8], [0.15, 0.65], [0.3, 0.5]]),
        ],
    ),
    (
        NodeKind::SortList,
        &[
            Part::Line(&[[0.15, 0.25], [0.35, 0.25]]),
            Part::Line(&[[0.15, 0.5], [0.55, 0.5]]),
            Part::Line(&[[0.15, 0.75], [0.8, 0.75]]),
        ],
    ),
    (
        NodeKind::CullPattern,
        &[
            Part::Dot([0.15, 0.5]),
            Part::Dot([0.85, 0.5]),
            Part::Line(&[[0.42, 0.4], [0.58, 0.6]]),
            Part::Line(&[[0.58, 0.4], [0.42, 0.6]]),
        ],
    ),
    (
        NodeKind::ShiftList,
        &[
            Part::Dot([0.2, 0.5]),
            Part::Dot([0.45, 0.5]),
            Part::Line(&[[0.6, 0.5], [0.9, 0.5]]),
            Part::Line(&[[0.8, 0.4], [0.9, 0.5], [0.8, 0.6]]),
        ],
    ),
    (
        NodeKind::ConcatList,
        &[
            Part::Line(&[[0.1, 0.35], [0.45, 0.35]]),
            Part::Line(&[[0.1, 0.65], [0.45, 0.65]]),
            Part::Line(&[[0.55, 0.5], [0.9, 0.5]]),
        ],
    ),
    (
        NodeKind::SubList,
        &[
            Part::Line(&[[0.15, 0.25], [0.85, 0.25]]),
            Part::Line(&[[0.15, 0.5], [0.85, 0.5]]),
            Part::Line(&[[0.15, 0.75], [0.85, 0.75]]),
            Part::Line(&[[0.35, 0.12], [0.35, 0.88]]),
            Part::Line(&[[0.7, 0.12], [0.7, 0.88]]),
        ],
    ),
    (
        NodeKind::Bounds,
        &[
            Part::Line(&[
                [0.2, 0.25],
                [0.8, 0.25],
                [0.8, 0.75],
                [0.2, 0.75],
                [0.2, 0.25],
            ]),
            Part::Dot([0.2, 0.25]),
            Part::Dot([0.8, 0.75]),
        ],
    ),
    (
        NodeKind::Remap,
        &[
            Part::Line(&[[0.15, 0.15], [0.15, 0.85], [0.85, 0.85]]),
            Part::Line(&[[0.2, 0.75], [0.5, 0.7], [0.85, 0.2]]),
        ],
    ),
    // Sigma: the math sign for an expression.
    (
        NodeKind::Expression,
        &[Part::Line(&[
            [0.8, 0.15],
            [0.2, 0.15],
            [0.55, 0.5],
            [0.2, 0.85],
            [0.8, 0.85],
        ])],
    ),
    (
        NodeKind::Random,
        &[
            Part::Line(&[
                [0.15, 0.15],
                [0.85, 0.15],
                [0.85, 0.85],
                [0.15, 0.85],
                [0.15, 0.15],
            ]),
            Part::Dot([0.35, 0.35]),
            Part::Dot([0.5, 0.5]),
            Part::Dot([0.65, 0.65]),
        ],
    ),
    (
        NodeKind::Populate,
        &[
            Part::Dot([0.2, 0.75]),
            Part::Dot([0.4, 0.55]),
            Part::Dot([0.6, 0.4]),
            Part::Dot([0.8, 0.25]),
        ],
    ),
    (
        NodeKind::Noise,
        &[
            Part::Dot([0.15, 0.2]),
            Part::Dot([0.35, 0.4]),
            Part::Dot([0.6, 0.25]),
            Part::Dot([0.8, 0.5]),
            Part::Dot([0.25, 0.7]),
            Part::Dot([0.55, 0.8]),
            Part::Dot([0.85, 0.85]),
        ],
    ),
    (
        NodeKind::NoiseDeform,
        &[Part::Line(&[
            [0.1, 0.5],
            [0.2, 0.3],
            [0.3, 0.55],
            [0.42, 0.25],
            [0.55, 0.6],
            [0.68, 0.3],
            [0.8, 0.6],
            [0.9, 0.45],
        ])],
    ),
    (
        NodeKind::GraphProfile,
        &[
            Part::Line(&[[0.15, 0.1], [0.15, 0.85], [0.9, 0.85]]),
            Part::Line(&[[0.2, 0.75], [0.45, 0.3], [0.75, 0.55], [0.85, 0.2]]),
        ],
    ),
    (
        NodeKind::Shell,
        &[
            Part::Line(&[
                [0.12, 0.12],
                [0.88, 0.12],
                [0.88, 0.88],
                [0.12, 0.88],
                [0.12, 0.12],
            ]),
            Part::Line(&[[0.3, 0.3], [0.7, 0.3], [0.7, 0.7], [0.3, 0.7], [0.3, 0.3]]),
        ],
    ),
    (
        NodeKind::OverhangCheck,
        &[Part::Line(&[[0.15, 0.85], [0.85, 0.85], [0.85, 0.35]])],
    ),
    (
        NodeKind::Distance,
        &[
            Part::Line(&[[0.18, 0.5], [0.82, 0.5]]),
            Part::Line(&[[0.18, 0.35], [0.18, 0.65]]),
            Part::Line(&[[0.82, 0.35], [0.82, 0.65]]),
        ],
    ),
    (
        NodeKind::PointComponent,
        &[
            Part::Dot([0.5, 0.5]),
            Part::Line(&[[0.15, 0.15], [0.15, 0.85], [0.85, 0.85]]),
        ],
    ),
    (
        NodeKind::Reroute,
        &[
            Part::Line(&[[0.08, 0.5], [0.32, 0.5]]),
            Part::Ring([0.5, 0.5, 0.18]),
            Part::Dot([0.5, 0.5]),
            Part::Line(&[[0.68, 0.5], [0.92, 0.5]]),
        ],
    ),
    (NodeKind::ImportMesh, IMPORT),
    (
        NodeKind::MatterRotatingSquares,
        &[
            Part::Line(&[
                [0.08, 0.35],
                [0.35, 0.08],
                [0.62, 0.35],
                [0.35, 0.62],
                [0.08, 0.35],
            ]),
            Part::Line(&[
                [0.38, 0.65],
                [0.65, 0.38],
                [0.92, 0.65],
                [0.65, 0.92],
                [0.38, 0.65],
            ]),
        ],
    ),
];

/// Draws the icon of `kind` into `rect`. `category` picks the fallback when the kind has no icon.
pub fn paint(painter: &egui::Painter, rect: Rect, kind: NodeKind, category: &str, color: Color32) {
    let glyph = KIND_GLYPHS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, g)| *g)
        .or_else(|| {
            CATEGORY_GLYPHS
                .iter()
                .find(|(c, _)| *c == category)
                .map(|(_, g)| *g)
        })
        .unwrap_or(GENERIC);
    draw(painter, rect, glyph, color);
}

/// Melt 3D icon proportions: a 64-unit box with 2.55 outlines, round caps and joins,
/// and a margin of 6 units. Icons carry no text.
const BOX_UNITS: f32 = 64.0;
const OUTLINE_UNITS: f32 = 2.55;
const MARGIN_UNITS: f32 = 6.0;

/// Outline width in pixels for an icon drawn in a square of `box_px`.
pub(crate) fn outline_width(box_px: f32) -> f32 {
    (box_px * OUTLINE_UNITS / BOX_UNITS).max(1.2)
}

pub(crate) fn draw(painter: &egui::Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let inner = rect.shrink(rect.width() * MARGIN_UNITS / BOX_UNITS);
    let at = |p: [f32; 2]| {
        Pos2::new(
            inner.left() + p[0] * inner.width(),
            inner.top() + p[1] * inner.height(),
        )
    };
    let width = outline_width(rect.width());
    let stroke = Stroke::new(width, color);
    let unit = inner.width();
    for part in glyph {
        match *part {
            Part::Line(points) => {
                painter.line(points.iter().map(|p| at(*p)).collect(), stroke);
                // Round caps and joins.
                for p in points {
                    painter.circle_filled(at(*p), width / 2.0, color);
                }
            }
            Part::Dot(p) => {
                painter.circle_filled(at(p), (unit * 0.075).max(width * 0.8), color);
            }
            Part::Ring(c) => {
                painter.circle_stroke(at([c[0], c[1]]), c[2] * unit, stroke);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_follows_the_64_unit_style_guide() {
        assert!((outline_width(64.0) - 2.55).abs() < 1e-4);
        assert!(outline_width(10.0) >= 1.2);
    }

    #[test]
    fn each_kind_appears_once_and_every_point_stays_in_the_square() {
        let mut seen: Vec<NodeKind> = Vec::new();
        for (kind, glyph) in KIND_GLYPHS {
            assert!(!seen.contains(kind), "icon listed twice");
            seen.push(*kind);
            for part in glyph.iter() {
                let points: Vec<[f32; 2]> = match part {
                    Part::Line(p) => p.to_vec(),
                    Part::Dot(p) => vec![*p],
                    Part::Ring(c) => vec![[c[0], c[1]]],
                };
                for p in points {
                    assert!(p.iter().all(|v| (0.0f32..=1.0f32).contains(v)));
                }
            }
        }
    }
}
