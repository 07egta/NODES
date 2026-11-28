# OMEGA 101 — INTUITION

Base: `8cac838229071509156d560391c3d1ba5237d761` (main, 2026-11-28 JST).

## Scope and coordination

Keep native Rust/egui, Japanese-first UI and the existing single-output graph format. No new dependency, no persistence migration. Main already has drag/click wiring, reroute points, selection move, duplicate, alignment, distribution, grid snap, compatible palette search and Undo/Redo. Focus this PR on gesture correctness, cycle-aware wire candidates and a safe reroute insertion. #174 touches canvas/data_view for data probes, #173 touches app numeric editors, #145 touches sketch UI, and #79 tracks broader shortcuts; avoid replacing those features. Work in this branch only; no merge before CI and native visual evidence.

## Validation plan

Add focused canvas regression tests (cycles, type mismatch, deterministic targeting, cancel, transactional reroute as applicable). Run fmt, strict Clippy, tests and native GUI checks where possible. Record actual evidence and unresolved limitations, including workflow click/time measurements.
