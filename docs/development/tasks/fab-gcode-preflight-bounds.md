# FAB-GCODE-PREFLIGHT-BOUNDS

- Base: main `8ca85234aac0c2abd4c158e5e70f6e3505d0afff`
- Branch: `feat/nodes-task-fab-gcode-preflight-bounds`
- Scope: `src/gcode_job.rs` and existing in-module regression tests.
- Goal: validate generated movement coordinates (including hops), reject invalid flow limits and empty/degenerate jobs, and prevent overflow in diagnostic layer labels.
- Coordination: checked open PRs #21, #50, #52–57; none listed G-code files. The current main has the G-code module; the older integration branch lacks it. Do not change shared registry or persistence.
- Alternatives considered: GHX migration, curve precision, mesh panel/morph, Data Tree inspector, bio patterns, standard node expansion, semantic graph diff (all active PRs), plus SDF, loop and lattice requests (open issues); prioritize incorrect machine-output handling rather than overlapping feature work.
- Status: reserved; implementation and CI pending. Local Rust compiler/network unavailable in this Chat environment. Use existing CI; do not claim printer verification.
- Next: implement narrow preflight changes, add regression tests, open/update draft PR, inspect checks.
