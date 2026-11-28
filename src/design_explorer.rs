//! Bounded, offline design exploration using the restricted graph API.
use crate::graph_api::{self, Edit, Request};
use crate::model::{Graph, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_TRIALS: usize = 512;
const MAX_VARIABLES: usize = 16;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub node: NodeId,
    pub input: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub node: NodeId,
    pub direction: Direction,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub node: NodeId,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Search {
    Grid,
    Random { seed: u64 },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub variables: Vec<Variable>,
    pub objectives: Vec<Objective>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    pub search: Search,
    pub budget: usize,
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub values: Vec<f64>,
    pub objectives: Vec<f64>,
    pub feasible: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub candidates: Vec<Candidate>,
    pub pareto: Vec<usize>,
    pub cancelled: bool,
}

fn edits(study: &Study, values: &[f64]) -> Request {
    Request {
        api_version: graph_api::API_VERSION,
        edits: study
            .variables
            .iter()
            .zip(values)
            .map(|(v, &value)| Edit::Set {
                node: v.node.to_string(),
                input: v.input.clone(),
                value,
            })
            .collect(),
    }
}

fn validate(graph: &Graph, study: &Study) -> Result<usize, String> {
    graph_api::validate_safe(graph)?;
    if !(1..=MAX_TRIALS).contains(&study.budget)
        || !(1..=MAX_VARIABLES).contains(&study.variables.len())
        || !(1..=8).contains(&study.objectives.len())
        || study.constraints.len() > 16
    {
        return Err("invalid study size or budget".into());
    }
    let mut seen = BTreeSet::new();
    let mut combinations = 1usize;
    for v in &study.variables {
        let n = graph.node(v.node).ok_or("unknown variable node")?;
        let port = n
            .kind
            .spec()
            .inputs
            .iter()
            .find(|p| p.key == v.input && p.default.is_some())
            .ok_or("unknown numeric variable port")?;
        if n.off
            || n.inputs.contains_key(&v.input)
            || !seen.insert((v.node, &v.input))
            || !(1..=MAX_TRIALS).contains(&v.values.len())
            || v.values.iter().any(|x| {
                !x.is_finite()
                    || *x < port.min
                    || *x > port.max
                    || (port.integer && x.fract() != 0.)
            })
        {
            return Err("invalid, connected or duplicate variable".into());
        }
        combinations = combinations.saturating_mul(v.values.len());
    }
    for id in study
        .objectives
        .iter()
        .map(|o| o.node)
        .chain(study.constraints.iter().map(|c| c.node))
    {
        let n = graph.node(id).ok_or("unknown metric node")?;
        if n.off || n.kind.spec().output != crate::model::DataType::Number {
            return Err("metric must be an enabled numeric node".into());
        }
    }
    if study
        .constraints
        .iter()
        .any(|c| !c.min.is_finite() || !c.max.is_finite() || c.min > c.max)
    {
        return Err("invalid constraint bounds".into());
    }
    if matches!(study.search, Search::Grid) && combinations > study.budget {
        return Err("grid exceeds evaluation budget; reduce axes or use random search".into());
    }
    Ok(combinations)
}

// SplitMix64: deterministic sampling, not cryptographic randomness.
fn next(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn scalar(e: &crate::engine::Evaluation, id: NodeId) -> Result<f64, String> {
    let tree = e
        .outputs
        .get(&id)
        .ok_or("missing metric")?
        .as_ref()
        .map_err(Clone::clone)?;
    if tree.item_count() != 1 {
        return Err("metric must contain exactly one number".into());
    }
    tree.branches
        .values()
        .flatten()
        .next()
        .ok_or("empty metric")?
        .number()
}

fn dominates(a: &Candidate, b: &Candidate, objectives: &[Objective]) -> bool {
    let mut strict = false;
    for ((x, y), o) in a.objectives.iter().zip(&b.objectives).zip(objectives) {
        let order = match o.direction {
            Direction::Minimize => x.partial_cmp(y),
            Direction::Maximize => y.partial_cmp(x),
        };
        match order {
            Some(std::cmp::Ordering::Less) => strict = true,
            Some(std::cmp::Ordering::Equal) => {}
            _ => return false,
        }
    }
    strict
}

/// Each evaluation owns only one candidate graph and result; the report stores no geometry.
/// Cancellation is cooperative through the existing evaluator.
pub fn explore(
    graph: &Graph,
    study: &Study,
    cancel: &(dyn Fn() -> bool + Sync),
) -> Result<Report, String> {
    let combinations = validate(graph, study)?;
    let (count, mut seed) = match study.search {
        Search::Grid => (combinations, 0),
        Search::Random { seed } => (study.budget, seed),
    };
    let mut report = Report {
        candidates: Vec::with_capacity(count),
        pareto: Vec::new(),
        cancelled: false,
    };
    for trial in 0..count {
        if cancel() {
            report.cancelled = true;
            break;
        }
        let mut index = trial;
        let values: Vec<_> = study
            .variables
            .iter()
            .map(|v| {
                let i = match study.search {
                    Search::Grid => {
                        let i = index % v.values.len();
                        index /= v.values.len();
                        i
                    }
                    Search::Random { .. } => (next(&mut seed) % v.values.len() as u64) as usize,
                };
                v.values[i]
            })
            .collect();
        let mut candidate = Candidate {
            values,
            objectives: Vec::new(),
            feasible: false,
            error: None,
        };
        let result = (|| -> Result<bool, String> {
            let mut g = graph.clone();
            graph_api::apply(&mut g, &edits(study, &candidate.values))?;
            let e = graph_api::evaluate(&g, cancel)?;
            if e.cancelled {
                report.cancelled = true;
                return Ok(false);
            }
            if e.outputs.len() != g.nodes.len() || e.outputs.values().any(Result::is_err) {
                return Err("candidate graph evaluation failed".into());
            }
            candidate.objectives = study
                .objectives
                .iter()
                .map(|o| scalar(&e, o.node))
                .collect::<Result<_, _>>()?;
            let mut feasible = true;
            for c in &study.constraints {
                let x = scalar(&e, c.node)?;
                feasible &= x >= c.min && x <= c.max;
            }
            Ok(feasible)
        })();
        if report.cancelled {
            break;
        }
        match result {
            Ok(feasible) => candidate.feasible = feasible,
            Err(e) => candidate.error = Some(e.chars().take(256).collect()),
        }
        report.candidates.push(candidate);
    }
    report.pareto = report
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.feasible)
        .filter(|(_, c)| {
            !report
                .candidates
                .iter()
                .any(|other| other.feasible && dominates(other, c, &study.objectives))
        })
        .map(|(i, _)| i)
        .collect();
    Ok(report)
}

/// Replay an evaluated parameter vector without changing the original graph.
pub fn replay(graph: &Graph, study: &Study, values: &[f64]) -> Result<Graph, String> {
    validate(graph, study)?;
    if values.len() != study.variables.len()
        || study
            .variables
            .iter()
            .zip(values)
            .any(|(v, x)| !v.values.iter().any(|y| y.to_bits() == x.to_bits()))
    {
        return Err("candidate values are outside the study".into());
    }
    let mut candidate = graph.clone();
    graph_api::apply(&mut candidate, &edits(study, values))?;
    Ok(candidate)
}
