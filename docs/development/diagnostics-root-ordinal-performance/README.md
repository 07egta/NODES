# Compact failing-root ordinal experiment artifacts

This directory preserves the unchanged source snapshots, Rust harness and raw
evidence for [the results report](../DIAGNOSTICS_ROOT_ORDINAL_RESULTS.md).
These documentation sources are opt-in experiment artifacts and are not added
to the normal application build or runtime.

## Exact source pair

- Before snapshot: local `9a2cccc6482f7a2b7da872b6e3b1dc32ef6ae856`, diagnostic
  bytes equal to published `545bcd4208cc6bf7069a75c7e4aadd6d2e915910`.
- After snapshot: local `24e92ebd46bfd59cc5ba532c5c9a79df86e16379`, source/tree
  equal to published `46417ae91af8cbe11eb45934d535278ce23df06b`.
- Shared library baseline: `7c1a0956adb8edcc29945717a6eb32e0d6335be3`.

`source-capture.json` records local/published equivalence. `build-manifest.json`
contains the byte-exact snapshot, harness, shim, modified library file, compiled
library and executable hashes. Keep the original snapshot bytes unchanged;
formatting or adapting their algorithms would create a different experiment.

## Disposable worktree and library module setup

1. Create a new detached worktree at the shared 7c library baseline.
2. Create its `benchmark-snapshots/` directory and copy `diagnostics_before.rs`
   and `diagnostics_after.rs` there without changing bytes.
3. Append `library-module-shim.rs` once to that worktree's `src/lib.rs`.
4. Copy `diagnostics_root_ordinal.rs` into its `examples/` directory.
5. Verify each snapshot against the archived SHA-256 and `git show` source.

The snapshots must be internal library modules because their unchanged
diagnostics calls use the crate-private `Graph::topological_nodes`. The two
modules then share the same original Graph/Evaluation types and validation code.
An external example alias would not provide that private access.

Set a dedicated `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`,
and these compile-time metadata variables from the build manifest:

- `NODES_ORDINAL_LIBRARY_SHA`
- `NODES_ORDINAL_BEFORE_SHA`
- `NODES_ORDINAL_AFTER_SHA`
- `NODES_ORDINAL_BEFORE_SHA256`
- `NODES_ORDINAL_AFTER_SHA256`

An entirely fresh target is valid. If reusing already built third-party
dependencies in an exclusively owned target, force a new project library:

```sh
cargo clean -p morphlet --release --target-dir "$diagnostics_target"
cargo build --release --locked --offline --example diagnostics_root_ordinal
```

`diagnostics_target` must be the same path exported as `CARGO_TARGET_DIR`.
Preserve and hash the resulting executable under a unique name, and hash the
fresh `libmorphlet` artifact. Do not infer linked-library provenance solely from
changed example metadata or a changed binary checksum.

The harness asserts different linked diagnosis function addresses, uses the two
explicit module types/functions and embeds both source files. Check the emitted
byte lengths and FNV-1a fingerprints against captured source bytes. External
SHA-256 checks plus the forced fresh library build are also recorded.

## Correctness mode: separate and excluded

Run each fixture in its own correctness process before collecting memory data:

```sh
diagnostics-root-ordinal --mode verify --case high-id-two-roots
diagnostics-root-ordinal --mode verify --case success-10000
```

These processes evaluate once, run both implementations, check typed analytic
oracles and serialize both complete reports for byte equality. Save their JSON,
stderr and exit codes. Their allocator/RSS observations and runtime are excluded
from all before/after comparisons.

## Measurement mode: one variant per fresh process

```sh
diagnostics-root-ordinal --mode measure --case high-id-two-roots --variant before
diagnostics-root-ordinal --mode measure --case high-id-two-roots --variant after
```

Each invocation constructs a real graph, evaluates it once, diagnoses once with
the selected variant, checks typed oracles, emits compact metadata and exits.
This mode never serializes the full diagnostic report. The other variant is
linked into the binary but is not invoked. Never reuse a process or replace this
with a before-then-after loop when attributing peak RSS.

The archived experiment uses five four-process blocks per case:
ABBA, BAAB, ABBA, BAAB, ABBA (A=before, B=after), reversing case order on even
blocks. This supplies ten observations per variant/case. Record every child's
exit code, first-diagnosis elapsed time, total wall/CPU time and `wait4` maximum
RSS. Use a quiet window for other owned build/test workloads. Keep all samples,
including timing outliers, and record the host and compiler.

First-diagnosis time excludes graph construction, real evaluation, oracle checks
and report destruction. The process and allocator are fresh per invocation,
but the system caches are not reset and graph evaluation occurs before diagnosis.
These are not repeated warm-diagnosis samples.

## Fixtures and memory interpretation

The high-ID fixture has 16,384 nodes. Two Divide-by-zero roots have IDs 16,382
and 16,383; every lower ID is an independent Add consuming both. Both roots
affect exactly 16,382 joins and retain smallest affected IDs 0–255. Each join has
exactly those two upstream root IDs. Storage is rotated/reversed.

Each join's former root bitset needs 256 u64 words; dense failing-root ordinals
reduce it to one word. The analytic word payload is 33,550,336 versus 131,056
bytes. This excludes object headers and every other allocation; it is not RSS.

The successful control is Number(2.5) followed by 9,999 Add(1) nodes, with final
value 10,001.5. The after source includes a no-failure early return after
validation; this control covers that additional change, not just bitset width.

`wait4` peak RSS is the whole fresh process, including graph/evaluation and one
diagnostic implementation. It is not the diagnostic live working set. The
separate-process protocol prevents cross-variant allocator retention, but system
load, allocator policy and page accounting still limit generalization.

## Evidence

- `verification-manifest.json` and `verify-*.json`: two excluded correctness
  processes and complete-report equality results.
- `*-block-*-position-*-before.json` / `*-after.json`: all 40 measured process
  outputs. Matching stderr logs are empty when no stderr was emitted.
- `measurement-manifest.json`: full execution order, commands, exit codes,
  timestamps, source/oracle checks, wall/CPU time and per-process maximum RSS.
- `comparison-summary.json`: every latency/RSS sample, medians, ranges and ratios.
- `build-manifest.json`, source files, shim and build logs: reproducible source
  and binary provenance.
- `validation.json`: final independent recheck of counts, source bytes, sample
  pooling and artifact links.
- `SHA256SUMS`: checksums for artifact files, excluding this checksum file itself.

Reuse the protocol with a new output directory for later source pairs. Preserve
this captured experiment and distinguish its first-call measurements from the
earlier repeated-call heap preview benchmark.
