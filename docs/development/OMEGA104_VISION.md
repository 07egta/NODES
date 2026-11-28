# OMEGA 104 — VISION

Base: 8cac838229071509156d560391c3d1ba5237d761 (main, 2026-11-28 JST).

## Audit and scope

- Read AGENTS.md, CONTRIBUTING.md, ARCHITECTURE.md, STATUS.md, Cargo.toml, viewport.rs, raster.rs, app.rs, cage_edit.rs and relevant cache tests.
- PR #122 is merged: preserve raster/picking depth agreement, first-wins ties and overlay order. PR #118 is open and owns startup/compact viewport; avoid main.rs. PR #48 is merged: existing vendored SolidCraft geometry is MIT OR Apache-2.0; docs/SOLIDCRAFT_CHANGES.md excludes its egui 0.36/wgpu UI from this egui 0.33.3 project.
- Current renderer is Glow/OpenGL + CPU raster, not wgpu. Do not switch renderer/dependencies in this focused interaction PR. GPU migration needs independent backend, licensing, CI, fallback and native benchmark work.
- Existing camera: orbit, pan, zoom around the target plane, orthographic/perspective, fit all, four view buttons. Existing picks identify preview node IDs, not persistent B-rep entities. Cage editing is a specialized graph-backed interaction; it is not a general transform gizmo.
- Plan: improve framing (including perspective and selection), navigation presets and orthogonal views; preserve scene caching, current/pending preview guards, graph IDs and existing cage workflow. Evaluate bounded pick indexing before hover; no unbounded per-frame scan. Native Japanese UI and no new dependencies.
- Unfinished: generalized subobject identity, occluded/region selection, full gizmos and graph transactions, GPU renderer, advanced display modes. These are separate acceptance gates, not claims of completion.
- Environment: direct GitHub DNS unavailable; Rust/Cargo/rustfmt not present on PATH. Connector source edits and CI only. Do not merge without current CI and actual GUI verification.

AI: ChatGPT. Integration via focused draft PR, never direct main writes.
