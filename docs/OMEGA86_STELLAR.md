# OMEGA 86 — STELLAR: IP and surface audit

Date: 2026-11-28 JST. Baseline: [09a2a54](https://github.com/07egta/NODES/commit/09a2a546d442fea822d80fb4c756a81f067a8b00).
This is a preliminary engineering screen, NOT a freedom-to-operate (FTO) opinion or patent license.

## Release decision

HOLD expansion or new public distribution of unreviewed T-junction/extraordinary-point algorithms.
The baseline already contains public T-spline refinement and an ACC approximation; this audit does not
erase that history or establish legality. Do not rely on a disabled feature flag, a source license, an
expired basic patent, or independent development alone as patent clearance. Do not delete or change
existing serialized IDs in this PR. Owner and patent counsel should review current distributed code and
release scope separately. For new work prioritize ordinary tensor-product NURBS and independently
specified diagnostics; assess alternative local-refinement methods before implementation. No new
patent-sensitive algorithm or dependencies in this PR.

## Patent record (Google Patents preliminary status, not official register confirmation)

Basic family 33131772; Thomas W. Sederberg; priority 2003-03-26 (US provisional 60/458231),
filed 2004-03-26, US grant 2007-09-25. BYU → T-Splines, Inc. → Autodesk (assignment recorded
2014, effective 2011-12-21). NSF confirmatory license recorded 2009, not a license to NODES.
- [US7274364B2](https://patents.google.com/patent/US7274364B2/en): expired lifetime 2024-03-26.
- [EP1606692A4](https://patents.google.com/patent/EP1606692A4/en): ceased; no EP grant shown.
- [JP2006523891A](https://patents.google.com/patent/JP2006523891A/ja): database says pending;
  do not infer grant, lapse or current enforceability from this stale/ambiguous label.
- [KR101177131B1](https://patents.google.com/patent/KR101177131B1/en): expired lifetime 2024-03-26.
- [CA2517463C](https://patents.google.com/patent/CA2517463C/en): expired lifetime 2024-03-26.
- [WO2004088468A2](https://patents.google.com/patent/WO2004088468A2/en): PCT ceased; not a world patent.

Basic claim scope includes local control-mesh refinement and blending functions; see original claims
rather than inferring scope from the title. The simple family lists one US application; this is not proof
that there are no related continuations, divisionals or other priority families.

### Related claims to map before release

| Document | Preliminary US status / dates | Screening focus |
|---|---|---|
| [US9269189B1](https://patents.google.com/patent/US9269189B1/en) Autodesk | Active; adjusted expiry 2034-02-01 | Claim 1: functional trimming curve + aligned trim-free control mesh + variable-knot bicubic patches. Do not add trim-free reconstruction based on these methods. Existing T-spline-to-Bézier extraction alone does not establish that all limitations are met. |
| [US11620793B2](https://patents.google.com/patent/US11620793B2/en) Autodesk | Active; priority/filed 2021-03-25, granted 2023-04-04, anticipated expiry 2041-03-25 | Claim 1: extraordinary-point refinement with eigen-polyhedron data and adjustment rules using knot interval differences. Hold new nonuniform star-point adjustment; this is not a claim on all Catmull-Clark algorithms. |
| [US10832471B2](https://patents.google.com/patent/US10832471B2/en) Mitsubishi Electric Research Laboratories | Active; filed 2017-03-27, granted 2020-11-10, anticipated expiry 2037-03-27 | Claims 1/8: depth image/point cloud, recursively partitioning based on fit RMSE, T-mesh fitting. Avoid that combined method; ordinary surface evaluation is not equivalent. Family includes JP6843237B2; Japanese status not verified. |
| [US8310481B2](https://patents.google.com/patent/US8310481B2/en) Bailey | Status not independently verified | Claims 1/5/10 target optical control lattices and flux-density/gradient weight optimization. |
| [US10339226B2](https://patents.google.com/patent/US10339226B2/en) EPFL | Status not independently verified | Watertight locally refinable interpolatory surfaces; examine before new method. |
| [WO2011025385A1](https://patents.google.com/patent/WO2011025385A1/en) / [EP2471046A4](https://patents.google.com/patent/EP2471046A4/en) | EP withdrawn, US20120191423A1 abandoned, WO ceased per aggregator | LR refinement-related family; not a blanket clearance for all LR methods. |
| [US8570324B2](https://patents.google.com/patent/US8570324B2/en) NVIDIA | Status not independently verified | Watertight evaluation of approximate Catmull-Clark; review existing ACC pipeline separately. |
| [US6389154B1](https://patents.google.com/patent/US6389154B1/en) | Status not independently verified | Exact subdivision evaluation at arbitrary parameter values; review before eigenbasis evaluator. |

Register attempts:
- [USPTO Patent Center](https://patentcenter.uspto.gov/applications/10811057): HTTP 403.
- [EPO Register](https://register.epo.org/application?number=EP04758398) / [Espacenet](https://worldwide.espacenet.com/): target record inaccessible in this environment.
- [WIPO PATENTSCOPE](https://patentscope.wipo.int/search/en/WO2004088468): HTTP 403.
- [J-PlatPat](https://www.j-platpat.inpit.go.jp/): service reported maintenance on 2026-11-28.
- Trademark: search and verify T-SPLINES/T-SPLINE marks, class, owner and jurisdiction through official
  registries (USPTO TSDR, EUIPO, J-PlatPat, WIPO). No trademark clearance obtained; use descriptive
  technical references, not a branded product name or logo. MIT OR Apache-2.0 copyright terms in
  SolidCraft do not establish permission from third-party patent/trademark owners.

Outstanding: INPADOC extended families; continuations/divisionals, assignments, maintenance fees,
PTA/PTE, terminal disclaimers, jurisdiction-specific claims and official grant/expiration history
(including JP, EU states, US, CA, KR, CN). Record all independent claims, prosecution history and
an element-by-element mapping to actual NODES code. Do not publish held algorithms as pseudocode/code
simply behind feature flags.

## Code audit at baseline

- `crates/sc-geom/src/tspline/mod.rs`: real local tensor-product basis on rectangular T-mesh,
  odd degree only; homogeneous positive finite weights, local knot vectors, rational evaluation
  and first partials. This is not just a quad mesh. It does not implement arbitrary valence
  extraordinary vertices, second derivatives or general trimmed/periodic T-mesh topology.
  Analysis-suitability is an explicit query, not proof for every accepted mesh.
- `refine.rs`: shape-preserving intent in homogeneous basis refinement, closure/extension
  propagation and work limits. No coarsening API observed. It can propagate beyond the requested
  region. Current main already exposes this through `Cage::refine_faces` in `src/subd.rs`.
- `extract.rs`: regular mesh to one NURBS tensor surface; irregular mesh to rational Bézier
  elements. Distinguish mathematically exact representation from finite-precision implementation.
- `crates/sc-kernel/src/tspline.rs`: stitches adjacent Bézier edges using parameter keys. Shared
  edge identities prove neither tangent agreement nor global geometric validity. Check sorted
  interior seam breaks for negative parameter values (u64 bit order is not numeric f64 order).
- `src/subd.rs`: grid cages are uniform cubic periodic/clamped splines; general cages use
  finite Catmull-Clark preview and Loop/Schaefer ACC bicubic approximation. The latter is not the
  exact limit at extraordinary points. ACC refuses open/nonmanifold edges; its patch assembly
  does not consult crease tags. Do not claim sharp crease preservation or G2 from that conversion.
  `to_brep` has a misleading exact-surface doc comment. `cap` applies to cylinder grids; a T-spline
  patch is an open sheet regardless. Closed B-rep does not certify smoothness/manufacturability.
- `tests/nurbs_kernel.rs`: existing local refinement checks a 11x11 parameter grid and
  1e-9 mm error, then converts/thickens; existing box test compares approximate volume at 2%.
  These are test definitions, not results run here. Neither is a global maximum-error certificate.
- `docs/SOLIDCRAFT_CHANGES.md`: identifies source revision and MIT OR Apache-2.0. Prior docs
  `docs/TSPLINE_PATENTS.md` record public local refinement since 2026-11-28. Related PRs #48, #67,
  #81 surfaced. No OMEGA 70/78/79/80 result established by the search; avoid assuming absent.

## Mathematical checks and experiments

Rational surface: S = sum(w_i P_i B_i) / W, W = sum(w_i B_i).
Check W>0 and finite; unweighted sum(B_i) is not necessarily 1 for arbitrary T-mesh.
Normalized rational R_i = w_i B_i/W should sum to 1 where defined. Test both separately.
Local support: outside the support of either local univariate knot interval B_i=0.
Linear independence is a theoretical property under a specified mesh admissibility class; a
sampled collocation rank/SVD is a diagnostic only (sampling, singular-value threshold, degree
and domain must be reported). Test weights over several orders of magnitude and common weight
scaling; avoid overflow/underflow and report rejected configurations, not NaN coordinates.

Refinement: compare homogeneous coefficient identity where possible; evaluate same parameter
positions before/after and sample every knot interval, boundaries, T-junction neighbourhoods.
Record max and RMS deviation in mm, coordinate scale, absolute/relative tolerance, control count,
element count, time and propagation. Finite sampling is a lower bound on supremum error; certified
bound requires interval/convex-hull methods on a common rational subdivision. Shape changes from
editing are not refinement error. Coarsening must be refused or declared approximate unless
coefficient-space equivalence is demonstrated.

Extraordinary vertices: measure interior edge valence, not confuse a T-junction on a rectangular
parameter mesh with a star vertex in a manifold control mesh. For standard smooth Catmull-Clark,
regular interior patches have bicubic B-spline behavior (C2 at simple knots); extraordinary-point
regularity requires characteristic-map/eigenvalue conditions. C1 in the appropriate regular
parameterization is not a claim of G2; curvature can diverge. ACC is a different surface: do not
transfer limit-surface guarantees. Evaluate coincident boundary points, oriented normals,
cross-boundary tangents, first/second fundamental forms or normal curvature with aligned tangents;
if the Jacobian is singular or normals nonfinite return indeterminate. A crease is an intentional
tangent discontinuity; do not smooth it away silently.

Fixtures: regular 4-valence torus; irregular closed quad meshes from subdivided triangular,
pentagonal and hexagonal prisms (3/5/6 valence), distorted non-planar ring, valence 8, boundaries,
annulus/hole, crease, nonmanifold and collapsed controls. Compare ordinary B-spline baseline,
finite subdivision limit estimate and ACC on the same geometry. Test repeated refinement and
large/small positive weights. Report preview versus export deviation, shared boundary mismatch,
normal angle, curvature discontinuity, sampling resolution and singular flags; no claim of
measured results without running the tests.

## Alternatives (research priority, not clearance)

| Method | Locality / shape | Extraordinary topology | CAD/implementation |
|---|---|---|---|
| THB-splines | nested local refinement; partition of unity, coefficient preservation | rectangular chart; seams explicit | high potential for exact patch extraction; medium-high complexity; patent search pending |
| Hierarchical B-splines | nested basis, not generally normalized without truncation | chart-based | useful baseline; medium complexity; pending |
| LR B-splines | flexible local line insertion; independence requires mesh conditions | chart-based | more complex validation, conversion; related family found above; pending |
| PHT-splines | local hierarchical T-mesh, classical bicubic C1 polynomial basis | chart-based | rational extension separate; patent search pending |
| Catmull-Clark | global classical subdivision, adaptive tessellation distinct | natural arbitrary valence; generally not G2 at stars | existing path; regular exact, irregular approximate export; review newer claims |
| Gregory patch | flexible interpolation across patch seams | handles irregular junctions with specific constructions | continuity conditional, conversion may be approximate; patent search pending |
| Multi-patch NURBS | local patch edits, explicit matching constraints | charts around stars | best near-term interoperable baseline; matching G1/G2 not automatic; review specific algorithms |
| Adaptive subdivision | local sampling of a fixed surface | depends on scheme | not necessarily local control refinement; watertight adaptive meshing claims require review |

Research references: Giannelli/Jüttler/Speleers 2012 [THB](https://doi.org/10.1016/j.cagd.2012.03.025);
Reif 1995 [characteristic map](https://doi.org/10.1016/0167-8396(94)00007-F);
Stam 1998 [Catmull-Clark evaluation](https://doi.org/10.1145/280814.280945).
Use references for mathematics, not as permission to copy code or practice later claims.
OMEGA 79: compare chart-based hierarchical methods and admissibility. OMEGA 80: surface editing UI
must show exact/approximate/unknown and crease limitations, not promise G2. OMEGA 77: reuse
continuity diagnostics rather than duplicate solver/analysis.

## Evidence and merge gate

- Read baseline AGENTS.md, architecture, development status, relevant source and test excerpts.
- Local cargo/rustc absent; direct git clone failed DNS. No Rust tests, benchmark, GUI or STEP export
  executed in this environment. No numerical results fabricated.
- Draft PR only; review source provenance, run CI, complete patent clearance separately.
- Proposed follow-up: collect the test evidence above; do not implement disputed methods until reviewed.
