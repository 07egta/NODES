# PROJECT INTEGRITY — planar predicate correctness (first increment)

Base: `8cac838229071509156d560391c3d1ba5237d761`. Scope: planar convex hull and Delaunay, reusing OMEGA 53's interval/exact-integer orientation kernel. No third-party code or dependency is added. SolidCraft is already integrated by PR #48; this change does not modify its vendored code. Surface intersection PR #153, surface quality PR #142 and mesh offset PR #156 are separate work.

## Findings and corrections

- Convex hull formerly removed all turns with determinant <= span² × 1e-12. Example: (0,0), (1,0), (2,1e-13), (2,1), (0,1). The second point is a true extreme vertex, yet the old algorithm removed it. The exact sign now controls the monotone chain. Signed zeros are canonicalized before coordinate sorting.
- Delaunay previously used rounded normalized coordinates, an unfiltered circumcircle determinant, a 1e-14 triangle orientation cutoff, and a 1e-12 final empty-circle allowance. A strict but shallow violation could pass the final audit. Keep normalized coordinates only for the existing nearby-site merge policy; construct and audit triangles on the original retained binary64 coordinates using certified orientation and incircle signs. The finite supertriangle retry budget and mesh validation remain.
- Incircle uses an outward-rounded interval filter followed by a fixed-point exact determinant if the filter cannot establish a strict sign. All finite binary64 values are integer multiples of 2^-1074. The determinant is homogeneous of degree four; a product needs fewer than 8392 bits, and 48 terms require fewer than 8398 bits. Two 132-limb magnitudes (8448 bits each) suffice. This covers subnormal and maximum finite values; nonfinite inputs are rejected. Exactly cocircular ties remain deterministic for a given input order.

## Validation and limits

New Rust regressions: shallow extreme vertex, signed-zero ordering, one-ULP near-cocircular diagonal, exact cocircular/subnormal/maximum-coordinate predicates, nonfinite rejection. Independent JavaScript BigInt check compared the translated 3×3 determinant against the expanded 4×4 determinant for 204 finite cases (including extreme exponents); this checks the algebra, not the compiled Rust implementation. Rust compiler and a local clone were unavailable in this execution environment, so fmt, Clippy and cargo test have not been executed here. Do not merge before CI gates pass.

Run: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --lib`.

Exactness applies to predicate signs on represented input coordinates, not construction coordinates, measured geometry or all CAD operations. The existing near-site merge tolerance, minimum mesh cross magnitude, coordinate bounds, bounded supertriangle retries and cancellation remain. This increment does not certify NURBS intersections, Boolean, offset/shell/fillet, sweep/loft, Voronoi, deformation or arbitrary mesh repair. Grasshopper comparison was not executed.
