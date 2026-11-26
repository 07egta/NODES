# Mission 10 supplementary audit (2026-11-26)

Baseline: evolve/all at df77c542473a5c637f3353dec48e367dd67db027. Main was 19 commits behind. This branch is review-only; no auto-merge, release, force push or direct main changes.

## Concurrent audit records

An independently written checkpoint, gh_catalog.json, GH1 reference inventory script, tests and benchmark examples appeared on this same branch during the audit. They are retained intact. Do not add the two catalogs' row totals or assume they use identical component identities, scopes, or status criteria. Reconcile them using pinned build + library identity + component ID, then inspect conflicting mappings and select a canonical catalog. GH2's published count in the other checkpoint must be verified against its version-specific source and must not be used as a count of implemented or individually audited nodes.

This addendum concerns mission10_catalog.json (113 selected GH1 candidates and 2 GH2 candidates), mission10_workflows.json, examples/mission10_inventory.rs, examples/mission10_benchmark.rs and tests/mission10_coverage.rs. No new node kinds were added. The source registry contains 64 picker entries + 1 definition-specific Cluster kind, not 65 verified implementations. The ten GH1 third-party category listings report 734 entries in mixed dated indexes, not an authoritative standard-node total. GH1 crosswalk statuses: VERIFIED 0, PARTIAL 43, APPROXIMATE 4, IMPLEMENTED_UNVERIFIED 4, MISSING 35, BLOCKED 27. Two GH2 candidates UNKNOWN. Missing metadata stays null. No overall compatibility percentage is justified.

## P0 and validation

Graft must not silently lose values when a populated generated child path collides with an empty mixed-depth input path. The shared engine fix preserves the existing branch. This supplementary suite adds six graph-level integration tests: registry metadata/round-trip; mixed-depth graft through real connected nodes with flatten and cache check; slider/number updates, cache and serialization; sampled circle geometry; closed extrusion bounds; node capacity rejection without mutation. Three inventory-example tests validate mapped IDs and prohibit unsupported VERIFIED promotion. Nine supplementary tests authored, not locally executed. The existing separate suite and focused regression are not included in that number.

Twenty workflow targets are recorded in mission10_workflows.json. All execution and GH-reference comparison flags NOT_RUN. Availability estimates: 7 available, 4 partial, 1 approximate, 4 missing, 4 blocked. The reference fixtures listed are not all complete user-facing saved graphs.

```sh
cargo test --locked --test mission10_coverage
cargo test --locked --example mission10_inventory
cargo test --all-targets --locked
cargo run --locked --example mission10_inventory > mission10-inventory.json
cargo run --release --locked --example mission10_benchmark > mission10-benchmark.json
cargo build --release --locked --bin Morphlet
```

The benchmark example includes 10/100/1000-node fanout, 10,000-node capacity request (2048 baseline cap; rejection is not a successful evaluation), 100,000 distributed point values (not a single 100,000-item output), 512 branches/8192 items, max-resolution sphere mesh, and 100 slider edits on a 100-node graph. Reports engine-only cold/warm min, median, p95 and max, not app startup, process memory, frame or input-to-display latency. Those four metrics remain null until measured on the Windows GUI. No benchmark timings are claimed.

No local cargo/rustc, direct clone blocked by DNS, no Rhino reference runtime or Windows GUI, no independently available reviewer or subagents. Existing Windows PR CI must pass; examine the exact head SHA and logs before marking any test passed. Keep release artifacts review-only; verify executable, DLLs, licenses, sample graphs, Japanese fonts and launch instructions, then compute SHA-256 of actual review ZIP. No distribution artifact generated here.

## Parallel branch integration

The initial scan compared 21 branch names including main/evolve/all, not every line or a build of all branches. PRs for multiple missions appeared while this audit was underway (including #10 and #11). Re-fetch their heads; the baseline status counts above must not be applied to their newer code. Preserve the Graft regression when integrating the overlapping tree and core branches, and rerun their existing tests without deletion, disabling or weakened assertions. Prioritize reproducible build/test, correct tree matching and typed multi-output ports, then exact NURBS and B-rep geometry, intersections, Boolean and generative design. GUI checks and full GH1/GH2 reference manifests remain required gates.
