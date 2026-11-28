# OMEGA 79 — BEYOND: two-level THB research checkpoint

Date: 2026-11-28 JST. Base main: 09a2a546d442fea822d80fb4c756a81f067a8b00.
Status: research only; do not merge or ship before IP and verification gates.

## Repository audit
AGENTS.md requires Rust, MIT OR Apache-2.0, no direct main push, verified claims. Existing crates/sc-geom/src/bspline.rs and spline_space.rs provide homogeneous B-spline primitives; existing tspline module is not replaced. PR #139 is an open draft on NURBS blend/match/trim/continuity; avoid shared files. Searches for OMEGA 78/PATENT PR and omega-78 branch did not locate a result. docs/TSPLINE_PATENTS.md dated Nov 27–28 records existing public T-junction exposure and unresolved related/foreign patents. That document is not an FTO clearance. This PR does not modify those existing features.

## Selection
First research candidate: truncated hierarchical B-splines (THB), single regular tensor-product parameter chart; keep Catmull-Clark for arbitrary quad topology, NURBS/Bézier for precise exchange. No invention or novelty claim. A hybrid is a research direction, not a delivered kernel.

| Method | Local refinement | Continuity / topology | Exact polynomial patches / practical tradeoff |
|---|---|---|---|
| Hierarchical B-spline | hierarchical nested regions | knot multiplicity dependent; regular chart | exact; standard HB basis need not sum to one; overlapping levels |
| THB | nested regions with truncation | same underlying spline smoothness; regular chart | exact per leaf; sparse active basis; admissibility needed for bounded overlap |
| LR B-spline | local mesh-line insertion | multiplicity dependent; regular chart | exact; linear independence depends on LR mesh; related patents require review |
| PHT | hierarchical T-mesh | typical bicubic C1; regular chart | exact patches; continuity lower than cubic simple-knot THB |
| Powell-Sabin | triangular macroelement split | typically C1 quadratic | piecewise polynomial; triangular rather than rectangular NURBS conversion |
| Catmull-Clark | local edits; adaptive refinement needs seam treatment | regular bicubic C2, extraordinary usually C1 under assumptions | regular regions exact; extraordinary limit requires special evaluation or approximation |
| Loop | triangular subdivision | regular quartic box spline, C1 typical extraordinary | triangular patches; conversion not general exact tensor NURBS |
| Gregory | patch construction, not standalone refinement hierarchy | boundary continuity depends on construction | special rational corner behavior; generally approximate SubD conversion |
| Multi-patch NURBS | per patch | boundary continuity imposed, not automatic | exact NURBS; patch management and constraints |
| Adaptive Bézier | subdivision; local independent edits need continuity constraints | per-patch, seam-dependent | exact de Casteljau splitting; control duplication |
| Hierarchical Subdivision | multilevel details | base subdivision dependent | limit evaluation; arbitrary topology; hierarchy management |
| Wavelet surface | local multiresolution details | construction dependent | representation/transform cost; exact exchange depends on basis |

CPU/GPU/memory/conditioning rankings are not measured across methods. THB sparse local evaluation is promising, not benchmark evidence. Existing upstream implementation license not inherited: no external code copied or dependency added. Library review (G+Smo/OpenSubdiv/LR implementations) and license verification are pending before any reuse.

## References (public mathematical literature; independent implementation)
- Giannelli, Jüttler, Speleers, THB-splines: The truncated basis for hierarchical splines, CAGD 29 (2012), https://doi.org/10.1016/j.cagd.2012.03.025
- Forsey and Bartels, Hierarchical B-spline refinement, SIGGRAPH 1988.
- Dokken, Lyche, Pettersen, Polynomial splines over locally refined box-partitions, CAGD 2013.
- Deng et al., Polynomial splines over hierarchical T-meshes, Graphical Models 2008.
- Powell and Sabin, Piecewise quadratic approximations on triangles, ACM TOMS 1977.
- Catmull and Clark, Recursively generated B-spline surfaces on arbitrary topological meshes, CAD 1978.
- Loop, Smooth subdivision surfaces based on triangles, 1987.
- Loop and Schaefer, Approximating Catmull-Clark subdivision surfaces with bicubic patches, ACM TOG 2008.
- Lounsbery, DeRose, Warren, Multiresolution analysis for surfaces of arbitrary topological type, ACM TOG 1997.
- Piegl and Tiller, The NURBS Book, 2nd ed.
- LR patent search lead: https://patents.google.com/?q=%22locally+refined+B-splines%22 ; T-spline baseline https://patents.google.com/patent/US7274364B2/en . Search leads, not legal conclusions. Full patent families, claims, status and jurisdiction checks pending; no FTO statement.

