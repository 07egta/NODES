# MATH-EXPRESSION-FUNCTIONS

- Base: `5b69003f31af50835f240f765ea4bcac2f830f35` (main, 2026-11-27 JST).
- Branch: `feat/nodes-task-math-expression-functions`.
- Scope: bounded native Rust Expression node: useful generative-design mathematical functions, finite-input validation, function arity, UI help, independent regression tests and editable example.
- Planned files: src/expr.rs, src/script_ui.rs, docs/USER_GUIDE_JA.md, tests/expression_functions.rs, this record.
- Checked open PR changed-file lists #21, #24, #50, #52–57; no overlap with the expression evaluator/UI. #50 touches model/engine/math nodes; avoid those files.
- Existing candidates reviewed: multi-port, Null, tree matching, unknown native nodes, general curve intersection, surface trim/Boolean, mesh containment, wall thickness, GHX, viewport, expression math. Several are claimed or cross shared architecture; choose expression math as a small independent unit.
- Environment: GitHub connector works; local GitHub DNS and Cargo/Rust compiler unavailable. Rust gates to be checked by existing CI; no local build claimed.
- Status: claimed; implementation next. No direct main push or auto-merge.
