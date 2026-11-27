# OMEGA: graph diagnostics and recovery checkpoints

For the latest tested source, verification results, integration constraints and
remaining work, start with the [2026-11-27 handoff](OMEGA_HANDOFF_20261127.md).
The checkpoint entries below retain their historical pending/completed states.

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

## Functional checkpoint 4: vector precision and actual desktop recovery

Checkpoint 3 is remotely preserved at
`e0fe2ef87075987fbf98fccc27087ae21cfe715e`.

The numerical review continued with vector inputs inside the existing `1e9`
coordinate bound. `dot([1e9,1,1e9], [1e9,1,-1e9])` incorrectly returned zero
instead of one. The cross product of `[n,n-1,0]` and `[n-1,n-2,0]`, `n=1e9`,
also returned zero instead of `[0,0,-1]` because the two separately rounded
products lost their difference. The two public math nodes now recover product
residuals with strict `f64::mul_add` before the existing compensated sum. Zero
signs are preserved when both old and corrected results are zero. Other geometry
kernel helpers are unchanged; ordinary underflow and finite precision still apply.

Public references: Rust's [strict fused multiply-add contract](https://doc.rust-lang.org/stable/std/primitive.f64.html#method.mul_add)
and Ogita, Rump and Oishi's [Accurate Sum and Dot Product, Algorithm 3.5](https://www.tuhh.de/ti3/paper/rump/OgRuOi05.pdf).
The residual arithmetic was independently reviewed, with accepted input bounds
keeping intermediate products finite. No new library or copied commercial code
was introduced.

Six vector regression tests passed, including exact integer reference cases,
orientation/permutations, signed zeros, subnormal boundaries, validation,
cancellation and real grafted-tree/broadcast/cache behavior. The focused suite
passed **24 tests** across vectors, linear algebra and existing math/precision
regressions. Strict all-target Clippy also passed with the completed vector code.
A final all-target suite is running; its result is not yet claimed here.

The native GUI recovery check is now complete: two cause candidates -> select and
repair the first Divide input -> one cause -> select and repair the second ->
zero errors. The timings view showed three computed nodes, then all five enabled
nodes when cached visits were included. See the reproducible fixture and exact
steps in [the GUI check record](DIAGNOSTICS_GUI_CHECK.md). Keyboard editing and
cross-platform GUI behavior remain unverified.

### Measurement audit: invalid first comparison

The first release before/after experiment reused one Cargo target directory
between worktrees. A schema-marker audit showed that the supposed after binary
still linked the baseline library: neither executable contained the new
`message_truncated` serialization field, and report sizes omitted its expected
per-failure bytes. Distinct executable hashes and accurate environment source
labels were insufficient to establish linked-library identity. The initial
comparison is **invalid for before/after claims**, not evidence of no improvement.
Raw results and an explicit invalidation record are retained. The after revision
is rebuilding in a fresh isolated target directory, followed by a real serialized
schema probe and a new matched measurement. No speedup is claimed until then.

### Held native project-diff work

To retain useful work without opening a competing PR, the native project-diff
prototype and its handoff were saved to the archival branch
`wip/omega-native-project-diff-20261127` at
`c6bb2c3d434c91986f93c8cfda7b9279f2b498cb`. Its exact tree/ref was verified.
That checkpoint is explicitly WIP and must not be merged; its independent fresh
target test build is pending. [The parallel review](OMEGA_PARALLEL_REVIEW.md)
records the native/Python comparison contracts and the PR #20 diagnostic command
collision so the next developer can make an explicit integration decision.

## Functional checkpoint 5: verified performance and interleaved-ID previews

Checkpoint 4 is preserved at
`eb256af16e6db50917018da9dbe1776a6e9d20ee`. Its final local whole-tree run passed
**472 tests, 0 failures, 2 ignored, 40 suites**, with strict all-target Clippy.

