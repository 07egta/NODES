# OMEGA 52 — Geometry compiler

- Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (`main`, inspected 2026-11-28 JST).
- Owner scope: typed structural IR and reusable validated execution schedule; opt-in while equivalence and performance are unverified.
- Existing engine already evaluates deterministic dependency levels, uses per-node semantic caches and CPU parallelism. Do not change kernel semantics, serialization, cache revisions, retention budgets or output ordering.
- Coordination: PR #112 touches cancellation in `src/engine.rs`; PR #125 changes semantic keys in `src/cluster.rs`; PR #120 restores diagnostic profiling; PR #83 touches UI preview performance. Keep this work limited to the scheduling entry point, graph validation helper, compiler module and existing large-graph tests. Avoid changing the worker/cancellation loop or semantic key code.
- No main push or merge. MISSION 21 owns integration.
- Environment: GitHub connector available; local checkout unavailable (DNS failure), Rust/Cargo not installed on this runtime. CI and exact-source comparison/benchmarks required before enabling optimizations by default.
