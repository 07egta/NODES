# Exact surface differential geometry

The three new node IDs are `surface_gaussian_curvature`, `surface_mean_curvature` and `surface_principal_curvatures`. Inputs are a mathematical Surface and normalized U/V. All outputs are numbers; Principal Curvatures returns two values in ascending signed order. No mesh-derived curvature is used.

## Mathematical contract

`SurfaceJet2` contains S, Su, Sv, Suu, Suv and Svv. NURBS jets use analytical B-spline basis derivatives, tensor products in homogeneous coordinates and the rational quotient rule. Native knot derivatives are scaled to normalized UV. Plane, frustum, sphere and torus jets use direct analytical expressions. The first and second fundamental forms determine the symmetric shape operator in an orthonormal tangent basis; its eigenvalues are the principal curvatures. K is their product and H their average.

Orientation is **n = Su × Sv / |Su × Sv|**, and the second fundamental form is **(n·Suu, n·Suv, n·Svv)**. Consequently an outward sphere of radius r has **K = 1/r², H = -1/r, k1 = k2 = -1/r**. Reversing surface parameter orientation preserves K and changes the signs of H and both principal curvatures. Units are reciprocal squared model units for K, reciprocal model units for H and principal curvatures.

## Limits and failure behavior

- UV is finite and lies in [0,1]. Cancellation is checked before work and during NURBS control-grid evaluation.
- Parameter singularities and nearly parallel tangent directions are explicit errors, not NaN. Sphere poles are explicit UV-singularity errors even though their geometric curvature has a known limit.
- At an interior knot of multiplicity m, a degree-p parameterization must be at least C2 (p-m >= 2). Lower-continuity knots are rejected. Evaluation at domain boundaries is one-sided; a simple cubic internal knot is allowed.
- This policy is conservative: a special curve or surface may happen to have smoother geometric curvature at a lower-continuity knot, but NODES does not infer that fact.
- Tangent angular sine <= 1e-10 is treated as ill-conditioned. Nonfinite arithmetic produces a clear error.
- No general trimmed-surface, arbitrary B-rep curvature or principal direction output is supplied by this milestone.

## Independent verification

`tests/surface_curvature_tests.rs` checks planes, spheres, cylinders, conical frusta, torus outside/inside/parabolic rings, a polynomial paraboloid, a bilinear saddle's mixed derivative, rational quarter-cylinder curvature and common-weight-scale invariance, orientation reversal, invalid and singular inputs, non-C2 knots versus a C2 cubic knot, cancellation, graph evaluation, cache invalidation, persistence and local error propagation.

Status at initial checkpoint: implementation and tests authored; execution results pending the integration log.
