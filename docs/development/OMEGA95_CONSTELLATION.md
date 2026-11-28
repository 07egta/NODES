# OMEGA 95 — CONSTELLATION: extraordinary vertex / local refinement checkpoint

Date: 2026-11-28 JST. Base: 09a2a546d442fea822d80fb4c756a81f067a8b00.
Research-only draft; no release or FTO clearance. No new production surface algorithm.

## Existing code and coordination

- src/subd.rs: classical general cages, periodic tensor grid cages, Loop/Schaefer approximate Catmull-Clark (ACC) B-rep conversion, and T-spline patch cages.
- crates/sc-geom/src/tspline: odd-degree rectangular index domain, not general extraordinary topology; mesh/refine/extract already exist. crates/sc-kernel/src/tspline.rs: sewing of extracted Bézier patches.
- docs/TSPLINE_PATENTS.md records existing public T-junction feature and unresolved foreign/improvement rights. This PR does not expand, remove or certify it.
- OMEGA 79 PR #144 (open draft, head d1dc479dd4add16192153472c7aec98ec87f296e): isolated two-level rational cubic THB, single rectangular chart; 145 vs 361 control functions and 5.55e-16 sampled preservation error reported by that PR, not re-run in OMEGA 95. No extraordinary topology. No completed OMEGA 78/86 PR located in the targeted search; do not assume clearance. PR #139 remains a separate NURBS effort.
- Follow AGENTS.md: independent branch from main, Rust, no third-party code, no runtime dependencies, no changes to shared source, schema or node IDs.

## Patent screening before implementation (not legal advice)

Public Google Patents bibliographic data checked 2026-11-28. Status is provisional, not official register confirmation.

| Publication | Reported status | Relevance |
|---|---|---|
| US7274364B2 | Expired - Lifetime; anticipated expiration 2024-03-26 | Base family. Independent claim 1 concerns a T-junction control mesh with inferred tensor bases and surface rendering; claim 4 shape-preserving point insertion; claim 9 local knot coordinates/insertion; claim 11 arbitrary topology subdivision. See full claims. |
| EP1606692A4 | Ceased, refusal event effective 2016-10-29 | European family member. |
| JP2006523891A | Pending in aggregator, application 2004-03-26 | Incomplete/unreliable status, verify J-PlatPat prosecution; do not infer active granted rights. |
| KR101177131B1 | Expired - Lifetime; anticipated 2024-03-26 | Korean family member. |
| CA2517463C | Expired - Lifetime (family page) | Canadian family member. |
| WO2004088468A2 | PCT publication | Not a worldwide granted patent. |
| US11620793B2 | Active; granted 2023-04-04; anticipated expiry 2041-03-25 | Autodesk; claim 1 describes extraordinary-vertex refinement using eigen-polyhedron data and knot-interval differences to adjust positions. Not the same as analyzing classical masks. Family includes CN115130253A and DE102022107038A1; individual statuses unverified. |
| US9269189B1, US10339226B2, US10832471B2 | Screening leads, not individually cleared | Trim-free T-spline conversion, watertight refinable surfaces, fitting. Check all claims, families, continuation/divisional, countries. |

Sources:
- https://patents.google.com/patent/US7274364B2/en
- https://patents.google.com/patent/EP1606692A4/en
- https://patents.google.com/patent/JP2006523891A/en
- https://patents.google.com/patent/KR101177131B1/en
- https://patents.google.com/patent/CA2517463A1/en
- https://patents.google.com/patent/US11620793B2/en
- https://patents.justia.com/patent/11620793

Do not publish new T-junction, nonuniform star-point refinement, adjusted eigen-polyhedron rules, or a combined local/irregular scheme before official USPTO, EPO, J-PlatPat, WIPO and target-country checks and claim mapping. Renaming does not resolve rights. Existing T-junction release requires independent review. No FTO conclusion; no new potentially claimed geometry-generation method in this change. Diagnostic classical masks from 1978 are not an implementation of the newer adjustment rules.

## Basis comparison and conditional guarantees

