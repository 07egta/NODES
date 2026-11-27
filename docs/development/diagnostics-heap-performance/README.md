# Paired heap preview experiment artifacts

This directory preserves the exact implementation snapshots, harness, build
provenance and raw output used by
[the results report](../DIAGNOSTICS_HEAP_BENCHMARK_RESULTS.md).
It is an opt-in experiment; Cargo does not build these documentation snapshots as
part of the normal application.

## Source contract

- `diagnostics_before.rs` is byte-exact `src/diagnostics.rs` from
  `7c1a0956adb8edcc29945717a6eb32e0d6335be3`.
- `diagnostics_after.rs` is byte-exact `src/diagnostics.rs` from
  local `9a2cccc6482f7a2b7da872b6e3b1dc32ef6ae856`, published with the identical
  Git tree at `545bcd4208cc6bf7069a75c7e4aadd6d2e915910`. Use that published
  revision for the `git show` reproduction below.
- `library-module-shim.rs` is appended once to a disposable 7c worktree's
  `src/lib.rs`. It includes both source files as separate library modules.
- `diagnostics_heap_perf.rs` is copied into that worktree's `examples` directory.
- `source-capture.json` and `build-manifest.json` record cryptographic hashes.
  Preserve the snapshots without formatting or changing private APIs.

Internal library modules are necessary because diagnostics calls the
crate-private `Graph::topological_nodes`. An external example that aliases
`morphlet::model` would not gain that private access. The shim preserves the
production call path and changes no diagnostic algorithm.

## Disposable setup

Choose separate locations for the worktree, owned build target and new output.
Do not overwrite this evidence directory or use an application build target
owned by another developer.

1. Create a detached worktree at 7c using `git worktree add --detach`.
2. Create `benchmark-snapshots/` in that worktree. Copy the two diagnostic snapshot
   files there, keeping their bytes and names unchanged.
3. Append `library-module-shim.rs` once to `src/lib.rs`.
4. Copy the paired harness to `examples/diagnostics_heap_perf.rs`.
5. Compare snapshot SHA-256 hashes with both the manifest and
   `git show COMMIT:src/diagnostics.rs`. Verify the harness and modified `lib.rs`
   hashes against the build manifest for an exact reproduction.

The only worktree modifications for this experiment are the two module
declarations, their source snapshots and the benchmark example. The shared engine,
model, graph validation, cluster code, `Cargo.toml` and `Cargo.lock` remain at 7c.

## Build and linked-code checks

The actual commands and environment are in `build-manifest.json`. Use
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, and a dedicated `CARGO_TARGET_DIR`.
Set these compile-time metadata variables to the manifest values:

- `NODES_HEAP_LIBRARY_SHA`
- `NODES_HEAP_BEFORE_SHA`
- `NODES_HEAP_AFTER_SHA`
- `NODES_HEAP_BEFORE_SHA256`
- `NODES_HEAP_AFTER_SHA256`

If reusing third-party dependency artifacts, explicitly clean only the project
package in the exclusively owned target before building:

```sh
cargo clean -p morphlet --release --target-dir "$diagnostics_target"
cargo build --release --locked --offline --example diagnostics_heap_perf
```

Here `diagnostics_target` is the same path exported as `CARGO_TARGET_DIR`.
An entirely fresh dedicated target is also valid. Do not merely switch worktrees
and rely on an example's rebuilt metadata as proof of a rebuilt library.

Copy the resulting executable to a unique preserved path and hash it. Record the
fresh `libmorphlet` artifact hash too. The first probe should run a small case:

```sh
diagnostics-heap-paired --case interleaved-32 --repetitions 3
```

The harness asserts that both linked diagnosis function addresses are distinct.
Its provenance includes the byte length and FNV-1a fingerprint of each embedded
source snapshot, both module type names, and source SHA-256 metadata. Check those
embedded bytes against the captured files as well as checking the fresh library
build. Both first reports must pass independent analytic oracles, and every
report must equal the full canonical serialized report. These are complementary
checks; runtime revision strings alone are insufficient provenance.

## Measurement

The fixed cases are `success-10000`, `prefix-512`, `interleaved-32`,
`prefix-2048`, `interleaved-512`, and `interleaved-4096`. The latter contains
8,973 nodes. With no `--case`, all six run in one process, but the archived
comparison uses a fresh process per case.

Pause other owned CPU-heavy builds/tests during measurements. For each case:

```sh
diagnostics-heap-paired --case prefix-2048 --repetitions 5 --first before
diagnostics-heap-paired --case prefix-2048 --repetitions 5 --first after
```

The captured experiment ran every case once in the listed order with `before`
first, then every case in reverse order with `after` first. Each process
alternates call order each round. Pool the ten repeated samples per
implementation/case and calculate their median, excluding the first calls.
Save every raw JSON, stderr file, exit code, process wall/CPU time and `wait4`
peak RSS. Reject failed invariants or source mismatches rather than summarizing
their timings.

JSON encoding, parsed oracle checks, report comparison and destruction happen
outside diagnosis timers. Complete-process RSS includes both implementations
and those checks. Do not compare that RSS as if it measured the two
implementations separately.

## Evidence files

- `*-paired-round-1.json` and `*-paired-round-2.json`: the 12 actual measured
  process outputs, with per-call order and raw elapsed samples.
- Matching `.stderr.log` files: captured process stderr; empty files mean no
  stderr was emitted.
- `measurement-manifest.json`: process exit codes, timestamps, CPU/RSS, host
  observations, executable checksum, source and oracle verification results.
- `comparison-summary.json`: pooled samples, medians, ratios, control costs,
  serialization sizes/times and peak complete-process RSS.
- `smoke-*.json`: earlier small correctness runs, excluded from timing claims.
- `package-clean.log` and `paired-build.log`: actual successful clean/build logs.
- The library artifact path in the manifest is historical: later owned benchmark
  rebuilds can replace that file. The preserved executable and source snapshots
  retain the evidence for this experiment.
- `SHA256SUMS`: checksums for the files in this artifact directory, excluding
  the checksum file itself.

The original earlier benchmark remains in `../diagnostics-performance/`.
This source pair and experiment do not revise or overwrite that evidence.
