//! Bounded CPU reference and optional accelerator boundary for approximate compute.
//!
//! This module does not initialize a GPU. The adapter is supplied by the caller;
//! call on an evaluation worker, never the UI thread. Exact f64 CAD predicates
//! and NURBS/B-rep evaluation must not use this approximate f32 contract.

use std::sync::atomic::{AtomicBool, Ordering};

/// At most 12 MiB of raw vector data across two inputs and two outputs.
pub const MAX_VECTOR_COUNT: usize = 262_144;
pub type Vector = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeMode {
    CpuOnly,
    PreferAccelerator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeError {
    LengthMismatch,
    LimitExceeded,
    NonFiniteInput,
    NonFiniteResult,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceleratorFailure {
    Unavailable,
    DeviceLost,
    ResourceLimit,
    Dispatch,
    Readback,
    InvalidOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    CpuForced,
    CpuUnavailable,
    CpuFallback(AcceleratorFailure),
    AcceleratorVerified,
}

#[derive(Debug, PartialEq)]
pub struct VectorResult {
    pub values: Vec<Vector>,
    pub backend: Backend,
}

/// An adapter must bound device allocation, validate dispatch dimensions, and
/// observe cancellation between dispatch/readback stages. A future wgpu adapter
/// must explicitly pack vec3 data (WGSL storage array stride is 16 bytes).
pub trait VectorAccelerator {
    fn add(
        &mut self,
        lhs: &[Vector],
        rhs: &[Vector],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vector>, AcceleratorFailure>;
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), ComputeError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ComputeError::Cancelled)
    } else {
        Ok(())
    }
}

/// Deterministic finite f32 output, computed through f64 to make rounding
/// explicit. Invalid values are rejected before an accelerator is called.
fn cpu_add(
    lhs: &[Vector],
    rhs: &[Vector],
    cancel: &AtomicBool,
) -> Result<Vec<Vector>, ComputeError> {
    check_cancel(cancel)?;
    if lhs.len() != rhs.len() {
        return Err(ComputeError::LengthMismatch);
    }
    if lhs.len() > MAX_VECTOR_COUNT {
        return Err(ComputeError::LimitExceeded);
    }

    let mut output = Vec::with_capacity(lhs.len());
    for (index, (a, b)) in lhs.iter().zip(rhs).enumerate() {
        if index % 256 == 0 {
            check_cancel(cancel)?;
        }
        if a.iter().chain(b).any(|component| !component.is_finite()) {
            return Err(ComputeError::NonFiniteInput);
        }
        let sum = std::array::from_fn(|i| (f64::from(a[i]) + f64::from(b[i])) as f32);
        if sum.iter().any(|component| !component.is_finite()) {
            return Err(ComputeError::NonFiniteResult);
        }
        output.push(sum);
    }
    check_cancel(cancel)?;
    Ok(output)
}

fn matches_reference(
    actual: &[Vector],
    reference: &[Vector],
    cancel: &AtomicBool,
) -> Result<bool, ComputeError> {
    if actual.len() != reference.len() {
        return Ok(false);
    }
    for (index, (actual, reference)) in actual.iter().zip(reference).enumerate() {
        if index % 256 == 0 {
            check_cancel(cancel)?;
        }
        for (&actual, &reference) in actual.iter().zip(reference) {
            if !actual.is_finite() {
                return Ok(false);
            }
            // Relative tolerance only: do not silently accept flushing small
            // nonzero reference values to zero.
            let difference = (f64::from(actual) - f64::from(reference)).abs();
            let tolerance = 4.0 * f64::from(f32::EPSILON) * f64::from(reference).abs();
            if difference > tolerance {
                return Ok(false);
            }
        }
    }
    check_cancel(cancel)?;
    Ok(true)
}

