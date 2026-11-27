# Graph diagnostics: verified matched benchmark results

The measured optimization reduced diagnosis time on the three tested incremental
merge graphs. Median time fell from **42.111 → 2.146 ms**, **201.041 → 5.848 ms**,
and **961.544 → 18.206 ms** as the number of independent failing roots increased
from 512 to 2,048. These are observations for `diagnostics::diagnose` on these
specific failure graphs. CAD evaluation and GUI responsiveness require separate
measurements.

All **13 corrected benchmark processes**, representing **19 case executions**,
completed with exit code 0 and passed their analytic invariants. Failure fixtures
intentionally contain evaluator errors; passing means the classifications, exact
counts, bounded ID previews, truncation flags and ordinary messages matched their
known expectations.

## Measured sources and linked-code verification

| Version | Immutable source | Linked library behavior |
|---|---|---|
| Before | [`78eac24ac730`](https://github.com/07egta/NODES/commit/78eac24ac730ff3a9d882df55a991923e25ad7c2) | Serialized failure has no `message_truncated` field |
| After | [`7c1a0956adb8`](https://github.com/07egta/NODES/commit/7c1a0956adb8edcc29945717a6eb32e0d6335be3) | Serialized failure includes `message_truncated: false` for the ordinary test error |

The before worktree received an untracked byte-identical copy of the after
benchmark harness. Its SHA-256, shared by both compiled harnesses, is
`133790dd07cd33937f9ead158764ca14d07f218ef4d7b73dcc9b9ec8efbce1a0`. `Cargo.toml` and `Cargo.lock` also match
byte-for-byte. The diagnostic module and executable hashes are recorded in the
[build manifest](diagnostics-performance/build-manifest.json).

Each version used an independently populated build artifact directory. The after
version was rebuilt in a new, initially absent target directory after the first
experiment exposed stale-library reuse. The copied executables were checked for
the expected serialized schema marker. Independent probes linked the exact
recorded `libmorphlet` artifacts, evaluated a real Divide-by-zero node, serialized
the returned report and asserted field absence/presence. Preserve the
[probe records](diagnostics-performance/linked-library-probes.json),
[before output](diagnostics-performance/before-linked-schema-probe.json),
[after output](diagnostics-performance/after-linked-schema-probe.json) and
[probe source](diagnostics-performance/linked-schema-probe.rs).

The working harness subsequently received a Clippy-required change from
`chunks_exact(2)` to `as_chunks::<2>().0.iter()`. That later lint adjustment is
outside the frozen measured revisions. Reproduce the archived source/hash for an
exact comparison; timings are not certification of every later commit.

## Machine and procedure

- Recorded at **2026-11-27 08:46:26–08:46:46 JST**
  (2026-11-26T23:46:26.891963+00:00 to 2026-11-26T23:46:46.500098+00:00 UTC).
- Linux `6.18.57`, x86-64; reported CPU model
  `AMD EPYC 9V74 80-Core Processor`; **9 logical CPUs** available to the process.
- Host `MemTotal` was 10206472 kB. This is a
  cloud Linux measurement, rather than a measurement on the user's Windows PC.
- `rustc 1.100.0 (0ef1bc386 2026-11-09)`; release profile `opt-level=3`, thin LTO,
  `codegen-units=16`, stripping enabled. The actual library fingerprint records
  no additional Rust flags. Builds used two jobs with incremental compilation off.
- Other agents deferred their build/test workloads during the corrected timing
  window. The machine was not an exclusive laboratory host; load, available
  memory and disk observations are retained in the manifest.
- Matched cases ran before/after in ascending size order, then after/before in
  descending order. Each process made one initial diagnostic call followed by
  **five repeated calls** on the same real evaluated graph. Each table median
  pools the **ten repeated samples** for that version/case across two processes.
- The optimized full matrix ran last, with five repeated samples per case.
  All samples and first-call times remain in the raw JSON. No confidence interval
  or universal speedup is inferred from this small controlled sample.

Diagnosis timers include validation, cause attribution, timings and report
construction. They exclude fixture construction, the one real evaluation, oracle
checks, serialization and report destruction. Allocator/OS caches were not reset;
“first call” is not a cold-process measurement, and repeated diagnosis is not a
warm evaluator-cache benchmark. See the
[full protocol](DIAGNOSTICS_PERF_PROTOCOL.md).

## Matched incremental-merge diagnosis times

| Case | Nodes | Failing roots | Before median | After median | Observed before/after ratio | Median reduction |
|---|---:|---:|---:|---:|---:|---:|
| prefix-512 | 1,023 | 512 | 42.111 ms | 2.146 ms | 19.62× | 94.90% |
| prefix-1024 | 2,047 | 1,024 | 201.041 ms | 5.848 ms | 34.38× | 97.09% |
| prefix-2048 | 4,095 | 2,048 | 961.544 ms | 18.206 ms | 52.82× | 98.11% |

The fixed shape is R independent failing roots followed by R−1 binary Add nodes;
each successive Add introduces one additional root. This creates many distinct
root sets. The optimization orders relationship groups by their smallest affected
ID, appends already ordered previews, skips further preview copying once the
bounded smallest-ID list is full, and uses a direct root-index table. Exact
counts remain independent of capped preview lengths.

Use the [comparison summary](diagnostics-performance/comparison-summary.json)
for every individual sample, per-process median, minimum/maximum and raw-file
links. This table measures the complete diagnostic API; it does not isolate each
internal change or imply the same ratio for successful graphs.

## Optimized full matrix

| Case | Nodes | Failing roots | Diagnosis median, five calls | JSON serialization, one call | Report JSON bytes |
|---|---:|---:|---:|---:|---:|
| success-1000 | 1,000 | 0 | 0.451 ms | 0.866 ms | 146,180 |
| success-10000 | 10,000 | 0 | 6.415 ms | 9.064 ms | 1,474,328 |
| failure-chain-16384 | 16,384 | 1 | 19.705 ms | 33.044 ms | 8,078,431 |
| balanced-1024-tail | 16,384 | 1,024 | 25.215 ms | 53.134 ms | 11,993,469 |
| prefix-1024 | 2,047 | 1,024 | 5.582 ms | 2.566 ms | 2,308,177 |
| prefix-4096 | 8,191 | 4,096 | 66.183 ms | 11.572 ms | 9,756,706 |
| prefix-8192 | 16,383 | 8,192 | 295.698 ms | 97.140 ms | 21,340,820 |

The largest incremental-merge case diagnosed **16,383 nodes / 8,192 roots in
295.698 ms median**, while the 16,384-node single-failure chain took 19.705 ms and
the balanced-join-plus-tail graph took 25.215 ms. Shape matters: a graph that
introduces a new distinct root set at each merge still costs more than a chain
sharing one root set. This is a concrete remaining optimization target.

Exact counts are checked for all roots and blocked nodes, while individual
relationship arrays stay capped at 64 upstream roots and 256 affected IDs. The
8,192-root report serialized to **21,340,820 bytes**. That is an output-size
measurement; application memory and process peak RSS are separate quantities.

Raw full-matrix evidence is
[`after-full-matrix-round-1.json`](diagnostics-performance/after-full-matrix-round-1.json).
Its complete process took **3.379 s** including fixture setup,
evaluation, all repeated diagnoses, analytic assertions and serialization.

## Observed process resources

Linux `wait4` recorded resource usage for each benchmark child. These figures
cover the whole headless benchmark process, including evaluation, report/oracle
allocations and JSON buffers; they do not isolate diagnosis or measure the GUI.
The table uses the larger observed peak of the two matched runs.

| Case | Before process peak RSS | After process peak RSS |
|---|---:|---:|
| prefix-512 | 13.38 MiB | 13.38 MiB |
| prefix-1024 | 14.51 MiB | 13.38 MiB |
| prefix-2048 | 26.82 MiB | 23.53 MiB |

The optimized full-matrix process peaked at **107,472 KiB
(104.95 MiB)**, with **3.025 s user CPU**
and **0.337 s system CPU**. Its peak covers the combined sequence
of seven cases and allocator retention between them. Per-process resource values,
exit codes, elapsed time, execution order, source/binary hashes and stdout hashes
are retained in the
[measurement manifest](diagnostics-performance/measurement-manifest.json).

## Invalid initial experiment retained for audit

The initial attempt reused one `CARGO_TARGET_DIR` across two worktrees. Cargo
rebuilt the example, changing its embedded source-SHA string and executable hash,
but reused the baseline `libmorphlet`. Both copied executables therefore omitted
`message_truncated`; their report sizes also lacked the expected extra field.
The library artifact timestamp remained at the before build while the example
fingerprint advanced. This made the initial supposed after timings invalid.

Those files are preserved unchanged under
[`invalid-shared-target`](diagnostics-performance/invalid-shared-target/INVALIDATION.json)
and **are excluded from every corrected result above**. In particular, the
initial 155.541-second purported after full-matrix process used the stale before
library and is not an optimized performance result. The corrected experiment
required an isolated target and actual linked-library behavior checks before any
new measurement was accepted.

## Practical limits and follow-up

These tests establish behavior and measured costs for the listed fixed shapes.
They do not establish Grasshopper equivalence, Windows performance, GUI frame
rate, memory bounds for arbitrary error text or prompt cancellation during
diagnosis. The full matrix includes success chains, one failing-root chain,
balanced root joins and incremental root joins; it does not cover all possible
DAG layouts or arbitrary ID arrangements.

For further optimization, profile distinct-root propagation and exact affected
count construction on the incremental family, and retain the independent oracle
when changing its representation. Add adversarial interleaved-ID graph coverage
before generalizing the ordered-preview shortcut. Repeat the measured protocol
against any later optimization with independent artifact directories and linked
behavior checks.
