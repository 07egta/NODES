# OMEGA 17 — named dimension parameters (phase 1)

Starting main: `ec3b840e81c76aa7e426da5dc47b716ba975a98e`. Branch: `feat/omega17-named-parameters`.

## Scope and reuse

- [AGENTS.md](../../AGENTS.md), [ROADMAP](../../ROADMAP.md), [architecture](../ARCHITECTURE.md), [STATUS](STATUS.md) and the open PR list were reviewed.
- [PR #48](https://github.com/07egta/NODES/pull/48) already integrates SolidCraft geometry and kernel.
- [PR #81](https://github.com/07egta/NODES/pull/81), head `562fc8eef99d7c7dba269d468a3695761e23fd82`, is developing the sketch crate and sketch node. Its public solver already has local finite-difference Jacobians, dense damped Gauss–Newton/LM per connected component, rank/DOF analysis, redundant constraints, continuation and branch/orientation diagnostics. The original source is the permissively licensed SolidCraft in [melt3D](https://github.com/07egta/melt3D/tree/7f18150bdcc156944fee71617705d66213757dab/crates/sketch). No solver rewrite, GPL-family code, or additional dependencies are introduced here.
- The PR #81 sketch text currently consumes numeric tokens and does not expose a persistent named dimension table. Work here is independent of its model, evaluator, sketch files and ports. PR #65 changes expression functions; the parameter layer reuses the existing parser and rejects future functions until a dimensional contract is defined.
- Fusion's [parameter documentation](https://help.autodesk.com/view/fusion360/ENU/?contextId=SLD-MODIFY-PARAMETERS) motivates unique names, units, expressions and re-use. No product implementation is copied.

## Implementation

A versioned, separate JSON parameter table resolves named quantities in a deterministic dependency order. Literals have a declared display unit; formulas use explicit references such as `$width * 2 + $clearance`. Internal values are mm, radians, or dimensionless; dimensional addition/subtraction and result units must agree. Multiplication/division and integer powers may temporarily produce derived dimensions. Up to four distinct references per formula use the existing bounded expression parser. An ordinary numeric literal is dimensionless, so a length offset must be another length parameter. No inline unit suffixes in phase 1.

An explicit binding applies a value in the selected target unit to an existing, enabled, unconnected Number node, checking its ID, exact name, input bounds and duplicate targets. The caller can checkpoint the original project once; failed resolution, invalid bindings or cancellation cannot partially modify the graph. The binding's target unit is explicit because Number ports do not carry units. The table does not alter .morph schema or existing evaluation semantics. A graph cycle remains an error independently of simultaneous equations inside the sketch solver.

## Validation and follow-up

This environment has no Rust toolchain and no direct git network access; GitHub connector writes are available. Source inspection is not a compile/test result. This branch must remain unmerged until exact-head fmt, Clippy and tests pass. Add regression cases for conversion, dimensions, dependencies, cycles, bounds, cancellation, binding rollback, save/reopen and history. Then connect the table to the sketch node after #81 merges, using explicit driving dimension IDs and SolidCraft's solve-after-edit analysis; preserve branch stability and report redundancy/DOF to Japanese UI. Table UI, design configuration selector, persistent project associations and live sketch editing are not implemented in this phase.
