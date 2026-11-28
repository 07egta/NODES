# OMEGA 68 — FLOW
Base: 09a2a546d442fea822d80fb4c756a81f067a8b00 (2026-11-28 JST).
AI: ChatGPT. Issues #71–#79 all open at initial inspection.

## Scope and coordination
This branch focuses on #75: deduplicate externally connected cluster inputs by source NodeId, preserving their original target port keys, and show internal targets/errors. Existing single-output model, cache, cancellation, and part format are reused. Avoid NodeKind registry changes (parallel #137/#139), scheduling changes (#132), and tree batching (#87). No direct main merge.
Other issues: #71 Data Dam/timer requires explicit state and completion-based scheduling; #72 remote references require implicit dependency edges in the cycle check; #73/#74 require value and port modifier design; #76 snapshot requires bounded serialization of runtime Values; #77 diagnostic severity; #78 tree components; #79 shortcuts. These are not fixed by this branch.
Existing tests/large_graph_tests.rs covers 10,000 nodes; existing engine cache/cancellation/parallel evaluation is not duplicated.

## Verification
Local checkout cannot reach GitHub (DNS) and cargo/rustc are absent. Connector-based edits only. No claim of fmt, clippy, tests, GUI or performance results until corresponding evidence is available. Keep draft until CI and native checks.
