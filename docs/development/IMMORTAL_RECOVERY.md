# PROJECT IMMORTAL — recovery and fault containment

Started 2026-11-28 JST from `8cac838229071509156d560391c3d1ba5237d761` (main). Existing atomic save, 8 MiB serialization bound, graph validation, revision cancellation, and pending-preview retention are retained. The current single global recovery file can be replaced by another document/session; recovery candidates need independent ownership and validation. Prioritize data preservation over automatic deletion or restoration to an original project path.

Scope: atomic save failure cleanup, session/document recovery ownership, bounded validation, regression/fault injection tests. No schema changes or dependency additions. Existing open PRs were checked; no direct recovery PR was identified. `src/app.rs` also has unrelated pending UI work (e.g. #173); merge by focused diff. No direct main updates.

Validation: pending. This environment cannot clone GitHub directly and has no Rust toolchain; use repository CI, report exact results, and do not merge unverified code. Native UI, actual power loss, GPU loss, allocator abort and non-cooperative kernels require separate evidence.
