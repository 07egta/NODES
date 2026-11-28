# OMEGA 102 — Aesthetics

Base: `8cac838229071509156d560391c3d1ba5237d761` (main).

Scope: extend the existing native theme system, preserve Melt default and saved settings, no new dependencies. Related PRs #148 (visual evidence), #174 (canvas/data view), #86 (visual color), #118 (startup). Avoid editing canvas or geometry in this phase.

Design principles: quiet flat surfaces, clear foreground hierarchy, restrained accent, readable Japanese text, visible focus and selection, data types still labeled independently of color. Existing vector icons remain intact.

Verification gates: fmt, check, strict Clippy, tests, native screenshots on Windows/Linux at 100%, 150%, 200% scaling and compact/4K windows. Compare idle frame time and startup on the same hardware. Native visual and performance validation is pending; no measured improvement is claimed.
