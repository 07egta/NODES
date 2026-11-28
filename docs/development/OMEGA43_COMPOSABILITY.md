# OMEGA 43 — Cross-node composability

Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (main). Dedicated branch; final merge belongs to Mission 21.

## Scope

Inspect actual graph and curve metadata, not just advertised nodes. Existing `tests/integrated_workflows.rs` and `tests/curve_surface_integration.rs` cover aliases, save/reload, cache, exact affine and curve/surface contracts. The existing SolidCraft integration is PR #48; do not duplicate its kernel. Related open PR search for curve array and composability returned no matches; integration and generative PRs still require final merge review.

Static finding: `CurveArray` currently uses display polylines even when the input retains exact `CurveGeometry`. `CurveAttractor` similarly measures distance to preview chords. This makes numeric geometry and generated patterns depend on display resolution. Use existing arc-length/closest-parameter kernels when exact geometry exists; keep the legacy polyline fallback. No new dependencies, serialized IDs or shared graph changes.

Validation environment: GitHub connector reads/writes available. Local GitHub DNS/checkout unavailable and Cargo not found in this session. New Rust regressions must be run by CI; do not claim passing tests until verified. No subagent execution available in this chat.
