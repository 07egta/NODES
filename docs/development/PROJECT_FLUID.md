# PROJECT FLUID — responsive rendering

Base: `8cac838229071509156d560391c3d1ba5237d761` (main, 2026-11-28 JST).

Scope: existing Glow/software viewport and canvas hot paths; preserve all interactions, geometry, exports, theme and file format. No wgpu migration or new dependency. PR #83 and #122 are merged; preserve their LOD and depth/pick correctness. Coordinate canvas changes with #174 (data probes) and #176 (wire gestures), and viewport changes with OMEGA 104. First reduce camera-motion raster cost, hoist camera-constant work, and eliminate repeated output-shape scans for visible fan-out wires. Add headless regression and opt-in timing workloads. No direct main writes.

Validation environment: connector reads work; container has no cargo/rustc and git transport fails DNS. No native GUI/GPU measurement possible locally. Record actual checks and keep draft pending CI. AI: ChatGPT GPT-6.2.
