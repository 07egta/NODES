# PROJECT FLOW — incremental evaluation, phase 1

Base: `8cac838229071509156d560391c3d1ba5237d761` (main).
Scope: strengthen the existing evaluator; no new engine, dependencies or saved-file changes.
Related work: #112 (cancellation, merged), #125 (semantic cache, merged),
#132 (execution schedule proposal), #143 (cluster input fan-out), #141/#145 (sketch UI).

## Audit

- Existing engine: validated topological traversal, input source revision keys, bounded cache,
  parallel independent levels, deterministic commits, cooperative cancellation and
  latest-only worker mailbox. Random nodes with explicit seed are intended to be deterministic.
- An input/key miss always minted a new revision even if the resulting tree was unchanged;
  this unnecessarily invalidated downstream nodes.
- ExactSketch reads `Node::script` but its semantic text was missing from the cache key.
- Current file import identities use path, size and modification time, not a content snapshot.
  A same-size/same-timestamp replacement or a concurrent modification can escape detection.
- Existing graph schema has one output per node; it is not a multi-output port model.

## Changes

- Recompute a dirty node normally, but preserve its dependency revision if its entire
  successful tree is demonstrably identical: same branch paths and lengths, exact binary64
  number/point bits, identical Arc handles for immutable geometry. No approximate equality,
  hash collision risk or deep B-rep comparison. Fresh error results and distinct geometry
  handles conservatively receive new revisions.
- Store the updated key even when output revision is retained. Existing output/resource
  budgets, cache eviction and cancellation behavior remain in force. A recomputed-but-equal
  node still counts as computed and its timing is not reported as a cache hit.
- ExactSketch text now participates in the semantic key.
- Add cold/warm/cutoff/tail benchmark contracts for 100, 1,000 and 10,000 scalar nodes,
  differential comparisons with a cold evaluator, a signed-zero regression and sketch
  edit/error/recovery regression.

## Validation

Local environment has neither Cargo nor rustc; direct checkout also failed DNS resolution.
No compilation, performance timings or CI success are claimed. Run:

```sh
cargo fmt --all -- --check
cargo test --locked --test engine_perf_tests -- --nocapture
cargo test --locked --test semantic_cache_regressions
cargo test --locked --test large_graph_tests
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

## Remaining design gates

- Schedule caching: coordinate with #132; never skip validation of modified parameters,
  IDs, ports, cycles or node kinds. Do not introduce a second scheduler.
- Demand-driven evaluation: preserve full-evaluation/export semantics and define the
  resource budget contract for target-only execution before enabling it.
- Branch/item cache: first restrict to pure Item access; list/tree and ordinal/longest/
  cross matching can make unrelated-looking branches semantically dependent.
- Content-addressed cache and CSE: require explicit purity classification, exact collision
  verification, deterministic resource accounting, and bounded retention.
- External dependencies: prefer a bounded, cancellable snapshot read and content identity
  tied to the bytes actually parsed; do not claim metadata is content identity. Treat
  transient I/O/timeouts, wall-clock and future stateful nodes separately from pure values.
- Preview/final fidelity: separate precision keys and prevent approximate preview from
  being exported. Priority scheduling/worker pool changes need cancellation and resource
  tests. No changes to these features in phase 1.
