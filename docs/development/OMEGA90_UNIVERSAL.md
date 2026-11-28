# OMEGA 90 — Native platform engineering

Base: `09a2a546d442fea822d80fb4c756a81f067a8b00`. Branch: `omega/90-universal-native`. Scope: Windows-first native desktop; no web, no new dependencies or geometry/schema rewrite.

- Existing Windows x64 and Linux x64 Check workflow and release packaging are retained. Historical CI is not evidence for this revision.
- eframe 0.33.3 uses Glow/OpenGL and X11; wgpu 27.0.1 is locked but not enabled in the NODES manifest. The viewport's CPU rasterizer is not an entire CPU-only desktop renderer.
- Native paths: retain LOCALAPPDATA/APPDATA on Windows; XDG or legacy directories on Linux; on macOS honor XDG overrides and existing legacy directories, otherwise use ~/Library/Application Support/NODES. No migration or deletion.
- Unix external slicer selection now accepts executable files without .exe and launches without a shell. Windows keeps .exe; macOS .app folder selection is not a LaunchServices integration. Unix auto-discovery is intentionally not enabled.
- Japanese fonts are bundled; actual IME, clipboard, high DPI, drag/drop, and file associations require manual native verification.
- No Rust toolchain or repository clone/network access in the local execution sandbox. Code and tests are authored, not locally compiled or executed; CI and real device testing remain pending.

## Verification and packaging plan (not results)

| Target | Native build/test gate | Render evidence | Distribution |
| --- | --- | --- | --- |
| Windows x64 | Existing Check + Japanese fixture | Manual Check dispatch | Existing portable ZIP |
| Linux x64 | Existing Check + Japanese fixture | Xvfb + Mesa llvmpipe, separate job | Existing tar.gz |
| Apple Silicon | Native platform validation, macos-15 | Manual gui_smoke input | .app in tar.gz |
| Intel macOS | Extended manual matrix | Not yet scheduled | .app in tar.gz |
| Linux ARM64 | Extended manual matrix | Not yet scheduled | Not yet packaged |
| Windows ARM64 | Extended manual matrix | Not yet scheduled | Not yet packaged |

Every row for the new revision is **unverified until its corresponding CI job actually passes**. A hosted VM is not a physical GPU test. Linux startup is intentionally independent of Windows build gating. Mac jobs reuse compiler/cache settings from Check; OMEGA 63 performance measurements are not available here. No sccache or LTO changes and no speed claims. Additional architectures run on demand to limit duplicated builds.

The smoke script uses the exact checked-in examples/01.morph, copied under a Japanese/space-containing filename, validates and evaluates it, exports to a Japanese filename, checks 5,906 vertices / 11,808 triangles, independently parses binary STL and verifies 104 mm height. Input and output digests, bounds and CLI responses are saved per OS. Floating-point STL bytes are recorded, not assumed identical across architectures. This is one mesh fixture, not comprehensive NURBS/B-rep/3MF/cluster/undo parity. Unit tests are still required, and future test fixtures should cover those cases and compare tolerances.

GUI mode launches the actual binary, waits for a complete screenshot from the existing --screenshot path and verifies PNG CRCs, dimensions and decompressed pixel length. It terminates only the spawned process, writes logs and reports failure on exit/timeout. A frame is not proof of Japanese IME interaction, semantic correctness of the displayed graph, or GPU performance. Configuration locations are isolated; no user data is removed.

macOS packaging checks native Mach-O architecture, validates Info.plist, and records linked libraries. The bundle contains the GUI, embedded Japanese font and example resources; a separate CLI is included. No Developer ID signature or Apple notarization is performed. AppImage, deb, DMG, installer, file associations and LaunchServices document-open are deferred.

## Real hardware checklist

On Windows x64 Intel Arc/Iris Xe, Radeon and GeForce; macOS Apple GPU; Linux X11 hardware GPU and Mesa llvmpipe:
1. Record exact commit, OS build, architecture, GPU/driver/OpenGL version and scaling.
2. Start the packaged binary offline; record first-frame time, peak memory and logs. Distinguish process start from rendered frame.
3. Open examples/01.morph, edit radius, undo/redo, save under a Japanese filename, restart and reopen; export STL and 3MF.
4. Test IME composition/cancel/commit, search, clipboard, file picker, keyboard shortcuts (Cmd on macOS), drag-and-drop and 100/150/200% DPI. Check screenshots, not only return codes.
5. Exercise low-complexity/wireframe mode; verify that a renderer failure is visible and does not overwrite a document.

## Renderer follow-up

Glow requires an OpenGL context even for CPU-rasterized geometry. DX12/Vulkan/Metal require enabling and auditing the currently optional wgpu dependency and explicitly selecting a renderer before event-loop creation. Do not attempt multiple winit event loops in one process; use an isolated launcher/subprocess with explicit user-selected retry, preserve data, and record selected backend. No automatic fallback is implemented here. Wayland native, software Windows/macOS drivers and virtual GPU support are unverified. See [eframe 0.33.3](https://docs.rs/crate/eframe/0.33.3/source/README.md), [wgpu 27.0.1 feature manifest](https://docs.rs/crate/wgpu/27.0.1/source/Cargo.toml.orig), [GitHub-hosted runner labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
