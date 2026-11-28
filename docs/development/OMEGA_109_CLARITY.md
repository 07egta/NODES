# OMEGA 109 — CLARITY: input inspection checkpoint

Base: main `8cac838229071509156d560391c3d1ba5237d761`.
Branch: `feat/omega-109-clarity-input-inspection`.

## Existing foundation

The existing `diagnostics.rs`, `diagnostics_ui.rs`, `data_view.rs` and
`Evaluation` already provide dependency-inferred roots, blocked descendants,
measured node timings, cache/cancellation state, pagination and a data-tree
viewer. Reuse these rather than introduce a competing evaluator or error report.

## Scope of this change

- Inspect a single failed enabled node on demand from the matching evaluation.
- Identify missing required inputs, optional inputs, local values, connected
  sources, upstream failure, missing results and cluster-internal defaults.
- Provide evidence of ordinary input type, numeric range and integer mismatches,
  with raw branch path and item index. Bound the preview work to 128 branch/item
  visits per port; do not claim validity or definitive causation.
- Reuse the paginated data viewer for borrowed upstream trees. No geometry
  cloning, graph mutation, new dependencies, evaluation, transform application
  or persistence/CLI schema change. Mark raw data before transforms/matching.
- Provide Japanese guidance and navigation; suppress inspection/navigation on
  stale snapshots and clear the inspection target on revision changes.

## Verification

Added six focused Rust regression tests using real evaluator graphs and explicit
synthetic snapshots for preview bounds and incomplete/empty results. Updated
the headless diagnostics UI test call for the new evaluation parameter.

The current shell has no Cargo/rustfmt and cannot resolve GitHub for a local
clone. No compile, fmt, Clippy, test or native GUI run is claimed for this branch.
Use CI and review before merging; run `cargo fmt --all -- --check`,
`cargo clippy --all-targets --locked -- -D warnings`, and
`cargo test --all-targets --locked`. Verify the native input-inspection flow,
including an edit or Undo that reuses an ID, with the previous result visible.

## Remaining CLARITY scope

Typed evaluator/kernel error provenance, exact consumed values after access
transforms, full trace/step evaluation, bounded history, trustworthy memory and
CPU/GPU attribution, visual overlays and transactional repair preview/Undo
integration are not implemented by this change. Empty trees are not classified
as errors, and no generic repair is silently applied. Later work must coordinate
with the evaluator, sketch/Boolean/GPU and canvas workstreams.