| Basis | Locality and refinement | Mathematical qualification |
|---|---|---|
| Tensor B-spline | compact support; knot insertion across a full row/column | nonnegative, unity, independent valid basis; interior C^(p-m) with degree p and multiplicity m |
| HB | nested regions; support-based selection | typically independent under standard hierarchy, nonnegative, not generally unity |
| THB | truncate coarse expansion over selected fine supports | nonnegative partition of unity, independent with standard nested-domain construction; exact homogeneous refinement, admissibility for bounded overlap; single-chart only |
| LR | local line segments; closure by minimal support | weighted nonnegative unity; independence requires mesh restrictions or a rank/structural check |
| PHT | hierarchical T-mesh | typical bicubic C1; specific basis/construction dependent |
| Analysis-suitable T | T-junction extensions | independence under analysis-suitable hypotheses; rational normalization is not a proof of polynomial unity; extraordinary topology not automatically covered |
| Rational basis | positive weights with nonzero denominator | normalized unity, positivity, affine invariance; numerical conditioning depends on weight range |
| Catmull-Clark subdivision | finite mask, arbitrary manifold quad topology | regular cubic C2; extraordinary tangent continuity requires characteristic-map conditions and nondegenerate geometry |
| Gregory patch | patchwise evaluation | boundary matching depends on construction; special rational corner behavior, not a general exact extraordinary conversion |

Positive weights and a valid basis preserve affine transformations; this does not establish a bound on conditioning. Approximation order depends on degree, space, regularity and admissibility. Coarsening after edits must use exact representability checks or explicit bounded approximation. Hanging nodes are parameter-grid constraints, not automatically mesh topology vertices.

## Classical interior ring derivation

Assumptions: smooth interior n-valent vertex, n>=3, all incident faces quads, no crease/boundary, no modified nonuniform masks. Index C (center), E_i (adjacent), D_i (opposite corner). Classical face F_i=(C+E_i+D_i+E_(i+1))/4, E'_i=(C+E_i+F_(i-1)+F_i)/4, D'_i=F_i, C'=(mean(F)+mean(E)+(n-2)C)/n. This closed (2n+1)-point ring matrix M is nonnegative and row stochastic. Thus it preserves constants and affine combinations. The left invariant limit functional is [n C + (4 sum(E)+sum(D))/n]/(n+5).

For Fourier wavenumber k != 0 let c=cos(2 pi k/n). The 2x2 edge/diagonal block has trace (5+c)/8, determinant 1/16, hence eigenvalues (5+c +/- sqrt((9+c)(1+c)))/16. At k=0 the constant eigenvalue is 1; other roots solve 16n x^2 + (28-12n)x + n-3=0. Let lambda be the larger k=1 eigenvalue (two real tangential modes); mu the largest absolute remaining local eigenvalue after the constant and tangent pair. This is a one-ring matrix, not a full limit-patch evaluation matrix. An outer support mode or geometric degeneracy is not characterized here.

The ratio mu/lambda compares higher-order and tangent scaling; mu/lambda^2 is a second-order scaling warning, not curvature itself. A nonzero normal mode with ratio greater than one can cause unbounded second-order behavior; zero coefficients and special planar/symmetric data change results. Reif's criterion needs a regular injective characteristic map, spectral conditions and appropriate nondegenerate geometric coefficients. Spectral ordering alone is not sufficient. Do not assert parametric C1 across a singular chart or G2 at a general star point.

## Independent numerical experiment (executed, NumPy 2.3.5)

Assembled M independently from the face/edge masks, computed dense eigenvalues and iterated it 60 times. For n=3,4,5,6,8,12,16,32, row-sum max error <=2.3e-16; analytical tangent eigenvalue vs numerical eigenvalue error <=1.3e-15; left-limit invariance error <=2.3e-16. With scalar data p_j=j^2, max deviation from the analytic limit after 60 steps <=4.8e-9 (n=32).

| n | lambda | mu | mu/lambda^2 |
|---|---:|---:|---:|
|3|0.410097051|0.166666667|0.991004|
|4|0.500000000|0.250000000|1.000000|
|5|0.549988355|0.340107388|1.124370|
|6|0.579682326|0.410097051|1.220412|
|8|0.611116527|0.500000000|1.338819|
|12|0.634796412|0.579682326|1.438537|
|16|0.643334922|0.611116527|1.476555|
|32|0.651694298|0.643334922|1.514779|