/// Verification-mode vector addition. The CPU reference is intentionally
/// calculated for every request; this is a correctness gate, not a speedup.
/// Empty workloads never dispatch. Any accelerator failure returns the
/// reference with an explicit backend status; cancellation never publishes a
/// partial result.
pub fn add_vectors(
    lhs: &[Vector],
    rhs: &[Vector],
    mode: ComputeMode,
    accelerator: Option<&mut dyn VectorAccelerator>,
    cancel: &AtomicBool,
) -> Result<VectorResult, ComputeError> {
    let reference = cpu_add(lhs, rhs, cancel)?;
    let (values, backend) = match (mode, accelerator) {
        (ComputeMode::CpuOnly, _) => (reference, Backend::CpuForced),
        (_, None) => (reference, Backend::CpuUnavailable),
        (_, Some(_)) if reference.is_empty() => (reference, Backend::CpuForced),
        (_, Some(accelerator)) => {
            let result = accelerator.add(lhs, rhs, cancel);
            check_cancel(cancel)?;
            match result {
                Ok(values) if matches_reference(&values, &reference, cancel)? => {
                    (values, Backend::AcceleratorVerified)
                }
                Ok(_) => (
                    reference,
                    Backend::CpuFallback(AcceleratorFailure::InvalidOutput),
                ),
                Err(error) => (reference, Backend::CpuFallback(error)),
            }
        }
    };
    check_cancel(cancel)?;
    Ok(VectorResult { values, backend })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub {
        calls: usize,
        result: Result<Vec<Vector>, AcceleratorFailure>,
        cancel: bool,
    }

    impl VectorAccelerator for Stub {
        fn add(
            &mut self,
            _: &[Vector],
            _: &[Vector],
            cancel: &AtomicBool,
        ) -> Result<Vec<Vector>, AcceleratorFailure> {
            self.calls += 1;
            if self.cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            self.result.clone()
        }
    }

    const A: [Vector; 1] = [[1.0, -2.0, 3.0]];
    const B: [Vector; 1] = [[4.0, 2.0, -1.0]];
    const EXPECTED: Vector = [5.0, 0.0, 2.0];

    #[test]
    fn forced_cpu_and_missing_adapter_return_reference() {
        let cancel = AtomicBool::new(false);
        let mut stub = Stub {
            calls: 0,
            result: Err(AcceleratorFailure::DeviceLost),
            cancel: false,
        };
        let forced = add_vectors(&A, &B, ComputeMode::CpuOnly, Some(&mut stub), &cancel)
            .expect("finite input");
        assert_eq!(forced.values, vec![EXPECTED]);
        assert_eq!(forced.backend, Backend::CpuForced);
        assert_eq!(stub.calls, 0);
        let absent = add_vectors(&A, &B, ComputeMode::PreferAccelerator, None, &cancel)
            .expect("finite input");
        assert_eq!(absent.values, vec![EXPECTED]);
        assert_eq!(absent.backend, Backend::CpuUnavailable);
    }

    #[test]
    fn valid_adapter_result_and_explicit_fallback() {
        let cancel = AtomicBool::new(false);
        for (result, backend) in [
            (Ok(vec![EXPECTED]), Backend::AcceleratorVerified),
            (
                Ok(vec![[5.0, f32::NAN, 2.0]]),
                Backend::CpuFallback(AcceleratorFailure::InvalidOutput),
            ),
            (
                Ok(vec![[5.0, 1.0, 2.0]]),
                Backend::CpuFallback(AcceleratorFailure::InvalidOutput),
            ),
            (
                Ok(Vec::new()),
                Backend::CpuFallback(AcceleratorFailure::InvalidOutput),
            ),
            (
                Err(AcceleratorFailure::DeviceLost),
                Backend::CpuFallback(AcceleratorFailure::DeviceLost),
            ),
        ] {
            let mut stub = Stub {
                calls: 0,
                result,
                cancel: false,
            };
            let output = add_vectors(
                &A,
                &B,
                ComputeMode::PreferAccelerator,
                Some(&mut stub),
                &cancel,
            )
            .expect("finite input");
            assert_eq!(output.values, vec![EXPECTED]);
            assert_eq!(output.backend, backend);
            assert_eq!(stub.calls, 1);
        }
    }

    #[test]
    fn cancellation_never_returns_a_fallback_result() {
        let cancel = AtomicBool::new(false);
        let mut stub = Stub {
            calls: 0,
            result: Ok(vec![EXPECTED]),
            cancel: true,
        };
        assert_eq!(
            add_vectors(
                &A,
                &B,
                ComputeMode::PreferAccelerator,
                Some(&mut stub),
                &cancel
            ),
            Err(ComputeError::Cancelled)
        );
        assert_eq!(stub.calls, 1);
        assert_eq!(
            add_vectors(&A, &B, ComputeMode::CpuOnly, None, &cancel),
            Err(ComputeError::Cancelled)
        );
    }

    #[test]
    fn rejects_malformed_and_nonfinite_inputs_before_dispatch() {
        let cancel = AtomicBool::new(false);
        let mut stub = Stub {
            calls: 0,
            result: Ok(vec![EXPECTED]),
            cancel: false,
        };
        let huge = vec![[0.0; 3]; MAX_VECTOR_COUNT + 1];
        for (a, b, expected) in [
            (&A[..], &[][..], ComputeError::LengthMismatch),
            (&huge[..], &huge[..], ComputeError::LimitExceeded),
            (
                &[[f32::NAN, 0.0, 0.0]][..],
                &B[..],
                ComputeError::NonFiniteInput,
            ),
            (
                &[[f32::MAX, 0.0, 0.0]][..],
                &[[f32::MAX, 0.0, 0.0]][..],
                ComputeError::NonFiniteResult,
            ),
        ] {
            assert_eq!(
                add_vectors(a, b, ComputeMode::PreferAccelerator, Some(&mut stub), &cancel),
                Err(expected)
            );
        }
        assert_eq!(stub.calls, 0);
    }

    #[test]
    fn tiny_nonzero_values_are_not_flushed_to_zero() {
        let cancel = AtomicBool::new(false);
        let tiny = f32::from_bits(1);
        assert_eq!(
            matches_reference(&[[0.0; 3]], &[[tiny, 0.0, 0.0]], &cancel),
            Ok(false)
        );
    }
}
