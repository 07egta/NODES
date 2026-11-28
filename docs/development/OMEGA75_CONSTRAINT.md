# OMEGA 75 — CONSTRAINT

Base: `09a2a546d442fea822d80fb4c756a81f067a8b00` (main, 2026-11-28 JST).
Scope: NODES sketch node and public session API; no changes to graph IDs, persistence, dependencies, or SolidCraft internals. Open PRs inspected; #139 is surface/NURBS, #137 standard components. Coordinate if either touches sketch_nodes.rs.

The existing MIT OR Apache-2.0 SolidCraft sketch kernel already provides connected-component LM, local finite-difference Jacobians, constructive seeding, rank analysis, redundancy analysis (bounded), and continuation/feasible drag. Reuse rather than introduce a second solver. See docs/SOLIDCRAFT_CHANGES.md for provenance.

Technical references (public mathematical documentation, no external code copied):
- Fudos and Hoffmann, 1997, A Graph-Constructive Approach to Solving Systems of Geometric Constraints, DOI: 10.1145/248210.248223.
- Ceres Solver, Solving Non-linear Least Squares: https://ceres-solver.readthedocs.io/latest/nnls_solving.html (LM and trust region tradeoffs).

Plan: expose transactional edit/drag and diagnostics; expose additional validated text commands; add independent regression and timing fixtures. Keep the existing model and full analysis. Sparse QR/SVD and globally certified solution/branch enumeration are future work; do not claim unsupported functionality.

Local Rust toolchain and direct GitHub network access are unavailable in this session. Validation will depend on committed-source CI; do not merge until checked.

## Implementation checkpoint

- `nodes::sketch_nodes::session::SketchSession`: parse, solve, atomic dimension batches, bounded expression evaluation, feasible point dragging, source-line diagnostics and read-only access to the solved sketch. A failed edit retains both previous geometry and previous constraints. IDs are stable only inside one session. Dimensions in the API use millimetres/radians; text angles use degrees.
- Text commands (existing commands are unchanged):
  - `ellipse e center_point major_point minor_radius`
  - `bezier b p0 p1 p2 p3`, `bspline s 2 p0 p1 p2 p3` (clamped control splines)
  - `construction curve`, `block curve`
  - `horizontal_points p q`, `vertical_points p q`, `collinear a b`, `symmetric p q axis`
  - `point_line_distance p l value`, `point_circle_distance p c value inside|outside`
  - `arc_length c value`, `arc_sweep c degrees`, `line_angle l degrees`.
- Existing `smooth` is G2 at a shared endpoint, not an unrestricted equal-curvature relation. Construction geometry is excluded from individual-curve output; block fixes its defining points and radius using existing kernel behavior.
- Node evaluation checks cancellation immediately before parsing and immediately after solving, and during output. It does **not** interrupt a running kernel iteration.
- Failed solve diagnostics identify violated constraints and source lines, not a guaranteed minimal unsatisfiable subset. Redundancy analysis is limited by the existing kernel; local rank is not a global rigidity proof.
- The public API is usable by OMEGA 76 without modifying persistence, but a GUI drag editor and persisted expression bindings are not included. No arbitrary local sketch plane, elliptical-arc entity, true external-reference associativity, full general point-to-curve distance, sparse QR/SVD, or general certified branch enumeration added. Existing exact output is used where available; existing profile/curve fallback approximation remains.

## Validation

New integration tests in `tests/omega75_constraint_session.rs`: analytical rectangle, continuation, atomic rejection, bounded expression evaluation, feasible drag, source mapping, geometry commands, cancellation. Timing fixture in `examples/constraint_benchmark.rs`: 10, 100, 1000 independent radial constraints, verifies analytical distances and rank-based DOF. Run:
```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --test omega75_constraint_session
cargo test --all-targets --locked
cargo run --release --locked --example constraint_benchmark
```
No local Rust toolchain; these commands have **not** been run here. Timing measurements, connected 1000-element workloads and GUI frame times remain unverified. Do not merge if CI fails.
