# OMEGA 91 — SINGULARITY (increment 1: conservative exclusion)

Base: 09a2a546d442fea822d80fb4c756a81f067a8b00. OMEGA 74 PR #140 is a
separate operator-acceptance effort; the OMEGA 87 PR could not be located in
the repository search at the time of this work. No new dependency, no mesh
fallback, no change to the existing node or Boolean paths.

## Problem and API

For A(u,v) and B(s,t), find the zero set of F=A-B in the product of their
native parameter rectangles. This is usually a one-dimensional set in four
variables, and may have rank-deficient points, isolated points, or 2D overlap.
A failed Newton step is not an exclusion test.

The new public module is `nodes::surface_kernel::certified_intersection`.
`search(&a, &b, SearchOptions::default(), &|| false)` returns a bounded
search report. A and B must be existing validated NURBS surfaces with
strictly positive weights and clamped, nonperiodic knot vectors. All output
UV boxes use NATIVE knots. They are not normalized UV, nor UV curves.

### Proof obligation

1. On each closed native rectangle, the active control points enclose the
   rational surface by nonnegative basis functions and positive weights.
   At an interior knot, either one-sided span gives the same position by
   continuity (multiplicity <= degree).
2. Within a single nonempty span the homogeneous tensor-product de Boor
   recursion is a sequence of convex interpolations. Interval operations
   enclose their exact-real counterparts; directed outward rounding uses
   the existing robust_geometry::Interval plus positive division.
3. Intersecting two enclosures is safe. If refinement fails, retain the
   coarse control-point hull. Strict axis separation proves no common
   point; equality of interval endpoints is NOT separation.
4. The two child parameter boxes share the exact same represented split
   coordinate. They cover their parent, including its boundary. Thus every
   root is in an unresolved box or a pending box. Cancellation and work
   exhaustion move all pending boxes into the report.
5. An empty unresolved cover proves disjointness for the mathematical
   surfaces represented by binary64 inputs, assuming IEEE-754 round-to-nearest
   basic operations and no flush-to-zero/unsafe fast-math. This is not a
   certificate about uncertain measurements or CAD tolerance-based contact.

The search does NOT prove existence, uniqueness, branch connectivity, or
complete tracing when boxes remain. In particular a zero residual is
ordinary floating-point sample evidence, not a root existence proof.
The parameter_fraction is only a refinement stop and does not bound world
space error. A small loop is retained, not necessarily resolved. Positive
division falls back to the conservative hull if bounds fail. No arbitrary
precision or exact algebraic arithmetic is added in this increment.

The finite work budget (max 100,000 visited boxes) and depth (max 128)
bound memory. Traversal is depth-first; a partial result may be uneven.
The report distinguishes cancellation, work, depth, parameter and
floating-point limits. Visited = excluded + subdivided + processed
unresolved leaves (pending leaves are not visited).

## Regression and reproducible benchmark

```sh
cargo test --lib surface_kernel::certified_intersection
cargo test --release --lib surface_kernel::certified_intersection::tests::stress_benchmark -- --ignored --nocapture
```

The regression cases use known dyadic roots for a tangency, tiny closed loop,
crossing branches, and boundary roots. Tests also check a 1e-12 separation,
coincidence under work exhaustion, cancellation, rational weights,
repeated knots, and tighter single-span bounds. Sampling a floating-point
evaluator is only a sanity check, not an independent proof. The ignored
benchmark reports timings, work counts, unresolved counts and a sampled
residual; it does not claim a complete intersection success rate.

Execution status at authoring: Rust/Cargo unavailable in the chat runtime,
and direct GitHub clone unavailable (DNS). The Rust tests, fmt, clippy,
benchmark and CI results have not been run/verified. Do not merge before
running them; review the enclosure proof and adversarial tests independently.

## Next research steps

- Bernstein / Bezier extraction with outward-rounded coefficients and
  interval difference/Jacobian bounds, to improve pruning.
- For a regular root curve (Jacobian rank 3), parameterize one coordinate
  or impose a transverse slice; use a square interval Newton/Krawczyk
  problem to certify existence and uniqueness on slices. Applying a square
  root solver directly to the underdetermined 3x4 system is not valid.
- Certified continuation between slices (including monotonicity and tube
  coverage), branch graph construction, and full-loop coverage; SVD and
  pseudo-arclength prediction remain numerical aids, not certificates.
- Rank-deficient cases need singularity isolation and local topology proof;
  positive-dimensional overlap must be classified separately. Never infer
  overlap merely from a cluster of near-zero residual samples.
- Native periodic/seam and trim domain handling; analytic sphere and torus
  support (or validated rational conversion); B-rep integration; UV curves,
  world-space error bounds, endpoint/branch identifiers, and full adaptive
  precision remain future work. Do not discard unresolved regions to get
  visually clean output.

## Background references

- Hu, Maekawa, Patrikalakis, Ye (1997), Robust interval algorithm for surface
  intersections, Computer-Aided Design 29(9), 617–627.
  https://doi.org/10.1016/S0010-4485(96)00099-1
- Patrikalakis, Maekawa, Ko, Mukundan (2004), Surface to Surface Intersections,
  Computer-Aided Design and Applications 1, 449–457.
  https://doi.org/10.1080/16864360.2004.10738287

The present implementation is independent, using standard interval
arithmetic, local B-spline support and de Boor recurrence. These papers
motivate the research direction; this is not an implementation of their
full interval continuation algorithms.
