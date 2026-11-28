//! Transactional sketch editing for NODES and downstream feature tools.
//! Uses the existing SolidCraft numerical/constructive solver; no duplicate kernel.
use super::parse_with_sources;
use solidcraft_geom::Vec2;
use solidcraft_sketch::{
    Budget, Sketch, SolveReport, constraint_residual, flips_from_stored, move_point_towards, solve,
    solve_after_edit,
};
use std::collections::{BTreeMap, BTreeSet};

/// An attempted solve. A rejected attempt never changes the session's stored sketch.
/// Its report describes the attempted solve, not necessarily the retained geometry.
#[derive(Debug)]
pub struct SolveAttempt {
    pub committed: bool,
    pub report: SolveReport,
}

impl SolveAttempt {
    /// Rank-based local degrees of freedom; absent for rejected attempts.
    pub fn dof(&self) -> Option<usize> {
        self.committed.then_some(self.report.dof)
    }
}

/// A constraint's origin and residual on the currently retained geometry.
/// A reference dimension has no residual because it does not constrain the sketch.
#[derive(Clone, Debug)]
pub struct ConstraintDiagnostic {
    pub id: String,
    pub source_line: Option<usize>,
    pub residual: Option<f64>,
}

/// Runtime editing session. Constraint IDs remain stable within one session; reparsing a
/// different source text can generate different IDs. The original text is not rewritten.
pub struct SketchSession {
    sketch: Sketch,
    sources: BTreeMap<String, usize>,
    solved: bool,
}

impl SketchSession {
    /// Parse a sketch without claiming it is satisfiable.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let (sketch, sources) = parse_with_sources(text)?;
        Ok(Self {
            sketch,
            sources,
            solved: false,
        })
    }

    pub fn sketch(&self) -> &Sketch {
        &self.sketch
    }

    pub fn is_solved(&self) -> bool {
        self.solved
    }

    pub fn source_line(&self, id: &str) -> Option<usize> {
        self.sources.get(id).copied()
    }

    pub fn diagnostic(&self, id: &str) -> Option<ConstraintDiagnostic> {
        let ci = self.sketch.constraints.iter().position(|c| c.id == id)?;
        Some(ConstraintDiagnostic {
            id: id.to_owned(),
            source_line: self.source_line(id),
            residual: constraint_residual(&self.sketch, ci).filter(|r| r.is_finite()),
        })
    }

    fn finish(&mut self, candidate: Sketch, report: SolveReport) -> SolveAttempt {
        let committed = report.keeps_orientation() && report.max_residual.is_finite();
        if committed {
            self.sketch = candidate;
            self.solved = true;
        }
        SolveAttempt { committed, report }
    }

    pub fn solve(&mut self) -> SolveAttempt {
        let mut candidate = self.sketch.clone();
        let report = solve(&mut candidate);
        self.finish(candidate, report)
    }

    /// Update a batch of driving dimensions by ID (millimetres or radians).
    /// Validate all changes first, then use continuation from the last solved state.
    /// Duplicate IDs, reference dimensions and externally driven dimensions are rejected.
    pub fn edit_dimensions(&mut self, changes: &[(&str, f64)]) -> Result<SolveAttempt, String> {
        if !self.solved {
            return Err("寸法変更の前にスケッチを解いてください。".into());
        }
        if changes.len() > self.sketch.constraints.len() {
            return Err("寸法変更が多すぎます。".into());
        }
        let mut candidate = self.sketch.clone();
        let old = candidate.driving_values();
        let stored: Vec<Vec2> = self.sketch.points.iter().map(|p| p.pos).collect();
        let mut seen = BTreeSet::new();
        for &(id, value) in changes {
            if !seen.insert(id) {
                return Err(format!("寸法 {id} が重複しています。"));
            }
            let c = candidate
                .constraints
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or_else(|| format!("寸法 {id} が見つかりません。"))?;
            if !c.kind.is_dimension() || c.driven || c.param.is_some() {
                return Err(format!("{id} は直接変更できる駆動寸法ではありません。"));
            }
            c.kind.set_value(value);
        }
        for c in &candidate.constraints {
            candidate.validate(&c.kind).map_err(|e| e.to_string())?;
        }
        let report = solve_after_edit(&mut candidate, &old, Some(&stored));
        Ok(self.finish(candidate, report))
    }

    /// Evaluate bounded NODES expressions with caller-supplied x/y/z/w, then edit
    /// the resulting dimensions atomically. Expressions are not persisted or evaluated
    /// automatically on future changes. An expression's result uses mm or radians.
    pub fn edit_expressions(
        &mut self,
        changes: &[(&str, &str)],
        vars: [f64; 4],
    ) -> Result<SolveAttempt, String> {
        if changes.len() > self.sketch.constraints.len() || vars.iter().any(|v| !v.is_finite()) {
            return Err("式の入力が無効です。".into());
        }
        let values = changes
            .iter()
            .map(|&(id, expression)| Ok((id, crate::expr::parse(expression)?.eval(vars)?)))
            .collect::<Result<Vec<_>, String>>()?;
        self.edit_dimensions(&values)
    }

    /// Move toward the cursor only in feasible freedom. A bounded frame may move
    /// part of the way; false means no accepted move. Work budget is not a deadline.
    pub fn drag_point(&mut self, id: &str, target: Vec2, max_step: f64) -> Result<bool, String> {
        if !self.solved {
            return Err("ドラッグの前にスケッチを解いてください。".into());
        }
        let p = self
            .sketch
            .resolve_point(id)
            .ok_or_else(|| format!("点 {id} が見つかりません。"))?;
        let mut candidate = self.sketch.clone();
        let mut budget = Budget::frame();
        let moved = move_point_towards(
            &mut candidate,
            p,
            target,
            max_step,
            Some(&mut budget),
            |previous, next| flips_from_stored(next, previous).is_empty(),
        );
        if moved && flips_from_stored(&candidate, &self.sketch).is_empty() {
            self.sketch = candidate;
            return Ok(true);
        }
        Ok(false)
    }

    /// Names violated/turned constraints and their original lines. Failure to
    /// converge is not a proof that the dimensions are mathematically inconsistent.
    pub fn failure_message(&self, attempt: &SolveAttempt) -> String {
        let report = &attempt.report;
        let why = report.failure(|names| names.to_owned());
        let mut seen = BTreeSet::new();
        let lines: Vec<_> = report
            .failing
            .iter()
            .chain(&report.folded)
            .filter(|id| seen.insert(id.as_str()))
            .filter_map(|id| self.source_line(id).map(|line| format!("{line} 行目 ({id})")))
            .collect();
        format!(
            "拘束を満たせませんでした（{}; 最大残差 {:.3e}; 反復 {} 回）。確認候補: {}。",
            why,
            report.max_residual,
            report.iterations,
            if lines.is_empty() {
                "拘束・初期配置・規模を確認".to_owned()
            } else {
                lines.join("、")
            }
        )
    }
}
