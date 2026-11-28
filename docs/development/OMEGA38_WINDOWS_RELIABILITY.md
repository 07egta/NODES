# OMEGA 38 — Windows startup reliability (2026-11-28 JST)

## Scope and baseline

Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620`. Read AGENTS.md, CONTRIBUTING.md, architecture, status, Windows runtime documentation and relevant PR file lists. The desktop renderer is eframe 0.33.3 with Glow only; wgpu/DX12/Vulkan are not enabled. No new dependency or renderer migration in this PR. PR #92 is handling responsive workspace layout, #105 packaging and #107 GUI interaction; this PR owns startup entry-point and diagnostics.

## Work planned

- Preserve existing GUI/headless entry points and native path arguments.
- Make uppercase .MORPH paths work when opened from Explorer.
- Reduce the hard minimum viewport size that prevents fitting on compact/high-DPI displays.
- Persist bounded, best-effort startup stages before the synchronous native renderer initialization and after application creation, without logging project arguments or uploading data.
- Verify with the available checks; distinguish actual Windows/Intel Arc tests from CI.

## Initial evidence and limitations

The reviewed main CI run 46110422520 was in progress; run 46086964638 passed on an older SHA. Windows runtime documentation explicitly lists Windows 11, Windows GUI and Intel Arc as unverified. This session has a Linux container, no Rust toolchain on PATH and no direct GitHub DNS access; GitHub connector reads/writes are available. No physical Windows/Intel Arc machine is attached. No FPS, startup-time, thermal or battery measurement is claimed. Main merge belongs to MISSION 21.
