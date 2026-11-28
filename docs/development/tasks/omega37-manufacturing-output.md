# OMEGA 37 — Manufacturing output verification: G-code modal state

Date: 2026-11-28 JST
AI: ChatGPT; branch based on main; no direct merge.

## Scope
- Audit the actual file writers and prioritize the generated G-code modal-state boundary.
- Read AGENTS.md, CONTRIBUTING.md, architecture/status, IO notes, SolidCraft reuse notes and merged PRs #47, #48 and #64. #64 covers generated travel bounds; do not duplicate it.
- Keep existing custom scripts, node IDs, printer settings and serialized projects compatible. Reassert the body assumptions (millimeters, absolute XYZ, configured extrusion mode) after user start script; arbitrary script execution and machine offsets remain unverified.
- Add source regression coverage. No new dependencies.
- Environment: direct GitHub git networking unavailable; rustc/cargo and slicer executables absent. Use authorized GitHub writes and CI; no physical printer validation. Do not merge until checked.

## Findings and handoff
- Source-level defect: AutoPlus and Custom start scripts can leave G20/G91 active. Previously only M82/M83 and G92 E0 were restored; generated absolute millimeter coordinates could be interpreted differently. Commit 9958f9edfdfeb1c3985d8c92be797489105f08dd restores G21/G90 before E mode and adds explicit custom-script warnings.
- Regression covers Auto, AutoPlus and Custom, each with absolute and relative E; checks command ordering before BODY-START. This is source coverage, not an executed test.
- Reproduce with `cargo test --locked --lib gcode_job::tests::render_always_sets_body_units_xyz_and_extruder_mode`; then fmt, check, strict clippy, full tests and release build on Linux and Windows.
- The fix does NOT validate arbitrary custom start/end commands, G92 XYZ offsets, work coordinates, homing, firmware-specific state, collisions or physical motion. Do not claim machine safety.
- Main has binary/ASCII STL, OBJ, experimental 3MF, STEP through SolidCraft, and two G-code paths. No implemented PLY/IGES/glTF/SVG/DXF export identified in the central export enum or README. OBJ omits colors/materials, 3MF currently geometry only. External slicer/STEP compatibility remains unverified in this session.
- Further review: compare serialized binary STL normals with rounded vertices; independently parse 3MF ZIP/XML and verify object/build references, winding and cavity preservation; legacy vase path layer height/bed limits; verify custom-script coordinate offsets and full emitted motion in a bounded firmware-aware parser. Existing historical checks in STATUS.md are not new measurements.
