# NODES development checkpoint

- Updated: 2026-11-27 01:01 JST / 2026-11-26 16:01 UTC.
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
- Stable inspector parameter widget IDs and compact horizontal rows after actual GUI focus/spacing bugs were found; fixed artifact retested. Requested Japanese search phrases and typed connection filtering are covered.
- 37 editable-example workflows with parameter edits, actual native saves/reloads, reevaluation and selected STL roundtrips.
- Dedicated console CLI: catalog, validate, evaluate, build, apply and strict explicit-Output STL export. Eight actual-process tests, including the Unix FIFO regression, passed in the current normal Linux CI.
- Expanded release benchmark for large trees, mesh subdivision and native serialization/save/load; actual results are preserved in performance.json.
- Dependency metadata auditor with bounded reads, source redaction, checksum/checked-in license evidence and explicit limits. Full locked-package declarations were obtained in CI; incomplete text/runtime coverage remains explicit.
- Preserved the exact official r-efi 6.0.0 AUTHORS notice and selected its MIT option, including the fixed upstream commit and byte/hash evidence. No dependency change.
- Bounded regular-file checks reject ordinary FIFO/device input before opening project/API/GHX files; regression tests and four actual binary entry-point checks passed.
- Native GUI edit/error recovery/undo/redo/save/restart/STL workflow verified; before/after screenshots and the saved graph are preserved.
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
6. `c19dca185380391ad1c875e412529e22cae05ba6` passed normal Linux Check `45849521685` with 431 tests and both release binaries; its Windows job was cancelled by the follow-up source and is not counted as successful.
7. `eefc9c1baab03dcb1e217730a0cd61382b298660` preserved the file-input guards before testing. Its Check stopped at one GHX rustfmt difference; that exact difference was corrected.
8. Current source `2f94df459533774f83593835ba1e22d4924a3c42`, normal Check `45852634502`, Linux job `140022721574`: **433 passed, 0 failed, 2 opt-in ignored, 35 Rust suites**; fmt/check/strict Clippy, 17 Python tests, desktop/console release build and artifact upload passed. The actual PR checkout `19717e70917718af1374e387720b29b619c1c1ed` has the same tree. Windows job `140024871303` also passed **432 tests, 0 failed, 2 opt-in ignored, 35 suites**, both release binaries and artifact upload. The single-test difference is the Unix-only FIFO regression.
9. Python checks also executed locally after environment recovery: GH2 parser 3/3, checkpoint exporter 6/6, dependency auditor 8/8. `git -c core.whitespace=cr-at-eol diff --check` passed.

The diagnostic workflow's repaired working tree is not the same as an unmodified commit. Formatting-only candidate import was completed at c19dca1; subsequent FIFO and layout fixes are real source changes and have their own successful Linux Check at 2f94df4. The recovery workflow is manual-only. Normal Check validates committed source and builds desktop/console binaries for Linux and Windows. Exact source/tree/artifact hashes are in CI_COMMITTED_VALIDATION.json.

## Measurements and GUI evidence

The latest release benchmark on a Linux x86_64 GitHub runner measured 10,000 scalar nodes at 16.363 ms cold and 12.604 ms warm; 50 full-chain slider edits had p50 23.632 ms and p95 24.151 ms. A 12,288-item Data Tree preserved every value/path and eight empty branches. Midpoint subdivision expanded 32,768 to 131,072 triangles in 16.687 ms while retaining analytical area/centroid. A 2,000-node native project saved in 3.531 ms and reloaded in 2.193 ms, including complete project equivalence checks. Raw scope/counts/results are in performance.json and TEST_RESULTS.md. Separate CI runs are not controlled speedup comparisons. These are not Intel Arc, GUI frame-time, startup, idle-CPU or memory measurements.

Before environment loss, the Linux GUI was actually launched: Japanese text, the 298-node library, vase preview, parameter dragging, Ctrl+Z/Ctrl+Y, error recovery and saving a .morph project were observed. Typing into the inspector exposed an unstable-widget-ID bug; source was fixed. The old saved file and binary were in the lost filesystem. After recovery, the c19dca1 Linux artifact was downloaded and SHA-256 verified. Its actual GUI retained focus through 32→3(error)→36, preserved keyboard/drag undo and redo, saved a native file, exported an 11,808-triangle STL, restarted with radius 36 and the light theme, and allowed another drag/undo/save. A new excessive Inspector row-height issue was observed and fixed at 2f94df4. The fresh artifact passed compact-row inspection, 36→3(error)→36 without refocusing, seven-control scrolling, 1116×758 Fit All and dark-theme checks. Before/after PNGs and the actual saved graph are under docs/development. Windows GUI and broad DPI/resolution coverage remain unverified.