## Prototype
tests/omega79_thb_research.rs is isolated from application runtime, no production API, dependencies, node IDs, persistence or feature changes. Fixed clamped cubic unit-square, 8x8 coarse cells, 16x16 fine cells, one aligned square refinement region, two levels. Use ordinary knot insertion to form tensor prolongation R. Retain coarse functions whose support is not contained in the refined region; truncate their expansion by removing active fine rows. Active fine coefficients are obtained from full prolongation of all original homogeneous controls. For positive weights, project the homogeneous surface. Fine z-edit changes one homogeneous coefficient while keeping weight fixed. Exact in real arithmetic for these nested spaces; binary64 only approximate. All fine basis terms live inside the refinement region. Refinement does not itself deform the surface.

Partition of unity follows because prolongating the all-ones coefficient vector yields all ones; deleted fine terms are replaced by active fine functions. Nonnegativity follows from nonnegative knot insertion weights and B-spline basis. Affine invariance follows from homogeneous linear combinations and positive partition. C2 for cubic simple interior knots and positive denominator; no cross-chart/extraordinary guarantee. Linear independence follows from standard hierarchical construction under proper nested domains; current finite sample rank is not a general proof. Approximation order, deep-level admissibility and conditioning remain open. No coarsening, crease, derivative evaluator, GPU, adaptive tessellation or B-rep integration.

## Independent numerical experiment (actually executed)
Python 3 / SciPy 1.17.0, BSpline.insert_knot on identity coefficients; 65x65 parameter samples including endpoints; clamped cubic; coarse 8 intervals, fine 16; region [0.25,0.75]^2; weighted curved net with x=i/10, y=j/10, w=1+0.25*cos(3*x+2*y)^2, z=sin(4*x)*cos(3*y). Matrix oracle, not execution of Rust code.
- Coarse 121 controls; full fine 361; hierarchical 145 (120 coarse,25 fine).
- Partition max error 4.440892098500626e-16; min basis 0.
- Max Euclidean preservation error 5.551115123125783e-16.
- Edit fine index 12 by z=0.2: maximum sampled displacement 0.0889446087039728; outside its parameter support zero; support area 0.0625.
- Affine transform max error 1.4936523181711914e-15.
- Sample matrix rank 145 (relative singular-value threshold 1e-12), condition 26.905753307767398; maximum 21 nonzero basis functions on sample grid at threshold 1e-14.
- No timing, memory, Catmull-Clark comparison or manufacturing validation yet.

## Gates / next steps
Rust compiler/cargo absent in current execution environment; direct git clone unavailable (DNS). Rust tests authored but NOT executed locally. Verify cargo fmt --all --check, cargo test --test omega79_thb_research, cargo clippy --all-targets --locked -- -D warnings and full workspace CI. Compare oracle output and run random regions, translated/scaled/rational cases, derivatives at refinement boundary. Reuse existing bspline functions rather than shipping duplicate code; implement multi-level admissible hierarchy and cell-local sparse evaluation. Exact finest-space expansion is possible but increases control count; Bézier extraction is exact mathematically for each fine cell, not implemented here. Extraordinary topology remains SubD; seam stitching and tolerance-controlled B-rep export need separate work. Coarsening must reject nonrepresentable edits or explicitly approximate with error reporting.
Do not merge until OMEGA 78/86 IP review and verified Rust gates. Research branch is public; no proprietary code included. No release, main change, or performance claim.
