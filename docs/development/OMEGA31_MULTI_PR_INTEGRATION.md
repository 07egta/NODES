# OMEGA 31 — Multi-PR integration laboratory (2026-11-28 JST)

Draft PR #110; branch `integration/omega31-multi-pr-20261128`. Do not merge until MISSION 21 reviews the exact integrated head and full CI passes. No original PR branch, main, or shared history was modified.

## Pinned sources

Initial main: `ec3b840e81c76aa7e426da5dc47b716ba975a98e`. Each selected PR had this exact merge base, zero commits behind, and only added/modified regular text paths. GitHub compare and changed-file lists were inspected for all 34 initially open PRs (full semantic audit remains incomplete).

| PR | Head SHA | Purpose | Source CI at observation |
| --- | --- | --- | --- |
| #102 | `d648a7bb4bdb3545d67236f6efa29665dc02605d` | bounded STL/OBJ input and shared polygon budget | run 46109997214, fmt failed |
| #106 | `4f1c1d4ea0caa88b5f11135123e23ff045ef366c` | 3 STL/OBJ regression tests | run 46110074879, 18 pass/3 fail in import_tests |
| #103 | `1b0735acf85a2f2241e9857e91a227c910125270` | Taubin buffer reuse and exact-reference tests | run 46110019770, in progress |
| #108 | `f86966b84ccb74f25fa6713452fc02aed850d87a` | duplicate native maps, bounded atomic save names | run 46110087199, in progress |
| #104 | `7f933c209418741352037421a4b428a89e6db89a` | suggested-node failure rollback | run 46110025989, fmt failed |
| #107 | `b83b6f63f7728f58ed3fd56ad134200eb4c501cd` | passive numeric rendering, graph restoration | run 46110057973, in progress |

Initial merge `1b5b11220ea5392245ce3bfde1c484e7919cc780` has all six source heads and original main as parents. 15 distinct paths, exact Git blobs. Verified Git commit parents and compare path set via API. Original source CI failures are not waived.

## Concurrent main advance and repairs

Main advanced via merged PR #80 to `c1213c885647c2e708017a63f6b7bdc8f9b66620`. Its changes overlap canvas.rs and model.rs only in the initial six-PR union. Used the new main tree as base and applied all five #107 canvas hunks and one #108 model hunk with uniquely matching exact old context, retaining #80's Reroute and 17 standard nodes, G-code examples and regression tests. Other nine unchanged source blobs retained verbatim.

Read actual failed Linux jobs 140871192986 (#102) and 140871287013 (#104): applied the exact rustfmt suggestions to polygon_triangulation.rs and node_shelf.rs. Job 140871452550 (#106): ascii_stl_rejects_missing_endsolid, ascii_stl_rejects_vertices_spanning_facets and obj_accepts_inline_comments_on_faces all failed. Implemented ASCII STL state machine (complete solid/facet/loop/three vertices, no cross-facet assembly, reject truncation; allow complete multi-solid blocks and UTF-8 BOM), OBJ inline-comment removal while retaining #102 size and predicate budgets, and added boundary regressions. Added a native duplicate-map rejection regression on the new #80 persistence test file. No Cargo manifest or lockfile changes; no new serialized IDs from the six fixes, and #80 IDs retained.

## Remaining integration order / overlap map

- #53 targets #52's branch; verify ancestry and take only a coherent stack. #109 is a replacement candidate for #81, not a second implementation; both touch shared NURBS/registry files and Cargo manifests.
- Registry/engine overlap group: #52, #55, #84, #88, #89, #91, #98, #99, #101, #109. Includes shared model.rs/engine.rs, lib.rs and/or node icons/access. Compare variant names and serialized keys before merging; #55 and #101 also overlap generative.rs. #99 overlaps #103 mesh_tools.rs; #91 overlaps #96 templates.rs.
- #83, #92, #107, #109 overlap app.rs or persistence.rs; integrate after UI state and save regressions. #84 overlaps app/model/engine and G-code/export contracts.
- #86, #90, #95, #97, #100 each touch lib.rs; #57 and #97 overlap CLI. #65 touches examples/edit workflow and expression nodes. #85, #87, #93, #94, #105 were documentation-only in the inspected snapshots; recheck heads before treating them as implementation. #96 currently only introduces a helper, not complete user-facing wiring.
- #80 was merged during this run. Check latest main and all PR heads again before selecting next group.

## Verification boundary and resume

Initial integration workflow 46110550521 was in progress; no integrated Rust run result confirmed here. The Chat container has git but no cargo/rustc and github.com DNS fails; no subagents exposed. GitHub Actions must run fmt, locked all-target check, strict clippy, all-target tests (including existing example editing/serialization and registry contracts), Linux/Windows release builds; run ignored performance benchmarks and native GUI/Intel Arc checks separately. No performance gain or complete Grasshopper compatibility claimed. Validate exact merge parents, compare file set and source SHA; re-check main, resolve any subsequent overlap and repeat. Next step: inspect #110 head CI, repair from logs, then test the registry group in small batches with duplicate NodeKind/port checks.
