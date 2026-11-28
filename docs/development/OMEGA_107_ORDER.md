# OMEGA 107 ORDER — graph organization

Base inspected: `8cac838229071509156d560391c3d1ba5237d761` (main).
Scope: isolated Rust layout engine and minimal existing canvas adapter; no graph
evaluation, persistence schema, dependencies, or geometry changes. Other UI work
may modify canvas/app; keep the adapter small and coordinate at integration.

## Current audit

- Graph rejects cycles; inputs map port keys to source NodeId, one output per node.
- Canvas auto_arrange currently uses longest-path depth and ID/topological order,
  scans Graph::node/node_mut repeatedly and does not score crossings or respect
  fixed nodes. Existing groups/notes, reroute dots, minimap and manual align/
  distribute are retained.
- Project history already distinguishes layout-only changes from evaluation.
- Open PRs #173 (numeric editors), #174 (live tree probe), #148 (visual regressions)
  and #143 (clusters) are relevant; avoid shared model/persistence edits.

## Algorithm selection

| Method | Decision |
| --- | --- |
| Sugiyama/layered/hierarchical | DAG rank assignment + deterministic bounded barycenter sweeps; left-to-right flow |
| Force-directed | Not default: arbitrary positions obscure dependency direction; possibly a separate overview |
| Constraint-based | Keep nodes outside requested scope fixed; do not mutate a proposal until accepted |
| Incremental | Use prior vertical order as stable tie-breaker; future local updates |
| Orthogonal routing | Future port-aware obstacle avoidance; separate from positions |
| Edge bundling | Future visual-only grouping; never conflate logical edges |

References: Sugiyama, Tagawa, Toda (1981), DOI 10.1109/TSMC.1981.4308636;
Martí et al., layered crossing heuristics, https://www.sciencedirect.com/science/article/pii/S0957417418302914;
Pupyrev et al., ordered bundles, DOI 10.1016/j.comgeo.2015.10.005.
No third-party implementation copied.

Phase 1: pure layout proposals, bounds, deterministic ordering, bounded crossing
proxy, scoped/fixed constraints, tests, then adapter. Subsequent phases:
preview UX/persistent pins, routing and bundles, group-aware placement and
semantic zoom. Do not report unverified or unfinished features as complete.
