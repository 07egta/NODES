# Graph diagnostics: compact root ordinals and process memory

For the tested 16,384-node graph with two high-ID failing roots, the median
**whole-process peak RSS fell from 82.09 to 51.08 MiB**, a **37.77% reduction**.
Every before sample used more peak memory than every after sample. The process
contained the graph, one real evaluation and one diagnosis by one implementation;
this is not a diagnosis-only live-memory measurement.

The first-diagnosis median changed from **92.437 to 46.479 ms**. Latency varied
substantially: after samples ranged from **35.963 to 218.593 ms**, so the median
is a description of this small shared-host experiment rather than a latency
guarantee. The memory result is more consistent across the captured samples.

All **40 fresh measurement processes** exited with code 0 and passed their typed
analytic oracles. Two additional, separate verification processes passed both
independent oracles and complete serialized report equality between the before
and after implementations. Full report serialization was excluded from every
memory measurement process.

## Measured source pair

| Version | Published source | Captured local source |
|---|---|---|
| Before: hybrid heap previews with graph-node bit indices | [`545bcd4208cc`](https://github.com/07egta/NODES/commit/545bcd4208cc6bf7069a75c7e4aadd6d2e915910) | `9a2cccc6482f7a2b7da872b6e3b1dc32ef6ae856` |
| After: compact failing-root ordinals and no-failure early return | [`46417ae91af8`](https://github.com/07egta/NODES/commit/46417ae91af8cbe11eb45934d535278ce23df06b) | `24e92ebd46bfd59cc5ba532c5c9a79df86e16379` |

The local captures predate publication through the GitHub API. Their diagnostic
source bytes match the respective published revisions. The after local and
published commits also have the same tree
`98b1ffc61c79fdeb7d0bbe11496a4cd3ec0833c3`. The
[source capture](diagnostics-root-ordinal-performance/source-capture.json)
records this equivalence explicitly; local snapshot identifiers remain in the
raw executable provenance.

The exact unchanged diagnostic files are archived as
[before](diagnostics-root-ordinal-performance/diagnostics_before.rs) and
[after](diagnostics-root-ordinal-performance/diagnostics_after.rs). Their SHA-256
hashes are:

- Before: `f78f992097b84075b0851474a326ac95e57a15abe5c27657594541025d7e9898`.
- After: `f9dd3eeaf30217fa72da252b213e44061eb19254aacdbc5562622392a87fffca`.

Both source files were included as separate internal library modules in one
disposable worktree at `7c1a0956adb8edcc29945717a6eb32e0d6335be3`. Thus the shared
engine, model, dependencies and graph validation stay fixed. The two additional
module declarations preserve access to the crate-private graph traversal API;
neither diagnostic algorithm was rewritten for benchmarking.

The owned target received an explicit `cargo clean -p morphlet --release`
before the fresh library/example build. Build time was **33.580 seconds**.
Source/harness/shim/library/executable hashes, commands, environment and logs
are in the [build manifest](diagnostics-root-ordinal-performance/build-manifest.json).
The copied executable hash is
`016f40926d6529d725cc9cd6a9926f6e8a9c96124790cb7468b07c1e382cf080`.

The executable invokes the two distinct module functions, asserts different
linked function addresses, and embeds the unchanged source bytes for runtime
length/fingerprint checks against the recorded captures. These checks and the
forced library rebuild prevent the stale-library ambiguity found in an earlier
experiment. No dependency was added.

## Why the fixture exposes excess storage

The graph has IDs 0 through 16,383. Divide-by-zero nodes **16,382 and 16,383** are
the only roots. Each node with ID **0 through 16,381** is an independent Add that
consumes both roots. Node storage is reversed and rotated before validation, so
storage order cannot supply the intended classification accidentally.

The oracle follows directly from those edges:

- Exactly **2 root failures** and **16,382 blocked nodes**; every node fails.
- Each root affects all 16,382 joins, with smallest affected-ID preview
  **0 through 255** and a true affected-list truncation flag.
- Every join has exactly the root IDs **[16,382, 16,383]**, without root-list
  truncation or its own affected records.
- Every ordinary error message matches the real evaluator output.
- All 16,384 node timing records are complete, uncached and unique.

The former implementation uses positions among all sorted graph nodes as bit
indices. A set containing these two roots therefore requires **256 u64 words**.
Each independent join constructs such a set. The new implementation assigns
the two actual failing roots ordinals 0 and 1, so the same set needs **one word**.

| Analytic membership-word payload | Before | After |
|---|---:|---:|
| Independent join root sets | 16,382 | 16,382 |
| u64 words per set | 256 | 1 |
| Total word payload | 33,550,336 bytes | 131,056 bytes |
| Total word payload in MiB | 31.996 | 0.125 |

That **256× word-payload reduction is an analytic calculation**, not a measured
256× process-memory reduction. It excludes Arc headers, maps, group metadata,
graph/evaluation data, messages, report objects and allocator effects. The
observed whole-process reduction is 37.77%, reported separately below.

More generally, indexing by actual failing roots bounds a root set by their
count instead of the number or positions of all graph nodes. Graphs where nearly
every node is a root still have a large worst-case bound; this change does not
remove all dense multi-root costs.

## Isolation and measurement procedure

The [Rust harness](diagnostics-root-ordinal-performance/diagnostics_root_ordinal.rs)
has two explicitly separate modes:

1. **Verification mode** evaluates once, runs both implementations, checks their
   typed analytic oracles and compares the entire serialized reports byte for
   byte. Those processes are excluded from timing/RSS comparisons.
2. **Measurement mode** starts in a fresh process, constructs and validates one
   graph, evaluates it once, calls exactly one selected implementation once, then
   checks the typed oracle and emits a compact metadata report. It never encodes
   the full diagnostic report. The lightweight oracle allocates a small visited
   flag array and summary metadata.

Each case ran five four-process blocks: **ABBA, BAAB, ABBA, BAAB, ABBA**, where
A is before and B is after. Case order reversed on even blocks. This yields
**ten fresh-process observations per implementation/case**. The operating system
did not reuse a process or its allocator between variants; both variants are
linked into the same executable, but only the requested one runs in measurement
mode.

The diagnosis timer excludes graph construction, the real evaluation, oracle
checks and report destruction. Each sample is the first diagnosis after graph
construction/evaluation in a fresh process. It is not a repeated warm-diagnosis
sample, an evaluator-cache speed measurement, or a cold-system measurement.

Linux `wait4` supplies each child's maximum RSS, CPU and wall-time records.
Whole-process RSS includes fixture construction, evaluation/cache allocations,
one diagnostic implementation and lightweight checks. It does not isolate the
live diagnostic working set. Unlike a paired before/after process, it cannot
inherit the other implementation's allocator high-water mark.

## Environment and captured variability

- Recorded at **2026-11-27 08:59:31–08:59:37 JST**
  (2026-11-26T23:59:31.602646+00:00 to
  2026-11-26T23:59:37.212468+00:00 UTC).
- Linux 6.18.57, x86-64, reported CPU model AMD EPYC 9V74 80-Core Processor;
  **9 available logical CPUs**. Host memory was 10206472 kB total and
  6722084 kB available when measurements started.
- Rust 1.100.0 / LLVM 23.1.5; release opt-level 3, thin LTO, 16 codegen units,
  stripping enabled, incremental compilation off, no additional Rust flags.
- Root and other agents deferred CPU-heavy builds/tests during this window.
  This remains a shared cloud host, with variable scheduling and cache state.

All sample values and execution order remain in the
[measurement manifest](diagnostics-root-ordinal-performance/measurement-manifest.json)
and [comparison summary](diagnostics-root-ordinal-performance/comparison-summary.json).
No sample was removed as an outlier. No confidence interval or general latency
bound is claimed. CAD evaluation and UI responsiveness were not benchmarked.

## Results for every measured case

The successful control is a Number(2.5) followed by 9,999 Add(1) nodes, with
known final value **10,001.5** and zero failures. The after source also contains
a **no-failure early return after graph validation and timing-summary assembly**.
The success control measures that path as part of this source pair; root-bitset
compaction alone cannot explain a change in a graph with no failing roots.

| Case | Before first-diagnosis median | After first-diagnosis median | Observed before/after ratio |
|---|---:|---:|---:|
| 16,384 nodes, two high-ID roots | 92.437 ms | 46.479 ms | 1.99× |
| 10,000-node successful chain | 11.012 ms | 8.792 ms | 1.25× |

| Case | Before median process peak RSS | After median process peak RSS | Median reduction |
|---|---:|---:|---:|
| 16,384 nodes, two high-ID roots | 84,056 KiB / 82.09 MiB | 52,304 KiB / 51.08 MiB | 37.77% |
| 10,000-node successful chain | 41,708 KiB / 40.73 MiB | 41,572 KiB / 40.60 MiB | 0.33% |

The success control's 136 KiB median RSS difference is small relative to the
complete process and is not presented as a material memory improvement.

| Case and implementation | Diagnosis min–max | Complete-process peak RSS min–max |
|---|---:|---:|
| High-ID roots, before | 54.646–161.362 ms | 83,988–84,180 KiB |
| High-ID roots, after | 35.963–218.593 ms | 52,248–52,440 KiB |
| Successful chain, before | 8.788–18.503 ms | 41,580–41,764 KiB |
| Successful chain, after | 8.046–18.782 ms | 41,516–41,708 KiB |

The high-ID fixture's memory ranges do not overlap. Its median peak-RSS
difference is **31.01 MiB**; the analytically removed word payload is 31.87 MiB.
The two numbers need not match because peak RSS includes other allocations and
allocator/page behavior.

The 40 measurement processes consumed 5.396 seconds of summed wall time,
3.317 seconds of user CPU and 2.037 seconds of system CPU. These totals include
graph construction, real evaluation and checks; they are not diagnostic-only
execution totals.

## Correctness evidence and reproduction

Both separate verification processes passed full report equality. Their reports
were 8,245,556 bytes for the high-ID failure graph and 1,472,513 bytes for the
successful chain. Those bytes include real evaluation timing values and may
change across new evaluations. Equality was asserted within each verification
process, where both implementations share exactly the same evaluation.

All 40 measurement processes independently checked exact classifications, root
and blocked IDs, affected counts/previews/truncation flags, error preservation,
timing coverage and the successful chain value. Each process also checked linked
function identity and embedded source fingerprints. The
[verification manifest](diagnostics-root-ordinal-performance/verification-manifest.json)
records the separate equality runs; per-process raw JSON records all measurement
oracle results.

The [artifact README](diagnostics-root-ordinal-performance/README.md) documents
source capture, module inclusion, build, correctness mode, fresh-process ABBA
execution and interpretation. Preserve the frozen source pair and hashes when
reproducing this experiment. The
[earlier heap preview experiment](DIAGNOSTICS_HEAP_BENCHMARK_RESULTS.md)
uses a different source pair and repeated-call timing protocol; its absolute
times should not be compared directly with these first-call samples.
