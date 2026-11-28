# GPU: bounded CPU-reference compute contract (#163)

Base: `8cac838229071509156d560391c3d1ba5237d761`.

Phase 1: establish a cancellable, bounded CPU reference and accelerator interface for vector operations; deterministic fallback and tolerance validation. Actual wgpu adapter is not included in this first increment; lock/dependency verification and real GPU tests remain. Related #163.

Local cargo/rustc and direct git access unavailable. Keep draft until CI and relevant native verification.
