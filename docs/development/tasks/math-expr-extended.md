# MATH-EXPR-EXTENDED

- Date: 2026-11-27 JST.
- Base: main `5b69003f31af50835f240f765ea4bcac2f830f35` (SolidCraft integration PR #48); unify/trunk `e16a496252921e529948a40f7bd4ca4e33cf727d` and develop/nodes-ultimate `7e32c9140789f9d9216c861a61f394a1b37d519e` also inspected. src/expr.rs is identical on main and unify/trunk.
- Branch: `feat/nodes-task-math-expr-extended`.
- Goal: extend the existing bounded Expression node for generative design, with explicit domain/arity checks and exact finite affine interpolation through the existing Rust numerical kernel.
- Scope: src/expr.rs, new isolated expression regression tests (inside the module), and this task record only. No node ID, graph or file schema change, new dependencies, or proprietary code.
- Coordination: the open PR list returned #21, #50, #52, #53, #54, #55, #56, #57; their changed-file lists did not include src/expr.rs. #50 touches help/script/math nodes, so avoid those files. This is a snapshot, not a guarantee that another developer cannot start later.
- Other candidates considered: unknown native node retention, Null/mixed access, multiple output integration, GHX migration (#57), numerical curve length (#56), shape morph/paneling (#55), tree inspection (#54), bio patterns (#52/#53), standard node expansion (#50), semantic project comparison (#21), SDF shell (#42), graph mapper/data recorder (#43). The first three are high impact but require shared graph/persistence changes; this task is an isolated, low-conflict way to improve math design without interfering with active work.
- Status: reserved; implementation and tests not yet written.
- Validation environment: connector access works; local git remote fails DNS; cargo/rustc absent from PATH. Do not claim tests pass until actual CI or suitable toolchain execution.
- Next: add function vocabulary, safe arity and finite operands, exact lerp reuse, analytic regression tests, example formulas; check CI and record the result. No auto-merge, direct main push, or force push.
