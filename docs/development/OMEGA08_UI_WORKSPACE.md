# OMEGA 08 — adaptive workspace

2026-11-28 JST. Base: current main. Scope: app-level workspace presentation and compact navigation, without changing Graph, persistence, evaluation or node IDs. Preserve existing search, tutorial, favorite and recovery functionality. Do not duplicate reroute work (#80) or preview LOD work (#83).

## Design references

- Fusion workspaces: https://help.autodesk.com/cloudhelp/ENU/Fusion-GetStarted/files/GS-WORKSPACES.htm — context-specific tool visibility.
- Houdini tab menu and network editor: https://www.sidefx.com/docs/houdini/basics/tabmenu.html and https://www.sidefx.com/docs/houdini/network/nodes.html — searchable, context-sensitive insertion. NODES already has its own palette and type filtering.

## Scope and validation

Use existing native egui panels, existing template and inspector workflows, and a view-only workspace choice. Keep graph, history and saved files unchanged by mode switches. Do not label an offline helper as AI generation. Prefer a small, reviewable PR. No dependency changes. Current environment: GitHub connector can read/write; local git network and Rust toolchain are unavailable. CI and actual Windows GUI/DPI checks remain required. No screenshot or performance result is claimed.