The corrected, independently built release comparison is complete. All 13
processes and 19 case executions passed their independent invariants. At frozen
revisions `78eac24` -> `7c1a095`, median diagnosis time on incremental root joins
was 42.111 -> 2.146 ms (1,023 nodes), 201.041 -> 5.848 ms (2,047 nodes), and
961.544 -> 18.206 ms (4,095 nodes). The largest optimized case, 16,383 nodes with
8,192 root failures, took 295.698 ms median. These are diagnostic API timings,
not application-wide or geometry-kernel speed claims. The complete report,
per-process samples/resources, exact source/binary fingerprints, linked-schema
probes and excluded invalid first experiment are in
[the verified benchmark report](DIAGNOSTICS_BENCHMARK_RESULTS.md).

Independent follow-up review identified a different ID arrangement that the
prefix benchmark did not cover. One shared group can contain a small first ID
and many large IDs, followed by singleton groups with intervening IDs. The old
bounded merge still copied a full 256-ID vector for each smaller replacement.
The new accumulator retains its sorted append/skip path and lazily converts only
overlapping previews to a max-heap. It reuses the existing allocation, replaces
the largest retained ID and returns sorted output. Exact counts and the schema
are unchanged. Group previews are disjoint because each blocked node belongs to
one group; truncating each group's own largest IDs cannot hide a global smallest
ID. A separate implementation reviewer checked this invariant and the code.

The independent real-evaluator regression has 845 nodes and 287 roots. Each core
root has exactly 532 affected descendants and must retain precisely IDs 0..255,
including after reversing storage order. All **13 focused diagnostic tests**
passed (11 existing, one BFS oracle, one new interleaved regression). Timing this
additional heap change is pending; the earlier measured speed ratios apply to
the frozen earlier revisions only. Dense root-set memory and cancellation remain
separate follow-up questions.

The held native project-diff prototype also passed its fresh isolated build and
all **10 tests**, including two real CLI processes. Its test record is preserved
at archival commit `fa56fb1b66c7f249a0972899cb61742327c4ea6f`. It remains outside
this active PR and held for PR #21 contract coordination.

## Functional checkpoint 6: compact causes and direction-preserving angles

Checkpoint 5, including the verified prefix experiment, is preserved at
`545bcd4208cc6bf7069a75c7e4aadd6d2e915910`.

The heap follow-up now has its own [paired benchmark record](DIAGNOSTICS_HEAP_BENCHMARK_RESULTS.md).
Two byte-exact diagnostic snapshots were compiled into the same disposable crate,
using the same real evaluations, independent analytic fixtures and byte-identical
complete JSON outputs. All 12 processes passed. The 8,973-node interleaved case
fell from 469.484 to 96.023 ms median (4.89x). This has a measured tradeoff:
the 4,095-node prefix control rose from 17.516 to 19.335 ms (+10.39%) in this
small sample; the successful 10,000-node and 1,023-node prefix controls changed
by +1.6% and +0.3%. Preserve those observations; the change is not a universal
speedup. Snapshot hashes, raw calls, validation and reproduction files are kept
with the report. Later compact-ordinal work is outside that measured comparison.

### Compact root membership

Root sets now index only actual failure roots, collected in sorted-ID order,
instead of using indices among every node in the graph. A local prepass uses the
same consumed-input and enabled-failure predicates as the topological propagation
pass. Independent review confirmed the equivalence, including disabled/successful
cuts, ignored cluster sockets and roots discovered out of ID order. The public
schema, exact counts and sorted IDs are unchanged. A no-failure fast return runs
only after topology validation and all visit/timing summaries are complete.

For two high-ID roots and 16,382 independent joins, the **derived bitset word
payload** drops from 33,550,336 to 131,056 bytes. This is not a measured RSS
claim and excludes maps, allocation overhead, error text and report arrays.
A separate fresh-process memory comparison is being prepared. Two new public
behavior regressions cover high-ID roots and a new local failure after a successful
chain break. They passed alongside the existing independent BFS/interleaved cases.

### Tiny VectorAngle values

