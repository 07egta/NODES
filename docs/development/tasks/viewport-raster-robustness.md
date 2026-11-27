# VIEWPORT-RASTER-ROBUSTNESS

- Base: `8ca85234aac0c2abd4c158e5e70f6e3505d0afff` (main, 2026-11-27 JST).
- Branch: `feat/nodes-task-viewport-raster-robustness`.
- Scope: `src/raster.rs` only (software preview rasterization and regression tests). No geometry-kernel, graph, or serialization changes.
- Open PR file lists #21, #50, #52–57, #64 do not modify this file. Other task branches include fabrication and expression work. Recheck before integration.
- Problem: finite f32 projected positions can overflow edge products, large offscreen triangles are unnecessarily evaluated, and independently interpolated constant depth/color can round away from the original values.
- Plan: preserve ordinary f32 hot path where safe; robust fallback for unusually large projected triangles, exact constant attributes, pixel-center clipping; add tests for both orientations, depth ties, extreme finite positions and offscreen bounds.
- Status: claimed; implementation pending. Cargo/rustc absent in current container; GitHub CI required. No completed Rust checks claimed.
