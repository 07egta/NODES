# NODES development checkpoint

- Updated: 2026-11-27 00:30 JST / 2026-11-26 15:30 UTC.
- Development branch: `develop/nodes-ultimate`.
- Original base: `evolve/all` at `df77c542473a5c637f3353dec48e367dd67db027`.
- Main initially inspected: `79f32745923adfa9e3de05ef4683b6e3b5f4948f`. No direct main push, force push or release promotion.
- Draft PR: https://github.com/07egta/NODES/pull/11
- State: integration preview; complete Grasshopper compatibility is not achieved.

## Start here after an interruption

Fetch the development branch and read its latest commits, this file and TEST_RESULTS.md. The original execution environment disappeared; its old checkout and compiled GUI must not be assumed to exist. A fresh source checkout now exists at `/workspace/scratch/a294e49d5e0f/NODES-recovered`. Git/Python work there, but Rust is absent and no approved installation source was found. Continue Rust validation in GitHub Actions unless a suitable toolchain becomes available.

All material recovery branches are on GitHub. [recovery-parents.json](recovery-parents.json) records their exact heads. Their histories are preserved during integration. The former local-only commit `7f59add921ed21769e693e147358828e1657777a` is historical evidence, not a fetchable dependency.

## Completed and saved

- Integrated the existing core, tree, math, generation, mesh, UI, GHX/API, exact curve, exact surface and compatibility work.
- Common Item/List/Tree access, empty-list behavior, canonical API connections, legacy port transforms, numerical precision, deterministic graph traversal/cache/cancellation and a bounded latest-only worker mailbox.
- Exact curves and surfaces, analytical surface curvature, NURBS knot-boundary regression fixes and editable curvature study.
- Cavity-preserving mesh repair, stable translated volume checks, non-manifold diagnostics, shared planar polygon triangulation for concave OBJ faces and bounded hole repair.
- Per-node timings/cache state, a 16,384 graph-node limit and a 10,000-node downstream recomputation regression.
- Stable inspector parameter widget IDs after an actual GUI focus bug was found; requested Japanese search phrases and typed connection filtering.
- 37 editable-example workflows with parameter edits, actual native saves/reloads, reevaluation and selected STL roundtrips.
- Dedicated console CLI: catalog, validate, evaluate, build, apply and strict explicit-Output STL export. Seven actual-process tests passed in the latest Linux CI candidate.
- Expanded release benchmark for large trees, mesh subdivision and native serialization/save/load; actual results are preserved in performance.json.
- Dependency metadata auditor with bounded reads, source redaction, checksum/checked-in license evidence and explicit limits. Full locked-package declarations were obtained in CI; incomplete text/runtime coverage remains explicit.
- Preserved the exact official r-efi 6.0.0 AUTHORS notice and selected its MIT option, including the fixed upstream commit and byte/hash evidence. No dependency change.
- README, architecture, roadmap, node coverage and handoff documentation updated.

## Counts and compatibility evidence

- Picker registry: 298 node kinds; definition-only Cluster: 1; total registered kinds: 299.
- Fully implemented node count and completely locally verified node count: not yet established. Registration cannot substitute for behavioral verification.
- GH versioned external-oracle VERIFIED components: 0.
- Compiled metadata: `docs/compatibility/nodes_inventory.json` (299 records).
- GH2 official index: `docs/compatibility/gh2_official_inventory.json` (1,419 entries, build 9.3.26329.6001). Detailed unresearched semantics remain UNKNOWN. It is not a GH1 denominator.
- GH1/GH2 curated mapping and references: `docs/compatibility/gh_catalog.json` and REFERENCE_NOTES.md.

## Exact verification checkpoints

1. `6349577399dd484a90fde5afe77ee173f8f9444a`: Linux and Windows each 350 Rust tests passed, 0 failed, 2 opt-in ignored. Windows release build passed.
   https://github.com/07egta/NODES/actions/runs/45822273755
2. Recovery diagnostics run `45836582370`, job `139966940840`: source `0e1032fc5409ff0212ed4d9b6b88c4f56df896ca` after formatter/Clippy repair, **424 passed, 0 failed, 2 opt-in ignored, 33 Rust suites**. Check, format, registry generation, pinned GH2 generation and release evaluator benchmark passed. Strict Clippy failed on explicit warnings.
   https://github.com/07egta/NODES/actions/runs/45836582370
3. That exact candidate was preserved at `2080af67dc4317bb4340aebcae22bbf0c01a8aa0`. All 369 fragments, 52 per-file SHA256/UTF8 lengths and original blob IDs were verified. See CI_RECOVERY_VALIDATION.json.
4. Strict-lint fixes are preserved at `d18183fd61c5f436099b4ba221ed630ef418c4e0`; no blanket suppression or test deletion. The console, search, benchmark and audit integration was saved at `b26d246341d359a6284a7b1036d1369a02e59a72`.
5. Diagnostics run `45846736773`, job `140002253199`, source b26d246 after formatting: **431 passed, 0 failed, 2 opt-in ignored, 35 Rust suites**. All required gates passed, including strict Clippy, metadata generation, dependency audit and expanded release measurements. All 299 fragments, nine file hashes/UTF8 lengths and source blob IDs were verified before importing the candidate. See CI_INTEGRATION_VALIDATION.json.
   https://github.com/07egta/NODES/actions/runs/45846736773
