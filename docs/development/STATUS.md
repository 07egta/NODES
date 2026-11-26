# NODES development checkpoint

- Updated: 2026-11-26 23:08 JST / 14:08 UTC.
- Branch: `develop/nodes-ultimate`.
- Original base: `evolve/all` at `df77c542473a5c637f3353dec48e367dd67db027`.
- Main inspected at `79f32745923adfa9e3de05ef4683b6e3b5f4948f`; no direct main push or force push.
- Draft PR: https://github.com/07egta/NODES/pull/11
- Product state: integration preview, not complete Grasshopper compatibility.

## Completed and externally preserved

- Integrated the core, tree, mathematics, generation, mesh, UI, GHX/API, exact curve, exact surface and coverage mission branches.
- Remote checkpoints: `219d92d`, `86a2276`, `2e6534f`, `6349577`.
- `6349577399dd484a90fde5afe77ee173f8f9444a`: Linux 350 tests passed / 0 failed / 2 ignored.
- The same commit passed Windows 350 tests / 0 failed / 2 ignored and the release build on Rust 1.100.0:
  https://github.com/07egta/NODES/actions/runs/45822273755
- 298 picker kinds were visible in the compiled GUI, plus definition-only Cluster. Registration is not a verified compatibility count.
- Externally oracle-verified GH component count: 0. Per-node local correctness coverage still needs a complete evidence ledger.

## Runtime interruption and recovery

The local execution environment became unavailable during the next GitHub tree checkpoint. Its function runtime was replaced and temporary tool state was lost. Do not assume that unreferenced GitHub tree writes completed. The last confirmed branch head before recovery was `6349577`.

The local source checkout was `/workspace/scratch/a294e49d5e0f/NODES`, with last local commit `7f59add921ed21769e693e147358828e1657777a`. Inspect it if the environment returns; do not overwrite it.

Recovery uses exact source patches from recorded authoring calls against fetched GitHub blobs. This checkpoint restores empty-list/SingleItem behavior, shared native/API tree-setting validation, canonical duplicate-wire rejection, legacy port-transform migration and stable inspector parameter IDs. The per-node timing UI and engine dependency have been restored in `326162ec`; timing regressions were recreated where exact source was unavailable and require CI.

The following recovery heads are incorporated by this checkpoint (histories retained as parents):
- `recovery/metrics-ci` at `27092a9719ff5a40b99c96f24677d65cc271024a`: CI and dependency records. Node timings/capacity were restored separately by the integration branch.
- `recovery/mesh-review` at `2a66b5fa6e19c3a62358eb6fe19c68a239931a9c`: cavity winding, robust volume/orientation diagnostics, concave planar OBJ/hole triangulation, newly reimplemented curvature edge case and study.
- `recovery/coverage-workflows` at `881e3c221cbb1e18ff59a79e6c04b24c85e629c9`: regression tests, GH2 generator/provenance and editable-example workflows.

These are recovery branches, not independent product releases. Preserve their commits when integrating. No unavailable artifact or generated inventory should be fabricated.

## Validation performed before interruption

- A 10000-node scalar chain passed initial evaluation, full cache reuse, middle edit with only 5000 descendants recomputed, and serialization/reload equality. Optimized-debug Linux elapsed: 48.370 / 23.725 / 33.476 ms. The restored node bound is now 16384, with item/geometry budgets unchanged.
- Node timings + large graph + surface curvature: 16 tests passed.
- Four empty-list/API regressions failed as expected, then passed after fixes.
- Focused common-access, API-tree, fabrication, sample and print-repair suites: 42 passed / 0 failed / 1 opt-in ignored.
- New concave-polygon module: 5 standalone rustc tests passed in its author's environment; full Cargo integration pending.
- Full 37-example parameter/save/reload/STL workflow tests were authored but not run before interruption.

## Native GUI observations

The Linux application was actually launched. Japanese rendering, 298-node library, vase preview, slider updates, undo/redo, error recovery and project save were observed. A test project was saved as `/workspace/scratch/a294e49d5e0f/gui-validation-vase.morph`.

The desktop needed its installed xfwm4 window manager started before reliable focus. The build shell itself had no DISPLAY, but native desktop UI could launch the shared compiled binary.

A real issue was reproduced: typing 32 -> 36 caused transient value 3 to produce an error, which moved implicit widget IDs and transferred focus. Explicit per-node/per-port widget IDs compiled successfully. Live revalidation of that fix was interrupted. Do not report that recheck, light theme, DPI tests, reopen or GUI STL export as completed.

## Build commands and next actions

```sh
cargo test --all-targets --locked
cargo check --all-targets --locked
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked --bin Morphlet
cargo run --locked --example coverage_inventory -- --source-revision <full-source-sha>
cargo run --release --locked --example coverage_perf
```

1. Fetch this branch and inspect PR #11, its exact HEAD and CI runs; recovery heads above are already integrated.
2. Complete the read-only CI diagnostic workflow, regenerate inventory/performance files, inspect and apply formatter/lint candidate patches.
3. Run full Linux and Windows CI on the resulting exact tree. Existing format/lint warnings remain; no clean-Clippy claim.
4. Next files: `src/engine.rs`, `src/model.rs`, `src/app.rs`, `.github/workflows/windows.yml`, coverage output and development documents.
5. Revalidate the numeric-focus scenario, then complete GUI reopen/export/light-theme/DPI workflows.
6. Generate the compiled registry and GH2 inventory from their scripts, recording source revision and official export provenance.

## Known limits

One output per node, no general mixed-access scheduler or native Null value; sequential kernels on one asynchronous worker; general B-rep booleans/trimmed surfaces, STEP/IGES/3DM, binary GH, full GHX, plugin ABI and complete GH1/GH2 semantics remain unfinished. General printing success, shell containment, minimum-wall and self-intersection completeness are not certified. Windows GUI/Intel Arc, startup/idle/RAM/frame time and 10000-node GUI use remain unmeasured.

## Documentation and remaining validation

ROADMAP.md, NODE_COVERAGE.md, TEST_RESULTS.md and docs/ARCHITECTURE.md now describe the combined implementation and its limits. CI is strict; formatting and Clippy findings are expected to remain visible until repaired. The separate diagnostic workflow may prepare reviewable patches in its checkout with contents:read, but never pushes source itself. Only the integration process updates this branch with an expected-SHA lease.

## Remote validation recovery

- Integrated diagnostic workflow from `1f9eb00add42f9a0c71bfb361ec98bf0023c6c44`; it has read-only repository permissions and exports review candidates, never commits or pushes.
- Normal Check run [45836118771](https://github.com/07egta/NODES/actions/runs/45836118771) stopped at formatting. Downstream compilation/tests were not run in that job.
- Windows run [45835122608](https://github.com/07egta/NODES/actions/runs/45835122608) compiled the timing recovery and exposed a reconstructed test fixture setting Add B to its existing default 10. The regression now changes B relative to its current value and independently asserts both downstream outputs change. The test remains present; revalidation is pending.
- Next: review the diagnostic CI checkpoint, commit formatter/lint candidates, regenerate registry/GH2 inventories and require clean committed-source CI.
