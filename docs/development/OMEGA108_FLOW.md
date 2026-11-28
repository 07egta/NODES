# OMEGA 108 — FLOW

Base: `8cac838229071509156d560391c3d1ba5237d761` (main, 2026-11-28 JST).

## Scope

Extend the existing OMEGA 08 three-mode workspace, egui panels, node shelf, search, favorites, templates and tutorial rather than replacing them. Add purpose-specific presets, independent per-preset panel preferences, explicit graph/preview focus, and a way to collapse or float the inspector. Recommend categories without hiding the full catalog or moving panels automatically. Keep project data, undo history, graph evaluation, node IDs and dependencies unchanged. Japanese-first native UI.

Open PRs #145 (sketch inspector) and #176 (wire gestures) may touch neighboring UI; avoid modifying their geometry/gesture logic.

## Validation

The current connector can read and write GitHub, but this container cannot resolve github.com and has no cargo/rustfmt on PATH. Do not claim build, CI, screenshot or manual GUI success until observed. Validate Linux/Windows gates and actual narrow-screen GUI before merge. Arbitrary drag docking, independent native multi-monitor windows, and configurable shortcut bindings are separate follow-ups if not implemented here.