Unit-length normalization could make the representable directions
`[1e9,1e9-1,0]` and `[1e9-1,1e9-2,0]` identical, incorrectly returning zero
instead of approximately `5.00000001e-19` radians. The angle node now scales each
direction by exact powers of two, then uses the compensated cross/dot products
inside `atan2`. The first scale lifts even the least subnormal component; the
second places the maximum in `[2^256,2^257)`, keeping accepted inputs and ensuing
products finite without normalizing away their differences.

An earlier `[1,2)` scaling proposal was rejected before publication because it
lost a representable minimum-subnormal angle. The final method includes that
counterexample in its tests. Four new tests use exact `i128` determinant/dot
references, permutations, independent and subnormal scales, angular quadrants,
invalid/cancelled input and real grafted-tree/cache execution. Unitize, projection
and other geometry kernels are unchanged; universal correctly rounded angles are
not claimed. Both implementations received independent code review.

All **31 focused tests** for the completed angle/ordinal edits passed, covering
linear algebra, vector products, angles and four diagnostic suites. Formatting
and strict all-target Clippy passed. A final whole-tree run after these changes
and the pending documentation-only upstream merge remains to be recorded.

### Upstream refresh

PR #23 was merged into `develop/nodes-ultimate` as `7e32c91`; its ten changed
documents establish the owner's Rust-first language policy. The new `AGENTS.md`
and policy were reviewed; all application, diagnostic, numerical and native
comparison code in this workstream is Rust. Measurement orchestration is confined
to development, with no new application runtime or dependency. These verified
upstream documents will be merged into this branch without importing other
unfinished feature branches. PR #16 now also touches `math_nodes.rs`, but its
change is confined to Summation/Mean aggregation and does not replace this
workstream's matrix/vector/angle sections. Preserve both during future integration.

## Functional checkpoint 7: measured memory and faithful numeric display

The compact-root and angle changes are preserved at `46417ae91af8cbe11eb45934d535278ce23df06b`.
The verified upstream language-policy merge is preserved at
`bf98da477f3cbe9826d04f12ce885ecaf1380cf3`, with both parents retained. It contains
only the ten upstream documentation changes; no unreviewed feature branch was
merged. GitHub Check run `45911704790` on upstream policy head `3a8c99e` was
independently confirmed successful before publication.

The [root-ordinal memory study](DIAGNOSTICS_ROOT_ORDINAL_RESULTS.md) is complete.
All 40 fresh measurement processes and two separate complete-report equality
checks passed. On 16,384 nodes with two high-ID roots, median whole-process peak
RSS fell from 84,056 to 52,304 KiB (82.09 to 51.08 MiB, **37.77%**). Every before
RSS sample exceeded every after sample. The first-diagnosis median was 92.437 vs
46.479 ms, but the full timing range includes an after sample of 218.593 ms; no
universal latency guarantee is inferred. The 10,000-node successful control's
memory was essentially unchanged, and its timing also reflects the new
no-failure early return. Each process includes graph construction, evaluation,
diagnosis and lightweight checks, rather than measuring only diagnostic memory.

The numerical UI review found a separate problem: the detailed value viewer
rounded every number to four decimal places, hiding recovered tiny angles as
`0.0000`. It now keeps ordinary four-place values and signed zero, uses compact
scientific notation for small nonzero/large values and offers lazy hover detail
plus **詳しい値をコピー** for full round-trippable scalar/XYZ text. A finite-range
guard avoids a rounded maximum value being described as infinity. Existing row
caps remain. Two new formatting tests cover extrema, bounded preview length,
exact finite-value reconstruction and coordinate components; all four value-viewer
tests passed. Independent review approved the implementation and context-menu
wiring. Actual clipboard interaction and a final full-suite/build run remain to
be checked at this checkpoint.

The successful numeric fixture
[`numeric-details.morph`](../../tests/fixtures/diagnostics/numeric-details.morph)
contains the tiny-angle graph, the least positive subnormal, signed zero and a
large number. It supports the next native GUI check and is separate from the
intentional two-root failure fixture.

## Functional checkpoint 8: whole-tree verification and actual clipboard QA

