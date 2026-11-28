# OMEGA 106 — CONTROL / precision parameter editing

Base: `8cac838229071509156d560391c3d1ba5237d761`. Branch: `feat/omega106-control-inspector`.

## Scope and decisions

- Reviewed AGENTS.md, ARCHITECTURE.md, STATUS.md, the current inspector, canvas numeric controls, parameter table, expression parser and open parameter/inspector PR search. PR #143 covers shared cluster inputs, not this feature.
- Preserve existing scalar parameter storage, port keys, .morph schema, signed-zero comparisons, native egui widgets and history/evaluator paths. Do not invent runtime types for unsupported geometry or change Grasshopper data semantics.
- Reuse the bounded expression parser and OMEGA 17 dimension algebra. Literal unit suffixes are an opt-in direct-entry mode, not a change to Expression node syntax. No additional dependencies.
- Native numeric input accepts bare values in the port's existing unit. Explicit suffixes (mm/cm/m/in/inch, rad/deg) are dimension checked, with no guessing for unknown ports. Existing plain numeric precision must remain unchanged. Unit-aware functions are limited to the existing dimensional contract.
- First commit adds parser and regression tests. Next integrate the inspector and canvas, native-unit mapping and safe common selection edits.

## Validation

Source inspection only. This environment has neither cargo nor rustc; direct git network DNS is unavailable. Connector writes are available. Exact-head CI, native UI interactions, persistence and Undo/Redo must be verified before merging. No performance claims.
