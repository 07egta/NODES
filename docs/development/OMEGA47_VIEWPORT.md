# OMEGA 47 — Viewport correctness

- Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (main, inspected 2026-11-28 JST).
- Scope: make picking agree with the actual viewport's software depth buffer and painter/overlay order; add deterministic headless regressions. No shared graph model, persistence or kernel changes.
- Checked AGENTS.md, architecture, STATUS, prior preview work (#16), GUI PR #107 and SolidCraft PR #48 / docs/SOLIDCRAFT_CHANGES.md. Native rendering currently uses eframe/egui glow and software rasterization, not wgpu. SolidCraft UI was deliberately not imported because of version differences; do not introduce it in this fix.
- Finding: the current picker compares average world depth even when the rasterizer compares screen-interpolated depth keys. Intersecting/slanted triangles can select the wrong node. Curve/point overlays are painted over surfaces but selection uses the same incompatible depth priority. Rounded texture dimensions also need a consistent coordinate transform.
- Native GUI, Rust toolchain and direct GitHub network access are not available in this chat container; use connector-based source changes and CI. Keep draft until validation passes. MISSION 21 owns main integration.
