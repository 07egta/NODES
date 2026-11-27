# Graph diagnostics: interleaved preview benchmark

The hybrid heap preview implementation reduced measured diagnosis time on all
three tested interleaved-ID graphs. The largest case, with **8,973 nodes and
4,351 failing roots**, changed from **469.484 to 96.023 ms**, an observed
**4.89× before/after ratio**. Every implementation produced the same complete
serialized report for the same real evaluation, and independent analytic
oracles passed.

The controls also matter: the **prefix-2048 median increased from 17.516 to
19.335 ms (+10.39%)** in this experiment. The successful 10,000-node control
increased 1.64%; prefix-512 increased 0.29%. These results support the targeted
interleaved-ID improvement and do not establish a universal improvement or
absence of regressions. They measure `diagnose`, not CAD evaluation or GUI
responsiveness.

All **12 measured processes** exited successfully. They contain **144 diagnosis
calls**: 24 first calls and 120 repeated calls, split equally between the two
implementations. Two earlier correctness smoke processes are archived separately
and excluded from the comparison.

## Sources and actual linked implementations

| Component | Frozen source |
|---|---|
| Shared library, engine, model and dependencies | [`7c1a0956adb8`](https://github.com/07egta/NODES/commit/7c1a0956adb8edcc29945717a6eb32e0d6335be3) |
| Before: sorted preview merging | Exact `src/diagnostics.rs` from `7c1a0956adb8edcc29945717a6eb32e0d6335be3` |
| After: hybrid sorted/heap previews | Exact `src/diagnostics.rs` from remote [`545bcd4208cc`](https://github.com/07egta/NODES/commit/545bcd4208cc6bf7069a75c7e4aadd6d2e915910), captured locally as `9a2cccc6482f` |

The diagnostic snapshots are archived as
[before](diagnostics-heap-performance/diagnostics_before.rs) and
[after](diagnostics-heap-performance/diagnostics_after.rs). Neither source file
was rewritten or instrumented. Their SHA-256 values are:

- Before: `d844ea73131327e51351edd38567f17e1c57d89c64666afdc7bf82a2912ac290`.
- After: `f78f992097b84075b0851474a326ac95e57a15abe5c27657594541025d7e9898`.

The measurement records retain the original local after-commit label
`9a2cccc6482f7a2b7da872b6e3b1dc32ef6ae856`. GitHub Git-data publication created
remote commit `545bcd4208cc6bf7069a75c7e4aadd6d2e915910` with the identical tree
`45b249a028b4a3f175b4826c9b5ee56bacdbc795`; the root agent verified that tree and
remote ref. Use the remote commit or archived snapshot when reproducing from
GitHub. The local-only commit label is not a publicly retrievable GitHub commit.

A disposable 7c worktree received two additional library module declarations and
the [paired harness](diagnostics-heap-performance/diagnostics_heap_perf.rs).
Putting both modules inside the library preserves access to the crate-private
`Graph::topological_nodes`; copying the source into an external example alone
would not preserve that access. Both implementations use the same `Graph`,
`Evaluation`, compiler, dependencies, release profile and executable.

Before compiling, `cargo clean -p morphlet --release` explicitly removed the
project package from the exclusively owned dependency target. The fresh project
library and paired example then built successfully in **38.113 seconds**. This
avoids relying on Cargo to notice a worktree change while keeping third-party
dependencies reusable. The earlier experiment's stale-library failure is
documented in the [previous benchmark report](DIAGNOSTICS_BENCHMARK_RESULTS.md);
none of its invalid results is used here.

The harness refers directly to the two distinct module functions, asserts that
their linked function addresses differ, and embeds each unmodified snapshot's
bytes. Runtime byte lengths and FNV-1a checks match the externally recorded
SHA-256 source captures. Those checks complement the forced project rebuild;
an embedded revision string alone would not prove linked code provenance. The
[build manifest](diagnostics-heap-performance/build-manifest.json) records source,
shim, harness, library and executable hashes, complete build commands and exit
codes. The copied executable SHA-256 is
`c9501562d64f0b72c50bc8ba132c13055f271f420575c1bc9e69306f1ee8d92e`.

Later compact-root ordinal changes are outside this captured source pair and are
not measured by these results.

## Method and environment

- Measurement window: **2026-11-27 08:53:41–08:53:52 JST**
  (2026-11-26T23:53:41.731644+00:00 to
  2026-11-26T23:53:52.904690+00:00 UTC).
- Linux 6.18.57, x86-64; reported CPU model AMD EPYC 9V74 80-Core Processor;
  **9 logical CPUs available** to the process. Host `MemTotal` was 10206472 kB,
  with 6716568 kB available at the start.
- Rust 1.100.0, LLVM 23.1.5; release opt-level 3, thin LTO, 16 codegen units,
  stripping enabled, no extra Rust flags, incremental compilation disabled.
- Root and peer agents paused their builds/tests for the timing window. The
  shared cloud host was not an exclusive laboratory machine.
- Each process constructed and validated one graph, then evaluated it once with
  a fresh real `Evaluator`. Every diagnostic call used that same immutable graph
  and evaluation; neither implementation reevaluated nodes.
- Each process made one initial call to each implementation, followed by five
  repeated calls to each, alternating their order every round. The second
  process for each case reversed the initial order. Case order was also reversed
  in the second pass.
- Each table median pools **ten repeated samples per implementation/case**.
  First calls remain in raw JSON but are excluded. The first call is not a
  cold-process measurement: the process has already built and evaluated its graph.
- Diagnostic timers include report construction, validation and attribution.
  They exclude fixture/oracle construction, evaluation, serialization, equality
  checks and report destruction. Allocator and OS caches were not reset.

The [measurement manifest](diagnostics-heap-performance/measurement-manifest.json)
contains all process commands, exact times, exit codes and resource measurements.
The [comparison summary](diagnostics-heap-performance/comparison-summary.json)
retains every sample and the pooling rule. For example, interleaved-4096 after
samples ranged from 85.551 to 172.383 ms; its 96.023 ms median should not be
interpreted as a latency guarantee. No confidence interval is claimed.

## All measured diagnosis results

Negative change means a lower median diagnosis time. Failing-root counts include
the extra independent roots used by the interleaved fixture.

| Case | Nodes | Failing roots | Before median | After median | Change |
|---|---:|---:|---:|---:|---:|
| success-10000 | 10,000 | 0 | 7.744 ms | 7.871 ms | +1.64% |
| prefix-512 | 1,023 | 512 | 2.001 ms | 2.007 ms | +0.29% |
| interleaved-32 | 845 | 287 | 4.617 ms | 1.137 ms | −75.37% |
| prefix-2048 | 4,095 | 2,048 | 17.516 ms | 19.335 ms | +10.39% |
| interleaved-512 | 1,805 | 767 | 55.070 ms | 8.021 ms | −85.44% |
| interleaved-4096 | 8,973 | 4,351 | 469.484 ms | 96.023 ms | −79.55% |

The interleaved before/after ratios are **4.06×**, **6.87×** and **4.89×** as the
core-root count increases from 32 to 4,096. The prefix-2048 control was slower in
both processes: 17.519 versus 18.982 ms in the first process and 17.488 versus
20.106 ms in the second. Keep this observed cost visible in follow-up work rather
than dismissing it as noise. This experiment does not isolate whether allocation,
code layout or another implementation detail causes that difference.

## Analytic fixtures and correctness checks

The successful chain starts with Number(2.5), followed by 9,999 Add(1) nodes. Its
last value must be **10,001.5**, and it must have no root or blocked errors.

Each prefix fixture has R independent Divide-by-zero roots followed by R−1 Add
joins. Each successive join introduces one more root. A root's exact descendant
interval and a join's exact prefix of root IDs follow directly from their
indices; the oracle uses those formulas, independently of diagnostics traversal.

The interleaved fixture follows the committed interleaved regression test, with
core-root count R set to 32, 512 or 4,096:

1. R independent Divide-by-zero roots feed a balanced binary Add tree.
2. Its final join and a tail form **273 shared affected nodes**.
3. An additional **255 independent roots** each feed one separate join with the
   core tree's final join.
4. The final core join is assigned ID 0; the 255 separate joins receive IDs
   1–255. All other IDs are at least 256. Storage order is rotated and reversed.

The node total is `2R − 1 + 272 + 510 = 2R + 781`, hence **8,973** at R=4,096.
This explicit calculation corrects an earlier planning estimate of 8,956; the
measured fixture preserves the regression test's 273-node shared segment.

Every core root must have exact affected count
`273 + 255 + log2(R) − 1` and smallest affected IDs **0 through 255**. The exact
counts are 532, 536 and 539 for the three cases, independently of the 256-ID
preview cap. Every extra root affects precisely its own low-ID join. Each such
join has R+1 upstream roots; each balanced join has the analytically known
contiguous interval of core-root ordinals; each shared tail node has all R core
roots. The oracle applies the known ID permutation and the 64-root preview cap.

For each implementation's first report, the harness verifies every root and
blocked record, sorted capped ID lists, exact counts, truncation flags, ordinary
error messages against the real evaluator, complete timing-record coverage and
summary counts. Every subsequent call must match the entire verified serialized
report byte-for-byte. Equality includes messages, names, kinds, elapsed values,
shares and all other serialized fields. JSON comparisons are within a process;
separate real evaluations naturally produce different measured node timings.

## Why the targeted optimization helps

In these interleaved fixtures, a filled affected-ID preview repeatedly receives
small IDs from distinct groups. The former path allocates and merges a bounded
sorted list for each group. With preview cap K, each such merge can copy O(K)
entries. The new path keeps ascending inputs in a vector, converts that vector
once to a max-heap when IDs interleave, and retains the smallest K IDs with
O(log K) replacements. The final heap is sorted once for the public report.
Exact affected counts remain independent of preview storage.

Ordered-prefix fast paths are still present in the new source, but the control
measurements above show that preserving the algorithmic shortcut does not by
itself guarantee identical measured cost. These results justify the interleaved
case fix and identify prefix overhead as a separate item to monitor.

## Serialization and complete-process memory

The two implementations produced identical JSON bytes within each process. The
small size ranges below come from measured timing values in the two separate
evaluations, not from a schema or content difference between implementations.
Serialization ranges cover the first report of each implementation in both
processes and are outside the diagnosis medians.

| Case | Report JSON bytes across two evaluations | First-report serialization range | Largest paired-process peak RSS |
|---|---:|---:|---:|
| success-10000 | 1,473,328–1,474,586 | 2.937–7.679 ms | 44.30 MiB |
| prefix-512 | 976,213–976,750 | 0.946–1.363 ms | 16.60 MiB |
| interleaved-32 | 505,449–505,662 | 0.516–0.995 ms | 13.21 MiB |
| prefix-2048 | 4,792,666–4,794,500 | 4.673–27.482 ms | 64.96 MiB |
| interleaved-512 | 1,483,999–1,484,054 | 1.551–2.567 ms | 25.59 MiB |
| interleaved-4096 | 8,327,956–8,329,027 | 11.531–13.543 ms | 125.41 MiB |

Peak RSS comes from `wait4` for a process containing **both implementations**,
fixture construction, evaluation, serialized buffers and independent parsed-JSON
oracle checks. It cannot establish per-implementation or diagnosis-only memory
use. A later memory comparison must use separate fresh processes for each
implementation so allocator retention and high-water marks do not cross variants.

The 12 measured processes consumed 11.117 seconds of summed wall time, 10.259
seconds of user CPU and 0.773 seconds of system CPU. These totals include all
validation and serialization; they are not diagnostic-only totals.

## Reproduction and follow-up

The [artifact README](diagnostics-heap-performance/README.md) explains the
disposable-worktree setup, unchanged source-module inclusion, clean/build
procedure, invocation, and evidence checks. Preserve the recorded source pair
when reproducing these numbers. A later source pair requires a new experiment
directory and new hashes.

The next useful checks are the observed ordered-prefix overhead and the memory
cost of sparse root sets whose node IDs are high. Those are distinct from this
heap preview experiment. Evaluate them with their own analytic fixtures and
preserve the successful and prefix controls. Avoid inferring CAD evaluation,
interactive UI latency, or Windows performance from the current Linux diagnosis
results.
