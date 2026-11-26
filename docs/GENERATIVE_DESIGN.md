# Generative design: Mission 08 / 独立実装

## Scope and references

Public interface research (not copied code): [Grasshopper 1 component index](https://grasshopperdocs.com/completeIndex.html), [Transform](https://grasshopperdocs.com/addons/grasshopper-transform.html), [Voronoi](https://grasshopperdocs.com/components/grasshoppermesh/voronoi.html), [Convex Hull](https://grasshopperdocs.com/components/grasshoppermesh/convexHull.html), [Delaunay Mesh](https://grasshopperdocs.com/components/grasshoppermesh/delaunayMesh.html), [Grasshopper 2 official component list](https://www.rhino3d.com/docs/grasshopper2/specs/). GH1 and GH2 are separate targets. The system nodes above do not require third-party plugins; [Nautilus](https://grasshopperdocs.com/components/nautilus/voronoiFrom2DPoints.html) and [NGon](https://grasshopperdocs.com/components/ngon/convexHull.html) are separate plugin examples, not dependencies or compatibility claims.

| Concept | This PR / existing | Limit |
|---|---|---|
| Move, Rotate, Scale, Scale NU, Mirror, Twist | existing | origin/axis restrictions, sampled geometry |
| Rotate 3D, Shear, Taper | new | center + numeric axis, Z shear/taper; not exact NURBS |
| Rectangular, triangular, hexagonal, radial grids | new | XY only, row-major, bounded |
| Linear and Polar Array | existing | sampled shapes |
| Grid and Point Array | new | one source per branch for Point Array; zero translation reuses Arc, other translations materialize |
| Hex and triangular panels | new | closed planar polylines, no Boolean union |
| Voronoi, Convex Hull | new | XY projection only, Voronoi rectangle centered at origin, 256 input sites, reject outside or tiny cells; near duplicates merged at relative 1e-9 |
| Closest Point, Multi Attractor, Curve Attractor | new | nearest only, linear search budget; Curve Attractor uses segments |
| Random Points, Populate 2D/3D, Perlin-like Noise | existing | seeded rectangular box and sampled displacement |
| Fractal Tree | new | bounded binary recurrence, XZ plane |
| Cylindrical wrapping | new, independent subset | curve only, 10-degree maximum angular sampling interval; not arbitrary Surface Morph |
| Orient, plane-to-plane, surface arrays, bend, flow, Box/Surface Morph | missing | need frame, exact curves and/or surface API |
| Delaunay | new | bounded XY Bowyer-Watson; floating-point predicate and topology checks, reject degeneracy |
| Concave Hull, generic polygon partition | missing | robust predicates needed |
| Populate Mesh, noise direction field, curve array | new | triangle area sampling (surface only), direction segments, arc-length translation only |
| Arbitrary surface panelization, general vector field and simplex | missing | mesh/curve/surface adapters needed |
| Cellular automata | new | Conway Life, finite grid, 100 steps maximum |
| General L-system, constrained optimization, design exploration | missing | bounded algorithms and UI needed |

## Contracts

- No new dependency or license changes (MIT OR Apache-2.0 project).
- New kinds append to the existing serde enum, no rename or schema version change.
- Per-item nodes use the existing engine matching; list nodes see each existing branch independently. Settings for a list node use first item. Exact Grasshopper matching is not asserted.
- No new public geometry types; integrate at NodeKind::spec and engine evaluation. Existing global weight, item and cancellation checks remain. List nodes have additional per-call bounds.
- Voronoi clips the rectangle by the perpendicular bisector halfplanes in normalized coordinates. Convex hull uses monotone chain and a relative orientation tolerance. This is floating-point, not an exact predicate kernel.
- Rotate3D uses Rodrigues' rotation; angles in degrees. Nonlinear changes affect sampled vertices. No general NURBS, surface/solid topology, Boolean or self-intersection guarantees.
- See examples/generative/README.md for ten editable graphs. These are design studies, not certified printable or structural designs.

## Verification

Unit tests exercise duplicate, near-collinear, out-of-domain, bounds, cancellation, counts, tie ordering, rotation and cylinder radius. Integration tests load all examples, require non-empty output, edit a parameter, and check branch preservation. Run `cargo test --all-targets --locked` and `cargo build --release --locked --bin Morphlet` on Windows. Local Rust/compiler and GUI unavailable in the authoring environment; no claim of successful compile, rendering or exact GH equivalence until CI/manual verification.

### Geometry regression: convex-hull coverage

`tests/generative_geometry_properties.rs` exercises Voronoi area and nearest-site ownership, Delaunay coverage/topology, seeded mesh sampling, determinism, and shared hex-panel edges through actual graph evaluation. A separate JavaScript reproduction of the Rust arithmetic found that for seed 23 (48 sites in a 120 × 80 rectangle), a finite supertriangle of normalized radius 32 omitted approximately 0.476863 square model units of the convex hull. The updated kernel checks normalized hull-area coverage and supporting boundary edges, retries at most six supertriangle radii (32 through 32768), and rejects results that cannot satisfy its validation. The same independent numerical reproduction recovered the full area at radius 512 (absolute error approximately 3.2e-12 square model units); seeds 11 and 47 also met the area check. These are numerical experiments, not compiled Rust or Windows GUI test results.
