# PROJECT LIGHTSPEED — startup performance

Base inspected: 8cac838229071509156d560391c3d1ba5237d761 (main), 2026-11-28 JST.

## Scope
- Existing desktop renderer: eframe 0.33.3, Glow/OpenGL, not wgpu. #118 owns native renderer startup/reliability and process-entry diagnostics; #105 owns Windows package checks. Do not change main.rs or add a renderer/dependency here.
- Constructor currently reads settings.txt separately for theme, two shelf lists, tutorial and grid; it creates a default demo even when an explicit project loads successfully. Recovery modal clones the entire recovery project every frame. Graph evaluation already runs on a worker.
- Implement one startup settings snapshot, avoid redundant demo creation and recovery cloning, cache repeated shelf key/category scans where appropriate, and add opt-in app-side timing. Preserve saved settings, Japanese UI, error behavior, CLI/screenshot semantics and recovery choice.
- Distinguish process entry, native window/renderer initialization, constructor, first complete UI update, first applied graph evaluation, OS presentation and actual input response. App-side measurements are proxies, not presentation or physical input latency.
- Windows first display <= 1 s is a challenge target, not an established result. Cold/warm/first-run and large .morph are separate workloads. No physical Windows/Intel Arc access; local Cargo/rustc not found on PATH and direct GitHub DNS failed. Source changes via connector, CI pending.

AI: ChatGPT GPT-6.2. No main merge until normal gates pass.
