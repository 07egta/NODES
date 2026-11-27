# Refreshed held PR16 / PR18 integration validation

This is a proposed combination, not a merge into main or an existing PR.
Its inputs are the first held candidate
**771b4760bf983fb35afa65a2cbf495772172e6d5**, PR16
**46ab578af63193c7dc3d37ce1926e1bc75216486**, and PR18's portable-angle fix
**ca6147a0b639b9429900313147d4e288972373e5**.
The [integration note](../OMEGA_PR16_INTEGRATION_20261127.md) records the
inherited PR15 changes, interface contracts, repairs and remaining work.

The automatic merge tree was textually clean but failed all-target check
with E0063 at two `ComputeResult` initializers in viewport cache tests.
`before-check.json`, `before-check.log` and `before-source-sha256.json`
preserve that failure and source. The fixtures now construct the real
diagnostic report together with evaluation, and the stale-result regression
also asserts that the current diagnostic report remains unchanged.

After this correction and the PR18 angle fix, the recorded index tree
**af7316c51d7a78fd9abcc7841ee711b64d10f538** passed formatting,
all-target check, strict all-target Clippy, all-target tests and both
debug/release application builds. This is the tree before evidence and
documentation additions, not the final published commit tree.

The actual full test run reports **572 passed, 0 failed, 4 ignored across
57 suites**. The ignored cases are `benchmark_canvas_pan`,
`benchmark_sparse_slider_edit`, `audit_stress` and the new
`benchmark_pending_preview`. They are manual performance/stress cases,
not failed functionality converted to ignored tests. The original held
candidate's 567/0/3 count belongs only to its older input source.

The exclusive target was originally seeded with dependency artifacts,
then the project package was cleaned before the initial candidate.
The refreshed candidate was recompiled in that same isolated target.
PR18's separate target and frozen benchmark binaries were not overwritten.
Release build duration was 95.754 seconds; this is not a startup benchmark.

`results.json` contains actual commands, exit codes, durations, profile,
input parents and release binary fingerprints. `source-sha256.json`
records the 164 runtime/test/example/Cargo files checked before and after
execution. The later evidence update preserves these source bytes.
Binary executables are not committed. Source and logs identify the run;
the `source_head` still names the old base because the new source was
staged in the worktree before the evidence commit was created.

Three actual release CLI processes were run on the committed diagnostic
fixtures. The JSON responses, expected summary fields, exit codes and
identical input hashes are saved. Their transient timing fields do not
support comparative performance claims.

Windows CI and Windows GUI have not been performed on this combined candidate.
A later bounded native check on source4ad872 repeated diagnosis2→1→0 and
Undo/Redo, and checked dark file-picker selection/cancellation; see
`native-gui.json`. It does not test native pending geometry or GPU latency.
The initial candidate771b476 observations remain separately attributed.
Imported PR16 benchmark/CI records remain parent-specific.

SHA256SUMS covers every file in this directory except itself.
