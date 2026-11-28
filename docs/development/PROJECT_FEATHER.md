# PROJECT FEATHER — memory efficiency audit

Baseline: `8cac838229071509156d560391c3d1ba5237d761` (main, 2026-11-28).

## Initial changes

- Ordinary evaluation borrows connected Data Trees with `Cow` instead of cloning all branch maps, paths, and values before reading them. Defaults and transformed ports are owned. Whole-tree adapters that still require owned `Tree` inputs retain their prior behavior; output and reroute nodes still return owned trees.
- Ordinal matching no longer allocates the unused exact-path union.
- The Data Tree builder borrows path slices and allocates a path only when inserting a new branch.
- Unflatten streams source slices into guide branches, eliminating an intermediate flattened tree. Reverse clones the validated tree once and reverses each list in place.
- Existing topology limits, data order, empty branches, matching modes, geometry sharing, and error handling are retained. No dependencies or product features are removed.

## Existing memory controls found on main

- Geometry variants are already shared through `Arc` (model.rs).
- Evaluator cache and aggregate output weights are independently bounded. The current cache eviction policy is oldest revision, not true LRU; logical weight is not a measured byte budget.
- Undo/redo retains full project snapshots with a 64-entry limit. Snapshot deltas and byte budgets need a separate design preserving undo depth, exact signed-zero bits and serialized compatibility.
- Release builds already enable fat LTO, stripped symbols, and one codegen unit. Do not change optimization settings without comparing runtime and distribution size.

## Verification and measurement

Regression tests cover borrowed versus owned port inputs and streaming unflatten across empty and unequal branch boundaries. Run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --test data_tree_tests
cargo test --locked connected_inputs_borrow_until_a_transform_is_needed
cargo test --all-targets --locked
```

For a reproducible workload, use `cargo build --release --locked --example feather_memory_benchmark`, then run the built executable from both the baseline checkout (copy the example only) and the FEATHER checkout. Use the same compiler, target, allocator and hardware. On Linux, wrap the executable in `/usr/bin/time -v` to capture peak RSS; on Windows, use a process working-set profiler. Record raw output and at least five repetitions. Compare peak RSS, elapsed time, and checksum. The example exercises 16,384 items per unflatten with differently partitioned branches.

No measured timing, RSS, binary size or GPU-memory result is claimed in this initial audit: this editing environment has no Rust toolchain and cannot clone the public repository. CI and physical Windows GUI measurements are outstanding.

## Remaining work

Measure evaluator cache/output structural duplication and total retained bytes; design a compatible shared tree representation and true LRU independently of semantic revisions. Profile long-lived worker/UI snapshots, undo history, CPU/GPU preview buffers, and idle working set. Compare release executable and archive sizes, audit duplicate dependencies and feature flags without dropping functionality. Record repeatable idle, large-model, and long-session traces, with correctness and cancellation regressions as gates.
