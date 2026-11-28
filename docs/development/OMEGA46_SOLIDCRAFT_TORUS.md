# OMEGA 46 — SolidCraft ring torus

## Scope and provenance (2026-11-28 JST)

- NODES base: [c1213c88](https://github.com/07egta/NODES/commit/c1213c885647c2e708017a63f6b7bdc8f9b66620).
- [AGENTS.md](../../AGENTS.md), [SOLIDCRAFT_CHANGES.md](../SOLIDCRAFT_CHANGES.md), [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), [ARCHITECTURE.md](../ARCHITECTURE.md) and the existing integration [#48](https://github.com/07egta/NODES/pull/48), [#67](https://github.com/07egta/NODES/pull/67), [#81](https://github.com/07egta/NODES/pull/81) reviewed. #81 covers sketch, IGES and diagnostic work; do not duplicate it.
- Reuse the already vendored SolidCraft 0.26.0 `solidcraft_kernel::torus`, not a second implementation. [melt3D source at 7f18150bdcc156944fee71617705d66213757dab](https://github.com/07egta/melt3D/blob/7f18150bdcc156944fee71617705d66213757dab/crates/kernel/src/build.rs) and `crates/sc-kernel/src/build.rs` have matching Git blob SHA `6f3ce61d91e02e4861b36faf5a84bae78b1edb12` at the inspected base. Upstream and vendored LICENSE-MIT blobs match (`b5f6f14ccca9d2512186f5664e9a012bd5a31f0d`); LICENSE-APACHE blobs match (`f433b1a53f5b830a205fd2df78e2b34974656c7b`). MIT OR Apache-2.0; existing notices retained. No upstream code copied again.
- Plan: append a dedicated ExactTorus kind with stable keys, wrap the existing kernel with input checks, add icon and tests. Preserve existing mesh Torus and TorusSurface behavior. No changes to upstream, dependencies, Cargo.lock, saved project schema or Arclet CAD.
- Local environment: GitHub HTTPS DNS resolution failed and cargo/rustc are unavailable on PATH. Read-only source review through GitHub is possible; Rust build, rustfmt, Clippy and test execution are not locally available. CI must be verified before merge. No subagents available in this Chat.

## Implementation checkpoint

- ExactTorus appended to registry, catalog and evaluator; dedicated ring icon. Stable input keys `major` and `minor`. The kernel bridge rejects non-finite values, radii at/below ABS, major-minus-minor at/below ABS, and outer radius above coordinate cap. Calls use the existing cancellation/guard and B-rep preview path.
- Tests added in `tests/omega46_solidcraft_torus.rs`: analytic toroidal face and volume oracle, invalid inputs/cancellation, project JSON roundtrip and graph evaluation through volume node. Not executed locally.
- Manual example: add 厳密トーラス (major 30 mm, minor 8 mm), connect to B-rep の体積 (expected 37,899.28 mm³) or B-rep → mesh / STEP export. Geometry is B-rep; displayed mesh and volume integration are numerical approximations. STEP roundtrip and real GUI not yet tested.
- Integration owner: MISSION 21. Shared registry and bridge may conflict with #81 or other concurrent PRs; preserve both sets of appended kinds and stable keys. Do not merge before fresh CI.
- Next assets to inspect: body_distance (bounded search, explicit fallback quality), measure_mass area/centroid (open-sheet semantics), split_faces, blend_between and seam_diagnostics. Coordinate with #81 before modifying shared geometry code. No claims of completed integration for these assets.
