# OMEGA 80 — SCULPT (phase 1)

Baseline: 09a2a546d442fea822d80fb4c756a81f067a8b00. Existing CageEdit node and Cage representations are reused, with no new dependency. PR #139 also touches subd.rs (B-rep conversion); reconcile on merge. OMEGA 78/79 results were not available at inspection. No new T-spline operations are introduced.

## Semantics
Select vertices, edges or faces in the cage editor. Set XYZ displacement, brush radius (mm), falloff and relax strength. Soft move / relax append one serializable Brush edit to the existing history. Distances are multi-source Dijkstra distances on cage edges, not exact surface geodesics. Linear, smoothstep and truncated normalized Gaussian weights vanish at radius. Relax is a simultaneous Laplacian step; crease and boundary vertices are held fixed. Explicit pinned indices are supported in the serialized operation, not yet exposed in UI. No brush symmetry yet; UI disables brushes when symmetry is enabled. Only general Catmull-Clark and B-spline grid cages are supported. Preview is rebuilt via existing CPU geometry path. This is not a GPU sculpt renderer.

Replay recomputes distances using upstream control positions and refuses changed ordered connectivity, representation or creases via a deterministic fingerprint. This is not persistent semantic IDs or collision-proof matching. Same topology with rearranged semantic roles is not detectable. Prior index-based history is unchanged. The operation does not prove freedom from self-intersections or manufacturing validity.

## Verification
Five Rust unit tests cover compact monotonic weights, disconnected distances, cancellation, replay, topology guard, pins/creases, grid preservation and invalid inputs. Run cargo test --lib subd::sculpt, cargo fmt --all -- --check, cargo test --workspace. Tests have not run in the authoring environment (Rust toolchain and direct Git network unavailable). GUI, Windows, Intel Arc and printing are not verified.

## Manual production acceptance (not executed)
Use box cage or cylinder cage -> cage edit -> preview / existing conversion -> existing mesh export. Test robot shell, lamp, grip, chair, car body, sculpture, architectural canopy, printable object. Record source SHA, dimensions, actual graph, screenshots, editing time, surface deviation, mesh diagnostics and slicer results. Do not mark these as completed until actual artifacts exist.

Remaining: persistent topology IDs, mask UI, interactive brush strokes and symmetry, rotate and topology editing tools, local independent refinement, GPU tessellation and dirty-region caching, curvature/Zebra integration, exact conversion and fabrication verification.
