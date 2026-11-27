# Diagnostic cancellation: ordinary-path cost experiment

**Measured on 2026-11-27; all six processes passed the independent expectations
and complete-report equality checks.** Cooperative cancellation has a measurable
ordinary-path cost in these failure-heavy fixtures. Compared with the earlier
API, the runtime atomic callback's repeated-call median increased by **0.645 ms
(11.52%)** for the 1,024-root prefix graph and **0.731 ms (9.37%)** for the
interleaved graph. This experiment does not establish zero overhead or measure
cancellation response time, CAD evaluation speed, or GUI responsiveness.

All raw samples, initial calls, source snapshots, manifests and the Rust harness
are preserved in [the dedicated evidence directory](diagnostics-cancellation-cost/).
No earlier benchmark result is reused for this source pair, and no extra runs or
outlier removal were used to improve the result.

## Repeated-call results

Values below are medians of **ten diagnosis calls per path and case**, pooled
from two processes with exactly reversed corresponding execution orders. An
initial call per path in each process is recorded separately and excluded from
these medians. Positive percentages mean a slower diagnosis.

| Fixture | Before `diagnose` | After `diagnose` | Wrapper change | After runtime atomic callback | Atomic change vs before |
|---|---:|---:|---:|---:|---:|
| Successful 10,000-node chain | 7.667 ms | 7.973 ms | +4.00% | 7.734 ms | +0.87% |
| Prefix: 1,024 roots, 2,047 nodes | 5.600 ms | 5.970 ms | +6.61% | 6.245 ms | +11.52% |
| Interleaved: 512 core roots, 1,805 nodes | 7.802 ms | 8.116 ms | +4.03% | 8.533 ms | +9.37% |

The runtime callback adds 0.275 ms (+4.61%) over the new wrapper's median in the
prefix case and 0.416 ms (+5.13%) in the interleaved case. The new wrapper itself
also has a higher observed median; comparing the source pair includes traversal
refactoring and polling infrastructure, so the full difference cannot be
attributed solely to atomic loads.

The successful chain has substantial variation between orders and processes.
Its runtime callback median is 3.01% below the new wrapper median, which is not
evidence that adding an atomic load improves performance. The small sample and
shared host do not establish statistical significance for these differences.

### Retained variation and initial calls

All repeated samples are included in the ranges below. Exact values and both
per-process orders are in [comparison-summary.json](diagnostics-cancellation-cost/comparison-summary.json)
and its linked raw process records.

| Fixture | Before repeated range | After wrapper repeated range | After atomic repeated range |
|---|---:|---:|---:|
| Success 10,000 | 6.689–10.589 ms | 6.918–10.183 ms | 6.706–8.917 ms |
| Prefix 1,024 | 5.332–6.247 ms | 5.686–6.493 ms | 5.884–6.774 ms |
| Interleaved 512 | 7.444–9.029 ms | 7.715–8.583 ms | 7.890–9.108 ms |

The following are **both initial calls**, in process order, rather than pooled
medians. They remain part of the evidence even where larger than repeated calls.
They are not cold-system measurements: all three paths share one process, and
neither OS nor allocator caches are reset.

| Fixture | Before initial calls | After wrapper initial calls | After atomic initial calls |
|---|---:|---:|---:|
| Success 10,000 | 10.032 / 7.026 ms | 10.352 / 11.389 ms | 21.786 / 12.600 ms |
| Prefix 1,024 | 6.600 / 5.594 ms | 6.463 / 6.374 ms | 6.543 / 7.112 ms |
| Interleaved 512 | 7.891 / 8.432 ms | 7.922 / 8.936 ms | 8.416 / 16.050 ms |

## What was compared

