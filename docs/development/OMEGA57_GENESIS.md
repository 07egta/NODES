# OMEGA 57 — GENESIS v1

Base main: c1213c885647c2e708017a63f6b7bdc8f9b66620. Dedicated branch omega/57-genesis. No main merge. Reviewed AGENTS.md, architecture, status, generative kernel, evaluator, model, persistence, and related PRs #5, #53, #89, #101. No new dependencies. Existing native box mesher reused; no new SolidCraft code required for this rectangular mesh milestone.

## Implemented contract
- GenesisScopes: recursively partition a positive XY rectangle with weighted axis selection, bounded jitter and minimum side. Root ID 1, children 2i and 2i+1. Depth-first left-first output.
- GenesisScopeIds: same grammar parameters, corresponding leaf IDs. Keep settings synchronized with GenesisScopes (wire shared Number nodes if desired). Rule tracing is leaf ID only, not a full derivation log.
- GenesisRelief: item-wise rectangular scope to separated closed box mesh, low/high weighted replacement. Strict canonical four-corner polygon input. Margin ensures separation; not a connected printable solid.
- Local regeneration: region=0 disables; otherwise choose an internal subtree ID and region_seed. Ancestors and siblings retain the same seed and geometry. An invalid/terminal region is an error. One override at a time; no persistent multi-region lock map.
- Randomness: fixed integer mixing, path/channel-separated selection; relief uses coordinate bits. No global RNG. Reproducibility target is same parameters and implementation; cross-platform bit identity not tested.
- Maximum depth 10, leaf count 1024, at most 2047 visited scopes, explicit user limit and cancellation. Bounded stack; no graph cycle. Preflight accounts for leaf count/weight and per-panel mesh weight. No explicit OS-level memory cap.
- Examples: examples/genesis/facade.morph and sculpture.morph. Native editable graphs; not yet executed.

## Research directions (public references, no upstream implementation copied)
- Stiny and Gips, Shape Grammars and the Generative Specification of Painting and Sculpture (1972): production-rule concept.
- Müller et al., Procedural Modeling of Buildings (2006): split rules and scoped architectural generation, https://doi.org/10.1145/1179352.1141931
- Prusinkiewicz and Lindenmayer, The Algorithmic Beauty of Plants, https://algorithmicbotany.org/papers/#abop : symbolic rewriting / plant growth.
- Wave Function Collapse: adjacency constraints, propagation and contradiction detection; distinct future solver, not implemented here.
- Houdini public documentation, https://www.sidefx.com/docs/houdini/ : procedural graph workflows. No copied code.

## Verification and resume
GitHub source inspection and commits completed. Local Rust/cargo absent, git ls-remote failed DNS. Unit tests added but not executed. Run cargo fmt --all (inspect diff), cargo fmt --all -- --check, cargo check --all-targets --locked, cargo clippy --all-targets --locked -- -D warnings, cargo test --all-targets --locked. Check exhaustive matches, registry inventory, graph examples, save/reopen, GUI, cancellation, output STL. Do not merge until verified.
Next: graph integration tests; shared parameters; explicit rule objects/trace and visual rule definition; bounded WFC 2D/3D with contradiction detection; persistent multi-region overrides; reusable cluster rules. General graph matching, arbitrary geometry rewriting and formal DSL are NOT implemented. Mission 21 owns final merge.
