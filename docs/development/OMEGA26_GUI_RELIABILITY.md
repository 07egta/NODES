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

## Implementation checkpoint

- 3769bf3: canvas numeric widgets use local values and write only when IEEE bits change; restore_graph clears stale gesture state and prunes selection. Added direct state regression.
- 3daff69: inspector uses the same non-mutating display approach; undo/redo and new/open/recovery clear old interaction state; recovery resets history and preview.
- 3a121f1: headless egui regressions cover sparse default parameters across zoom/repaint, explicit signed-zero edit, and 120 nodes across 30 panning/idle frames.

Manual reproduction (not executed here):
- Open a valid v1 project with an omitted Number value and omitted Helix inputs. Inspect/zoom/pan; no parameter insertion, dirty marker or undo checkpoint should occur. Edit a number explicitly; only that key should be inserted.
- Add a node, undo, redo, select it, undo again; start a wire and perform history restoration. No deleted ID or old wire should remain active.
- Start editing a note in one project, then open another project with notes at the same index; editing must not carry over. Recover a project and check that preview/history belong only to the recovered document.

Pending: CI fmt/clippy/test/build, native GUI verification, Windows/Intel Arc and real IME/DPI coverage. This is a focused repair, not exhaustive UI certification.
