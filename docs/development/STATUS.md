# NODES development checkpoint

- Updated: 2026-11-26 23:00 JST / 14:00 UTC.
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

Recovery uses exact source patches from recorded authoring calls against fetched GitHub blobs. This checkpoint restores empty-list/SingleItem behavior, shared native/API tree-setting validation, canonical duplicate-wire rejection, legacy port-transform migration and stable inspector parameter IDs. The per-node timing UI will be restored with its engine dependency.

Separate recovery branches are being saved and must be integrated:
- `recovery/metrics-ci`: node timings, 16384 capacity, CI and dependency records.
- `recovery/mesh-review`: cavity winding, robust volume/orientation diagnostics, concave planar OBJ/hole triangulation, curvature edge case and sample.
- `recovery/coverage-workflows`: regression tests, GH2 generator/provenance and editable-example workflows.

These are recovery branches, not independent product releases. Preserve their commits when integrating. No unavailable artifact or generated inventory should be fabricated.

## Validation performed before interruption

- A 10000-node scalar chain passed initial evaluation, full cache reuse, middle edit with only 5000 descendants recomputed, and serialization/reload equality. Optimized-debug Linux elapsed: 48.370 / 23.725 / 33.476 ms. The capacity change is in recovery until merged.
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

1. Fetch this branch and the recovery branches; inspect PR #11 and their exact trees.
2. Integrate recovery files, resolving engine/model overlaps without reverting these access/API fixes.
3. Run full Linux and Windows CI. Existing format/lint warnings remain; no clean-Clippy claim.
4. Next files: `src/engine.rs`, `src/model.rs`, `src/app.rs`, `.github/workflows/windows.yml`, coverage output and development documents.
5. Revalidate the numeric-focus scenario, then complete GUI reopen/export/light-theme/DPI workflows.
6. Generate the compiled registry and GH2 inventory from their scripts, recording source revision and official export provenance.

## Known limits

One output per node, no general mixed-access scheduler or native Null value; sequential kernels on one asynchronous worker; general B-rep booleans/trimmed surfaces, STEP/IGES/3DM, binary GH, full GHX, plugin ABI and complete GH1/GH2 semantics remain unfinished. General printing success, shell containment, minimum-wall and self-intersection completeness are not certified. Windows GUI/Intel Arc, startup/idle/RAM/frame time and 10000-node GUI use remain unmeasured.
