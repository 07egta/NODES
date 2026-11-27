# MATH-EXPRESSION-FUNCTIONS

- Base: `5b69003f31af50835f240f765ea4bcac2f830f35` (main, 2026-11-27 JST).
- Branch: `feat/nodes-task-math-expression-functions`.
- Scope: bounded native Rust Expression node: useful generative-design mathematical functions, finite-input validation, function arity, UI help, independent regression tests and editable example.
- Planned files: src/expr.rs, src/script_ui.rs, src/gh_nodes.rs, docs/USER_GUIDE_JA.md, tests/expression_functions.rs, this record.
- Checked open PR changed-file lists #21, #24, #50, #52–57; no overlap with the expression evaluator/UI. #50 touches model/engine/math nodes; avoid those files.
- Existing candidates reviewed: multi-port, Null, tree matching, unknown native nodes, general curve intersection, surface trim/Boolean, mesh containment, wall thickness, GHX, viewport, expression math. Several are claimed or cross shared architecture; choose expression math as a small independent unit.
- Environment: GitHub connector works; local GitHub DNS and Cargo/Rust compiler unavailable. Rust gates to be checked by existing CI; no local build claimed.
- Status: implementation checkpoint; source and six regression tests added; Rust CI pending. No direct main push or auto-merge.

## Implementation checkpoint

- Added 16 functions to the existing Expression node, not 16 new node types.
- Reused the repository's existing exact finite affine interpolation (number_interpolation::lerp); no new dependencies or schema changes.
- Reject nonfinite AST literals/variables, out-of-range variable indices, malformed call arity, and invalid clamp/smoothstep bounds before computation.
- Smoothstep uses t*t*(3-2*t), with a scaled ratio if finite endpoint subtraction overflows.
- References: https://doc.rust-lang.org/std/primitive.f64.html and https://registry.khronos.org/SPIR-V/specs/unified1/GLSL.std.450.html (NODES uses x,lo,hi argument order).
- Editable example: examples/math/smooth-vase-radius.morph. Tests exercise numerical identities, extreme operands, signed zero, arity, domain errors, Japanese IME, graph evaluation and serialization. Tests are authored, not locally executed.
- Main advanced to 8ca85234 while working; compared intervening changes (PR infrastructure only); no overlap with planned files.
