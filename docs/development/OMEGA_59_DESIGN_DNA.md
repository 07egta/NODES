# OMEGA 59 — Design DNA

Base main: c1213c885647c2e708017a63f6b7bdc8f9b66620. Branch: `feat/omega-59-design-dna`.
Scope: reuse project_diff, implement conservative 3-way graph merge, and an in-memory
parameter revision DAG with shared root and sparse deltas. No v1 .morph schema changes.

- Preserve source projects. Return conflict paths rather than silently choosing one side.
- Treat new NodeId collisions, incompatible kind changes, and invalid merged graphs as conflicts.
- NodeId is only stable within a known shared design lineage; undo and independent allocations can reuse it.
- Exact SolidCraft provenance is available in crates/sc-kernel/src/history.rs (merged PR #48).
  Do not infer persistent face identity from array indexes or proximity. Integration is a later checkpoint.
- Review references: https://git-scm.com/docs/git-merge (common ancestor and conflict behavior);
  Kripac, 1997, DOI 10.1016/S0010-4485(96)00040-1 (persistent topology naming);
  Farjana et al., 2016, DOI 10.1016/j.jcde.2016.01.001 (split/merge ambiguity).
- Existing project_diff reports evaluation roots and downstream impact; it does not evaluate geometry.
- Related open PR #100 adds named parameters. Do not edit shared model/persistence structures.
- Local Rust toolchain and git network checkout unavailable in this session; run CI before integration.
- AI tool: ChatGPT. Keep draft until quality gates pass; final merge belongs to Mission 21.
