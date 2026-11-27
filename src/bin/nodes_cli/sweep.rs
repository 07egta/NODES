//! Bounded, deterministic parameter sweeps for the offline console.
//! This is read-only: no project or geometry files are written.
use super::{CliError, problem, tree_summary};
use nodes::engine::Evaluator;
use nodes::graph_api;
use nodes::model::{NodeId, Tree, Value};
use nodes::persistence::Project;
use serde::Deserialize;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

const MAX_AXES: usize = 8;
const MAX_AXIS_VALUES: usize = 64;
const MAX_CASES: usize = 128;
const MAX_OBSERVATIONS: usize = 8;
const MAX_NODE_VISITS: usize = 262_144;
const ERROR_PREVIEW: usize = 8;
const SWEEP_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SweepRequest {
    sweep_version: u32,
    axes: Vec<Axis>,
    observe: Vec<NodeId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Axis {
    node_id: NodeId,
    input: String,
    values: Vec<f64>,
}

fn invalid(message: impl AsRef<str>) -> CliError {
    problem("invalid_sweep", message)
}

fn validate(project: &Project, request: &SweepRequest) -> Result<usize, CliError> {
    if request.sweep_version != SWEEP_VERSION {
        return Err(invalid("Unsupported sweep version."));
    }
    if request.axes.is_empty() || request.axes.len() > MAX_AXES {
        return Err(invalid("A sweep needs 1 to 8 axes."));
    }
    if request.observe.is_empty() || request.observe.len() > MAX_OBSERVATIONS {
        return Err(invalid("A sweep needs 1 to 8 observation node IDs."));
    }
    let mut seen = BTreeSet::new();
    let mut cases = 1usize;
    for axis in &request.axes {
        if axis.values.is_empty() || axis.values.len() > MAX_AXIS_VALUES {
            return Err(invalid("Each axis needs 1 to 64 values."));
        }
        cases = cases
            .checked_mul(axis.values.len())
            .filter(|&count| count <= MAX_CASES)
            .ok_or_else(|| invalid("A sweep supports at most 128 combinations."))?;
        let node = project
            .graph
            .node(axis.node_id)
            .ok_or_else(|| invalid(format!("Unknown axis node {}.", axis.node_id)))?;
        let spec = node
            .kind
            .spec()
            .inputs
            .iter()
            .find(|p| p.key == axis.input.as_str() && p.default.is_some())
            .ok_or_else(|| invalid(format!("Unknown numeric input {}.", axis.input)))?;
        if !seen.insert((axis.node_id, axis.input.as_str())) {
            return Err(invalid("Duplicate axis for a node input."));
        }
        if node.off || node.inputs.contains_key(spec.key) {
            return Err(invalid("An axis must be an enabled, unconnected numeric input."));
        }
        if axis.values.iter().any(|&value| {
            !value.is_finite()
                || value < spec.min
                || value > spec.max
                || (spec.integer && value.fract() != 0.0)
        }) {
            return Err(invalid(format!(
                "Value outside the declared range for node {} input {}.",
                axis.node_id, axis.input
            )));
        }
    }
    let mut observed = BTreeSet::new();
    for &id in &request.observe {
        if !observed.insert(id) {
            return Err(invalid("Duplicate observation node."));
        }
        if !project.graph.node(id).is_some_and(|node| !node.off) {
            return Err(invalid(format!("Unknown or disabled observation node {id}.")));
        }
    }
    if cases.saturating_mul(project.graph.nodes.len()) > MAX_NODE_VISITS {
        return Err(invalid("Sweep exceeds the 262144 planned node visits limit."));
    }
    Ok(cases)
}

// Last axis changes fastest; never materialize a Cartesian-product allocation.
fn case_indices(mut ordinal: usize, axes: &[Axis]) -> Vec<usize> {
    let mut indices = vec![0; axes.len()];
    for (index, axis) in axes.iter().enumerate().rev() {
        indices[index] = ordinal % axis.values.len();
        ordinal /= axis.values.len();
    }
    indices
}

#[derive(Default)]
struct Metrics {
    numeric_range: Option<[f64; 2]>,
    sampled_bounds_mm: Option<[[f64; 3]; 2]>,
    mesh_vertices: usize,
    mesh_triangles: usize,
    unsampled_geometry_items: usize,
}

impl Metrics {
    fn point(&mut self, point: [f64; 3]) {
        if !point.iter().all(|value| value.is_finite()) {
            return;
        }
        let bounds = self.sampled_bounds_mm.get_or_insert([point, point]);
        for (axis, value) in point.into_iter().enumerate() {
            bounds[0][axis] = bounds[0][axis].min(value);
            bounds[1][axis] = bounds[1][axis].max(value);
        }
    }

    fn number(&mut self, number: f64) {
        if !number.is_finite() {
            return;
        }
        let range = self.numeric_range.get_or_insert([number, number]);
        range[0] = range[0].min(number);
        range[1] = range[1].max(number);
    }

    fn collect(tree: &Tree) -> Self {
        let mut metrics = Self::default();
        for value in tree.branches.values().flatten() {
            match value {
                Value::Number(number) => metrics.number(*number),
                Value::Point(point) => metrics.point(*point),
                Value::Curve(curve) => {
                    for &point in &curve.points {
                        metrics.point(point);
                    }
                }
                Value::Mesh(mesh) => {
                    metrics.mesh_vertices =
                        metrics.mesh_vertices.saturating_add(mesh.vertices.len());
                    metrics.mesh_triangles =
                        metrics.mesh_triangles.saturating_add(mesh.triangles.len());
                    for &point in &mesh.vertices {
                        metrics.point(point);
                    }
                }
                Value::Surface(_) | Value::Solid(_) | Value::Gcode(_) => {
                    metrics.unsampled_geometry_items += 1;
                }
            }
        }
        metrics
    }

    fn json(&self) -> Json {
        json!({
            "numeric_range": self.numeric_range,
            "sampled_bounds_mm": self.sampled_bounds_mm,
            "mesh_vertices": self.mesh_vertices,
            "mesh_triangles": self.mesh_triangles,
            "unsampled_geometry_items": self.unsampled_geometry_items
        })
    }
}

pub(super) fn run(project: &Project, request: &SweepRequest) -> Result<Json, CliError> {
    graph_api::validate_safe(&project.graph).map_err(invalid)?;
    let case_count = validate(project, request)?;
    let mut graph = project.graph.clone();
    let mut evaluator = Evaluator::default();
    let mut reports = Vec::with_capacity(case_count);
    let mut successes = 0;
    let mut total_computed = 0usize;
    let mut total_cached = 0usize;
    for ordinal in 0..case_count {
        let indices = case_indices(ordinal, &request.axes);
        let mut parameters = Vec::with_capacity(request.axes.len());
        for (axis, index) in request.axes.iter().zip(indices) {
            let value = axis.values[index];
            let node = graph
                .node_mut(axis.node_id)
                .ok_or_else(|| invalid("Validated axis node disappeared."))?;
            node.params.insert(axis.input.clone(), value);
            parameters.push(json!({
                "node_id": axis.node_id, "input": axis.input, "value": value
            }));
        }
        // Cross-parameter constraints still use the existing restricted graph policy.
        if let Err(message) = graph_api::validate_safe(&graph) {
            reports.push(json!({
                "case": ordinal, "parameters": parameters, "ok": false,
                "error": {"code": "invalid_graph", "message": message}
            }));
            continue;
        }
        let evaluation = evaluator.evaluate(&graph, &|| false);
        total_computed = total_computed.saturating_add(evaluation.computed);
        total_cached = total_cached.saturating_add(evaluation.cached);
        let mut error_count = 0;
        let mut errors = Vec::new();
        for node in &graph.nodes {
            let message = match evaluation.outputs.get(&node.id) {
                Some(Ok(_)) => continue,
                Some(Err(message)) => message.as_str(),
                None => "No evaluation result was produced.",
            };
            error_count += 1;
            if errors.len() < ERROR_PREVIEW {
                errors.push(json!({
                    "node_id": node.id,
                    "message": message.chars().take(super::MAX_ERROR_CHARS).collect::<String>()
                }));
            }
        }
        let ok = !evaluation.cancelled && error_count == 0;
        if ok {
            successes += 1;
        }
        let observations: Vec<_> = request
            .observe
            .iter()
            .map(|id| match evaluation.outputs.get(id) {
                Some(Ok(tree)) => json!({
                    "node_id": id, "data": tree_summary(tree), "metrics": Metrics::collect(tree).json()
                }),
                _ => json!({"node_id": id, "unavailable": true}),
            })
            .collect();
        reports.push(json!({
            "case": ordinal, "parameters": parameters, "ok": ok,
            "computed": evaluation.computed, "cached": evaluation.cached,
            "elapsed_ms": evaluation.elapsed_ms, "cancelled": evaluation.cancelled,
            "error_count": error_count, "errors": errors,
            "errors_truncated": error_count > ERROR_PREVIEW, "observations": observations
        }));
    }
    Ok(json!({
        "sweep_version": SWEEP_VERSION,
        "case_count": case_count,
        "successful_cases": successes,
        "failed_cases": case_count - successes,
        "all_cases_succeeded": successes == case_count,
        "total_computed": total_computed,
        "total_cached": total_cached,
        "cases": reports
    }))
}
