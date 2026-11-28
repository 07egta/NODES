# OMEGA 90 — Native platform engineering

Base: `09a2a546d442fea822d80fb4c756a81f067a8b00`. Branch: `omega/90-universal-native`. Scope: Windows-first native desktop; no web, no new dependencies or geometry/schema rewrite.

- Existing Windows x64 and Linux x64 Check workflow and release packaging are retained. Historical CI is not evidence for this revision.
- eframe 0.33.3 uses Glow/OpenGL and X11; wgpu 27.0.1 is locked but not enabled in the NODES manifest. The viewport's CPU rasterizer is not an entire CPU-only desktop renderer.
- Native paths: retain LOCALAPPDATA/APPDATA on Windows; XDG or legacy directories on Linux; on macOS honor XDG overrides and existing legacy directories, otherwise use ~/Library/Application Support/NODES. No migration or deletion.
- Unix external slicer selection now accepts executable files without .exe and launches without a shell. Windows keeps .exe; macOS .app folder selection is not a LaunchServices integration. Unix auto-discovery is intentionally not enabled.
- Japanese fonts are bundled; actual IME, clipboard, high DPI, drag/drop, and file associations require manual native verification.
- No Rust toolchain or repository clone/network access in the local execution sandbox. Code and tests are authored, not locally compiled or executed; CI and real device testing remain pending.
