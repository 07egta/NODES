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

## Functional checkpoint 2: bounded worst-case work

The initial feature is remotely preserved at `78eac24ac730ff3a9d882df55a991923e25ad7c2`.
Independent review of a growing-prefix fan-in graph found that saturated affected
previews were repeatedly copied for different upstream root sets. The follow-up
keeps exact counts but orders shared groups by their smallest affected ID, appends
ordered previews directly and skips updates unable to change an already full
preview. This targets diagnostic-report construction only.

Error text is now capped at 8,192 UTF-8 bytes with `message_truncated`; real error
messages normally remain unchanged. Timing normalization also remains finite for
synthetic extreme values. Core regressions after these edits: **11 passed**.
The independent reviewer rechecked the grouping/counting, preview ordering,
message boundaries and index invariants without finding a correctness issue.

The reproducible `examples/diagnostics_perf.rs` benchmark and
[protocol](DIAGNOSTICS_PERF_PROTOCOL.md) measure real evaluated graphs, including
10,000-node success chains, maximum-length failure chains, balanced multi-root
joins and a 16,383-node growing-prefix adversary. The executable uses independent
closed-form fixture expectations, separates evaluation/diagnosis/serialization,
and records source/profile metadata. Full matched before/after results are pending.

The first all-target local run exhausted the shared filesystem while emitting
large debug binaries/logs. It is **not** counted as a passing suite. Only this
workstream's reproducible `target` cache was removed (6.3 GiB), then rebuilding
resumed with `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`. Source files and remote checkpoints
were preserved. These local flags do not change repository Cargo profiles or CI.

The native desktop is available through Xfce, although the ordinary command shell
has no display variables. GUI launch and interaction will use that desktop after
the rebuilt binary is ready. Additional diagnostic-list navigation and an
independent BFS oracle are being developed in the same workstream.

## Functional checkpoint 3: searchable diagnosis and extreme-scale matrices

The optimized diagnostic core and benchmark protocol are remotely preserved at
`7c1a0956adb8edcc29945717a6eb32e0d6335be3`.

- Diagnostic lists now offer independent 100-row pages and a name or exact `#ID`
  filter. Japanese kana/width normalization follows the existing search rules.
  Filtering uses live renamed nodes without re-evaluation, resets/clamps pages,
  and keeps exact filtered/total counts. The 101st failure remains reachable.
- A separate breadth-first oracle verifies diagnostic roots and affected IDs on
  four deterministic real-evaluator DAGs, including reordered storage, shuffled
  IDs, disabled nodes and successful nodes that interrupt a failure chain.
- Fixed `LinearSolve` and `MatrixInverse` rejecting a finite well-conditioned
  system when raw row subtraction overflowed. For example, `1e308 * [[1,1],
  [1,-1]]` with RHS `[1e308,0]` now returns `[0.5,0.5]`; its inverse is finite.
  Large rows are downscaled only when every coefficient and RHS survives a
  division/multiplication round trip within 16 machine epsilons. Small rows are
  left unchanged. This preserves meaningful subnormal answers and the existing
  relative pivot rejection rule. It does not solve arbitrary conditioning or
  eliminate every possible intermediate overflow.
- The initial normalization audit caught a second issue: merely rejecting zero
  underflow still allowed a nonzero subnormal RHS to lose about 1.2% precision.
  The round-trip guard and an analytic regression protect that case as well.
- Local all-target tests with compact debug settings completed: **460 passed,
  0 failed, 2 ignored, 38 suites**. That run includes the new UI/oracle tests but
  preceded the last numerical guard. A subsequent focused run on the final
  numerical edit passed all **18 tests** across linear algebra, math nodes and
  existing precision regressions. A final whole-tree run remains to be done.
- Remote Check run `45911036122` on `7c1a095` passed formatting and all-target
  compilation, then failed strict Clippy on the new benchmark's fixed-size
  `chunks_exact(2)`. The harness now uses `as_chunks::<2>()`; no lint suppression
  or CI weakening was added. Windows was skipped after the Linux lint failure.
- Native NODES launched and rendered the six-node diagnostic fixture in Xfce.
  Four runtime failures, one success and one disabled node were visible. Mouse
  interaction has not yet been verified; launching a window alone is not a
  completed GUI workflow test.

### Numerical references and design decision

LAPACK's public [DGEEQU documentation](https://netlib.org/lapack/explore-html/d5/d8a/group__geequ_ga7238812c5c26fc2fbe8bbbc22b6ee885.html)
describes row/column equilibration and its range bounds. The
[DGESVX documentation](https://netlib.org/lapack/explore-html/d5/dbe/group__gesvx_ga82173a93234afc15d70b64233b3e5bc8.html)
explains consistent scaling of coefficients and right-hand sides. NODES uses
these public numerical principles with its existing small dense Rust solver;
it does not add LAPACK, copy its implementation or claim its full condition
estimation/refinement guarantees. Direct division avoids an overflowing
reciprocal, while a local precision guard preserves the existing range where
normalization would be lossy. Six new tests include analytic solutions, inverse
identities, pivot swaps, mixed `1e±300` rows, smallest subnormals, singular and
nonfinite inputs, cooperative cancellation and actual evaluator/cache reuse.

### Parallel development refresh

A subsequent GitHub refresh found new PR #20 (`feat/omega-graph-diagnostics`) and
PR #21 (`feat/omega-semantic-project-diff`) created by other workstreams after
this branch's first checkpoint. PR #20 currently adds a separate
`graph_diagnostics` module and CLI command on the modeling branch; PR #21 adds a
Python project-diff script on main. They must be compared before integrating
either branch. Our exploratory Rust project-diff work is isolated in a separate
local worktree and is not yet published as another competing PR. Its owner and
the independent reviewer have switched to overlap/contract comparison. PR #16
continues to own curve/surface and legacy data-tree corrections; those files
remain outside this workstream.
