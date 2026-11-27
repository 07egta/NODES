# Verification after cancellable diagnosis and the search-layout fix

Local Linux validation of published commit
`3e3bb46185628e479888053fe4e4ff606047c244`, Git tree
`8f5f93cf0c4b64ac2059b698b7e4f88de12fc75c`, on 2026-11-27 JST.
All-target tests passed **486 tests, 0 failed, 2 ignored across 44 suites**.
Formatting, all-target check, strict all-target Clippy, debug binaries and release
binaries also passed. This supersedes the older 481-test run as the current
runtime-source validation; its historical evidence remains unchanged.

`results.json` records the actual commands, return codes, elapsed times, compact
local debug/test profile settings and the release binary sizes/SHA256 values.
The release build took 91.066 seconds with cached dependencies on this host; that
is a build duration, not an application startup measurement. Logs retain local
paths. No compiled binary is committed.

`source-sha256.json` hashes every tracked file under `src`, `tests`, and
`examples`, plus Cargo manifests/lockfile and any tracked build script, at that
exact source commit. Later evidence-only commits can compare those files rather
than misattribute this run to a different runtime implementation.

The ignored cases are the existing opt-in `benchmark_sparse_slider_edit` and
`audit_stress`; neither is counted as executed. The earlier full-debug run that
exhausted disk space is not included as a success. The 17 developer utility
self-tests were separately verified in the earlier cycle; this local run does
not claim to have rerun them.

`diagnostic-filter-before.log` records the newly added 60-frame, real-theme
regression failing against the old filter layout. It is a controlled pre-fix
layout check, not a full historical-commit suite. The matching post-fix log
records all five diagnostic UI tests passing. The full 486-test run also
includes that regression and the four new cancellation tests.

The [native GUI report](../DIAGNOSTICS_GUI_CHECK.md) records the post-fix width,
ID/name filtering, clear-button behavior and exact limitations. Native checks
and earlier clipboard checks are kept separate from automated test counts.

The release CLI fixture results include the complete JSON responses, expected
exit statuses and before/after file hashes. Their elapsed values are transient
and are not performance measurements. SHA256SUMS covers every file in this
directory except the manifest itself.
