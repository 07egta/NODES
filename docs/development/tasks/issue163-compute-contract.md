# GPU: bounded CPU-reference compute contract (#163)

Base: `8cac838229071509156d560391c3d1ba5237d761`.

Phase 1: establish a cancellable, bounded CPU reference and accelerator interface for vector operations; deterministic fallback and tolerance validation. Actual wgpu adapter is not included in this first increment; lock/dependency verification and real GPU tests remain. Related #163.

Local cargo/rustc and direct git access unavailable. Keep draft until CI and relevant native verification.

## Increment 1

Added `src/compute.rs`: finite bounded f32 vector add, explicit CPU-only and missing-adapter paths, checked accelerator results, typed failures and cooperative cancellation. Unit tests use stub adapters only; they are not GPU evidence. The CPU oracle always runs, so no performance gain is claimed. Maximum vector count 262144; adapter-side allocation and timeout still need implementation. No graph/persistence changes, no new dependency.

Cargo.lock already includes wgpu 27.0.1 as an optional transitive entry. The current eframe feature set is Glow, not wgpu. Enabling real compute backends changes the resolved feature/dependency graph and must be verified using Cargo and license/notice audit before publishing. Do not treat the lock entry as a compiled GPU backend. Remaining #163: optional feature, asynchronous adapter lifecycle, WGSL dispatch/readback, resource limits/device loss, feature-matrix CI, actual Intel Arc measurements. #164–#172 depend on these gates.
