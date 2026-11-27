//! Shared input-access metadata for the evaluator, inspector and machine-readable registry.
//!
//! This records the behavior of the current NODES adapters. `FirstItem` is an
//! explicit limitation: scalar settings of a list/tree adapter use its first value,
//! not an independently matched stream of settings. `SingleItem` requires exactly
//! one value per matched branch. These modes are not a GH1/GH2 compatibility claim.
use crate::model::NodeKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAccess {
    /// Ordinary per-item evaluation, using the node's item matching mode.
    Item,
    /// The complete list in a branch (or the path-list input of a tree adapter).
    List,
    /// The whole data tree, preserving paths and empty branches.
    Tree,
    /// Only the first value in the adapter's input scope is read.
    FirstItem,
    /// Exactly one value is required in each matched branch.
    SingleItem,
    /// Script/cluster adapters own their input semantics.
    Custom,
}
impl InputAccess {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Item => "Item",
            Self::List => "List",
            Self::Tree => "Tree",
            Self::FirstItem => "First item",
            Self::SingleItem => "Single item",
            Self::Custom => "Custom",
        }
    }
    pub const fn description(self) -> &'static str {
        match self {
            Self::Item => "対応する要素を1つずつ評価します。",
            Self::List => "枝のリスト全体をまとめて渡します。パス指定では整数リストを使います。",
            Self::Tree => "パスと空の枝を含むツリー全体を渡します。",
            Self::FirstItem => {
                "現在は入力範囲の先頭の値だけを設定に使います。設定値のリスト展開は行いません。"
            }
            Self::SingleItem => "対応する枝ごとに、ちょうど1個の値が必要です。",
            Self::Custom => "スクリプトまたはクラスターの入力規則に従います。",
        }
    }
}

/// Whether a node consumes branch lists, including legacy Merge/Loft special routes.
/// The evaluator must retain those two nodes' existing empty-input behavior when
/// using this helper in place of its former narrower list-adapter dispatch flag.
pub fn uses_list_access(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Merge | NodeKind::Loft | NodeKind::MeshConstruct
    ) || crate::gh_nodes::is_list_kind(kind)
        || crate::data_tree_nodes::is_list_kind(kind)
        || crate::math_nodes::is_list_kind(kind)
        || crate::curve_nodes::is_list_kind(kind)
        || crate::generative::is_list(kind)
        || crate::nodes_shape::is_list_kind(kind)
        || crate::nurbs_nodes::is_list_kind(kind)
}

/// Return the actual input-access contract, or None for a nonexistent port.
/// First-item scope is a matched branch for list adapters, and the complete input
/// tree for whole-tree/custom imports. A general mixed Item/List access scheduler
/// has not been implemented; this function deliberately does not imply one.
pub fn for_input(kind: NodeKind, index: usize) -> Option<InputAccess> {
    use InputAccess::*;
    use NodeKind::*;
    kind.spec().inputs.get(index)?;
    let access = match kind {
        Script | Cluster => Custom,
        ImportMesh => FirstItem,
        Output | Flatten | Graft => Tree,
        k if crate::gcode_nodes::is_kind(k) => Tree,
        MergeTree | EntwineTree | UnflattenTree => Tree,
        TreeBranch | TreeItem if index == 0 => Tree,
        TreeBranch | TreeItem if index == 1 => List,
        TreeItem => FirstItem,
        k if crate::data_tree_nodes::is_tree_kind(k) => {
            if index == 0 {
                Tree
            } else {
                FirstItem
            }
        }
        Merge | Loft if index == 0 => List,
        Loft => FirstItem,
        MeshConstruct => List,
        ExactNetworkSurface if index <= 1 => List,
        ExactStepExport => Tree,
        CurveWeightedNurbs if index <= 2 => List,
        CurveShatter if index <= 1 => List,
        PointArray if index == 1 => SingleItem,
        // These adapters consume both leading lists, not only the first one.
        ConcatList | SetContains | SetUnion | SetIntersection | SetDifference
        | DomainIntersection | DomainUnion | EvaluateDomain | ClosestPoint | MatrixMultiply
        | LinearSolve | MultiAttractor
            if index <= 1 =>
        {
            List
        }
        ListItem | CullPattern | CullIndex | ReplaceItems | InsertItems if index == 1 => List,
        ReplaceItems | InsertItems if index == 2 => List,
        MemberIndex if index == 1 => List,
        k if uses_list_access(k) => {
            if index == 0 {
                List
            } else {
                FirstItem
            }
        }
        _ => Item,
    };
    Some(access)
}

