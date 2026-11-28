# OMEGA 92 — SOLID: first bounded acceptance increment

Baseline: `09a2a546d442fea822d80fb4c756a81f067a8b00`. OMEGA 74 PR #140 already changes the NODES bridge and native shell query. This PR avoids those source files (except the shared change log) and addresses the vendored kernel. OMEGA 72/87/91 are not assumed merged; no general certified surface-intersection result was identified in main. Existing SolidCraft 0.26.0 (MIT OR Apache-2.0) and vendored truck (Apache-2.0) remain in use; no new dependency or upstream code is imported.

## Findings and changes
- `boolean_guard::check_retry` previously returned success immediately for *every* intersection. Nonempty native intersection retries now check closed shells, positive finite volumes, the smaller operand's volume bound, both operand bounds, and a bounded sample of boundary points near either source surface.
- A failed tessellation must not approve an empty cut. The existing empty-cut bounding-box heuristic is still not a containment proof.
- `boolean_checked::boolean_checked` is a conservative opt-in typed result for callers: native input/output checks, scoped retry diagnostics, ambiguity for unverified/truncated/empty candidates, explicit approximation, numerical failures and cancellation at stage boundaries. The legacy API stays in place; existing NODES node/bridge does not yet use the new typed API.
- A sampled volume or surface test only rejects obvious inconsistencies. No claim of certified trimmed-NURBS containment, absence of self-intersections, vertex-manifoldness, UV residual bounds, or certified topology. An empty result is not a certificate of no material intersection. Existing union/cut checks still have limitations. Smaller volume slack for intersections is a conservative numerical policy, not a rigorous error interval.
- The existing healing and history code is reused unchanged. No new sewing, topology IDs, displacements, or healing tolerance increases.

## Reproduce
```sh
cargo test --locked --test omega92_boolean
cargo test --locked -p solidcraft-kernel --lib omega92_tests
cargo test --locked --release --test omega92_boolean seeded_box_stress -- --ignored --nocapture
```
The public integration tests use analytical box volumes, commutativity, idempotence, empty self-difference status and cancellation. The internal fault-injection test supplies too-large, misplaced, and invented interior intersection bodies plus a correct candidate. The ignored deterministic workload times 96 operations; it is not a measured performance result yet.

No local Rust/Cargo or direct repository clone available in the authoring environment; these commands have not run. Draft PR until CI and a compatible kernel test environment confirm results. In particular upstream kernel unit tests may rely on dev dependencies omitted by NODES; the root integration test does not require those. Remaining: exact classification of contact and emptiness; arbitrary coplanar face arrangements, UV trim fragmentation and seam matching, precise error bounds, topology repair accounting, complex nested shells and independent reference/stress fixtures.
