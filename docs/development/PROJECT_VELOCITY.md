# PROJECT VELOCITY — Item matching allocation audit

Base: 8cac838229071509156d560391c3d1ba5237d761 (main, observed 2026-11-28 JST).

## Scope and coordination
- First increment: src/data_tree.rs item matching and its hot call sites in src/engine.rs and cross_reference. No new dependencies, no changed numerical operations, persisted schema, item/weight limits or matching semantics.
- Reuse the existing evaluator, level parallelism and cache. Do not duplicate the GPU work in #175 or the structural compiler in #132; #87 is a separate tree-batching proposal (observed changed file: its development note).
- Source audit: ItemMatches::next currently creates a Vec of borrowed arguments for every emitted combination. The evaluator immediately consumes that Vec; it does not need a fresh allocation for each combination.
- Approach: reusable caller-owned buffer, while retaining the allocating Iterator API. Verify independently expected Longest/Shortest/Cross order, empty inputs, bounds, signed zero and buffer reuse.
- Benchmark: paired release-mode measurements of the two APIs; report input size, samples and elapsed time. Allocation opportunity is not a measured allocator count or speedup.

## Evidence status
Historical docs/PERFORMANCE.md measurements are not measurements of this change. Local checkout fails DNS resolution; cargo/rustc are unavailable in this environment. CI, Windows/Intel Core Ultra and peak RSS are not yet verified. Keep draft until checked.

AI: ChatGPT.
