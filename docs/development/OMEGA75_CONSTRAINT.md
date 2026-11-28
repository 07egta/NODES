# OMEGA 75 — CONSTRAINT

Base: `09a2a546d442fea822d80fb4c756a81f067a8b00` (main, 2026-11-28 JST).
Scope: NODES sketch node and public session API; no changes to graph IDs, persistence, dependencies, or SolidCraft internals. Open PRs inspected; #139 is surface/NURBS, #137 standard components. Coordinate if either touches sketch_nodes.rs.

The existing MIT OR Apache-2.0 SolidCraft sketch kernel already provides connected-component LM, local finite-difference Jacobians, constructive seeding, rank analysis, redundancy analysis (bounded), and continuation/feasible drag. Reuse rather than introduce a second solver. See docs/SOLIDCRAFT_CHANGES.md for provenance.

Technical references (public mathematical documentation, no external code copied):
- Fudos and Hoffmann, 1997, A Graph-Constructive Approach to Solving Systems of Geometric Constraints, DOI: 10.1145/248210.248223.
- Ceres Solver, Solving Non-linear Least Squares: https://ceres-solver.readthedocs.io/latest/nnls_solving.html (LM and trust region tradeoffs).

Plan: expose transactional edit/drag and diagnostics; expose additional validated text commands; add independent regression and timing fixtures. Keep the existing model and full analysis. Sparse QR/SVD and globally certified solution/branch enumeration are future work; do not claim unsupported functionality.

Local Rust toolchain and direct GitHub network access are unavailable in this session. Validation will depend on committed-source CI; do not merge until checked.
