# OMEGA 58 — REVERSE

Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (main). Scope: a standalone Rust reconstruction API, using existing native node types and evaluator; no persistence schema changes, external AI, or new dependencies. Do not touch the shared registry or other missions' branches. Initial search found no open PR explicitly matching Geometry-to-Graph / inverse CAD / OMEGA 58. Recheck before merge.

First milestone: infer axis-aligned analytic mesh primitives and translation, build editable candidate graphs, evaluate and reject poor fits. No import-mesh fallback. Candidate history is inferred, not original. Follow-up: robust fitting, arbitrary axes, CSG, sketch/extrude, symmetry and UI. See InverseCSG (Du et al., 2018), https://inversecsg.csail.mit.edu/ (conceptual reference only). No upstream code copied.

Local environment has no Cargo/rustc/rustfmt on PATH and no direct GitHub DNS access; connector commits and CI are available. Do not claim unrun checks. Main merge reserved for integration mission.
