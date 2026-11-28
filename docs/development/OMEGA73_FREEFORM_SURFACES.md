# OMEGA 73 — Freeform surface construction

Base: main 09a2a546d442fea822d80fb4c756a81f067a8b00.
Related work: PR #139 (blend/match/trim/continuity) and #137 (component coverage).
OMEGA 71/72 PRs were not found by title search at the time of checkout.
Reuse the existing NURBS surface basis and validation rather than adding another kernel.
First slice: bounded clamped tensor-product control grids and interpolating point grids,
with a graph adapter if safe, regression tests and a reproducible example.
Do not register unfinished nodes, modify the shared B-rep layer or claim Rhino parity.
Local Rust execution is unavailable in this environment; keep draft until CI is verified.
Public functional references: https://docs.mcneel.com/rhino/9/help/en-us/commands/srfcontrolptgrid.htm
and https://docs.mcneel.com/rhino/9mac/help/en-us/commands/srfptgrid.htm .
