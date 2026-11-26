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

## Functional checkpoint 1

- Implemented `diagnostics::diagnose`, read-only `nodes-cli diagnose`, and the
  desktop 「診断と処理時間」 window. The report distinguishes independent root
  candidates, blocked failures, disabled nodes, unvisited nodes, and measured
  cached/computed/partial node visits.
- Desktop worker calculates the report once per matching evaluation. Revision
  checks surround diagnosis and publication; stale UI navigation is disabled.
- Relationship previews are capped at 64 roots and 256 affected nodes, with
  exact counts and explicit truncation flags. Desktop sections display at most
  100 rows and 8 upstream links, with exact total counts.
- Local Rust 1.100.0 verification: real CLI integration **11 passed**;
  diagnostic regressions **9 passed**; diagnostic UI logic **2 passed**.
  `cargo fmt --all -- --check` passed. Strict Clippy and full-suite results are
  still pending; these focused passes do not certify the full application.
- Two initial test expectations were corrected after execution: a successfully
  propagated empty branch `{0}: []` differs from a tree with no branches;
  `graph_error: null` is legitimate JSON and must not fail a finite-timing test.
- Independent review found a distinct-prefix fan-in worst case worth optimizing:
  many different root sets can spend excessive time merging bounded previews.
  Preserve this functional checkpoint, benchmark the same workload, then save the
  optimization separately. No performance improvement is claimed yet.
- Initial GUI launch from the command shell failed because that shell has no
  display environment. Native desktop interaction remains to be attempted.

The initial record is saved at remote commit
`9d38348400dba22dbe353e25c4234de219d2a3c9` in
[Draft PR #18](https://github.com/07egta/NODES/pull/18).
Normal git push could not authenticate. The connected GitHub Git-data API saved
the same tree, its branch ref was verified, and local history was aligned to the
remote checkpoint without dropping in-progress changes. Continue using a
fast-forward ref update with the expected old SHA; never force-push.

### Public specification research

McNeel's [GH1 downstream expiration API](https://developer.rhino3d.com/api/grasshopper/html/M_Grasshopper_Kernel_GH_Component_ExpireDownStreamObjects.htm)
and [runtime message API](https://developer.rhino3d.com/api/grasshopper/html/M_Grasshopper_Kernel_GH_ActiveObject_AddRuntimeMessage.htm)
separate dependency invalidation and recording runtime errors. The
[GH2 component migration guide](https://developer.rhino3d.com/en/guides/grasshopper2/migrating-components-to-gh2/)
documents cancellable solving and null gaps on unassigned item outputs, which
differ from compact whole-twig routing. These inform review questions; the new
NODES report is dependency inference and does not implement or certify GH error
semantics, native null values, multi-output gap rules, or cluster-internal causes.