The exact diagnostic sources were compiled as two internal modules in one
freshly rebuilt release library. The shared model, evaluator, graph validation
and locked dependency baseline is
[`7c1a0956adb8edcc29945717a6eb32e0d6335be3`](https://github.com/07egta/NODES/commit/7c1a0956adb8edcc29945717a6eb32e0d6335be3).
This fixes the surrounding implementation rather than comparing two different
whole application builds.

| Source | Public revision | Diagnostic source SHA-256 |
|---|---|---|
| Before | [`f858186`](https://github.com/07egta/NODES/commit/f858186b6d8907ca1184054a25ff127d71564b15) | `f9dd3eeaf30217fa72da252b213e44061eb19254aacdbc5562622392a87fffca` |
| After | [`3e3bb46`](https://github.com/07egta/NODES/commit/3e3bb46185628e479888053fe4e4ff606047c244) | `865324a95c61991e84c6797ab15573935d17245cb95c6da7575343adb9c65a33` |

The before source is byte-identical to the previously published ordinal source
`46417ae91af8cbe11eb45934d535278ce23df06b`. The snapshots are included unchanged,
including their original visibility and traversal calls. The library shim only
registers those modules.

The three paths are intentionally separate:

1. `before`: the earlier `diagnose` function.
2. `after_wrapper`: the new `diagnose`, which supplies a literal-false callback.
3. `after_atomic`: `diagnose_cancellable` with the Worker's unchanged-revision
   comparison, `latest.load(Ordering::Relaxed) != request_revision`.

For the third path, the `Arc<AtomicU64>` allocation and black-boxing of its final
reference happen before timing. The callback performs only the load and
comparison: it has no counter, Arc cloning, allocation or inner `black_box` call.
No thread changes the token, and any unexpected `None` fails the experiment.

## Correctness and measurement boundaries

Each process constructs its graph and independently derived expected result,
then uses the real evaluator exactly once. All three paths receive the same
immutable graph and evaluation snapshot. The successful chain ends at 10,001.5.
The prefix graph has exactly 1,024 roots and 1,023 blocked nodes. The interleaved
graph has 512 core roots plus 255 extra roots, for **767 roots and 1,038 blocked
nodes**; each core root affects exactly 536 nodes.

The first report from each path passes an analytic oracle covering all
classifications, exact counts, sorted capped relationship IDs, truncation flags,
actual evaluator error messages, complete timing coverage and the successful
numeric output. Every report is then required to have identical complete JSON
bytes within its process. There were **90 repeated calls and 18 initial calls**,
all passing these checks; all six processes exited successfully with empty
stderr. Report bytes can differ between processes because the real evaluator's
timings are newly measured in each process.

Only diagnosis and report construction are inside the timer. Fixture/oracle
construction, evaluation, serialization, equality checks and report destruction
are outside it. Initial serialization timings and full JSON sizes are retained
in the raw records. The fixture is repeatedly diagnosed without re-evaluation;
this is not the whole time taken by the actual Worker, which diagnoses each new
revision once. It also does not measure cancellation latency or concurrent edits.

The order rotates among the three paths for each round. A second process reverses
every corresponding round exactly, and the case order also reverses. No sample
was discarded, and the fixed three-case matrix was not expanded after observing
the results. The protocol and standalone reproduction steps are in the
[artifact README](diagnostics-cancellation-cost/README.md).

## Build, host and evidence

The dedicated Cargo target retained third-party dependencies but explicitly
cleaned the project package before compiling both modules together. This avoids
the stale-library reuse found in an earlier, separately archived experiment.
Exact source hashes, embedded source fingerprints, distinct linked wrapper
addresses, the shim, harness, compiled library and executable were checked.

- Rust 1.100.0, `x86_64-unknown-linux-gnu`; release opt-level 3, thin LTO,
  16 codegen units, incremental compilation disabled, two build jobs.
- Reported CPU: AMD EPYC 9V74; nine logical CPUs available to the process.
  Linux reported 10,206,472 kB total memory and 6,181,156 kB available at start.
- The fresh package build took 35.420 s and exited successfully. The six bounded
  measurement processes ran at 2026-11-27 09:30:04–09:30:06 JST (UTC+09:00).
  Raw manifests retain their original UTC timestamps.
- Owned builds and tests were stopped during measurement. The shared host and
  another developer's idle desktop instance could not be guaranteed inactive.
- Executable SHA-256:
  `47543affda15e551e515df6d885bb88827ed5814b1dbc864c1a5fe949668aa75`.

Process CPU time and peak RSS cover graph preparation, real evaluation, all
three paths and full JSON verification. They do not isolate any path's memory
cost. The linked-library cache path is historical provenance; the uniquely
preserved executable identifies what was measured.

The [build manifest](diagnostics-cancellation-cost/build-manifest.json),
[measurement manifest](diagnostics-cancellation-cost/measurement-manifest.json)
and [validation record](diagnostics-cancellation-cost/validation.json) retain
commands, exits, source checks and independent recalculation. The measurement
protocol and raw arithmetic were reviewed by an agent other than the harness
author. These results bound the observed ordinary-path cost for the specified
fixtures; they do not establish an application-wide performance change.
