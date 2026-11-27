# Full Linux validation of the PR18 portable-angle correction

These results belong to **ca6147a0b639b9429900313147d4e288972373e5**,
tree **55903180e8609378657c00095833c295b9e4caf9**, in the independent
PR18 checkout. They are stored on the held integration branch so a
documentation-only update does not cancel the active PR18 Windows CI.
They must not be attributed to the combined PR16 candidate.

The actual `cargo test --locked --offline --all-targets` run passed
**487 tests, 0 failed, 2 ignored across 44 suites**. Debug and release
desktop/CLI builds passed. Release build time was 93.308 seconds with
existing dependencies; it is not a startup or performance benchmark.
The two ignored manual performance/stress cases were not executed.

The same source had already passed formatting, strict all-target Clippy
and all five focused VectorAngle regressions. Those raw logs and the
actual preceding Windows failure are preserved separately in
[windows-angle-portability](../windows-angle-portability).
The Windows failure was not converted into an ignored test or a weaker
assertion. See the [numerical scope and limits](../WINDOWS_ANGLE_PORTABILITY.md).

`results.json` records commands, exit codes, profile, durations and release
binary fingerprints. `source-sha256.json` records 142 source, test, example
and Cargo files, rechecked after the run. The original raw logs are retained.
Compiled executables are not committed.

Three actual release `nodes-cli diagnose` processes returned the expected
success/error exit codes and diagnostic counts. Their full JSON responses
and `release-fixtures.json` are included. Input hashes were unchanged.
Transient timing fields in the CLI responses are not comparative benchmarks.

Windows CI must be evaluated from its own run. Local Linux success is not
Windows success. The parent integration's earlier native GUI observations
also do not validate the later viewport source.

SHA256SUMS covers every file in this directory except itself.
