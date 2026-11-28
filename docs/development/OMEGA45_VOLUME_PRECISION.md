# OMEGA 45 — numerical precision / volume contract

- Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620`, main inspected 2026-11-28 JST.
- Scope: translation-stable signed volume of closed triangle meshes in the NODES bridge and the SolidCraft faceted measurement path; no units, serialized values, tolerances or geometry are rescaled. Main merge belongs to mission 21.
- Existing PR #48 integrated SolidCraft; #100 covers unit-aware named parameters separately. Avoid overlapping that feature.
- Code inspected: AGENTS.md, architecture/status, CONTRIBUTING.md, kernel_bridge.rs, mesh_fabrication.rs, stl_export.rs, sc-geom mesh.rs, sc-kernel measure.rs/meshbody.rs, import.rs, STEP unit parser and tests. Existing fabrication/STL volume uses a local origin and compensated accumulation.
- Reproduction (Python binary64 arithmetic, same dot/cross/order as bridge, not an executed Rust test): outward cube, side 2^-9 mm, offset (500000, 500000, 500000) mm: expected 2^-27 = 7.450580596923828e-9 mm³, old 1.271565755208333 mm³, shifted 7.450580596923828e-9 mm³. Offset (999999,999999,999999): old 2.543128967285156 mm³, shifted exact expected. Coordinates and side are binary-exact. No geometric epsilon changes are needed; subtract a nearby referenced vertex before determinants and compensate the sums.
- Local environment has no Cargo/rustc and cannot resolve github.com for git clone; authenticated GitHub API is available. Rust build/test/fmt/clippy not run locally. Draft until CI is verified.
- Next: implement and add independent analytic regressions to existing tests, document exact scope and remaining risks.
