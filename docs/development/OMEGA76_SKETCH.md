# OMEGA 76 — visual sketch inspector (increment 1)

Base: main at 09a2a546d442fea822d80fb4c756a81f067a8b00.
The existing SolidCraft sketch solver, parser and NURBS output remain authoritative.
No new dependencies or persistence schema. Visual operations generate candidate text,
parse and solve it, then commit it to Node::script only on success.
Explicit text Apply remains available even for invalid or larger scripts.
Existing application history tracks graph changes; undo/redo integration needs manual verification.

## Operator recipes (not executed)
Select a constrained sketch node, expand the inspector, choose a drawing tool,
click positions or enter X/Y numerically. Use output mode 0 for closed regions,
mode 1 for individual curves. The canvas is XY, not the 3D viewport.

| Model | Recipe / acceptance |
|---|---|
| Bracket | Rectangle 60x40; circle at (30,20), radius 8; closed profile has a hole. Extrusion with hole must be checked independently. |
| PCB case | Outer rectangle 70x50 and inner 66x46; inspect nested profiles; wall and base operations are not part of this PR. |
| Gear outline | Use existing arc/line text commands or existing gear nodes; gear outline not demonstrated. |
| Adjustable box | Rectangle; add horizontal edge length and vertical edge length; edit values and inspect output. |
| Surface sections | Draw two separate sketch nodes; placing on separate planes and lofting is a follow-up. |
| Symmetric sculpture | Center rectangle and circle with numeric inputs; persistent symmetry is a follow-up. |
| Robot part | Rectangle and two holes, change radius and inspect the closed regions. |

## Verification
- cargo test --bin NODES sketch_ui::tests
- cargo fmt --all -- --check
- cargo check --bin NODES
- cargo test --workspace
- Manual Windows test at 100%, 125%, 150% scaling on 14-inch display:
  select sketch, draw, cycle overlapping snaps with right-click, pan with middle
  button, enter numeric positions, edit a dimension, verify invalid edits do not
  change script, undo/redo, save/reload, compare downstream output.
- Capture screenshots of actual GUI only after launching the application.
- Headless egui test exercises frame construction, not actual GPU rendering or mouse event integration.

No Rust toolchain or GUI runtime was available to the authoring session.
No tests, screenshots, manufacturing or build results are claimed as passing.
This is a draft increment, not completion of all OMEGA 76 requirements.

## Follow-up
Arc, three-point circle/arc, ellipse, spline, Bézier, slot, polygon, construction line,
constraint-preserving trim/extend/offset/fillet/pattern transforms,
point dragging with solver holds, full snap taxonomy, dimension labels and
per-entity constraint diagnostics, graph-owned driving inputs, driven outputs,
external references, explicit profile IDs, arbitrary planes, 3D viewport editing.
Coordinate with OMEGA 75 and 85 rather than duplicating solver changes.
Visual editing currently capped at 128 commands to limit synchronous solver work;
performance is unmeasured. Polyline creates consecutive segments; persistent
coincidence requires the snap checkbox, and is not a single polyline entity.