6. Current Python checks executed locally after environment recovery: GH2 parser 3/3, checkpoint exporter 6/6, dependency auditor 8/8. `git -c core.whitespace=cr-at-eol diff --check` passed.

The diagnostic workflow's repaired working tree is not the same as an unmodified commit. Final committed-source Check must pass after importing this candidate. The Rust changes being imported are formatting only. The recovery workflow is now manual-only; normal Check validates committed source and builds both desktop/console binaries for Linux and Windows.

## Measurements and GUI evidence

The latest release benchmark on a Linux x86_64 GitHub runner measured 10,000 scalar nodes at 16.363 ms cold and 12.604 ms warm; 50 full-chain slider edits had p50 23.632 ms and p95 24.151 ms. A 12,288-item Data Tree preserved every value/path and eight empty branches. Midpoint subdivision expanded 32,768 to 131,072 triangles in 16.687 ms while retaining analytical area/centroid. A 2,000-node native project saved in 3.531 ms and reloaded in 2.193 ms, including complete project equivalence checks. Raw scope/counts/results are in performance.json and TEST_RESULTS.md. Separate CI runs are not controlled speedup comparisons. These are not Intel Arc, GUI frame-time, startup, idle-CPU or memory measurements.

Before environment loss, the Linux GUI was actually launched: Japanese text, the 298-node library, vase preview, parameter dragging, Ctrl+Z/Ctrl+Y, error recovery and saving a .morph project were observed. Typing into the inspector exposed an unstable-widget-ID bug; source was fixed. **The fixed GUI has not yet been exercised.** The old saved file and binary were in the lost filesystem. Latest Windows GUI, light theme, DPI layouts, restart/reopen and GUI STL export remain unverified.

## Current work

- Save the reviewed formatter candidate, actual measurements and dependency evidence, retaining the r-efi notice branch history.
- Require committed-source formatting, check, strict Clippy, all tests and Linux/Windows release binaries.
- Obtain a Linux CI build for native GUI revalidation if the restored desktop can execute it.
- Update the PR and this checkpoint with the exact committed-source results and GUI scope.

## Build commands

Use Rust 1.100.0 and the Linux desktop packages listed in the workflow, or Windows MSVC Build Tools.

```sh
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
cargo run --release --locked --example coverage_perf
```

The GUI binary is Morphlet; the console binary is nodes-cli. Cargo's default run remains Morphlet. The build-windows.ps1 helper builds both. No dependency versions were added or changed for the console or audit work.

## Known limitations / unfinished work

- One output per node; native Null, general mixed Item/List matching and opaque native unknown-node editing require coordinated model/schema work.
- Native unknown node/field decoding currently fails closed and leaves the original file untouched. GHX archive preservation is a separate limited implementation.
- General trimmed B-rep, robust unrestricted Boolean/fillet/shell, full curve/surface intersections, persistent SubD/crease and STEP/IGES/3DM/glTF are incomplete or missing.
- GH/GHX full compatibility, external plugin ABI and a general MCP server are incomplete.
- Full node-by-node oracle/error/null/edge-case evidence is incomplete. Sample tests do not imply complete node coverage.
- Mesh repair, intersection and thickness estimates have explicit limits. No physical 3D printing success is guaranteed.
- Real Windows/Intel Arc performance, GUI quality across resolutions/DPI, startup/idle CPU/RAM remain unmeasured.
- License declarations are evidence, not legal approval; the historical bundled-license audit is incomplete and existing MPL-2.0+ must remain visible.

## Next files, commands and implementation steps

1. Inspect Check for the latest branch SHA. The formatter candidate has passed all diagnostic gates; confirm the unmodified committed source on Linux and Windows next. Record any failed gate and its exact log; retain failing tests.
2. If needed, edit only files named by compiler/Clippy/test failures. Priority new files: `src/bin/nodes-cli.rs`, `tests/cli_workflows.rs`, `examples/coverage_perf.rs`, `scripts/audit_dependencies.py`.
3. Download the Linux workflow artifact, run Morphlet in an isolated user-data directory and verify inspector entry 32→36, undo/redo, save/close/reopen, themes and explicit-output STL export. Record actual observations without substituting mockups.
4. Commit/push with a remote-head check; update TEST_RESULTS.md and PR #11 using actual results. Normal Check now also exports desktop/console development packages.
5. Use the manual recovery workflow only if a new candidate export is necessary. Its export hashes and repaired-tree results must never be confused with committed-source gates.
6. Next substantial feature: plan and implement multiple output endpoints plus a backward-compatible v1 reader in `src/model.rs`, `src/persistence.rs`, `src/engine.rs`, `src/node_access.rs`, `src/graph_api.rs` and canvas/cluster code. Pair this with opaque unsupported-node preservation; do not silently discard old ports or payloads.

## Save transport in the restored environment

The fresh checkout can clone/fetch public GitHub source, but its direct git push failed with 'could not read Username for https://github.com'. No credentials were requested, exposed or committed. The authenticated GitHub connector is used to create an identical tree, preserve all reviewed parent commits and advance the development ref with an expected-SHA lease. Verify the remote tree against the local tree before treating this checkpoint as saved.