A separate 12-second settled native-GUI sample on this Linux virtual desktop observed no added CPU ticks at 100 Hz, with RSS 103,888 KiB (101.45 MiB) and peak RSS 109,388 KiB. This is one focused, static, three-node / 11,808-triangle workload; no claim of zero CPU consumption or Windows/GPU memory performance. See GUI_IDLE_VALIDATION.json.

## Current work

- New-graph manual testing found a numerical UI bug at 2f94df4: a Sphere default radius 25 becomes 25.1 on display, and typing 30 saves 30.1. The slider step is anchored to its 0.1 minimum and is applied during passive rendering. A focused fix and headless egui regression are in progress in app.rs/canvas.rs; do not call this precision issue resolved until source and actual GUI checks pass. See NUMERIC_INPUT_VALIDATION.json.

- Preserve final CI/GUI/CLI evidence and update Draft PR #11. Current source is 2f94df4; subsequent evidence-only commits must not be described as separately tested source unless their own Check has finished.
- Linux artifact 15134610988 was downloaded, its ZIP digest checked, and both executable hashes recorded. The repaired binaries rejected FIFO input through four entry points in 1.3–2.3 ms with preserved destination bytes. The older binary's bounded two-second blocking reproduction is retained in FIFO_VALIDATION.json.
- `examples/interop/ai-vase.json` and `scripts/validate_cli_vase.py` passed eight commands again with the 2f94df4 console binary. An independent STL parser checked 5,906 unique positions, 11,808 nonzero triangles, 590,484 bytes and 104 mm height, plus determinant volume against the CLI within the documented tolerance. See CLI_VASE_VALIDATION.json.
- GUI layout/focus correction is actually retested, not pending. Windows all-target tests/build/artifact also passed. Additional new-graph GUI testing then exposed the separate numeric input issue below.
- Fresh dependency audit output from the verified artifact records 303 declarations, 226 matching text files across 122 packages, including the r-efi MIT notice. Distribution/runtime/font review remains incomplete.

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
- Real Windows/Intel Arc performance, broad GUI resolutions/DPI, startup/frame time remain unmeasured. Linux idle CPU/RSS has only the single narrow sample described above.
- License declarations are evidence, not legal approval; the historical bundled-license audit is incomplete and existing MPL-2.0+ must remain visible.

## Next files, commands and implementation steps

1. Fetch `develop/nodes-ultimate`, inspect PR #11 and this file; do not restart the completed branch/Grasshopper inventory recovery. Verify the remote head and the exact source of the latest Check. Run 45852634502 is successful on Linux and Windows. Verify the next numeric-input fix at its own source SHA.
2. Next substantial feature is backward-compatible multiple output endpoints; see MULTI_OUTPUT_MIGRATION.md for the reviewed contracts and Dispatch proof. Start with `src/model.rs`, `src/persistence.rs` and migration fixtures; keep old v1 single-output graphs readable and preserve connections. Then update `src/engine.rs`, `src/node_access.rs`, `src/graph_api.rs`, `src/canvas.rs` and cluster code together.
3. Introduce runtime Null with explicit missing-input versus empty-branch semantics and independent matching fixtures; do not conflate it with the existing generic Slot type. Plan opaque unsupported-native-node storage before claiming unknown nodes round-trip safely.
4. First commands: `git fetch origin develop/nodes-ultimate`, `git log -8 --oneline`, `cargo fmt --all -- --check`, `cargo test --locked --test cli_workflows`. Run the full normal Check after source changes. Rust is absent in the restored local container; use CI if that remains true.
5. Native GUI evidence can be reproduced by opening `docs/development/gui-vase-36.morph` in the exact recorded artifact with an isolated user-data directory. The independent CLI example uses `python3 -B scripts/validate_cli_vase.py --cli <release nodes-cli> --request examples/interop/ai-vase.json --output-dir <new-empty-directory>`.
6. For every implementation unit, commit to a development branch, advance the remote with an expected-head check, and record actual tests. Never reset main, discard other branches or call a repaired diagnostic working tree an unmodified-source test.

## Save transport in the restored environment

The fresh checkout can clone/fetch public GitHub source, but its direct git push failed with 'could not read Username for https://github.com'. No credentials were requested, exposed or committed. The authenticated GitHub connector is used to create an identical tree, preserve all reviewed parent commits and advance the development ref with an expected-SHA lease. Verify the remote tree against the local tree before treating this checkpoint as saved.
