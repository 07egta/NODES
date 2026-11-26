# OMEGA: graph diagnostics and recovery checkpoints

## Scope and ownership — 2026-11-27 JST

Branch: `feat/omega-graph-diagnostics-20261127`.
Base: `develop/nodes-ultimate-api-edits` at
`6b29c1cab3239494766e11c4799b557acc54469a`.

This work makes an evaluated graph explainable: identify independent failing
nodes, distinguish downstream failures, jump to their nodes in the desktop UI,
and expose measured node timings through a bounded offline CLI report. Cause
relationships are inferred from graph dependencies, not a claim that the engine
has structured error provenance. No new geometry kernel, dependency or native
file-format migration is planned in this workstream.

Ownership during parallel development:

| Area | Files | Owner |
| --- | --- | --- |
| Diagnostic analysis and regression cases | `src/diagnostics.rs`, `tests/diagnostics_tests.rs`, library export | diagnostic-core agent |
| Japanese desktop diagnostics and navigation | `src/app.rs`, `src/main.rs`, `src/diagnostics_ui.rs` | diagnostic-UI agent |
| Offline console, examples, verification, checkpoints | `src/bin/nodes-cli.rs`, CLI tests, this record | coordinating agent |
| Public GH1/GH2 contracts and independent review | source notes and review findings | research/review agent |

The coordinating agent alone commits and pushes this branch. Existing remote
branches, other developers' checkouts and main are not edited by this workstream.

## Repository audit at start

Read the public repository, all 15 open PRs, open Issues (none), branch heads and
the latest 15 Actions runs using the GitHub connector. Git clone succeeded.

- `main` is `79f32745923adfa9e3de05ef4683b6e3b5f4948f` and predates the integrated
  CAD/data-tree/CLI development. PR #11 contains the integrated foundation.
- PR #12's head `6b29c1c` has a successful
  [Check run](https://github.com/07egta/NODES/actions/runs/45905591281).
  This is the base of the present branch, not evidence for new changes.
- PR #15 owns modeling/performance work at `a1ffa400` and has a successful
  [Check run](https://github.com/07egta/NODES/actions/runs/45907598179).
- PR #14 owns the multi-port migration at `35ccdb49`; its last inspected
  [Check run failed](https://github.com/07egta/NODES/actions/runs/45906728248).
  Separate repair branches `fix/omega-multi-port-ci`, `fix/omega-multiport-ci`
  and `omega/multi-port-ci-repair` already exist. Do not duplicate their work.
- `evolve/all` is separately advancing, last inspected at `5cfab8e0`, and must
  not be assumed identical to the integrated development branch.
- Rust 1.100.0 is installed at `/home/agent/.cargo/bin` in this development
  environment; an offline baseline build/test is running. Its result is not yet
  known. No new verification success is claimed in this initial checkpoint.

## Diagnostic contract

- Analyze actual `Evaluation` outcomes without evaluating a second time or
  cloning geometry payloads.
- Preserve invalid-graph, cancelled, unevaluated, disabled and successful states.
- Show all independent root failure candidates and deduplicated affected failed
  nodes; do not report every downstream error as a separate root cause.
- Use deterministic nonrecursive graph traversal and preserve node IDs for
  navigation. Cluster internals remain represented by their outer node.
- Show wall-clock node timings as measured; cached and interrupted visits remain
  distinct. Do not describe timings as CPU time or predict a speedup.
- A new `nodes-cli diagnose PROJECT.morph` command will use the existing
  capability restrictions. Scripts and external imports remain outside that CLI.
- Desktop analysis is cached with the evaluation revision and not recomputed
  on every paint. Stale results must be labeled and must not navigate to an
  unrelated/reused node ID.

## Next steps / resume

1. Finish diagnostic analysis and analytic graph regressions.
2. Expose CLI JSON and desktop navigation; test real console behavior.
3. Run formatting, strict Clippy and all-target tests; inspect actual UI if able.
4. Save each passing functional checkpoint to this branch and update the same
   Draft PR. Verify remote SHA after every push.
5. Refresh upstream/other PR state before integration; retain draft status until
   verification is recorded. No automatic merge is authorized by this document.

This first checkpoint intentionally records the useful repository audit and
collision-free ownership before the implementation is finished.
