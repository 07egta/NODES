//! Conservative opt-in Boolean acceptance. Uses the existing native kernel; no mesh Boolean.
//! A success passed necessary topology and sampled geometry checks, not a proof of exactness.
use crate::{Body, BoolOp, BooleanDiagnostics, KernelError, boolean, capture_boolean_diagnostics, guard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanStatus {
    Success,
    Approximate,
    InvalidInput,
    AmbiguousIntersection,
    UnresolvedTopology,
    NumericalFailure,
    Cancelled,
}

/// Only Success or Approximate carries a body; empty candidates remain explicitly ambiguous.
#[derive(Debug)]
pub struct CheckedBoolean {
    pub status: BooleanStatus,
    pub body: Option<Body>,
    pub diagnostics: BooleanDiagnostics,
    pub detail: String,
}

fn closed(body: &Body) -> bool {
    !body.is_mesh()
        && !body.solid.boundaries().is_empty()
        && body.solid.boundaries().iter().all(|shell| shell.shell_condition() == truck_modeling::ShellCondition::Closed)
}

fn result(status: BooleanStatus, body: Option<Body>, diagnostics: BooleanDiagnostics, detail: impl Into<String>) -> CheckedBoolean {
    CheckedBoolean { status, body, diagnostics, detail: detail.into() }
}

/// Cancellation is checked at stage boundaries; it does not preempt a running native Boolean.
/// No silently accepted unverified shortcut or missing candidate. The legacy Boolean API is
/// retained for existing callers, which may have their own specialized empty-result handling.
pub fn boolean_checked(a: &Body, b: &Body, op: BoolOp, stop: &dyn Fn() -> bool) -> CheckedBoolean {
    use BooleanStatus::*;
    let empty = BooleanDiagnostics::default();
    if stop() {
        return result(Cancelled, None, empty, "cancelled before input validation");
    }
    match guard("Boolean input topology", || Ok(closed(a) && closed(b))) {
        Ok(true) => {}
        Ok(false) => return result(InvalidInput, None, empty, "input: expected nonempty closed native B-rep shells"),
        Err(e) => return result(NumericalFailure, None, empty, format!("input validation: {e}")),
    }
    if stop() {
        return result(Cancelled, None, empty, "cancelled before native Boolean");
    }
    let (candidate, diagnostics) = capture_boolean_diagnostics(|| guard("checked Boolean", || boolean(a, b, op)));
    if stop() {
        return result(Cancelled, None, diagnostics, "cancelled after native Boolean");
    }
    if diagnostics.truncated || diagnostics.unverified().is_some() {
        return result(AmbiguousIntersection, None, diagnostics, "classification: incomplete retry report or unverified shortcut");
    }
    let body = match candidate {
        Ok(Some(body)) => body,
        Ok(None) if op == BoolOp::Union => return result(UnresolvedTopology, None, diagnostics, "reconstruction: unexpected empty union"),
        Ok(None) => return result(AmbiguousIntersection, None, diagnostics, "classification: empty candidate; absence of shared material not independently proved"),
        Err(e) => {
            let status = if matches!(&e, KernelError::Invalid(_)) { InvalidInput } else { NumericalFailure };
            return result(status, None, diagnostics, format!("native Boolean: {e}"));
        }
    };
    match guard("Boolean result topology", || Ok(closed(&body))) {
        Ok(true) => {}
        Ok(false) => return result(UnresolvedTopology, None, diagnostics, "reconstruction: not a closed native B-rep"),
        Err(e) => return result(NumericalFailure, None, diagnostics, format!("topology validation: {e}")),
    }
    if stop() {
        return result(Cancelled, None, diagnostics, "cancelled before candidate validation");
    }
    let validation = guard("Boolean candidate validation", || crate::boolean_guard::check_retry(a, b, op, Some(&body)));
    if stop() {
        return result(Cancelled, None, diagnostics, "cancelled after candidate validation");
    }
    if let Err(e) = validation {
        return result(NumericalFailure, None, diagnostics, format!("candidate validation: {e}"));
    }
    let altered = diagnostics.events.iter().any(|e| e.strategy.alters_result());
    if altered {
        result(Approximate, Some(body), diagnostics, "validated candidate used a geometry-altering retry; not an exact result")
    } else {
        result(Success, Some(body), diagnostics, "native shell and sampled checks passed; not a certified Boolean")
    }
}
