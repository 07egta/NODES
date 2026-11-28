# OMEGA 30 — independent release audit, 2026-11-28 JST

## Evidence boundary

Audited main: `ec3b840e81c76aa7e426da5dc47b716ba975a98e` (merge #56).
This is a fixed snapshot, not a claim about subsequent main or this PR's head.
AGENTS.md, README, ROADMAP, NODE_COVERAGE, TEST_RESULTS, STATUS, Cargo manifests, CI workflow, registry, inventory generator, shelf/graph operations and existing test sources were inspected. PR collection: 22 open / 47 closed, including merged, at intake; PR metadata was reviewed, not every patch. No other PR was changed or merged.

## Current evidence

- [Check 46107335381](https://github.com/07egta/NODES/actions/runs/46107335381): Linux job 140862913362 completed successfully. Actual log: 683 passed, 0 failed, 5 ignored, 62 Rust test summaries; fmt, locked check, strict Clippy, dependency audit and release build passed. Windows job 140866989168: test step succeeded, release build in progress at observation; log unavailable for counts.
- Prior main `95382a69b5a8a49ecd17e73b1f51570ed600aa0b`, run 46086964638: Linux 655 passed, 0 failed, 5 ignored, 61 summaries; both jobs succeeded. Six subsequent runs were cancelled. Do not transfer older pass counts to current source.
- Source enumeration: 397 unique entries in NodeKind::all, 398 enum variants including definition-only Cluster; compiled registry metadata test passed in the exact Linux CI. Committed inventory has 299 rows, null source_revision; README says 370; NODE_COVERAGE and ROADMAP describe older 298+1. These are not full verification counts. Regenerate with cargo run --locked --example coverage_inventory -- --source-revision <full SHA> on a clean checkout, rather than fabricating metadata.
- CI log confirms registry uniqueness/metadata/project serialization, category grouping, inventory generator contract and editable example fixture coverage tests passed. These do not establish that every node has correct numerical outputs, native UI behavior, search, file export or GH oracle parity.

## Findings and disposition

| Finding | Status | Action |
|---|---|---|
| Shelf suggestion may leave a node and consumed ID on failed connection | BROKEN by static inspection of audited source; fix IMPLEMENTED, execution pending | Commit 51497bc adds rollback and two regression tests. Run cargo test --locked --bin NODES node_shelf::tests. |
| Registry inventory stale | BROKEN as current inventory, historical data preserved | Regenerate after integration; current source count 398. |
| Windows compact ZIP includes NODES.exe but not nodes-cli.exe, unlike development artifact | PARTIAL packaging | Coordinate with CI owner (#83 touches workflow); decide and document package contract, test extracted ZIP. |
| Workspace gates omit --workspace | PARTIAL test scope | Cargo.toml has root package plus SolidCraft workspace members. Run cargo test --workspace --all-targets --locked and corresponding check/clippy on suitable runner; root-only CI is not standalone kernel test evidence. |
| Main protection disabled and recent runs cancelled | Release-process risk | Integration owner should consider required checks and merge queue; no settings modified. |

## Node and workflow status

VERIFIED requires recorded expected outputs and applicable UI/search/add/connect/parameter/error/save/reopen/history tests on exact source. IMPLEMENTED means implementation observed but incomplete verification; PARTIAL means a known limitation; BROKEN means demonstrated defect or logically established code failure (label static vs executed); STUB means placeholder confirmed; MISSING means absent implementation confirmed; BLOCKED means explicit dependency or unavailable test environment. No full-node VERIFIED count is asserted; GH oracle VERIFIED is separate.

Source fixtures cover vase, Voronoi lamp (curve output, not a verified solid lamp), Gyroid, organic sculpture, facade, mesh fabrication and NURBS surfaces. Existing suite checks parameter edits, reevaluation, save/reload, and selected STL round trips. Native GUI and end-to-end export for each requested work remain unverified; jewelry, origami, mechanism/gears, multimedia and multicolor print require targeted evidence. Open feature PRs are not main functionality.

## Execution constraints and release decision

Local container has no cargo executable on PATH and cannot resolve github.com; GitHub connector was used to read source and CI logs and publish focused fixes. No local Rust execution, Windows hardware, Intel GPU, DPI/locale inspection, clean-machine offline test, package download/inspection or physical print was performed. Subagent execution was unavailable. New PR tests not executed at this checkpoint. HOLD release approval pending exact integrated SHA checks and product acceptance tests.

Next: (1) run regression tests and full gates on this PR; (2) regenerate inventory with source SHA; (3) verify category-by-category default/invalid inputs and independent numerical expectations; (4) add native GUI search/history and Windows package smoke evidence; (5) execute standalone SolidCraft workspace tests; (6) audit newly merged PR patches and update evidence without rewriting historical records.
