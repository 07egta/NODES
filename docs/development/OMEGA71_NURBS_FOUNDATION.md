# OMEGA 71 — NURBS foundation

Base at inspection: `09a2a546d442fea822d80fb4c756a81f067a8b00` (main).

## Scope and coordination

- PR #139 (`feat/nurbs-3`) is an active draft for blend/match, trim, continuity and curvature display. Do not edit its files or replace its implementation.
- Reuse the vendored MIT OR Apache-2.0 SolidCraft `solidcraft-geom::bspline` implementation (upstream revision and notices: ../SOLIDCRAFT_CHANGES.md). No new dependency or copied proprietary code.
- Extend existing mathematical curve/surface types in place, not the graph schema. Prioritize shape-preserving knot refinement, robust parameter-domain checks and independent regression tests.
- Native compilation and benchmarking are unavailable in this session; CI must verify the branch before merge. No performance claims until measured.