/// Only ordinary item adapters implement Shortest/Longest/Cross item matching.
/// The UI and graph validator must not expose those controls for list or tree
/// adapters: their branches may be matched ordinally, but their items are not.
pub fn supports_item_matching(kind: NodeKind) -> bool {
    kind.spec().inputs.len() > 1
        && kind
            .spec()
            .inputs
            .iter()
            .enumerate()
            .all(|(index, _)| for_input(kind, index) == Some(InputAccess::Item))
}

// Keep legacy project evaluation stable without relying on enum discriminants,
// registry order or a count that changes when new node families are added.
// evolve/all df77c542 has 64 catalog entries plus the custom Cluster kind.
const LEGACY_PATH_MATCHING_KINDS: &[NodeKind] = &[
    NodeKind::Number,
    NodeKind::Add,
    NodeKind::Multiply,
    NodeKind::Divide,
    NodeKind::Subtract,
    NodeKind::Power,
    NodeKind::Abs,
    NodeKind::Min,
    NodeKind::Max,
    NodeKind::Modulo,
    NodeKind::Sine,
    NodeKind::Series,
    NodeKind::Range,
    NodeKind::Point,
    NodeKind::Circle,
    NodeKind::Polygon,
    NodeKind::Star,
    NodeKind::Rectangle,
    NodeKind::Helix,
    NodeKind::Extrude,
    NodeKind::Loft,
    NodeKind::Revolve,
    NodeKind::Vase,
    NodeKind::Sphere,
    NodeKind::Box,
    NodeKind::Cylinder,
    NodeKind::Cone,
    NodeKind::Torus,
    NodeKind::Plane,
    NodeKind::Move,
    NodeKind::Rotate,
    NodeKind::Scale,
    NodeKind::ScaleXyz,
    NodeKind::Mirror,
    NodeKind::Twist,
    NodeKind::Wave,
    NodeKind::LinearArray,
    NodeKind::RadialArray,
    NodeKind::Merge,
    NodeKind::Flatten,
    NodeKind::Graft,
    NodeKind::Output,
    NodeKind::Script,
    NodeKind::Cluster,
    NodeKind::ListItem,
    NodeKind::ListLength,
    NodeKind::ReverseList,
    NodeKind::SortList,
    NodeKind::CullPattern,
    NodeKind::ShiftList,
    NodeKind::ConcatList,
    NodeKind::SubList,
    NodeKind::Bounds,
    NodeKind::Remap,
    NodeKind::Expression,
    NodeKind::Random,
    NodeKind::Populate,
    NodeKind::Noise,
    NodeKind::NoiseDeform,
    NodeKind::GraphProfile,
    NodeKind::Shell,
    NodeKind::OverhangCheck,
    NodeKind::Distance,
    NodeKind::PointComponent,
    NodeKind::ImportMesh,
];

/// New kernels use deterministic ordinal branch matching by default. Original
/// evolve/all kinds retain their exact-path/broadcast behavior unless the graph
/// explicitly chooses a matching mode. This flag is relevant only for adapters
/// evaluated branchwise; whole-tree operations continue to receive complete trees.
/// The chosen output topology is the documented NODES policy, not certified GH1.
pub fn uses_ordinal_matching(kind: NodeKind) -> bool {
    !LEGACY_PATH_MATCHING_KINDS.contains(&kind)
}
