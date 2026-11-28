# OMEGA 36: deterministic adversarial property tests

- Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (main, 2026-11-28 JST).
- Scope: data-tree transforms, item matching, graph API malformed requests and rollback; prioritize independent invariants over raw case count. No new dependencies. Avoid overlapping PRs #102, #106, #108 and #65 for mesh import, STL, persistence and expressions.
- No subagents available in this session. GitHub connector can read/write source; local container has no Rust toolchain and cannot resolve github.com. Do not mark tests passed without CI evidence.
- Work in a dedicated draft PR. No direct main writes or merge; MISSION 21 owns integration.