Checkpoint `147148edb7d1fe70a0cb9096b9fec4578e020ab6` passed formatting, all-target
check, strict all-target Clippy and all-target tests: **481 passed, 0 failed,
2 ignored across 43 suites**. The two ignored cases are existing opt-in
performance/stress measurements; they were not silently counted as executed.
Both debug binaries and both release binaries built. The release build took
182.234 seconds with the recorded local dependency cache/profile; this is not a
startup or clean-build performance comparison. All 17 existing developer utility
self-tests also passed.

The second native GUI workflow verified scientific output for the nearly parallel
angle, a full-precision hover, actual right-click copy and OS paste into a blank
Mousepad document, the smallest positive subnormal and negative zero. The pasted
value was exactly `5.00000001e-19`. See [GUI steps](DIAGNOSTICS_GUI_CHECK.md).
This exposed an outdated four-decimal footer; the follow-up changes only that
explanation and the diagnosis timing-percentage explanation in application source.
All-target Clippy passed again for those wording changes. Numerical behavior,
evaluation, storage and the 481-test implementation remain unchanged.

A separate old input-slider layout issue remains: a full decimal rendering of
the least subnormal can widen the inspector. The compact output viewer is correct.
Record this fixture for the signed-zero input-editor work in PR #16 rather than
overwriting that developer's implementation. The latest inspected #16 head
`0aa286a93a8b61f23ce691b1d15a025780ec1fc8` also adds interpolation, beyond its
earlier Summation/Mean changes. Neither its matrix/vector/angle sections nor
`data_view.rs` overlap our changes. Combined-branch integration is still untested.

Independent final review identified the next substantive work: cancellation
inside diagnosis, early public-API size checks and aggregate long-message memory
budgets. Current worker revision checks prevent stale publication, but do not
stop an obsolete diagnosis before the next queued evaluation. The measured
memory improvements do not establish a global bound for arbitrary error text.

The final [handoff](OMEGA_HANDOFF_20261127.md) records the single-contract
integration sequence for PR #20, preserves the separate tested project-diff WIP
at `fa56fb1`, and distinguishes actual CI/source results from pending runs.

## Functional checkpoint 9: interruptible diagnosis and a reproduced UI defect

After checkpoint `f858186b6d8907ca1184054a25ff127d71564b15`, development continued
with two issues found by independent audit and actual use.

`diagnose_cancellable` now accepts a cheap caller-supplied cancellation callback
and returns `None` when report construction is abandoned. The existing `diagnose`
API remains the always-false wrapper, with unchanged report schema and ordering.
Worker uses the revision token in both evaluation and diagnosis, and still checks
revision after diagnosis and at UI receipt. Abandoning a diagnostic never publishes
the evaluation alone or an incomplete/default report. `Evaluation.cancelled`
keeps its existing meaning, distinct from report-construction cancellation.

Checks occur at phase boundaries and periodically inside node/edge/root-set,
membership-transpose and preview loops. Bulk small-bitset copies and sorted
BTreeMap construction are preserved. Standard sorting, graph validation,
individual allocations/copies and destruction remain synchronous; there is no
hard latency bound, forced thread stop or measured zero-overhead claim. Frozen
earlier performance tables are still attributed only to their recorded source pairs.

All **19 focused diagnosis tests** passed, including four new cancellation tests
and the existing independent BFS/interleaved/root-ordinal suites. The new tests
use a 512-root/300-tail analytic graph, one-shot entry/interior/final cancellation,
invalid/empty/success/disabled/partial inputs, unchanged input state and a fresh
diagnosis after interruption. Both separate reviewers approved the API and Worker
publication boundaries. A new full-tree/build run is the next gate.

Native GUI checks also verified the 101st cause, navigation to node #100, exact-ID
and name-fragment filtering. They exposed a real layout feedback defect: the
search field's margin and the actual clear-button width exceeded the row budget,
making the window wider every frame. The fix allocates the button first and fits
the field into the remaining width. A 60-frame real-theme egui regression failed
on the old code and passed after the fix; all **five diagnostic UI tests** passed.
The [GUI record](DIAGNOSTICS_GUI_CHECK.md) and saved paging fixture preserve the
reproduction and distinguish native checks from automated layout verification.
