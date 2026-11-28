//! On-demand input inspection for a failed node in a matching graph/evaluation.
//!
//! This does not execute kernels, apply transforms, allocate geometry or infer
//! error kinds from localized messages. Checks of upstream data are bounded
//! previews, not proof that the input is valid or the cause of the failure.

use crate::cluster;
use crate::data_tree::{PORT_PREFIX, PortTransform};
use crate::engine::Evaluation;
use crate::model::{DataType, Graph, InputSpec, NodeId, NodeKind, Tree, Value};

/// Count both branches and items so a tree with many empty paths is bounded too.
pub const MAX_INPUT_PREVIEW_WORK: usize = 128;

#[derive(Clone, Copy, Debug)]
pub enum SourceResult<'a> {
    Available(&'a Tree),
    Failed,
    NotEvaluated,
}

#[derive(Clone, Copy, Debug)]
pub enum InputState<'a> {
    Connected {
        source: NodeId,
        result: SourceResult<'a>,
    },
    LocalNumber(f64),
    MissingRequired,
    OptionalUnconnected,
    /// A default supplied by the cluster definition, not its generic port spec.
    ClusterDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputTransform {
    Configured(PortTransform),
    Invalid,
    /// Script and cluster inputs have their own access semantics.
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputIssue {
    TypeMismatch(DataType),
    NonFinite,
    OutOfRange { value: f64, min: f64, max: f64 },
    NonInteger(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputCheck<'a> {
    NotChecked,
    NoIssueInPreview {
        limited: bool,
    },
    Issue {
        path: Option<&'a [u32]>,
        index: Option<usize>,
        issue: InputIssue,
    },
}

#[derive(Debug)]
pub struct FailureInput<'a> {
    pub key: &'static str,
    pub label: String,
    pub expected: DataType,
    pub state: InputState<'a>,
    pub transform: InputTransform,
    pub check: InputCheck<'a>,
}

fn value_issue(spec: &InputSpec, value: &Value) -> Option<InputIssue> {
    if !spec.kind.accepts(value.kind()) {
        return Some(InputIssue::TypeMismatch(value.kind()));
    }
    if spec.default.is_some()
        && let Value::Number(n) = value
    {
        if !n.is_finite() {
            return Some(InputIssue::NonFinite);
        }
        if *n < spec.min || *n > spec.max {
            return Some(InputIssue::OutOfRange {
                value: *n,
                min: spec.min,
                max: spec.max,
            });
        }
        if spec.integer && n.fract() != 0. {
            return Some(InputIssue::NonInteger(*n));
        }
    }
    None
}

fn check_tree<'a>(spec: &InputSpec, tree: &'a Tree) -> InputCheck<'a> {
    let mut remaining = MAX_INPUT_PREVIEW_WORK;
    for (path, values) in &tree.branches {
        if remaining == 0 {
            return InputCheck::NoIssueInPreview { limited: true };
        }
        remaining -= 1;
        for (index, value) in values.iter().enumerate() {
            if remaining == 0 {
                return InputCheck::NoIssueInPreview { limited: true };
            }
            remaining -= 1;
            if let Some(issue) = value_issue(spec, value) {
                return InputCheck::Issue {
                    path: Some(path),
                    index: Some(index),
                    issue,
                };
            }
        }
    }
    InputCheck::NoIssueInPreview { limited: false }
}

/// Inspect declared inputs of one enabled, failed top-level node. The caller
/// must provide the same graph revision that produced the evaluation and reject
/// structural graph errors before presenting these observations. An empty tree
/// is a successful upstream result, not intrinsically a failure.
///
/// Connected trees are borrowed, never cloned. We check raw inputs only when
/// the ordinary evaluator would consume them without a port transform. Local
/// number and tree checks mirror its type/range/integer checks, but do not
/// execute cross-input constraints or geometry kernels. A finding is evidence
/// about a port, not necessarily the first error the evaluator encountered.
pub fn inspect_failure_inputs<'a>(
    graph: &Graph,
    evaluation: &'a Evaluation,
    id: NodeId,
) -> Option<Vec<FailureInput<'a>>> {
    let node = graph.node(id)?;
    if node.off || !evaluation.outputs.get(&id).is_some_and(Result::is_err) {
        return None;
    }
    let mut result = Vec::new();
    for (index, spec) in node
        .kind
        .spec()
        .inputs
        .iter()
        .enumerate()
        .take(cluster::real_input_count(graph, node))
    {
        let state = if let Some(&source) = node.inputs.get(spec.key) {
            let result = match evaluation.outputs.get(&source) {
                Some(Ok(tree)) => SourceResult::Available(tree),
                Some(Err(_)) => SourceResult::Failed,
                None => SourceResult::NotEvaluated,
            };
            InputState::Connected { source, result }
        } else if cluster::input_needs_link(graph, node, index, spec) {
            InputState::MissingRequired
        } else if cluster::input_is_optional(node, spec) {
            InputState::OptionalUnconnected
        } else if node.kind == NodeKind::Cluster {
            match node.params.get(spec.key) {
                Some(&value) => InputState::LocalNumber(value),
                None => InputState::ClusterDefault,
            }
        } else if let Some(default) = spec.default {
            InputState::LocalNumber(node.params.get(spec.key).copied().unwrap_or(default))
        } else {
            InputState::OptionalUnconnected
        };
        let transform = if matches!(node.kind, NodeKind::Script | NodeKind::Cluster) {
            InputTransform::Custom
        } else {
            let raw = node
                .params
                .get(&format!("{PORT_PREFIX}{}", spec.key))
                .copied()
                .unwrap_or(0.);
            match PortTransform::from_number(raw) {
                Ok(transform) => InputTransform::Configured(transform),
                Err(_) => InputTransform::Invalid,
            }
        };
        let check = if transform == InputTransform::Configured(PortTransform::None) {
            match state {
                InputState::Connected {
                    result: SourceResult::Available(tree),
                    ..
                } => check_tree(spec, tree),
                InputState::LocalNumber(value) => match value_issue(spec, &Value::Number(value)) {
                    Some(issue) => InputCheck::Issue {
                        path: None,
                        index: None,
                        issue,
                    },
                    None => InputCheck::NoIssueInPreview { limited: false },
                },
                _ => InputCheck::NotChecked,
            }
        } else {
            InputCheck::NotChecked
        };
        result.push(FailureInput {
            key: spec.key,
            label: cluster::input_label(graph, node, index, spec),
            expected: spec.kind,
            state,
            transform,
            check,
        });
    }
    Some(result)
}
