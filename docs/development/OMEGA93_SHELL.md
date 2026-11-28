# OMEGA 93 SHELL — first increment: bounded mesh offset safety

Base: `09a2a546d442fea822d80fb4c756a81f067a8b00`. Independent NODES mesh implementation, no added dependencies.
The owner requires Rust-first and MIT/Apache-2.0 compatible code.

## Existing work and coordination
- OMEGA 74 [PR #140](https://github.com/07egta/NODES/pull/140) was open/unmerged at inspection; it covers B-rep operator acceptance. Do not duplicate or override that work.
- No identified OMEGA 88, 91 or 92 PR was available at initial search. Recheck the PR queue before integration.
- `crates/sc-kernel/src/surface_offset.rs` offsets planes/cylinders analytically and fits other surfaces; its sampled principal curvature and midpoint residual checks are not certified global bounds. `offset.rs` handles restricted analytic face offsets. `blend*` handles existing blends. This change does not modify vendored code.
- Legacy `gh_nodes::shell` checked only axis-aligned extents and directed edges, then generated a mitered inner mesh; `mesh_tools` offset/thicken returned unchecked results. Both now share bounded diagnostics.

## Mathematics and implemented acceptance policy

Let S be a regular oriented surface, n its unit normal and shape operator A=-Dn. The normal offset F=S+d n has tangential differential I-dA. The factors 1-d*k_i vanish at focal distances. Even when both are nonzero, distant surface points can meet. Reach is limited by focal behavior and the medial axis; curvature samples alone cannot certify global injectivity.

For a triangulated face under linear vertex motion, let u,v be original edge vectors, du,dv the edge displacements and n=u×v. The normalized projected area is
q(t)=1+t n·(du×v+u×dv)/|n|²+t² n·(du×dv)/|n|².
Evaluate t=0,1 and the interior stationary point of a convex quadratic. Reject near-zero or negative projected area. This intentionally conservative binary64 test may reject valid rotating faces; it is not a proof of immersion. Checking only final orientation misses q(t)=(1-2t)².

Reuse indexed topology, translation-stable signed volume, and the bounded AABB/SAT nonadjacent contact diagnostic. Contact pairs sharing a vertex are not inspected; tangencies within tolerance count. A budget limit returns an error, not zero contacts. Input geometry is not welded, simplified, or rescaled without consent. The old axis-aligned extent heuristic was removed because it rejects valid tilted planar sheets. Legacy normal/miter placement is retained and remains approximate. No automatic thickness change.

## Repair research and future gates

- For smooth regular patches below reach, use analytic offsets where available or interval-bounded fitted patches; use certified curvature and residual subdivision. Build offset envelopes by splitting at intersection curves, classifying regions using signed distance/winding, then reconstructing trim loops and validating B-rep connectivity. Do not assume all self-intersections can be repaired while retaining a normal correspondence.
- For planar convex polyhedra the inward erosion is the intersection of shifted half-spaces; topology may change at critical distances. The original offset and erosion boundary are different definitions outside reach.
- For closed solids, use a signed-distance sublevel set for regularized dilation/erosion (Minkowski sum with a ball / morphological erosion). For open sheets, define boundary caps explicitly. A Lipschitz distance field can bound cells using their center and half-diagonal; adaptive octree + manifold extraction needs certified sign and feature handling. Output must be labeled approximate mesh; do not replace the exact path automatically.
- For rolling-ball blends, intersect offset supports to locate the center spine; solve contact points, trim the parent faces and validate mutually interacting corner patches. Variable radii and three-edge setback/G2 patches remain unresolved.
- Minimum wall thickness is not the displacement of corresponding vertices. A sample is not a lower bound; certify distances between appropriately defined opposing boundary regions with bounded subdivision and explicit nesting/orientation.
- Candidate repair choices are user-facing: reduce requested thickness, widen local gaps, round a crease, or explicitly opt into regularized erosion/mesh reconstruction. The current messages suggest these options, not a computed feasible radius or guaranteed repair.

## Tests / limitations

Added regressions: a double orientation flip; a translated triangle; cancellation; two separated planar sheets (volume 160 mm³ at thickness 0.8 each, collision at 1.2); tilted sheet that the previous extent test rejected.
The authoring environment has no Rust/Cargo and direct Git DNS fails. Source changes are statically reviewed only. Run:
`cargo fmt --all -- --check`
`cargo test --lib omega93`
`cargo test --workspace`
`cargo clippy --workspace --all-targets -- -D warnings`

No Rust execution, timing, B-rep validation, STL/3MF export or real print is claimed. General repair, variable thickness, certified intersections, global injectivity, nesting, continuous collision and complete fillets remain incomplete. The mesh pair budget may reject dense otherwise valid models; benchmark and improve the broad phase before relaxing it.