Second experiment: x/y are the real first Fourier eigenmodes. z uses a radial perturbation at n=3 and a second Fourier mode at n>=4. Iterate classical masks; angle is the maximum angle of center-edge-edge control triangles from the horizontal; H/r^2 is max relative ring height over squared max tangent radius. Not the actual limit surface normal or principal curvature.

| n | angle at depth 0 / 4 / 8 (deg) | H/r^2 at depth 0 / 4 / 8 |
|---|---|---|
|3|63.435 / 0.781 / 0.021|1 / 0.241 / 0.233|
|4|54.736 / 5.051 / 0.317|1 / 1 / 1|
|5|57.059 / 12.718 / 1.890|1 / 1.598 / 2.554|
|6|56.789 / 20.938 / 5.475|1 / 2.218 / 4.921|
|8|54.736 / 32.363 / 15.853|1 / 3.213 / 10.322|
|12|62.632 / 53.336 / 43.050|1 / 4.282 / 18.339|

Rust test-only prototype tests classical weights, affine equivariance, invariant limit, Fourier modes and scaling warnings over valences 3..32. NumPy is independent evidence; rustc/cargo absent, direct git DNS unavailable. Rust tests, fmt, clippy, CI and actual application not executed here.

## Precision CAD architecture and gates

1. Keep regular-chart THB (#144), classical arbitrary-topology SubD and NURBS exchange separate pending review. No assumption that a T-junction is an extraordinary vertex.
2. Exact Bézier extraction per regular THB leaf via homogeneous knot insertion, then compare against source in position and derivatives. For seams use common edge IDs/orientation, synchronized sample positions, and matching knots; no merely visual watertightness. Validate B-rep edge/face residuals before accepting.
3. Regular Catmull-Clark patches are bicubic; extraordinary limit generally requires special evaluation or approximation. ACC is approximate there. Error-controlled adaptive conversion must bound patch error, tangent deviation and seam mismatch, with budget failure reported explicitly. Sampled deviation is not a certified Hausdorff bound. Never mark an approximate face exact.
4. Build an independent limit-surface evaluator/derivative oracle from public classical mathematics, verify characteristic map Jacobian and injectivity, actual normals and principal curvatures at shrinking rings; test degenerate, adjacent extraordinary vertices, boundaries and crease cases separately. No new nonuniform adjustment rule without IP clearance.
5. Before any combined architecture: multi-level THB admissibility, shape-preserving homogeneous prolongation, coarsening representability, local support and seam G0/G1 tests; verify whether cross-chart C1/C2 is even achievable for chosen transitions. No unconditional G2.
6. Same-shape comparison: measure control count, symmetric distance (state sample/certificate), derivative/normal/curvature error, edited support, CPU time, memory, mesh and B-rep integrity under identical units/tolerances. Only the #144 historical THB control count and this diagnostic experiment exist; no cross-method performance, geometric error, or curvature benchmark yet.
7. Run cargo fmt --all --check, cargo test --test omega95_constellation, cargo clippy --all-targets --locked -- -D warnings and workspace CI. Review PR #144 and OMEGA 78/86 before merging. No main/release/runtime changes.

## Literature / provenance
No proprietary source, copied external code or dependencies. Classical formulas independently derived from face/edge/vertex masks. Literature references:
- Catmull & Clark, Recursively generated B-spline surfaces on arbitrary topological meshes (1978).
- Reif, A unified approach to subdivision algorithms near extraordinary vertices (1995), https://doi.org/10.1016/0167-8396(94)00007-F.
- Stam, Exact evaluation of Catmull-Clark subdivision surfaces at arbitrary parameter values (1998), https://doi.org/10.1145/280814.280945.
- Loop & Schaefer, Approximating Catmull-Clark subdivision surfaces with bicubic patches (2008).
- Giannelli, Jüttler & Speleers, THB-splines (2012), https://doi.org/10.1016/j.cagd.2012.03.025.
- Dokken, Lyche & Pettersen, Polynomial splines over locally refined box-partitions (2013), https://doi.org/10.1016/j.cagd.2012.12.005.
- Deng et al., Polynomial splines over hierarchical T-meshes (2008).
- Scott et al., Local refinement of analysis-suitable T-splines (2012).
