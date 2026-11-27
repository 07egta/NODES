# Ordinary-path cancellation cost: frozen experiment

This directory contains the exact Rust diagnostic snapshots, three-path harness
and evidence for [the cancellation cost report](../DIAGNOSTICS_CANCELLATION_COST.md).
It is a bounded developer experiment and does not add normal application runtime
dependencies. Earlier benchmark directories are preserved separately.

## Source and build contract

- Before: `f858186b6d8907ca1184054a25ff127d71564b15:src/diagnostics.rs`.
- After: `3e3bb46185628e479888053fe4e4ff606047c244:src/diagnostics.rs`.
- Shared engine, model, graph validation and dependencies:
  `7c1a0956adb8edcc29945717a6eb32e0d6335be3`.

The before source is byte-identical to the earlier published ordinal source
`46417ae91af8cbe11eb45934d535278ce23df06b`; the after source adds cooperative
diagnostic cancellation. Both archived source files must remain unchanged.

Create a detached worktree at the shared library baseline, copy both snapshots
into `benchmark-snapshots/`, append `library-module-shim.rs` once to `src/lib.rs`,
and copy `diagnostics_cancellation_cost.rs` into `examples/`. Internal library
modules retain legal access to the crate-private graph traversal method. Neither
diagnostic algorithm is adapted for the experiment.

Use an exclusively owned Cargo target with incremental compilation disabled and
two build jobs. Set these metadata variables to `build-manifest.json` values:

- `NODES_COST_LIBRARY_SHA`
- `NODES_COST_BEFORE_SHA`
- `NODES_COST_AFTER_SHA`
- `NODES_COST_BEFORE_SHA256`
- `NODES_COST_AFTER_SHA256`

A fresh target is valid. When reusing third-party dependency artifacts, explicitly
remove the project package before rebuilding both implementations together:

```sh
cargo clean -p morphlet --release --target-dir "$diagnostics_target"
cargo build --release --locked --offline --example diagnostics_cancellation_cost
```

`diagnostics_target` must match `CARGO_TARGET_DIR`. Copy the executable to a unique
path before another build. Record source, harness, shim, compiled library and
executable hashes. The library artifact path is a historical build-cache location;
a later package clean can replace it. The preserved executable and recorded hash
remain the identity of the measured binary.

The harness checks distinct linked wrapper function addresses and emits embedded
snapshot byte lengths/fingerprints. Check those against the captured files in
addition to the forced project-library rebuild; a revision label alone does not
prove which library was linked.

## Three separately named API paths

1. `before`: the old `diagnose` function.
2. `after_wrapper`: the new `diagnose` function, which supplies a literal-false
   callback to `diagnose_cancellable`.
3. `after_atomic`: the new cancellable API with a runtime
   `latest.load(Ordering::Relaxed) != request_revision` callback, matching the
   Worker's revision comparison.

The runtime token is an `Arc<AtomicU64>` created before timing. Its final atomic
reference is passed through `black_box` once outside timing so the callback is
not expressed as a known constant. The timed callback only loads and compares;
it does not count invocations, clone an Arc, allocate, or call `black_box`.
No thread changes the revision during the experiment, so an unexpected `None`
is a failed benchmark. This measures the unchanged-revision path, not cancellation
response latency or Worker scheduling.

## Fixed cases and independent checks

The protocol has exactly three cases:

| Case | Nodes | Failing roots | Oracle |
|---|---:|---:|---|
| `success-10000` | 10,000 | 0 | Number(2.5) plus 9,999 Add(1), final value 10,001.5 |
| `prefix-1024` | 2,047 | 1,024 | Closed-form prefix root and descendant intervals |
| `interleaved-512` | 1,805 | 767 | Balanced 512-core-root tree, 273 shared nodes, 255 extra roots/joins, known ID permutation |

The interleaved case is the same analytic shape documented by the earlier heap
experiment. Its core roots each affect 536 nodes and retain smallest affected
IDs 0–255. Every extra root affects exactly its own low-ID join. Each join's
upstream roots follow directly from its balanced-tree interval or explicit edge.

The first report from each path independently checks every classification,
exact count, sorted capped relationship list, truncation flag, evaluator message,
timing-record coverage and the known successful numeric value. Every report from
every later call must be byte-identical to that same verified full report within
the process. Graphs are evaluated once using the real evaluator, with immutable
graph/result references shared by all three paths.

## Bounded measurement procedure

Use a quiet window for owned CPU-heavy builds/tests. Shared cloud-host idleness
cannot be guaranteed. For each case run two separate processes:

```sh
diagnostics-cancellation-cost --case prefix-1024 --repetitions 5
diagnostics-cancellation-cost --case prefix-1024 --repetitions 5 --reverse
```

The recorded run executed all three cases in the table order, then reversed the
case order for the second pass. Each process records one first report call per
path and five repeated calls per path. The three-path order rotates with each
round; `--reverse` reverses each corresponding round exactly. This balances the
pairwise orderings. Pool the ten repeated samples per path/case and compute their
median, keeping the first calls separate. Do not remove outliers or extend the
case matrix to seek a favorable result.

Only diagnosis/report construction is timed. Graph/oracle construction, the one
real evaluation, serialization, verification and report destruction are outside
the timer. OS/allocator caches are not reset; these are repeated diagnostic calls,
not cached node evaluation or cold-system measurements.

The old/new comparison includes traversal refactoring as well as cancellation
instrumentation. The wrapper and runtime callback are intentionally reported
separately. A positive percentage change means a slower median diagnosis. Do not
infer zero overhead, CAD evaluation speedup, GUI latency, or cancellation latency.

## Evidence interpretation

`build-manifest.json` records actual build commands, exit codes, compiler/profile
and binary/library hashes. `measurement-manifest.json` records the six process
commands, exit codes, order verification and host observations. The matching raw
JSON files retain all per-call orders and timings. `comparison-summary.json`
retains all ten samples per path/case, medians, ranges and signed changes.

`build-experiment.py` and `measure-experiment.py` preserve the developer-only
orchestration used for this run; all graph, diagnostic and timing work is Rust.
The scripts retain this workspace's absolute paths as provenance. Reproduction
requires adapting those paths to a new worktree, exclusively owned target and
fresh output directory. Do not execute them against this frozen evidence:
the build script writes manifests, and the measurement script deliberately
refuses to replace an existing measurement manifest.

Any process CPU/peak-RSS records cover the entire three-path process, including
all three API paths and full report verification. They are not per-path memory
measurements. `validation.json` records the completed checks and independent
raw-result audit; `SHA256SUMS` covers the final artifact files and report.
Do not reuse another benchmark's numbers as
results for this source pair.
