# OMEGA 74 — operator reliability, first increment

Baseline: `09a2a546d442fea822d80fb4c756a81f067a8b00`. Reuse the already-vendored SolidCraft 0.26.0 (MIT OR Apache-2.0) through the NODES bridge; do not implement another Boolean kernel or replace a native body by a triangle mesh. See SOLIDCRAFT_CHANGES.md. PR #139 adds adjacent NURBS operations; keep this change confined to existing operator paths, one read-only topology query, and an independent test file.

## Contracts

- Native closed shell topology and display mesh closure are separate checks. Solid Boolean, constant fillet/chamfer, variable fillet, shell and horizontal solid trim reject invalid input/output instead of returning an open or mesh-only result. This does **not** prove geometric self-intersection freedom; signed mesh volume is a sanity check, not a certified mass measurement.
- A faceted Boolean may repair the *preview* only after checking closure, positive signed volume, mass and bounds agreement. It never substitutes the native B-rep. If repair fails, return the error.
- A multi-part union returning None is an error, never a reason to silently discard a part.
- A closed-solid intersection uses the SolidCraft Boolean history and fitted curves. For supported analytic face pairs, sample both underlying surfaces at span subdivisions (16 times the degree), including degree-one spans, and require residual at most 0.0001 mm. This is sampled support-surface error, not a bound over the continuous curve, not trimmed-face classification, and not a tangency/coincidence detector. Unsupported freeform pairs or failed fits return errors; no silent closed-solid mesh fallback. Open sheets continue to use the explicitly approximate mesh route. Curve/B-rep points are also approximate.
- Pair visits and mesh grid allocation/search have explicit budgets and periodic cancellation checks. Native third-party calls still check cancellation only before/after the call; this is not a wall-clock deadline for the native kernel.

## Remaining scope

| Area | Scope |
| --- | --- |
| Curve-curve | Existing sampled seed/refinement, now bounded; no complete tangential/overlap classification |
| Curve-surface/ray-surface | Existing mesh approximation for curve/B-rep; exact solver and ray solver not added |
| Surface-surface | Analytic fitted solid meet curves with sampled residual; arbitrary NURBS SSI, tangent/coincident and trim completeness remain research |
| Boolean | Existing native union/difference/intersection, stricter acceptance; generalized classification/fragment algorithms not implemented here |
| Fillet/chamfer | Existing supported solid operations, stricter validation; multi-edge variable radius, corner blends, asymmetric/angle chamfer UI not added |
| Offset/shell | Existing offsets and supported shell paths; global self-intersection, variable thickness, certified offset approximation not added |
| Robustness | Work limits for bridge searches; no adaptive exact arithmetic or general BVH added |
| Verification | Box volume/closure, trim, malformed mesh, cancellation regressions; existing cylinder intersections and curved solid tests retained |

Run `cargo fmt --all -- --check`, `cargo test --test omega74_operator`, `cargo test --test nurbs_kernel`, and the full CI before merging. The authoring environment lacks Rust and direct Git access, so these are not yet reported as passing. Additional regression fixtures: cylinder/sphere, torus cut, tangential/coincident pairs, micro-holes, thin and singular shells, and self-intersecting offsets.
