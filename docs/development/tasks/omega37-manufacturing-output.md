# OMEGA 37 — Manufacturing output verification: G-code modal state

Date: 2026-11-28 JST
AI: ChatGPT; branch based on main; no direct merge.

## Scope
- Audit the actual file writers and prioritize the generated G-code modal-state boundary.
- Read AGENTS.md, CONTRIBUTING.md, architecture/status, IO notes, SolidCraft reuse notes and merged PRs #47, #48 and #64. #64 covers generated travel bounds; do not duplicate it.
- Keep existing custom scripts, node IDs, printer settings and serialized projects compatible. Reassert the body assumptions (millimeters, absolute XYZ, configured extrusion mode) after user start script; arbitrary script execution and machine offsets remain unverified.
- Add source regression coverage. No new dependencies.
- Environment: direct GitHub git networking unavailable; rustc/cargo and slicer executables absent. Use authorized GitHub writes and CI; no physical printer validation. Do not merge until checked.
