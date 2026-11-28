# Issue #36: numeric editor expressions

Base: `8cac838229071509156d560391c3d1ba5237d761`. Reuse existing `expr` and numeric binding; preserve literal bit patterns, finite bounds, integer handling and saved `.morph` format. Add constant-only expression entry for both Inspector and canvas; no new dependency. Scope avoids pending graph and geometry PRs. Native UI and full CI must be verified before closing #36. Local Rust and direct GitHub network are unavailable.
