# Issue #36: numeric editor expressions

Base: `8cac838229071509156d560391c3d1ba5237d761`. Reuse existing `expr` and numeric binding; preserve literal bit patterns, finite bounds, integer handling and saved `.morph` format. Add constant-only expression entry for both Inspector and canvas; no new dependency. Scope avoids pending graph and geometry PRs. Native UI and full CI must be verified before closing #36. Local Rust and direct GitHub network are unavailable.

Implementation: literal-first parsing followed by bounded constant expression evaluation using the existing parser. Nonfinite literals and graph variables are rejected. Both editors share the parser; existing setters enforce bounds and integer rounding. Added headless widget regressions for arithmetic, full-width input, invalid expressions, rounding and bounds. Rust tests and actual screenshots are pending; no merge or issue closure.
