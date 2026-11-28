# OMEGA 03: XPBD dynamics

2026-11-28 JST — work in progress. Based on current main. Existing physics.rs contains dynamic relaxation, mesh draping and soap-film form finding; keep these nodes and their IDs intact. This PR introduces a separate bounded XPBD solver, with deterministic frame replay rather than cyclic graph edges. Shared registration changes will be limited to the new node IDs; coordinate any further graph state/timeline work separately. No dependency additions or safety/structural-analysis claims. Verification pending.
