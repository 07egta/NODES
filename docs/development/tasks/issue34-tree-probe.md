# UX: bounded live data-tree probe and pin (#34)

Base: `8cac838229071509156d560391c3d1ba5237d761`.

Hover on wire midpoint or output for bounded branch and value preview; pin one source in a live window; reset transient ID on graph restore and deletion. No schema changes. Existing data_view is reused. Related #34.

Local cargo/rustc and direct git access unavailable. Keep draft until CI and relevant native verification.

Implemented source-port and wire-midpoint bounded tooltip, up to four branches and three values each with counts and truncation. Right-click pins one live source; deletion/graph restore clears the pin, and stale evaluations are not shown as current. Tests cover empty branches, truncation, and restore. General curve-hit testing beyond midpoint and native screenshot remain unverified; no closure.
