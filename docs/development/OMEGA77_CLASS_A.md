# OMEGA 77 — Class-A quality laboratory (draft)

Base: 09a2a546d442fea822d80fb4c756a81f067a8b00. Coordination: OMEGA 71/73; existing [NURBS PR #139](https://github.com/07egta/NODES/pull/139) adds B-rep blend, match, continuity and mesh curvature bands. This PR uses the existing mathematical Surface jet and SolidCraft fair_net instead of adding another NURBS kernel, or editing those B-rep nodes.

## Mathematical contract

- Principal lines use the eigenvectors of the existing symmetric shape operator in its orthonormal tangent frame; the lower signed curvature comes first. Signs are arbitrary, umbilics are errors. Normal curvature is the quadratic form in a unit direction, angle from increasing U.
- UV condition is the ratio of singular values of the normalized-UV Jacobian. It depends on parameterization; it is not a physical fairness measure.
- Boundary report compares two *explicitly corresponding* untrimmed UV boundaries at 3–257 stations, including ends. IDs 0/1/2/3 mean U0/U1/V0/V1. Reverse B if needed. Output order: maximum position gap (model units), unoriented tangent-plane angle (degrees), embedded shape operator Frobenius difference (1/model unit, with normal signs aligned), max first derivative jump and max second derivative jump (normalized seam coordinates), then five t values of maxima. C1/C2 residuals use canonical seam derivatives, with the A transverse direction pointing out and B pointing in. They are parameterization-dependent. G2 tensor comparison only makes geometric sense at G0/G1 within tolerance; slight normal tilt contributes to the residual. No certified G-grade, G3/C3, correspondence search or adjacency/self-overlap proof; samples can miss defects. Singular/low-regularity stations fail, not zero.
- Fairing accepts open, clamped NURBS with equal positive weights, up to 256 control points and 32 iterations; at least two free interior points required by the existing solver. Fixed boundary rows (optionally two layers) and unchanged knots/weights preserve boundary position (and first derivatives with two layers). The solver uses parameter-dependent thin plate energy, not intrinsic curvature or Class-A certification. Nonuniform rational weights rejected. Cancellation checked before and after the bounded solve, not inside it. Result/report nodes independently evaluate identical settings.
- Bicubic panel provides an editable polynomial 4×4 fixture. The boundary is planar, the alternating interior height produces a saddle-like disturbance. With 4×4, keep-tangents option leaves no free points and correctly fails; use larger nets for that option.

## Evidence

Authored analytic and graph tests plus a .morph example; execution pending CI. No local Rust toolchain or Git network in this environment; no numerical results or GUI screenshots claimed. No new dependencies. No direct main changes. Existing serialized IDs untouched. Existing B-rep zebra and draft visualization retained.

## Remaining

G3/C3 and certified adaptive seam analysis; general trimmed surface correspondence; curvature/UV heatmaps with explicit legends and singular masks; curve torsion/comb; viewport diagnostic picking; rational/intrinsic fairing and multipatch constraints; actual Windows and GPU screenshot verification; formal industry Class-A evaluation. Do not label the product Class-A certified.
