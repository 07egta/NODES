# OMEGA 65 — EQUIVALENCE / differential geometry lab

Started 2026-11-28 JST, based on main `09a2a546d442fea822d80fb4c756a81f067a8b00`.

Scope: isolated Rust developer example and independent oracles. No production
node IDs, data model, dependencies, CI workflow, or compatibility catalog changes.
Coordinate with #119 (independent test oracles), #137 (component additions),
#139 (NURBS), and #87 (data-tree performance). This is not a replacement for their tests.

The existing catalog's VERIFIED state requires actual, versioned Grasshopper
component evidence. No licensed Rhino/GH runtime has been located in this
session; no external runtime results have been captured. The existing catalog
must not be upgraded based on analytic or documentation checks alone.

Planned initial scope: numeric range and matching rules from the official
public guide, fixed-seed rational Bernstein vs NURBS evaluation, matrix
inverse, vector and plane projection, explicit tree paths and empty/invalid
inputs, and sampled point-set distance. A separate optional capture file
will compare a supplied external reference without silently promoting
catalog status. All changes are additive to avoid conflicts with the merge train.
