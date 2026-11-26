# NODES autonomous integration status

- Updated: 2026-11-26 21:41 JST / 12:41 UTC
- Branch: `develop/nodes-ultimate`
- Base: `evolve/all` at `df77c542473a5c637f3353dec48e367dd67db027`
- Main inspected at `79f32745923adfa9e3de05ef4683b6e3b5f4948f`; never pushed directly.
- Integration draft PR: https://github.com/07egta/NODES/pull/11
- Stage: integrated development preview. Not a complete Grasshopper implementation.

## Completed

- Inspected all remote branches and existing PRs; retained all original mission histories.
- Integrated core, Data Tree, mathematics, generation, mesh, UI, GHX/API, exact curves, exact surfaces, and coverage research.
- Added legacy `ClosestPoint` input migration, including nested clusters and conflict rejection.
- Unified input access metadata and ordinal branch matching for new nodes while preserving original graph behavior.
- Preserved exact curves through affine transforms, arrays, ruled/extruded surfaces and iso curves.
- Fixed B-spline span tessellation, variance cancellation and product underflow/overflow intermediates.
- Added analytical surface Gaussian, mean and principal curvatures.
- Detected and repaired Delaunay hull gaps with independent area/convex-hull regression checks.
- Exposed safe new kernel families through the offline Graph API and excluded scripts/file input.
- Evaluated all 36 existing sample graphs, serialized/reloaded them, and checked warm-cache reuse.
- Published checkpoints `219d92d`, `86a2276`, `2e6534f`; subsequent validated changes are being saved now.

## Current work

1. Publish the integrated 350-test checkpoint.
2. Incorporate reviewed curvature edge-case fix, full GH2 official index inventory, node timing and capacity work.
3. Measure graph performance and add inspector timing, then run formatting, lint and release-build gates.
4. Finish CI, compiled node inventory, architecture, roadmap and test evidence documents.

## Counts and latest verification

- Registered node count: compiled inventory regeneration pending (298 picker kinds expected, plus Cluster); do not infer Grasshopper compatibility from this count.
- Grasshopper VERIFIED node count: 0 backed by external Grasshopper oracle fixtures; no parity claim.
- Local command: `cargo test --all-targets --locked` using Rust/Cargo 1.100.
- Latest combined result: **350 passed, 0 failed, 2 ignored**, 24 test suites, exit 0. Ignored tests are opt-in benchmarks/stress checks.
- Tests include the common access contracts, numeric precision, exact geometry, generation, saved examples and API catalog. Later changes require their own validation.
- Windows CI and Intel Arc runtime: not verified for this checkpoint.
- GUI attempt: native desktop is available, but its terminal window did not accept focused text input. The build environment has no DISPLAY. No application GUI test is claimed.

## Build and test

```sh
cargo test --all-targets --locked
cargo check --all-targets --locked
cargo fmt --all -- --check
cargo clippy --all-targets --locked
cargo build --release --locked --bin Morphlet
```

In the current sandbox Cargo is at `/home/agent/.cargo/bin`. Only the integration checkout may use `CARGO_TARGET_DIR=/workspace/scratch/a294e49d5e0f/nodes-trees/target`; concurrent worktrees with the same package/version previously reused incompatible artifacts. Other worktrees must use isolated targets.

## Known limitations and unverified work

- One output per node; no general mixed Item/List/Tree scheduler and no native runtime Null value.
- MAX_NODES is currently 2048. A separately reviewed 16384-capacity change still needs integration and measurement.
- General B-rep booleans, trimmed surfaces, STEP/IGES/3DM, binary GH, complete GHX, plugin ABI and complete GH1/GH2 parity are unfinished.
- Scalar settings in list adapters intentionally use their declared FirstItem/SingleItem access. Port metadata records this limitation.
- GUI, DPI, Japanese font rendering, Windows deployment, idle CPU/RAM, GPU performance and real printing remain unverified.
- Existing Rust 1.100 warnings and formatting differences remain to be addressed; passing tests are not a clean-clippy claim.

## Exact next resume steps

1. `git fetch origin` and inspect `develop/nodes-ultimate`, PR #11, this file and `git status --short`; preserve other work.
2. Reviewed local follow-ups: curvature `997183b`, GH2 inventory `6f77c9f`, timings `299f510`, capacity `95a070e`. Check history before cherry-picking to avoid duplicates.
3. Next files: `src/app.rs`, `src/engine.rs`, `tests/large_graph_tests.rs`, `examples/coverage_perf.rs`, `.github/workflows/`, coverage and development documents.
4. Next commands: `cargo test --test node_timing_tests --test large_graph_tests -- --nocapture`, followed by `cargo run --release --example coverage_perf` after saving source.
5. Next features: per-node elapsed/cached UI, measured 10000-node evaluation, full official index tracking and robust validation gates.

## Publication mechanism

Git CLI push lacks write credentials. Authenticated GitHub tree/commit/ref tools publish to this branch with expected-old-SHA and `force=false`. Fetch the resulting remote commit and verify its SHA after every checkpoint. No external developer branch is rewritten. Source mission commits are retained as merge parents. Local integration commits may be consolidated into an equivalent remote snapshot by this API path.
