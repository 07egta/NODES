# OMEGA 26 — GUI reliability / interaction audit

- Baseline main: `ec3b840e81c76aa7e426da5dc47b716ba975a98e`.
- Related open PR: [#92](https://github.com/07egta/NODES/pull/92), adaptive workspace; overlaps app.rs, not canvas.rs. Keep changes local and integrate with it by reviewing the hunks.
- Read AGENTS.md, architecture, development status, OMEGA_UI_VALIDATION.json and OMEGA_NATIVE_GUI.json. Historical GUI evidence does not establish validation of this branch.

## Findings and scope

1. Valid graphs can omit default numeric parameters. The canvas and inspector currently insert defaults using entry().or_insert() just by rendering. This changes the document and can create false dirty/history/recalculation events. Preserve sparse parameters until an actual bitwise value change.
2. Redo can leave deleted IDs selected, and history/project replacement can leave wire, note and drag state from the old snapshot. Clear transient interaction state and retain only valid selection IDs when restoring history; reset selection on new/open/recovery.
3. Add regressions for sparse numeric fields, signed zero, and restored canvas state.

## Validation limits

The chat execution environment has no cargo/rustc and cannot resolve github.com for a local checkout. Source is accessed through the GitHub connector. New regression tests are to be run by CI; do not claim that they passed before evidence exists. Native desktop, Windows/Intel Arc, high DPI and screenshots remain unverified. No merge to main.
