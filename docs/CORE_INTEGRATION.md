# Core integration: node authoring and verification

Base: `evolve/all` at `df77c542473a5c637f3353dec48e367dd67db027`. This change deliberately leaves the on-disk schema (v1), node IDs, port keys, dependencies and limits unchanged.

## Node authoring contract

- Add a stable snake_case serialized NodeKind ID in `src/model.rs`; do not rename an existing ID or port key. Add it to `NodeKind::all()` (Cluster is intentionally not in the picker).
- Define metadata in the owning category module: Japanese display label, category, description, Japanese/English search keywords, typed inputs, defaults/ranges/steps, and output type. Reuse InputSpec constructors.
- For a normal item-wise node, dispatch its kernel through the existing evaluator, with cancellation and checked size. Whole-list or whole-tree operations belong in their respective modules, not a second independent tree implementation.
- Use `Value` and `Tree` from model. Do not introduce a new implicit coercion or silently discard a branch. Handle empty input, type mismatch, non-finite values, excessive allocations and cancellation.
- Provide deterministic unit tests and an integration test exercising Graph -> Evaluator. Include save/load compatibility for a new serialized ID and a regression for its failure cases.
- Keep UI display independent from evaluation. Presentation-only edits should not invalidate computation. Include any non-port semantic state in the cache key, including external resource metadata.
- Do not add dependencies without updating and reviewing the lockfile, license notices and the Windows dependency audit. MIT/Apache-2.0 project policy remains unchanged.

## Integration boundaries

Graph and node metadata: model; evaluation and cache: engine; category nodes: gh_nodes (and future separate category modules); cluster definitions and execution: cluster; sandbox scripts: script; file I/O: persistence/import/stl_export/gcode; UI: app/canvas/viewport. Preserve current entry points so parallel agents can add category modules without changing the schema.

The validated topological traversal now supplies references to nodes directly, rather than repeatedly looking up IDs with a linear scan. The public ID order and its validation rules remain intact. Nested cluster execution shares the traversal and honors disabled nodes. A nested import's cache key includes external file metadata, like a top-level import; this is evaluated when a new evaluation is requested, not a new file watcher. Metadata checks cannot detect an edit that preserves both size and modification time.

## Reproducible checks

```powershell
cargo test --all-targets --locked
cargo build --release --locked --bin Morphlet
cargo test --release --locked --test core_integration benchmark_sparse_slider_edit -- --ignored --nocapture
```

Record CPU, RAM, OS, Rust version, commit SHA and power mode. Compare five release-mode samples per graph size on the same machine. The benchmark excludes graph construction, runs cold/warm/sparse-edit evaluations, and prints computed/cached counts and elapsed milliseconds. Existing limit is 2,048 nodes; 10,000 is explicitly tested as a rejection, not a performance claim. Start-up, UI frame time, idle CPU, memory, GPU and real geometry workloads need separate Windows measurements.

## Follow-up work (not implemented here)

Multi-output port IDs and migration (retain v1 reader); opaque unknown node payloads with explicit disabled behavior; bounded latest-only worker queue and cooperative parallelism; versioned plugin API; category-specific registries; large-graph resource policy backed by measurements. These require coordinated schema/UI work and compatibility tests, not an unreviewed merge into the current graph model.

## Second hardening pass

Cluster instance keys now share per-root dependency token arrays. Each reachable definition is compared against its full serialized signature once per evaluation and receives a new revision only on change; tokens are not collision-prone hashes. This avoids storing the entire definition closure in every instance's cache entry. The evaluator still serializes active definitions once per evaluation and does not provide a file watcher. Definition validation memoizes depths and stops before descending past the configured maximum. Part import checks ID allocation before mutation; part export validates the generated payload and uses the same atomic, flushed writer as projects.

Regression tests cover shared nested definitions, graph replacement with reused IDs, pathological definition fanout/depth, ID exhaustion and preservation of an existing part on failed saves.

## Third hardening pass: bounded worker mailbox

The GUI and worker now exchange snapshots through a dependency-free latest-only mailbox rather than unbounded channels. Each direction retains at most one undelivered snapshot; superseded snapshots are released outside the lock. The worker skips stale requests before starting and still checks the revision during evaluation and before publishing; the UI retains its final revision check. Dropping the worker requests cooperative cancellation. This is a single-worker scheduling change, not parallel node execution.

Mailbox unit tests exercise 10,000 updates, release of superseded payloads, final delivery on producer close, monotonic concurrent delivery and consumer disconnect. UI responsiveness and shutdown behavior still require Windows execution.
