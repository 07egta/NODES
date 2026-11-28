# OMEGA 02 — BioMorph growth

2026-11-28 JST. Branch: feat/omega02-biomorph-growth-20261128. Base: ec3b840e81c76aa7e426da5dc47b716ba975a98e (main).
AGENTS.md, LANGUAGE_POLICY, ARCHITECTURE and STATUS reviewed. Do not merge before green CI. No direct main writes or new dependencies.

## Scope and coordination
- Main already contains FractalTree and LifeGrid. Open PR #52 adds Phyllotaxis, FibonacciSphere, ReactionDiffusion and other bio/math nodes; #53 improves that work. Do not duplicate them. This branch owns src/biomorph.rs and two new append-only IDs (BioSpaceColonization and BioCoiledShell); shared registration edits should be reconciled when merging.
- Phase 1: point-list driven space colonization (bounded deterministic growth, 2D/3D, branch curves, input seed via existing Populate), and logarithmic coiled hollow shell mesh with editable growth/rib/wall parameters.
- Use the existing graph, numeric ports, cooperative cancellation, output budgets and Data Tree matching. No new field/runtime type. No claim of botanical or mollusc species accuracy.
- SolidCraft reuse considered; these are sampled curves and explicit triangle meshes, so no B-rep kernel is required for this phase. No upstream code copied.

## Sources (algorithm/behavior only)
- Runions, Lane, Prusinkiewicz (2007), Modeling Trees with a Space Colonization Algorithm: https://doi.org/10.2312/NPH/NPH07/063-070
- SideFX Labs Tree Branch Generator: https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_branch_generator-.html
- Nervous System, Hyphae Lamps: https://n-e-r-v-o-u-s.com/projects/networks/hyphae/
- Raup (1966), Geometric Analysis of Shell Coiling: https://doi.org/10.2307/1301992
- Parametric shell morphology discussion: https://www.mdpi.com/1424-2818/15/3/431
- Prior NODES research: GENERATIVE_DESIGN_REFERENCES.md.

## Validation and handoff
GitHub connector reads and branch creation succeeded. This Chat runtime has no Rust/Cargo toolchain and direct git access failed DNS resolution. Do not report cargo fmt/clippy/test or native GUI as passed until CI or a Rust-enabled checkout executes them. No subagent runtime is exposed. Next: implement kernels, register ports and icons, add editable examples and tests, open draft PR and inspect CI.
