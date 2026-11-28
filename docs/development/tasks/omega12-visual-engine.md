# OMEGA 12 — Visual engine checkpoint

Started 2026-11-28 JST. This branch targets main only.

## Scope and coordination

- Main currently uses eframe Glow/OpenGL and a bounded software depth buffer. Do not replace the renderer or introduce wgpu as a dependency in this checkpoint.
- PR #83 changes viewport performance; keep visual controls and shading local and coordinate any conflicts before merge.
- PR #84 owns PLY/3MF and fabrication exports. Do not alter export implementations or the saved graph schema here.
- PR #81 owns NURBS diagnostic shading. Do not duplicate zebra or draft analysis.
- Implement renderer-independent color science and lightweight preview-only shading modes. Keep existing default/selection/overhang/wireframe behavior.
- No new runtime dependencies. No main push or merge until CI and GUI checks.

## Initial evidence

Read AGENTS.md, CONTRIBUTING.md, architecture/status, Cargo.toml, src/lib.rs, src/raster.rs and relevant viewport/model/main sections. Local container cannot resolve github.com and cargo/rustc are not on PATH; GitHub connector is available. Build, GUI, Intel GPU timing and attribute export round-trip are not yet verified.

## Next checkpoint

Implement independent color utilities and integration into the existing preview; add bounded tests and report the exact validation status. Full PBR, HDRI, actual material graph ports, attribute transfer/export, transparent PNG and turntable output remain separate work.
