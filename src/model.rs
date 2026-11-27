//! Serializable, renderer-independent graph model. Geometry is evaluated separately.
use crate::cluster::ClusterDef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

pub type NodeId = u64;
pub const MAX_NODES: usize = 16_384;
pub const MAX_ITEMS: usize = 16_384;
pub const MAX_VERTICES: usize = 250_000;
pub const MAX_TRIANGLES: usize = 500_000;
pub const MAX_TOTAL_WEIGHT: usize = 3_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Number,
    Point,
    Curve,
    Mesh,
    Geometry,
    Any,
    Surface,
    Solid,
    DisplayGeometry,
    /// Exact SolidCraft-kernel B-rep (solid or sheet); see `kernel_bridge`.
    Brep,
    /// SubD / T-spline control cage; see `subd`.
    Cage,
    /// A G-code job (printer, settings, toolpaths). See `gcode_job.rs`.
    Gcode,
}
impl DataType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Number => "数値",
            Self::Point => "点",
            Self::Curve => "曲線",
            Self::Mesh => "メッシュ",
            Self::Geometry => "形状",
            Self::Any => "データ",
            Self::Surface => "数学曲面",
            Self::Solid => "B-rep 立体",
            Self::DisplayGeometry => "表示形状",
            Self::Brep => "厳密 B-rep",
            Self::Cage => "SubD ケージ",
            Self::Gcode => "Gコード",
        }
    }
    pub fn accepts(self, source: Self) -> bool {
        self == Self::Any
            || source == Self::Any
            || self == source
            || (self == Self::Geometry && matches!(source, Self::Point | Self::Curve | Self::Mesh))
            || (source == Self::Geometry && matches!(self, Self::Point | Self::Curve | Self::Mesh))
            || (self == Self::DisplayGeometry
                && matches!(
                    source,
                    Self::Geometry
                        | Self::Point
                        | Self::Curve
                        | Self::Mesh
                        | Self::Surface
                        | Self::Solid
                        | Self::Brep
                        | Self::Cage
                ))
            || (source == Self::DisplayGeometry
                && matches!(
                    self,
                    Self::Geometry
                        | Self::Point
                        | Self::Curve
                        | Self::Mesh
                        | Self::Surface
                        | Self::Solid
                        | Self::Brep
                        | Self::Cage
                ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InputSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: DataType,
    pub default: Option<f64>,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub integer: bool,
}
#[derive(Debug)]
pub struct NodeSpec {
    pub label: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub keywords: &'static str,
    pub inputs: &'static [InputSpec],
    pub output: DataType,
}
pub(crate) const fn n(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
    step: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: DataType::Number,
        default: Some(default),
        min,
        max,
        step,
        integer: false,
    }
}
pub(crate) const fn i(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: DataType::Number,
        default: Some(default),
        min,
        max,
        step: 1.,
        integer: true,
    }
}
pub(crate) const fn g(key: &'static str, label: &'static str, kind: DataType) -> InputSpec {
    InputSpec {
        key,
        label,
        kind,
        default: None,
        min: 0.,
        max: 0.,
        step: 0.,
        integer: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Number,
    Add,
    Multiply,
    Divide,
    Subtract,
    Power,
    Abs,
    Min,
    Max,
    Modulo,
    Sine,
    Series,
    Range,
    Point,
    Circle,
    Polygon,
    Star,
    Rectangle,
    Helix,
    Extrude,
    Loft,
    Revolve,
    Vase,
    Sphere,
    Box,
    Cylinder,
    Cone,
    Torus,
    Plane,
    Move,
    Rotate,
    Scale,
    ScaleXyz,
    Mirror,
    Twist,
    Wave,
    LinearArray,
    RadialArray,
    Merge,
    Flatten,
    Graft,
    Output,
    /// User script (see `script.rs`). Added after `Output` to ease merging.
    Script,
    /// Instance of a cluster definition stored in `Graph::clusters` (see `cluster.rs`).
    Cluster,
    // List, number, pattern and print-checking nodes (see `gh_nodes.rs`). Appended at the
    // end so the serde names and the order of the older kinds stay as they were.
    ListItem,
    ListLength,
    ReverseList,
    SortList,
    CullPattern,
    ShiftList,
    ConcatList,
    SubList,
    Bounds,
    Remap,
    /// Typed formula of x, y, z, w; its text is kept in `Node::script`.
    Expression,
    Random,
    Populate,
    Noise,
    NoiseDeform,
    GraphProfile,
    Shell,
    OverhangCheck,
    Distance,
    PointComponent,
    /// Mesh file import (STL or OBJ, see `import.rs`). The file path is kept in `Node::script`.
    ImportMesh,
    SimplifyTree,
    TrimTree,
    PruneTree,
    MergeTree,
    EntwineTree,
    FlipMatrix,
    UnflattenTree,
    PathMapper,
    PartitionList,
    TreeBranch,
    TreeItem,
    TreeStatistics,
    CullNth,
    CullIndex,
    DuplicateData,
    RepeatData,
    MassAddition,
    MassMultiplication,
    ReplaceItems,
    InsertItems,
    MemberIndex,
    RandomReduce,
    Jitter,
    // Mathematics mission: appended to preserve existing serialized node identities.
    Negate,
    Round,
    Floor,
    Ceiling,
    Exponential,
    Sinh,
    Cosh,
    Tanh,
    Degrees,
    Radians,
    Asin,
    Acos,
    Atan,
    SinRadians,
    CosRadians,
    TanRadians,
    Atan2,
    Root,
    Logarithm,
    Factorial,
    Clamp,
    Summation,
    Product,
    Average,
    Median,
    Variance,
    StdDeviation,
    GhRange,
    ConstructDomain,
    DomainEndpoint,
    DomainLength,
    EvaluateDomain,
    DivideDomain,
    DomainIntersection,
    DomainUnion,
    GraphMapper,
    BoolAnd,
    BoolOr,
    BoolXor,
    BoolNot,
    Equals,
    LargerThan,
    SmallerThan,
    SetContains,
    SetUnion,
    SetIntersection,
    SetDifference,
    CreateSet,
    DuplicateDetection,
    ConstructVector,
    VectorComponent,
    UnitX,
    UnitY,
    UnitZ,
    VectorLength,
    Unitize,
    ReverseVector,
    DotProduct,
    CrossProduct,
    VectorAngle,
    VectorAddition,
    VectorSubtraction,
    VectorProjection,
    VectorBetweenPoints,
    RotateVector,
    ClosestPoint,
    PlaneProject,
    PlaneCoordinates,
    PlanePoint,
    PointInPlane,
    PlaneNormal3Pt,
    RandomNormal,
    VectorMultiply,
    VectorDivide,
    VectorAmplitude,
    Lerp,
    SmoothStep,
    BezierValue,
    NormalPdf,
    MatrixTranspose,
    MatrixMultiply,
    MatrixInverse,
    LinearSolve,
    FourierMagnitude,
    PlaneIntersectionPoint,
    PlaneIntersectionDirection,
    PlaneRotateAxis,
    PlaneAlignAxis,
    WorldPlaneAxis,
    ChangeBasis,
    // Procedural design (append-only serialized kinds).
    RectGrid,
    TriangularGrid,
    HexGrid,
    RadialGrid,
    HexPanels,
    TriPanels,
    Voronoi2d,
    ConvexHull2d,
    MultiAttractor,
    CurveAttractor,
    GridArray,
    PointArray,
    Rotate3d,
    Shear,
    Taper,
    FractalTree,
    CylinderWrap,
    Delaunay2d,
    PopulateMesh,
    CurveArray,
    NoiseField,
    LifeGrid,
    // Mesh and fabrication nodes; append-only serialized identifiers.
    MeshArea,
    MeshVolume,
    MeshCenter,
    MeshDimensions,
    MeshMass,
    MeshAudit,
    MeshVertices,
    MeshFaces,
    MeshNormals,
    MeshEdges,
    MeshBoundary,
    MeshClosest,
    MeshFlip,
    MeshWeldExact,
    MeshUnweld,
    MeshUnify,
    MeshSmooth,
    MeshSubdivide,
    MeshCatmullClark,
    MeshOnBed,
    MeshDeleteFace,
    MeshConstruct,
    MeshSelfContact,
    MeshThicknessSample,
    MeshCompact,
    MeshNonManifold,
    // Mathematical curve nodes: appended to preserve legacy serde names.
    CurveLine,
    CurveLineSdl,
    CurvePolyline,
    CurveCircleCnr,
    CurveCircleThree,
    CurveArc,
    CurveArcThree,
    CurveEllipse,
    CurveSpiral,
    CurveBezier,
    CurveNurbs,
    CurveWeightedNurbs,
    CurveInterpolate,
    CurveLength,
    CurveEvaluate,
    CurveEvaluateLength,
    CurveClosestParameter,
    CurveClosestPoint,
    CurveStart,
    CurveEnd,
    CurveMiddle,
    CurveTangent,
    CurveCurvature,
    CurveDomain,
    CurveClosed,
    CurveDivide,
    CurveDivideLength,
    CurveSubcurve,
    CurveShatter,
    CurveReverse,
    CurveControls,
    CurveWeights,
    CurveKnots,
    CurveDegree,
    CurveInsertKnot,
    CurveOffsetPreview,
    CurveFrameX,
    CurveFrameY,
    CurveLineLine,
    CurveLinePlane,
    // Mathematical geometry, appended for stable old node ordering and names.
    PlaneSurface,
    NurbsPanel,
    BilinearSurface,
    CylinderSurface,
    FrustumSurface,
    SphereSurface,
    TorusSurface,
    EvaluateSurface,
    SurfaceDirection,
    SurfaceNormalLine,
    SurfaceArea,
    SurfaceClosestPoint,
    SurfaceIsoCurve,
    DivideSurface,
    SurfaceMesh,
    BrepBox,
    BrepVolume,
    BrepArea,
    BrepIntersection,
    BrepMove,
    BrepFaces,
    BrepEdges,
    BrepVertices,
    BrepMesh,
    NurbsVase,
    RuledSurface,
    SurfaceExtrude,
    BrepUnion,
    BrepDifference,
    BrepOffset,
    SurfaceGaussianCurvature,
    SurfaceMeanCurvature,
    SurfacePrincipalCurvatures,
    // Curve, tube, mesh-repair, transform and gyroid nodes (see `nodes_shape.rs`). Appended
    // after the kinds above; the serde names are the snake_case names of these variants.
    ArcCurve,
    Polyline,
    SmoothCurve,
    OffsetCurve,
    PointOnCurve,
    DivideCurve,
    Pipe,
    Sweep,
    SmoothMesh,
    Subdivide,
    WeldMesh,
    UnifyNormals,
    MeshInfo,
    // Same meaning as `Taper` but base/top scale ports; renamed to avoid a serde-name clash.
    TaperScale,
    Bend,
    // 3D grid; `GridArray` (generative) is the 2D one.
    GridArray3d,
    Gyroid,
    RevolvePoints,
    // Exact NURBS / B-rep / T-spline nodes on the vendored SolidCraft kernel (`nurbs_nodes`).
    ExactBox,
    ExactCylinder,
    ExactSphere,
    ExactExtrude,
    ExactRevolve,
    ExactLoft,
    ExactLoftSurface,
    ExactSweep1,
    ExactSweep2,
    ExactNetworkSurface,
    ExactRebuildCurve,
    ExactCurveCurve,
    ExactCurveBrep,
    ExactBrepBrep,
    ExactUnion,
    ExactDifference,
    ExactIntersection,
    ExactFillet,
    ExactChamfer,
    ExactShell,
    ExactCapHoles,
    ExactBrepMesh,
    ExactMove,
    ExactVolume,
    ExactStepExport,
    ExactStepImport,
    SubdBoxCage,
    SubdCylinderCage,
    SubdSubdivide,
    SubdCrease,
    SubdExtrudeFace,
    SubdToBrep,
    SubdToMesh,
    // G-code nodes (see `gcode_nodes.rs`); appended, serde names are snake_case.
    GcodePrinter,
    GcodeSlice,
    GcodeContour,
    GcodeSpiral,
    GcodeSort,
    GcodeSplit,
    GcodeExtrusion,
    GcodeSpeed,
    GcodeTemp,
    GcodeRetract,
    GcodeZHop,
    GcodeNonplanar,
    GcodeTexture,
    GcodeClay,
    GcodeStart,
    GcodeEnd,
    GcodeJoin,
    GcodeWrite,
    GcodePreview,
    GcodeEstimate,
    GcodeSafety,
    // More exact NURBS / SubD nodes (`nurbs_nodes`), appended.
    SubdCageEdit,
    ExactTrimPlane,
    ExactFilletVariable,
    ExactOffsetSurface,
    ExactDraft,
    ExactMirror,
    ExactLinearArray,
    ExactPolarArray,
    ExactPipe,
    ExactPatch,
    ExactBlendCurve,
    ExactMatchCurve,
    ExactContinuity,
    ExactCurvatureComb,
    ExactOffsetCurve,
    /// Reroute dot placed on a wire (passes its input through unchanged), appended.
    Reroute,
    // Standard components that were missing (see `gh_gaps.rs`), appended.
    MathSquareRoot,
    MathSquare,
    MathCube,
    MathCubeRoot,
    MathLn,
    MathLog10,
    GateNand,
    GateNor,
    GateXnor,
    DomainIncludes,
    PointPolar,
    PointCylindrical,
    CurveArea,
    CurveAreaCentroid,
    CurveExplode,
    RotateDirection,
    ProjectToZ,
}
impl NodeKind {
    /// Preserve the input keys used by the independent generative-design mission.
    /// ClosestPoint is one operation in the unified registry; canonical keys are
    /// candidates/queries. Unknown keys remain unknown and are still rejected.
    pub fn canonical_input_key(self, key: &str) -> &str {
        match (self, key) {
            (Self::ClosestPoint, "points") => "candidates",
            (Self::ClosestPoint, "targets") => "queries",
            _ => key,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Number,
            Self::Add,
            Self::Multiply,
            Self::Divide,
            Self::Subtract,
            Self::Power,
            Self::Abs,
            Self::Min,
            Self::Max,
            Self::Modulo,
            Self::Sine,
            Self::Series,
            Self::Range,
            Self::Point,
            Self::Circle,
            Self::Polygon,
            Self::Star,
            Self::Rectangle,
            Self::Helix,
            Self::Extrude,
            Self::Loft,
            Self::Revolve,
            Self::Vase,
            Self::Sphere,
            Self::Box,
            Self::Cylinder,
            Self::Cone,
            Self::Torus,
            Self::Plane,
            Self::Move,
            Self::Rotate,
            Self::Scale,
            Self::ScaleXyz,
            Self::Mirror,
            Self::Twist,
            Self::Wave,
            Self::LinearArray,
            Self::RadialArray,
            Self::Merge,
            Self::Flatten,
            Self::Graft,
            Self::Output,
            Self::Script,
            // `Cluster` is intentionally not listed: it is created from a definition.
            Self::ListItem,
            Self::ListLength,
            Self::ReverseList,
            Self::SortList,
            Self::CullPattern,
            Self::ShiftList,
            Self::ConcatList,
            Self::SubList,
            Self::Bounds,
            Self::Remap,
            Self::Expression,
            Self::Random,
            Self::Populate,
            Self::Noise,
            Self::NoiseDeform,
            Self::GraphProfile,
            Self::Shell,
            Self::OverhangCheck,
            Self::Distance,
            Self::PointComponent,
            Self::ImportMesh,
            Self::SimplifyTree,
            Self::TrimTree,
            Self::PruneTree,
            Self::MergeTree,
            Self::EntwineTree,
            Self::FlipMatrix,
            Self::UnflattenTree,
            Self::PathMapper,
            Self::PartitionList,
            Self::TreeBranch,
            Self::TreeItem,
            Self::TreeStatistics,
            Self::CullNth,
            Self::CullIndex,
            Self::DuplicateData,
            Self::RepeatData,
            Self::MassAddition,
            Self::MassMultiplication,
            Self::ReplaceItems,
            Self::InsertItems,
            Self::MemberIndex,
            Self::RandomReduce,
            Self::Jitter,
            Self::Negate,
            Self::Round,
            Self::Floor,
            Self::Ceiling,
            Self::Exponential,
            Self::Sinh,
            Self::Cosh,
            Self::Tanh,
            Self::Degrees,
            Self::Radians,
            Self::Asin,
            Self::Acos,
            Self::Atan,
            Self::SinRadians,
            Self::CosRadians,
            Self::TanRadians,
            Self::Atan2,
            Self::Root,
            Self::Logarithm,
            Self::Factorial,
            Self::Clamp,
            Self::Summation,
            Self::Product,
            Self::Average,
            Self::Median,
            Self::Variance,
            Self::StdDeviation,
            Self::GhRange,
            Self::ConstructDomain,
            Self::DomainEndpoint,
            Self::DomainLength,
            Self::EvaluateDomain,
            Self::DivideDomain,
            Self::DomainIntersection,
            Self::DomainUnion,
            Self::GraphMapper,
            Self::BoolAnd,
            Self::BoolOr,
            Self::BoolXor,
            Self::BoolNot,
            Self::Equals,
            Self::LargerThan,
            Self::SmallerThan,
            Self::SetContains,
            Self::SetUnion,
            Self::SetIntersection,
            Self::SetDifference,
            Self::CreateSet,
            Self::DuplicateDetection,
            Self::ConstructVector,
            Self::VectorComponent,
            Self::UnitX,
            Self::UnitY,
            Self::UnitZ,
            Self::VectorLength,
            Self::Unitize,
            Self::ReverseVector,
            Self::DotProduct,
            Self::CrossProduct,
            Self::VectorAngle,
            Self::VectorAddition,
            Self::VectorSubtraction,
            Self::VectorProjection,
            Self::VectorBetweenPoints,
            Self::RotateVector,
            Self::ClosestPoint,
            Self::PlaneProject,
            Self::PlaneCoordinates,
            Self::PlanePoint,
            Self::PointInPlane,
            Self::PlaneNormal3Pt,
            Self::RandomNormal,
            Self::VectorMultiply,
            Self::VectorDivide,
            Self::VectorAmplitude,
            Self::Lerp,
            Self::SmoothStep,
            Self::BezierValue,
            Self::NormalPdf,
            Self::MatrixTranspose,
            Self::MatrixMultiply,
            Self::MatrixInverse,
            Self::LinearSolve,
            Self::FourierMagnitude,
            Self::PlaneIntersectionPoint,
            Self::PlaneIntersectionDirection,
            Self::PlaneRotateAxis,
            Self::PlaneAlignAxis,
            Self::WorldPlaneAxis,
            Self::ChangeBasis,
            Self::RectGrid,
            Self::TriangularGrid,
            Self::HexGrid,
            Self::RadialGrid,
            Self::HexPanels,
            Self::TriPanels,
            Self::Voronoi2d,
            Self::ConvexHull2d,
            Self::MultiAttractor,
            Self::CurveAttractor,
            Self::GridArray,
            Self::PointArray,
            Self::Rotate3d,
            Self::Shear,
            Self::Taper,
            Self::FractalTree,
            Self::CylinderWrap,
            Self::Delaunay2d,
            Self::PopulateMesh,
            Self::CurveArray,
            Self::NoiseField,
            Self::LifeGrid,
            Self::MeshArea,
            Self::MeshVolume,
            Self::MeshCenter,
            Self::MeshDimensions,
            Self::MeshMass,
            Self::MeshAudit,
            Self::MeshVertices,
            Self::MeshFaces,
            Self::MeshNormals,
            Self::MeshEdges,
            Self::MeshBoundary,
            Self::MeshClosest,
            Self::MeshFlip,
            Self::MeshWeldExact,
            Self::MeshUnweld,
            Self::MeshUnify,
            Self::MeshSmooth,
            Self::MeshSubdivide,
            Self::MeshCatmullClark,
            Self::MeshOnBed,
            Self::MeshDeleteFace,
            Self::MeshConstruct,
            Self::MeshSelfContact,
            Self::MeshThicknessSample,
            Self::MeshCompact,
            Self::MeshNonManifold,
            Self::CurveLine,
            Self::CurveLineSdl,
            Self::CurvePolyline,
            Self::CurveCircleCnr,
            Self::CurveCircleThree,
            Self::CurveArc,
            Self::CurveArcThree,
            Self::CurveEllipse,
            Self::CurveSpiral,
            Self::CurveBezier,
            Self::CurveNurbs,
            Self::CurveWeightedNurbs,
            Self::CurveInterpolate,
            Self::CurveLength,
            Self::CurveEvaluate,
            Self::CurveEvaluateLength,
            Self::CurveClosestParameter,
            Self::CurveClosestPoint,
            Self::CurveStart,
            Self::CurveEnd,
            Self::CurveMiddle,
            Self::CurveTangent,
            Self::CurveCurvature,
            Self::CurveDomain,
            Self::CurveClosed,
            Self::CurveDivide,
            Self::CurveDivideLength,
            Self::CurveSubcurve,
            Self::CurveShatter,
            Self::CurveReverse,
            Self::CurveControls,
            Self::CurveWeights,
            Self::CurveKnots,
            Self::CurveDegree,
            Self::CurveInsertKnot,
            Self::CurveOffsetPreview,
            Self::CurveFrameX,
            Self::CurveFrameY,
            Self::CurveLineLine,
            Self::CurveLinePlane,
            Self::PlaneSurface,
            Self::NurbsPanel,
            Self::BilinearSurface,
            Self::CylinderSurface,
            Self::FrustumSurface,
            Self::SphereSurface,
            Self::TorusSurface,
            Self::EvaluateSurface,
            Self::SurfaceDirection,
            Self::SurfaceNormalLine,
            Self::SurfaceArea,
            Self::SurfaceClosestPoint,
            Self::SurfaceIsoCurve,
            Self::DivideSurface,
            Self::SurfaceMesh,
            Self::BrepBox,
            Self::BrepVolume,
            Self::BrepArea,
            Self::BrepIntersection,
            Self::BrepMove,
            Self::BrepFaces,
            Self::BrepEdges,
            Self::BrepVertices,
            Self::BrepMesh,
            Self::NurbsVase,
            Self::RuledSurface,
            Self::SurfaceExtrude,
            Self::BrepUnion,
            Self::BrepDifference,
            Self::BrepOffset,
            Self::SurfaceGaussianCurvature,
            Self::SurfaceMeanCurvature,
            Self::SurfacePrincipalCurvatures,
            Self::ArcCurve,
            Self::Polyline,
            Self::SmoothCurve,
            Self::OffsetCurve,
            Self::PointOnCurve,
            Self::DivideCurve,
            Self::Pipe,
            Self::Sweep,
            Self::SmoothMesh,
            Self::Subdivide,
            Self::WeldMesh,
            Self::UnifyNormals,
            Self::MeshInfo,
            Self::TaperScale,
            Self::Bend,
            Self::GridArray3d,
            Self::Gyroid,
            Self::RevolvePoints,
            Self::ExactBox,
            Self::ExactCylinder,
            Self::ExactSphere,
            Self::ExactExtrude,
            Self::ExactRevolve,
            Self::ExactLoft,
            Self::ExactLoftSurface,
            Self::ExactSweep1,
            Self::ExactSweep2,
            Self::ExactNetworkSurface,
            Self::ExactRebuildCurve,
            Self::ExactCurveCurve,
            Self::ExactCurveBrep,
            Self::ExactBrepBrep,
            Self::ExactUnion,
            Self::ExactDifference,
            Self::ExactIntersection,
            Self::ExactFillet,
            Self::ExactChamfer,
            Self::ExactShell,
            Self::ExactCapHoles,
            Self::ExactBrepMesh,
            Self::ExactMove,
            Self::ExactVolume,
            Self::ExactStepExport,
            Self::ExactStepImport,
            Self::SubdBoxCage,
            Self::SubdCylinderCage,
            Self::SubdSubdivide,
            Self::SubdCrease,
            Self::SubdExtrudeFace,
            Self::SubdToBrep,
            Self::SubdToMesh,
            Self::GcodePrinter,
            Self::GcodeSlice,
            Self::GcodeContour,
            Self::GcodeSpiral,
            Self::GcodeSort,
            Self::GcodeSplit,
            Self::GcodeExtrusion,
            Self::GcodeSpeed,
            Self::GcodeTemp,
            Self::GcodeRetract,
            Self::GcodeZHop,
            Self::GcodeNonplanar,
            Self::GcodeTexture,
            Self::GcodeClay,
            Self::GcodeStart,
            Self::GcodeEnd,
            Self::GcodeJoin,
            Self::GcodeWrite,
            Self::GcodePreview,
            Self::GcodeEstimate,
            Self::GcodeSafety,
            Self::SubdCageEdit,
            Self::ExactTrimPlane,
            Self::ExactFilletVariable,
            Self::ExactOffsetSurface,
            Self::ExactDraft,
            Self::ExactMirror,
            Self::ExactLinearArray,
            Self::ExactPolarArray,
            Self::ExactPipe,
            Self::ExactPatch,
            Self::ExactBlendCurve,
            Self::ExactMatchCurve,
            Self::ExactContinuity,
            Self::ExactCurvatureComb,
            Self::ExactOffsetCurve,
            Self::Reroute,
            Self::MathSquareRoot,
            Self::MathSquare,
            Self::MathCube,
            Self::MathCubeRoot,
            Self::MathLn,
            Self::MathLog10,
            Self::GateNand,
            Self::GateNor,
            Self::GateXnor,
            Self::DomainIncludes,
            Self::PointPolar,
            Self::PointCylindrical,
            Self::CurveArea,
            Self::CurveAreaCentroid,
            Self::CurveExplode,
            Self::RotateDirection,
            Self::ProjectToZ,
        ]
    }
    pub fn spec(self) -> &'static NodeSpec {
        match self {
            Self::Number => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "数値",
                    category: "数値",
                    description: "数値を他のノードへ渡します。複数の形の寸法を一緒に調整できます。",
                    keywords: "number slider パラメータ スライダー",
                    inputs: &[n("value", "値", 1., -1e6, 1e6, 0.1)],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Add => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "足し算",
                    category: "数値",
                    description: "A + B を計算。リストは順に対応し、短い側の最後の値を繰り返します。",
                    keywords: "add plus sum 和 加算",
                    inputs: &[
                        n("a", "A", 0., -1e6, 1e6, 0.1),
                        n("b", "B", 10., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Multiply => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "掛け算",
                    category: "数値",
                    description: "A × B を計算します。",
                    keywords: "multiply product 積 乗算 倍",
                    inputs: &[
                        n("a", "A", 1., -1e6, 1e6, 0.1),
                        n("b", "B", 2., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Divide => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "割り算",
                    category: "数値",
                    description: "A ÷ B を計算します。ゼロでの割り算はエラーを表示します。",
                    keywords: "divide quotient 除算 比率",
                    inputs: &[
                        n("a", "A", 1., -1e6, 1e6, 0.1),
                        n("b", "B", 2., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Subtract => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "引き算",
                    category: "数値",
                    description: "A − B を計算します。リストは順に対応し、短い側の最後の値を繰り返します。",
                    keywords: "subtract minus difference 差 減算 引く マイナス",
                    inputs: &[
                        n("a", "A", 10., -1e6, 1e6, 0.1),
                        n("b", "B", 3., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Power => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "べき乗",
                    category: "数値",
                    description: "底を指数で累乗します。0 の負の指数、負の数の小数乗、結果が大きすぎる場合はエラーを表示します。",
                    keywords: "power pow exponent square 累乗 べき乗 二乗 指数 冪",
                    inputs: &[
                        n("base", "底", 2., -1e6, 1e6, 0.1),
                        n("exponent", "指数", 2., -64., 64., 0.5),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Abs => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "絶対値",
                    category: "数値",
                    description: "値の符号を取り除き、0 以上の値にします。",
                    keywords: "abs absolute magnitude 絶対値 符号 正の数",
                    inputs: &[n("value", "値", -1., -1e6, 1e6, 0.1)],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Min => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "最小",
                    category: "数値",
                    description: "A と B の小さい方を返します。",
                    keywords: "min minimum smaller lower 最小 小さい方 下限 クランプ",
                    inputs: &[
                        n("a", "A", 0., -1e6, 1e6, 0.1),
                        n("b", "B", 10., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Max => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "最大",
                    category: "数値",
                    description: "A と B の大きい方を返します。",
                    keywords: "max maximum larger upper 最大 大きい方 上限 クランプ",
                    inputs: &[
                        n("a", "A", 0., -1e6, 1e6, 0.1),
                        n("b", "B", 10., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Modulo => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "剰余",
                    category: "数値",
                    description: "A を B で割った余りを 0 以上 |B| 未満で返します。B が 0 のときはエラーを表示します。",
                    keywords: "modulo mod remainder wrap 剰余 余り 割った余り 繰り返し",
                    inputs: &[
                        n("a", "A", 10., -1e6, 1e6, 0.1),
                        n("b", "B", 3., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Sine => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "三角関数",
                    category: "数値",
                    description: "振幅 × sin(2π × 周波数 × 入力 + 位相) を計算します。位相は度。範囲や数列をつなぐと波の値のリストになります。",
                    keywords: "sine sin cosine trig wave oscillate 三角関数 サイン 正弦 波 周期",
                    inputs: &[
                        n("x", "入力", 0.25, -1e6, 1e6, 0.05),
                        n("amp", "振幅", 1., -1e6, 1e6, 0.5),
                        n("frequency", "周波数", 1., -1000., 1000., 0.1),
                        n("phase", "位相", 0., -3600., 3600., 5.),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Series => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "数列",
                    category: "データ",
                    description: "開始値から等間隔の数値リストを生成。位置や角度をまとめて変えられます。",
                    keywords: "series range list 連番 リスト 配列",
                    inputs: &[
                        n("start", "開始", 0., -1e5, 1e5, 1.),
                        n("step", "間隔", 10., -1e4, 1e4, 0.1),
                        i("count", "個数", 8., 0., 512.),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Range => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "範囲",
                    category: "データ",
                    description: "開始値から終了値までを等間隔に分けた数値リストを作ります。個数が 1 のときは開始値だけを返します。",
                    keywords: "range domain linspace interval divide 範囲 区間 等分 リスト 分割",
                    inputs: &[
                        n("start", "開始", 0., -1e5, 1e5, 1.),
                        n("end", "終了", 100., -1e5, 1e5, 1.),
                        i("count", "個数", 11., 1., 512.),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Point => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "点",
                    category: "曲線",
                    description: "X・Y・Z 座標から点を作ります。単位は mm。",
                    keywords: "point xyz 座標",
                    inputs: &[
                        n("x", "X", 0., -1e5, 1e5, 1.),
                        n("y", "Y", 0., -1e5, 1e5, 1.),
                        n("z", "Z", 0., -1e5, 1e5, 1.),
                    ],
                    output: DataType::Point,
                };
                &SPEC
            }
            Self::Circle => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "円",
                    category: "曲線",
                    description: "XY 平面に正確な有理円を作ります。分割数は表示とメッシュ変換だけに使います。",
                    keywords: "circle ring 丸 輪",
                    inputs: &[
                        n("radius", "半径", 30., 0.1, 1000., 0.5),
                        i("segments", "分割数", 64., 8., 256.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Polygon => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "正多角形",
                    category: "曲線",
                    description: "外接円半径と辺数で正多角形を作ります。",
                    keywords: "polygon triangle hexagon 六角形 三角形",
                    inputs: &[
                        n("radius", "半径", 30., 0.1, 1000., 0.5),
                        i("sides", "辺の数", 6., 3., 64.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Star => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "星形",
                    category: "曲線",
                    description: "外半径と内半径を交互に結んだ閉じた断面を作ります。",
                    keywords: "star flower 星 花 断面",
                    inputs: &[
                        n("radius", "外半径", 30., 0.1, 1000., 0.5),
                        n("inner_radius", "内半径", 16., 0.1, 1000., 0.5),
                        i("points", "角の数", 7., 3., 48.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Rectangle => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "長方形",
                    category: "曲線",
                    description: "原点を中心に、XY 平面上の長方形を作ります。",
                    keywords: "rectangle square 四角 四角形",
                    inputs: &[
                        n("width", "幅", 40., 0.1, 2000., 1.),
                        n("depth", "奥行", 30., 0.1, 2000., 1.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Helix => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "らせん",
                    category: "曲線",
                    description: "Z 軸を中心に上昇する解析的ならせんを作ります。分割数は表示とメッシュ変換だけに使います。",
                    keywords: "helix spiral 螺旋 バネ スパイラル",
                    inputs: &[
                        n("radius", "半径", 25., 0.1, 1000., 0.5),
                        n("height", "高さ", 100., 0.1, 2000., 1.),
                        n("turns", "巻き数", 3., 0.1, 30., 0.1),
                        i("segments", "分割数", 192., 16., 1024.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Extrude => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "押し出し",
                    category: "立体",
                    description: "閉じた平面の断面を Z 方向に押し出します。凹形状も三角形に分割。",
                    keywords: "extrude extrusion 押出 厚み プリズム",
                    inputs: &[
                        g("curve", "断面", DataType::Curve),
                        n("height", "高さ", 40., 0.1, 2000., 1.),
                        i("layers", "縦の分割", 16., 1., 128.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Loft => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "ロフト",
                    category: "立体",
                    description: "同じ点数の曲線を順に結びます。1 ブランチに 2 本以上の曲線を接続。",
                    keywords: "loft surface 曲面 断面 接続",
                    inputs: &[
                        g("curves", "断面リスト", DataType::Curve),
                        i("cap", "端を閉じる", 1., 0., 1.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Revolve => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "回転面",
                    category: "立体",
                    description: "断面を Z 軸の周りに回転。断面は XZ 平面上で半径 X > 0 に配置。端の開いた断面は開いた面になります。",
                    keywords: "revolve lathe 回転体 回転 曲面",
                    inputs: &[
                        g("curve", "断面", DataType::Curve),
                        n("angle", "回転角度", 360., 1., 360., 1.),
                        i("steps", "分割数", 64., 8., 192.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Vase => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "花器",
                    category: "立体",
                    description: "底・内壁・外壁・上縁を備えた中空の花器を生成。肉厚は半径方向の厚さです。",
                    keywords: "vase pot vessel 花瓶 容器 花器 プリント",
                    inputs: &[
                        n("radius", "半径", 32., 3., 500., 0.5),
                        n("height", "高さ", 100., 5., 1000., 1.),
                        n("thickness", "肉厚", 2.4, 0.5, 40., 0.1),
                        i("lobes", "波の数", 7., 0., 24.),
                        n("amplitude", "凹凸", 0.12, 0., 0.4, 0.01),
                        i("segments", "周の分割", 64., 16., 192.),
                        i("layers", "縦の分割", 32., 4., 128.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Sphere => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "球",
                    category: "立体",
                    description: "緯度と経度で分割した閉じた球のメッシュを作ります。",
                    keywords: "sphere ball 球体 丸",
                    inputs: &[
                        n("radius", "半径", 25., 0.1, 1000., 0.5),
                        i("segments", "横の分割", 48., 8., 192.),
                        i("rings", "縦の分割", 24., 4., 96.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Box => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "箱",
                    category: "立体",
                    description: "XY の中心を原点とし、底面 Z=0 の直方体を作ります。",
                    keywords: "box cube cuboid 立方体 直方体 箱",
                    inputs: &[
                        n("width", "幅", 40., 0.1, 2000., 1.),
                        n("depth", "奥行", 40., 0.1, 2000., 1.),
                        n("height", "高さ", 40., 0.1, 2000., 1.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Cylinder => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "円柱",
                    category: "立体",
                    description: "底面 Z=0 から上へ伸びる、底と蓋のある閉じた円柱を作ります。",
                    keywords: "cylinder tube column pillar 円柱 円筒 柱 シリンダー",
                    inputs: &[
                        n("radius", "半径", 20., 0.1, 1000., 0.5),
                        n("height", "高さ", 40., 0.1, 2000., 1.),
                        i("segments", "周の分割", 48., 3., 256.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Cone => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "円錐 / 角錐",
                    category: "立体",
                    description: "底面 Z=0、頂点が上の閉じた円錐を作ります。分割数 3 で三角錐、4 で四角錐になります。",
                    keywords: "cone pyramid tetrahedron 円錐 角錐 三角錐 四角錐 コーン ピラミッド",
                    inputs: &[
                        n("radius", "半径", 20., 0.1, 1000., 0.5),
                        n("height", "高さ", 40., 0.1, 2000., 1.),
                        i("segments", "周の分割 (3:三角錐 4:四角錐)", 48., 3., 256.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Torus => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "トーラス",
                    category: "立体",
                    description: "Z 軸まわりの輪を断面の円で作る、閉じたドーナツ形を作ります。副半径は主半径より小さくしてください。",
                    keywords: "torus donut ring doughnut 輪 ドーナツ トーラス 環",
                    inputs: &[
                        n("major", "主半径", 30., 0.2, 1000., 0.5),
                        n("minor", "副半径", 8., 0.1, 500., 0.5),
                        i("segments", "輪の分割", 64., 3., 192.),
                        i("tube", "断面の分割", 24., 3., 96.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Plane => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "平面",
                    category: "立体",
                    description: "XY の中心を原点とする、Z=0 の格子状の平面メッシュを作ります。閉じていない面なので、そのままでは書き出し検査で開いた辺が出ます。",
                    keywords: "plane grid sheet surface 平面 格子 板 グリッド 面",
                    inputs: &[
                        n("width", "幅", 100., 0.1, 2000., 1.),
                        n("depth", "奥行", 100., 0.1, 2000., 1.),
                        i("divisions", "分割数", 8., 1., 128.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Move => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "移動",
                    category: "変形",
                    description: "点・曲線・メッシュを平行移動します。数列を接続すると複数配置。",
                    keywords: "move translate offset 動かす ずらす 平行移動",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("x", "X", 0., -1e5, 1e5, 1.),
                        n("y", "Y", 0., -1e5, 1e5, 1.),
                        n("z", "Z", 0., -1e5, 1e5, 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Rotate => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "回転",
                    category: "変形",
                    description: "原点を通る指定軸の周りに回転します。軸: 0=X、1=Y、2=Z。",
                    keywords: "rotate rotation 回す 角度",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("angle", "角度", 45., -3600., 3600., 1.),
                        i("axis", "軸 (0:X 1:Y 2:Z)", 2., 0., 2.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Scale => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "拡大縮小",
                    category: "変形",
                    description: "原点を基準に形状全体の大きさを変更します。",
                    keywords: "scale resize size サイズ 倍率",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("factor", "倍率", 1.2, 0.01, 100., 0.05),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::ScaleXyz => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "非一様スケール",
                    category: "変形",
                    description: "原点を基準に、X・Y・Z をそれぞれ別の倍率で伸縮します。倍率は 0.01〜100。",
                    keywords: "scale non-uniform nonuniform stretch xyz 非一様 軸別 縦横 引き伸ばし 伸縮 倍率",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("sx", "X 倍率", 1., 0.01, 100., 0.05),
                        n("sy", "Y 倍率", 1., 0.01, 100., 0.05),
                        n("sz", "Z 倍率", 1., 0.01, 100., 0.05),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Mirror => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "ミラー",
                    category: "変形",
                    description: "原点を通り、指定した軸に垂直な平面で反転します。元も残すと元の形と反転した形を 1 つにまとめます（メッシュの場合）。軸: 0=X、1=Y、2=Z。",
                    keywords: "mirror reflect symmetry flip 鏡 反転 対称 左右 ミラー",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        i("axis", "軸 (0:X 1:Y 2:Z)", 0., 0., 2.),
                        i("keep", "元も残す", 1., 0., 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Twist => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "ねじり",
                    category: "変形",
                    description: "形状の最下部から最上部まで Z 軸周りの回転を徐々に増やします。縦の分割が多いほど滑らかです。",
                    keywords: "twist torsion ねじる 捻る 螺旋",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("angle", "ねじり角", 90., -1080., 1080., 5.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Wave => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "波打つ",
                    category: "変形",
                    description: "X 方向に沿う正弦波で Z 座標を変形します。細分割した形状に使用。",
                    keywords: "wave sine 波 ウェーブ 波形",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("amplitude", "振幅", 4., 0., 200., 0.5),
                        n("wavelength", "波長", 40., 0.1, 2000., 1.),
                        n("phase", "位相", 0., -3600., 3600., 5.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::LinearArray => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "直線配列",
                    category: "配列",
                    description: "指定した間隔で形状を複製。個数には元の形状を含みます。",
                    keywords: "linear array repeat duplicate 並べる 複製 反復",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        i("count", "個数", 5., 1., 128.),
                        n("x", "X 間隔", 50., -1e4, 1e4, 1.),
                        n("y", "Y 間隔", 0., -1e4, 1e4, 1.),
                        n("z", "Z 間隔", 0., -1e4, 1e4, 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::RadialArray => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "円形配列",
                    category: "配列",
                    description: "原点の Z 軸を中心に複製。先に移動すると円状に並びます。",
                    keywords: "radial polar circular array repeat 円周 回転配列 放射",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        i("count", "個数", 8., 1., 128.),
                        n("angle", "全体の角度", 360., 1., 360., 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Merge => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "メッシュをまとめる",
                    category: "データ",
                    description: "同じブランチのメッシュを 1 つにまとめます。交差部分のブーリアン結合は行いません。",
                    keywords: "merge join combine 集合 結合 まとめる",
                    inputs: &[g("geometry", "メッシュ", DataType::Mesh)],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Flatten => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "フラット化",
                    category: "データ",
                    description: "全ブランチのデータを順に 1 つのリストへまとめます。",
                    keywords: "flatten list tree フラット 階層 リスト",
                    inputs: &[g("geometry", "データ", DataType::Any)],
                    output: DataType::Any,
                };
                &SPEC
            }
            Self::Graft => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "枝を分ける",
                    category: "データ",
                    description: "各要素を個別のブランチへ分けます。空ブランチも保持します。",
                    keywords: "graft tree branch ツリー 枝 階層",
                    inputs: &[g("geometry", "データ", DataType::Any)],
                    output: DataType::Any,
                };
                &SPEC
            }
            Self::Output => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "出力",
                    category: "出力",
                    description: "完成した形状を表示・書き出しの対象にします。",
                    keywords: "output preview export 出力 完成 表示 STL",
                    inputs: &[g("geometry", "形状", DataType::DisplayGeometry)],
                    output: DataType::DisplayGeometry,
                };
                &SPEC
            }
            Self::Reroute => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "リルート点",
                    category: "データ",
                    description: "接続線の途中に置く小さな点。入力をそのまま出力へ渡し、配線を整理したり曲げたりできます。",
                    keywords: "reroute dot relay wire anchor リルート 中継 配線 ドット 点",
                    inputs: &[g("value", "入力", DataType::Any)],
                    output: DataType::Any,
                };
                &SPEC
            }
            Self::Script => &crate::script::SCRIPT_SPEC,
            Self::Cluster => &crate::cluster::CLUSTER_SPEC,
            other if crate::curve_nodes::is_curve_kind(other) => crate::curve_nodes::spec(other),
            other if crate::surface_nodes::KINDS.contains(&other) => {
                crate::surface_nodes::spec(other)
            }
            other if crate::nurbs_nodes::is_kind(other) => crate::nurbs_nodes::spec(other),
            other if crate::gcode_nodes::is_kind(other) => crate::gcode_nodes::spec(other),
            Self::MeshArea
            | Self::MeshVolume
            | Self::MeshCenter
            | Self::MeshDimensions
            | Self::MeshMass
            | Self::MeshAudit
            | Self::MeshVertices
            | Self::MeshFaces
            | Self::MeshNormals
            | Self::MeshEdges
            | Self::MeshBoundary
            | Self::MeshClosest
            | Self::MeshFlip
            | Self::MeshWeldExact
            | Self::MeshUnweld
            | Self::MeshUnify
            | Self::MeshSmooth
            | Self::MeshSubdivide
            | Self::MeshCatmullClark
            | Self::MeshOnBed
            | Self::MeshDeleteFace
            | Self::MeshConstruct
            | Self::MeshSelfContact
            | Self::MeshThicknessSample
            | Self::MeshCompact
            | Self::MeshNonManifold => crate::mesh_fabrication::spec(self),
            other => crate::data_tree_nodes::spec(other)
                .or_else(|| crate::math_nodes::spec(other))
                .or_else(|| crate::gh_gaps::spec(other))
                .or_else(|| {
                    crate::generative::is_kind(other).then(|| crate::generative::spec(other))
                })
                .or_else(|| {
                    crate::nodes_shape::is_kind(other).then(|| crate::nodes_shape::spec(other))
                })
                .unwrap_or_else(|| crate::gh_nodes::spec(other)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub position: [f32; 2],
    pub params: BTreeMap<String, f64>,
    pub inputs: BTreeMap<String, NodeId>,
    pub preview: bool,
    /// Script source of a `Script` node. Empty (and not saved) for other nodes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub script: String,
    /// Definition id (`Graph::clusters`) of a `Cluster` node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<u64>,
    /// Disabled (bypassed) node: outputs an empty tree. Not written when `false`.
    #[serde(default, skip_serializing_if = "crate::annotations::is_false")]
    pub off: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub next_id: NodeId,
    /// Cluster definitions stored inside the work (empty in old files).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clusters: Vec<ClusterDef>,
    /// Coloured groups and free notes on the canvas (see `annotations.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<crate::annotations::Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<crate::annotations::Note>,
}
impl Graph {
    pub fn add(&mut self, kind: NodeKind, position: [f32; 2]) -> Result<NodeId, String> {
        if self.nodes.len() >= MAX_NODES {
            return Err(format!("ノードは {MAX_NODES} 個までです。"));
        }
        if !valid_position(position) {
            return Err("ノードの位置が無効です。".into());
        }
        let id = self.next_id;
        if self.node(id).is_some() {
            return Err("ノード ID が重複しています。".into());
        }
        let next = id
            .checked_add(1)
            .ok_or("ノード ID をこれ以上追加できません。")?;
        self.nodes.push(Node {
            id,
            kind,
            name: kind.spec().label.into(),
            position,
            params: kind
                .spec()
                .inputs
                .iter()
                .filter_map(|s| s.default.map(|v| (s.key.into(), v)))
                .collect(),
            inputs: BTreeMap::new(),
            preview: kind != NodeKind::Reroute,
            script: if kind == NodeKind::Expression {
                crate::expr::DEFAULT_EXPRESSION.into()
            } else if kind == NodeKind::PathMapper {
                "{A;B}->{B;A}".into()
            } else {
                String::new()
            },
            cluster: None,
            off: false,
        });
        self.next_id = next;
        Ok(id)
    }
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    pub fn remove(&mut self, id: NodeId) {
        self.nodes.retain(|n| n.id != id);
        for n in &mut self.nodes {
            n.inputs.retain(|_, source| *source != id);
        }
    }
    pub fn connect(&mut self, from: NodeId, to: NodeId, input: &str) -> Result<(), String> {
        let source = self
            .node(from)
            .ok_or("接続元が見つかりません。")?
            .kind
            .spec()
            .output;
        let node = self.node(to).ok_or("接続先が見つかりません。")?;
        let input = node.kind.canonical_input_key(input);
        let target = node
            .kind
            .spec()
            .inputs
            .iter()
            .find(|s| s.key == input)
            .ok_or("入力が見つかりません。")?;
        if !target.kind.accepts(source) {
            return Err(format!(
                "{} には {} を接続できません。",
                target.kind.label(),
                source.label()
            ));
        }
        let old = self
            .node_mut(to)
            .ok_or("接続先が見つかりません。")?
            .inputs
            .insert(input.into(), from);
        if let Err(e) = self.topological_order() {
            if let Some(n) = self.node_mut(to) {
                match old {
                    Some(v) => {
                        n.inputs.insert(input.into(), v);
                    }
                    None => {
                        n.inputs.remove(input);
                    }
                }
            }
            return Err(e);
        }
        Ok(())
    }
    pub fn disconnect(&mut self, to: NodeId, input: &str) {
        if let Some(n) = self.node_mut(to) {
            let input = n.kind.canonical_input_key(input);
            n.inputs.remove(input);
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.topological_order()?;
        crate::annotations::validate(self)?;
        crate::cluster::validate_graph(self)
    }
    pub fn topological_order(&self) -> Result<Vec<NodeId>, String> {
        self.topological_nodes()
            .map(|nodes| nodes.into_iter().map(|node| node.id).collect())
    }

    /// Validated topological order with direct node references. Evaluators must use this
    /// instead of repeatedly scanning `Graph::nodes` by id. The transient index does not
    /// alter the serialized graph or need invalidation after an edit.
    pub(crate) fn topological_nodes(&self) -> Result<Vec<&Node>, String> {
        if self.nodes.len() > MAX_NODES {
            return Err(format!("ノード数が上限 {MAX_NODES} を超えています。"));
        }
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_unstable_by_key(|node| node.id);
        if nodes.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("ノード ID が重複しています。".into());
        }
        // Dense new projects and sparse/reordered v1 files use the same compact
        // scratch storage. Never allocate up to a serialized node ID.
        let dense_ids = nodes
            .last()
            .is_some_and(|node| node.id == (nodes.len() - 1) as NodeId);
        let mut indegree = vec![0; nodes.len()];
        let mut children = vec![Vec::new(); nodes.len()];
        for (index, &node) in nodes.iter().enumerate() {
            if node.id >= self.next_id {
                return Err("次のノード ID が無効です。".into());
            }
            if !valid_position(node.position) || node.name.len() > 512 {
                return Err("ノードの位置または名前が無効です。".into());
            }
            let specs = node.kind.spec().inputs;
            for (key, value) in &node.params {
                if crate::data_tree_nodes::validate_parameter(node.kind, key, *value)? {
                    continue;
                }
                if !value.is_finite() || !specs.iter().any(|s| s.key == key && s.default.is_some())
                {
                    return Err(format!("「{}」のパラメータが無効です。", node.name));
                }
            }
            for (key, source) in &node.inputs {
                let spec = specs
                    .iter()
                    .find(|s| s.key == key)
                    .ok_or("不明な入力への接続があります。")?;
                let parent = if dense_ids {
                    usize::try_from(*source).ok().filter(|&i| i < nodes.len())
                } else {
                    nodes.binary_search_by_key(source, |node| node.id).ok()
                }
                .ok_or("存在しないノードへの接続があります。")?;
                let other = nodes[parent];
                if !spec.kind.accepts(other.kind.spec().output) {
                    return Err(format!(
                        "「{}」の「{}」は接続の型が違います。",
                        node.name, spec.label
                    ));
                }
                // Count wires, so the same source feeding two ports is released twice.
                // Visiting children by node ID also provides their stable enqueue order.
                children[parent].push(index);
                indegree[index] += 1;
            }
        }

        let mut order = Vec::with_capacity(nodes.len());
        order.extend((0..nodes.len()).filter(|&i| indegree[i] == 0));
        let mut cursor = 0;
        while cursor < order.len() {
            for &child in &children[order[cursor]] {
                indegree[child] -= 1;
                if indegree[child] == 0 {
                    order.push(child);
                }
            }
            cursor += 1;
        }
        if order.len() != self.nodes.len() {
            return Err("この接続は循環します。途中のノードへ戻る線はつなげません。".into());
        }
        Ok(order.into_iter().map(|i| nodes[i]).collect())
    }
}
fn valid_position(p: [f32; 2]) -> bool {
    p.into_iter()
        .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    pub points: Vec<[f64; 3]>,
    pub closed: bool,
    /// Exact runtime geometry; points are a separate display/legacy meshing cache.
    pub geometry: Option<crate::curve_geometry::CurveGeometry>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Point([f64; 3]),
    Curve(Arc<Curve>),
    Mesh(Arc<Mesh>),
    Surface(Arc<crate::surface_kernel::SurfaceValue>),
    Solid(Arc<crate::surface_kernel::ClosedSolid>),
    /// Exact B-rep from the SolidCraft kernel.
    Brep(Arc<crate::kernel_bridge::BrepValue>),
    /// SubD / T-spline control cage.
    Cage(Arc<crate::subd::Cage>),
    Gcode(Arc<crate::gcode_job::GcodeJob>),
}
impl Value {
    pub fn kind(&self) -> DataType {
        match self {
            Self::Number(_) => DataType::Number,
            Self::Point(_) => DataType::Point,
            Self::Curve(_) => DataType::Curve,
            Self::Mesh(_) => DataType::Mesh,
            Self::Surface(_) => DataType::Surface,
            Self::Solid(_) => DataType::Solid,
            Self::Brep(_) => DataType::Brep,
            Self::Cage(_) => DataType::Cage,
            Self::Gcode(_) => DataType::Gcode,
        }
    }
    pub fn weight(&self) -> usize {
        match self {
            Self::Number(_) | Self::Point(_) => 1,
            Self::Curve(c) => c
                .points
                .len()
                .max(1)
                .saturating_add(c.geometry.as_ref().map_or(0, |g| g.weight())),
            Self::Mesh(m) => m
                .vertices
                .len()
                .saturating_add(m.triangles.len().saturating_mul(3)),
            Self::Surface(s) => s.weight(),
            Self::Solid(s) => s.weight(),
            Self::Brep(b) => b.weight(),
            Self::Cage(c) => c.weight(),
            Self::Gcode(j) => j.weight(),
        }
    }
    pub fn number(&self) -> Result<f64, String> {
        match self {
            Self::Number(v) if v.is_finite() => Ok(*v),
            _ => Err("数値を接続してください。".into()),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub branches: BTreeMap<Vec<u32>, Vec<Value>>,
}
impl Tree {
    pub fn single(value: Value) -> Self {
        Self {
            branches: BTreeMap::from([(vec![0], vec![value])]),
        }
    }
    pub fn item_count(&self) -> usize {
        self.branches.values().map(Vec::len).sum()
    }
    pub fn weight(&self) -> usize {
        self.branches.iter().fold(0usize, |sum, (path, items)| {
            items
                .iter()
                .fold(sum.saturating_add(path.len()).saturating_add(1), |a, v| {
                    a.saturating_add(v.weight())
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_reordered_nodes_with_repeated_parents_keep_stable_order() {
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let left = graph.add(NodeKind::Add, [0., 0.]).unwrap();
        let right = graph.add(NodeKind::Add, [0., 0.]).unwrap();
        let tip = graph.add(NodeKind::Add, [0., 0.]).unwrap();
        graph.connect(a, left, "a").unwrap();
        graph.connect(a, left, "b").unwrap();
        graph.connect(b, right, "b").unwrap();
        graph.connect(a, right, "a").unwrap();
        graph.connect(left, tip, "a").unwrap();
        graph.connect(right, tip, "b").unwrap();
        let big_id = NodeId::MAX - 1;
        graph.node_mut(tip).unwrap().id = big_id;
        graph.next_id = NodeId::MAX;
        graph.nodes.reverse();
        assert_eq!(
            graph.topological_order().unwrap(),
            [a, b, left, right, big_id]
        );
        graph
            .node_mut(right)
            .unwrap()
            .inputs
            .insert("b".into(), big_id);
        assert!(graph.topological_order().unwrap_err().contains("循環"));
    }

    #[test]
    fn failed_wire_keeps_previous_connection() {
        let mut g = Graph::default();
        let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = g.add(NodeKind::Add, [1., 0.]).unwrap();
        let c = g.add(NodeKind::Add, [2., 0.]).unwrap();
        g.connect(a, b, "a").unwrap();
        g.connect(b, c, "a").unwrap();
        let before = g.clone();
        assert!(g.connect(c, b, "a").is_err());
        assert_eq!(g, before);
    }
    #[test]
    fn impossible_types_are_rejected() {
        let mut g = Graph::default();
        let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = g.add(NodeKind::Extrude, [1., 0.]).unwrap();
        assert!(g.connect(a, b, "curve").is_err());
    }
    #[test]
    fn duplicate_ids_and_missing_sources_are_rejected() {
        let mut g = Graph::default();
        let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
        g.nodes.push(g.nodes[0].clone());
        assert!(g.validate().is_err());
        g.nodes.pop();
        g.node_mut(a).unwrap().inputs.insert("value".into(), 999);
        assert!(g.validate().is_err());
    }
    #[test]
    fn metadata_and_defaults_cover_every_node() {
        let mut g = Graph::default();
        for kind in NodeKind::all() {
            let id = g.add(*kind, [0., 0.]).unwrap();
            let node = g.node(id).unwrap();
            for spec in kind.spec().inputs {
                if let Some(default) = spec.default {
                    assert_eq!(node.params.get(spec.key), Some(&default));
                    assert!(default >= spec.min && default <= spec.max);
                }
            }
        }
        assert!(g.validate().is_ok());
    }
}
