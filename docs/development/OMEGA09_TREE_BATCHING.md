# OMEGA 09 — Data Tree branch batching

- Base source: `ec3b840e81c76aa7e426da5dc47b716ba975a98e` (main inspected 2026-11-28 JST).
- Scope: `src/data_tree.rs` reverse/flip_matrix, existing Data Tree regression tests, and a reproducible paired benchmark. No graph API, persistence, geometry precision, preview or dependencies changed.
- Coordination: open PR #83 handles engine/preview/LOD, #81 handles NURBS. Checked changed filenames of open PRs #84, #83, #81, #80, #65, #57, #55, #53, #52; none changed `src/data_tree.rs` or the Data Tree test files. No subagents available in this session.
- Existing evidence: `docs/development/OMEGA_WORKLOAD_PERFORMANCE.json` recorded 0.935956 ms for transpose of 128 x 96 numeric values on Linux, source `0aa286a93a8b61f23ce691b1d15a025780ec1fc8`. This is historical, single-sample evidence, not the baseline for a new comparison.
- Source inspection: reverse currently appends each item individually; transpose clones a path and performs branch map lookup for each cell. Batch by destination branch; preserve validation, weight budget, order, empty branches, signed zero and shared geometry.
- Environment limitation: no cargo/rustc available on PATH and git clone/ls-remote cannot resolve github.com in the execution container. GitHub connector read/write is available. No current timing, build, or test result is claimed.
- Next: add benchmark-only checkpoint first, then the production change and regression checks. Compare exact source SHAs on one machine using identical compiler/profile, record both raw results, and do not merge until quality gates and comparison are complete.
