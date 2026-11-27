# Verification evidence — 2026-11-27 JST

The full fmt/check/Clippy/test/release-build run is for published commit
`147148edb7d1fe70a0cb9096b9fec4578e020ab6`, tree
`19c51100ef2193028bc6181c9faeecc2e96d5bb9`. It passed **481 tests, 0 failed,
2 ignored across 43 suites**. The two ignored cases are existing opt-in
performance/stress tests, not silently counted as executed.

`results.json` preserves commands, source identifiers, elapsed run times and
local profile settings. Log paths are relative here; raw Cargo logs retain their
original local build paths. Development utility self-tests also passed (3 + 6 +
8), but their raw output was not saved, explicitly recorded in the JSON.

The following source change adjusts only the value-viewer footer and timing
percentage explanation. Its fmt/Clippy success and exact source-file hashes are
recorded separately. The earlier full-suite pass is not falsely attributed to a
different tree.

The numeric fixture JSON comes from the actual debug CLI diagnosis, with six
successful nodes. Transient elapsed values in that file are not a benchmark.
The native clipboard workflow and its scope are recorded in
[the GUI report](../DIAGNOSTICS_GUI_CHECK.md).

The SHA256SUMS manifest covers all files in this directory except itself. No
compiled binary, credential, application setting or private project is included.
