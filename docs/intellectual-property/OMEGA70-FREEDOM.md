# OMEGA 70 — FREEDOM: preliminary IP review
Date: 2026-11-28 JST. Not a freedom-to-operate opinion. Default application T-spline paths disabled pending review; vendored source remains present, so this is not a distribution clearance.

## Patent leads
- US7274364B2 https://patents.google.com/patent/US7274364B2/en — foundational T-spline/local refinement; Google Patents reports expiration 2024-03-26. Thomas W. Sederberg; BYU / Autodesk assignment history. Map independent claims to local knot vectors, T-mesh and refinement in sc-geom; verify exact claim elements and official status.
- EP1606692A4 https://patents.google.com/patent/EP1606692A4/en — corresponding European application, aggregator reports ceased. Verify EPO register and national rights.
- JP2006523891A https://patents.google.com/patent/JP2006523891A/en — Japanese application, official status unresolved.
- WO2004088468 — international family lead; verify national phases in PATENTSCOPE.
- US9269189B1 https://patents.google.com/patent/US9269189B1/en — later trim-free T-spline conversion; check independent claims, continuations, ownership and term; do not assume coverage or clearance.
- US20080043023A1 https://patents.google.com/patent/US20080043023A1/en — approximate subdivision surface conversion to Bézier patches; aggregator reports abandoned; family and later claims unresolved.
- US8310481B2 and US7170516B2 are additional search leads only, not classified.
Official verification: https://patentcenter.uspto.gov/ ; https://register.epo.org/ ; https://patentscope.wipo.int/ ; https://www.j-platpat.inpit.go.jp/ . No completed official claim chart or national status verification. J-PlatPat maintenance prevented verification. Record priority, filing, grant, expiry adjustments, fees, assignments, continuations/divisionals and all independent claims before release. Search later patents on crease, extraordinary vertices, local refinement, conversion, extraction and editing. No inference of legal safety from publication date or abandoned application alone.

## Code audit
Main file snapshots: src/subd.rs 9c4c3c07239e46a5868af25a94ec41d25a86d642; crates/sc-geom/src/tspline/mod.rs 531f13b5f89cf88f8758baaff4555bcbf4878f8b; crates/sc-kernel/src/tspline.rs 7688c72082f3ed210ba3cf5dc49b703622fe98e3. The geom module documents Sederberg et al. 2003/2004 and Scott et al. 2012, rational local basis, rectangular domains and odd degree. refine_faces inserts T-mesh cuts; kernel extracts Bézier elements and sews shared boundaries. Normal cylindrical tensor grids and ACC quad patches previously passed through regular TSpline::from_nurbs despite not needing T-junctions. Separate that route into clamped tensor NURBS sheet construction. ACC is approximate at extraordinary points; do not claim exact conversion. Existing SolidCraft import is documented in ../SOLIDCRAFT_CHANGES.md; notices in ../THIRD_PARTY_NOTICES.md. No independent full source provenance or transitive-license audit performed. MIT OR Apache-2.0 does not grant third-party patent clearance; no dependencies added. Follow AGENTS.md and DEPENDENCIES.md. 

## Alternatives (engineering assessment, not legal clearance)
| Method | Locality / quality | Complexity | Decision |
|---|---|---|---|
| Catmull–Clark (1978) | quad cage, smooth regular limit; global refinement | medium | retain existing mesh editing, review modern extensions |
| Loop (1987) | triangular cage, smooth limit | medium | defer; review extensions |
| Hierarchical B-spline | nested local support | high | research |
| THB (Giannelli/Jüttler/Speleers 2012) | truncated nested basis | high | research; check patents |
| LR B-spline (Dokken/Lyche/Pettersen 2013) | local mesh splits | high | research; check rights |
| Locally refined NURBS | rational local basis | high | defer pending claim chart |
| Multi-patch NURBS | patch-local edits, seam constraints | medium/high | preferred alongside tensor grids |
| Gregory patch | extraordinary-point transitions | high | review later variants |
| PHT-spline (Deng et al. 2008) | hierarchical T-mesh polynomial basis | high | research, distinct claims review |
| Adaptive subdivision | local tessellation; crack prevention needed | medium/high | defer until topology tests |
| Quad mesh editing | local cage edits, no exact limit guarantee | medium | retain |
| Free-form deformation | local/global cage depending on basis | medium | research; check variants |

## Release gate
Keep serialized IDs; deny new T-spline cage creation and refinement by default, with Japanese diagnostic. Research Cargo feature is not permission for public distribution. Existing vendored public APIs are NOT removed or gated at crate level; source publication and all alternate call paths need separate review. Do not enable experimental-tspline in release or merge until CI and integration review. Check current open PR overlap. Verify default and research-feature test suites, including existing box/cylinder conversion, seams, topology, cancellation, local refinement invariance. This environment lacks rustc/cargo and direct Git networking; no tests executed here. CI results pending. No professional infringement opinion.
