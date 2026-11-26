# Mesh review recovery — 未再検証

- Date: 2026-11-26 UTC
- Recovery branch: `recovery/mesh-review`
- Safe remote base: `6349577399dd484a90fde5afe77ee173f8f9444a`
- Original local worktree base: `2a72c7c`
- Recovered local change: `cf0b6afdff276416e59576a1e65790077323ab40`
- Pending recovery: `4d40820e12145cfcc2ae7eb6c692f8edb2c8d7ac`

The execution environment became unavailable and the orchestration runtime lost
stored values. Recovery uses the exact edit inputs and diffs retained in the
conversation, applied to GitHub files whose blob hashes match the original
worktree base. No local execution environment remains available.

The integration owner reported that the six mesh-repair regressions passed
before the outage. The restored branch has **not been rebuilt or retested**.
The base commit's 350 passing tests describe the base, not this recovered change.
Do not merge this recovery branch as a verified release.

The recovered `src/stl_export.rs` is checked against its original diff blob
prefix `b27b70e`. New test and documentation source is recovered from the exact
creation input and formatting diff; rerun formatting and all related tests.

Next commands after restoring an execution environment:

```sh
cargo test --locked --test mesh_repair_regressions --test stl_export_tests --test fabrication_tests
cargo fmt --all -- --check
```

Keep `main` and `develop/nodes-ultimate` unchanged during recovery.
