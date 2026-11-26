# Mission 10 — checkpoint (2026-11-26)

## Scope and evidence

Base: evolve/all at df77c542473a5c637f3353dec48e367dd67db027. Main at 79f32745923adfa9e3de05ef4683b6e3b5f4948f was 19 commits behind. This branch does not integrate or overwrite other feature branches. No new dependency or release was added.

Local cargo/rustc and Rhino/Grasshopper/Windows execution were unavailable; direct Git clone failed DNS. All code changes were submitted using the repository connector. Static review only: no test, build, GUI, or performance run is represented as passing. No independent reviewer or parallel agent was available.

## Inventory

- NODES base: 64 normal picker kinds + Cluster = 65 distinct registered kinds, not 65 proven implementations.
- GH1: 101 selected candidates, not a complete official manifest. SDK: https://developer.rhino3d.com/api/grasshopper/ . Use scripts/audit_gh1_inventory.py inside a trusted GH1 installation to collect the actual build-specific proxy registry. Review library classification, obsolete flags, GUIDs and ports before promotion.
- GH2: 35 selected records from the official documentation; documented index of 1,419 components/parameters for build 9.3.26329.6001 (2026-11-25) is not a verified standard-component count. GH2 records and GH1 records must never be merged by name alone.
- Catalog totals (136 candidate records): VERIFIED 0; IMPLEMENTED_UNVERIFIED 22; PARTIAL 29; APPROXIMATE 5; MISSING 46; BLOCKED 34. These are component mappings, not unique NODES kinds. Unenumerated entries remain unknown; do not derive a compatibility percentage.
- Official-reference execution and tolerance comparison are required before VERIFIED. Missing GUID/port/default/tree evidence stays null/unknown.

## Branch scan

Snapshot: 21 branch names, including main and evolve/all. Reviewed compare metadata and changed-file lists, not every line of every branch. Relative to evolve/all:

| Branch | Ahead/behind |
|---|---:|
| main | 0/19 |
| evolve/free-build | 0/14 |
| evolve/gcode | 0/9 |
| evolve/gh-works | 0/14 |
| evolve/more-nodes | 0/17 |
| evolve/node-picker | 1/0 |
| evolve/stl-export | 0/9 |
| evolve/stl-import | 0/9 |
| evolve/ui-polish | 0/9 |
| evolve/ux-help | 0/14 |
| evolve/viewport-print | 0/9 |
| feat/grasshopper-core-nodes | 1/0 |
| feat/nodes-complete-curve-geometry | 0/0 |
| feat/nodes-complete-data-trees | 6/0 |
| feat/nodes-core-integration | 1/0 |
| feat/nodes-generative-design | 0/0 |
| feat/nodes-mathematics-and-vectors | 4/0 |
| feat/nodes-mesh-and-fabrication | 0/0 |
| feat/nodes-premium-interface | 3/0 |
| feat/nodes-professional-surface-kernel | 0/0 |

Open PR at scan: #1, core integration. Re-fetch branch heads before integration; simultaneous mission branches can advance.

## P0 defect fixed (pending execution)

Graft of a mixed-depth input can generate path {0;0} from a value in {0}, then encounter an empty existing {0;0} branch. The previous insert replaced that value with an empty vector. Preserve an existing branch via entry(...).or_default(). A focused regression injects mixed-depth branches directly and checks count, values and preservation of an unrelated empty branch. Review this same invariant when integrating data_tree.rs from the parallel branch.

## Twenty graph verification targets

All rows are NOT_RUN at this checkpoint. "Existing/added" identifies a possible fixture, not a pass; blocked rows need implementation and an independent oracle.

| # | Workflow | Fixture / remaining gap |
|---:|---|---|
| 1 | Number and slider | Added deterministic scalar + slider update check |
| 2 | Series and lists | Added series graph; existing list tests |
| 3 | Flatten/Graft | Added mixed-depth regression and numeric graph |
| 4 | Path Mapper | Full syntax/matching not present in base |
| 5 | Point and vector | Point tests exist; distinct vector type needed |
| 6 | Grid and random | Existing seeded random/population tests; compare GH sequences separately |
| 7 | Circle and division | Sampled circle only; arc-length division contract required |
| 8 | NURBS curve | NURBS kernel required |
| 9 | Curve intersection | Exact/tolerance-based intersection required |
| 10 | Curve offset | Robust offset and self-intersection semantics needed |
| 11 | Loft | Existing closed loft mesh fixture; exact surface blocked |
| 12 | Sweep | Rail/profile frame and exact surface implementation needed |
| 13 | Surface point placement | UV domains and surface evaluation needed |
| 14 | Solid Boolean | Watertight solid/B-rep kernel and tolerance contract needed |
| 15 | Voronoi | Voronoi fixture, clipping and degeneracy oracle needed |
| 16 | Attractor | Assemble distance/remap graph; check expected values |
| 17 | Twisted architecture | Assemble sections/transform/loft graph and geometric oracle |
| 18 | Print vase | Existing profile/shell/printability tests; validate manifold/STL |
| 19 | Cluster | Existing cluster-related code; nested cluster fixture and persistence required |
| 20 | Save/reload | Added JSON project round-trip; existing persistence tests; full filesystem workflow still to run |

## Run order on a Rust-enabled Windows runner

```powershell
cargo test --locked --test grasshopper_coverage
cargo test --all-targets --locked
cargo build --release --locked --bin Morphlet
cargo run --locked --example coverage_inventory
cargo run --release --locked --example coverage_perf
```

Six new tests added, all unexecuted locally. Existing tests not deleted, skipped or weakened. Full suite test count unknown until executed. CI may run on PR creation; inspect exact head and logs.

The benchmark reports headless evaluator cold/warm times and cache behavior plus 50 slider changes, p50/p95/max, at 10/100/1000/2048 nodes. At 10000 it reports explicit limit rejection, not successful evaluation. No recorded timings. Current MAX_NODES=2048, MAX_ITEMS=8192, MAX_VERTICES=250000, MAX_TRIANGLES=500000. 100k standalone points, large tree, large mesh, RSS, process startup, frame time and input-to-display latency need separate instrumentation. Do not increase safety caps before resource testing.

## GUI / distribution gates

NOT_RUN: add node, Japanese search, English search, wires, numeric input, slider, data preview, group, 3D preview, undo/redo, save, reopen, errors, light/dark theme, small display, high DPI. Use fresh Windows build; archive screenshots and version/commit evidence. Older release screenshots are not evidence of this branch.

Release exe, dependent DLL review, Japanese glyphs, sample files, launch instructions, complete license review, SHA-256 checksum, and reproducible CI build all require verification. No release created.

## Next checkpoint priorities

1. Execute Windows CI, inspect failures without relaxing tests; verify Graft fix and reproduce prior failure on the parent if feasible.
2. Coordinate shared model/evaluator edits with the Data Tree, mathematics, vector and core integration branches; audit duplicate registration and one-output/single-source limitations.
3. Export a trusted GH1 runtime manifest, import full GH2 build-specific metadata, distinguish standard/obsolete/experimental/third-party, and gather independent numerical fixtures.
4. Extend exact curve/NURBS, surface/UV, intersection and B-rep capability with deterministic tolerances and topology tests. Treat tessellated geometry as approximate.
5. Exercise all 20 workflow fixtures, add save/cancellation/error propagation and large-data tests; capture UI and performance on actual hardware; commission independent review before merge.

This is a partial engineering checkpoint, not a completion or release claim.
