# OMEGA 28 — Taubin smoothing resource audit

- Base: `ec3b840e81c76aa7e426da5dc47b716ba975a98e` (main, 2026-11-28 JST).
- Scope: `src/mesh_tools.rs`, Taubin smoothing only. No UI, dependency, file-format, node-ID or GPU changes.
- Existing work: #83 covers preview sharing/LOD and #87 covers Data Tree batching; avoid their scope. No subagents were available in this Chat.
- Finding: each lambda/mu pass clones the entire vertex vector and allocates new storage, even though the previous pass is only read. The edge map also stays alive during smoothing after adjacency and boundary masks are built.
- Plan: reuse two buffers, preserve the exact neighbor and floating-point operation order, drop the edge map before passes, and test bitwise equivalence to the original loop, boundary/isolated vertices and cancellation. Add an opt-in paired release benchmark. Preserve all existing caps and error handling.
- Verification: this Chat has no Rust/Cargo toolchain or direct Git network; GitHub connector is available. No execution results, Windows/Intel Arc measurements, or speedup claims yet. The base allows at most 250,000 vertices / 500,000 triangles per mesh; do not bypass this to benchmark a 1M-vertex single mesh.
