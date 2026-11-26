# Mesh review recovery — 未再検証

- Date: 2026-11-26 UTC
- Recovery branch: `recovery/mesh-review`
- Safe remote base: `6349577399dd484a90fde5afe77ee173f8f9444a`
- Original local worktree base: `2a72c7c`
- Recovered local changes:
  - `cf0b6afdff276416e59576a1e65790077323ab40` — shell winding and print diagnostics
  - `4d40820e12145cfcc2ae7eb6c692f8edb2c8d7ac` — concave OBJ and planar hole triangulation
- First verified remote recovery checkpoint: `74ef321424d07b1ff6b55a9514e9cbc4574243a0`

The execution environment became unavailable and the orchestration runtime lost
stored values. Recovery uses the exact edit inputs and diffs retained in the
conversation, applied to GitHub files whose blob hashes match the original
worktree base. No local execution environment remains available.

The original source changes were recovered without new implementation decisions.
The new polygon module and its integration tests are restored from the exact
creation inputs before rustfmt and the subsequent added test input; whitespace
may differ from the original local commit. They require formatting and a fresh
build. These recovery commits are intentionally not claimed to retain the
original commit IDs.

Before the outage:
- The integration owner reported the six mesh-repair regressions passed.
- The isolated pure polygon module passed five tests with rustc --test.
- The seven OBJ/hole-repair integration tests had not been run.
- The base commit's 350 passing tests describe the base only.

The restored branch has **not been rebuilt or retested**. No Windows, GUI,
printability or Grasshopper compatibility claim is made. Do not merge this
recovery branch as a verified release. Original design/test notes are retained
in `MESH_REVIEW.md`; their historical results do not validate this restored tree.

The first recovered `src/stl_export.rs` exactly matches original blob
`b27b70e44771af08ee35c1878e6622331d676425`. Source base blobs:
- stl_export.rs: `b594c556645f31f188e9dfadb58021e04dd80c18`
- import.rs: `27d48c6b089ab97e36a73880cc3bd5adcda28e93`
- lib.rs: `fffb71e09fbd23ca6e8227a97b4dc3f63b7ae2c4`
- tests/stl_export_tests.rs: `c3cc0c78b6673d32a1feb9827753bf383f91fa7b`

Next commands after restoring an execution environment:

```sh
cargo fmt --all
cargo test --locked --lib polygon_triangulation
cargo test --locked --test polygon_import_repair --test mesh_repair_regressions --test import_tests --test stl_export_tests --test fabrication_tests
cargo test --all-targets --locked
```

Keep `main` and `develop/nodes-ultimate` unchanged during recovery.
The original curvature follow-up `997183b` could not be recovered bytewise.
New commit `cf91be9b3d05074af07cf8016ae250733f4867ff` separately reimplements
the non-C2 knot roundoff fix with three regressions, based on saved source.
A new nine-branch torus study is also provided to satisfy the documented
37-example fixture contract (node 0 minor -> 10, node 5 mean curvature).
See `CURVATURE_REIMPLEMENTATION.md`. These new implementations are also
uncompiled and untested locally; all related CI tests are pending.
