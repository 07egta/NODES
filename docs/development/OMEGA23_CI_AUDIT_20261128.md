# OMEGA 23: CI audit and release artifact verification (2026-11-28 JST)

Base: `ec3b840e81c76aa7e426da5dc47b716ba975a98e`. AGENTS.md and CONTRIBUTING.md reviewed. Do not merge directly. Scope: unify Windows ZIP contents with the existing staged development package and verify released CLI and archive integrity on CI; preserve all existing gates. Python/PowerShell are CI-only helpers, not application dependencies.

## Evidence from GitHub Actions

- [Main Check 46107335381](https://github.com/07egta/NODES/actions/runs/46107335381): Linux succeeded; Windows release build in progress at inspection. Earlier [main Check 46086964638](https://github.com/07egta/NODES/actions/runs/46086964638) succeeded on both OSs (different SHA).
- Actual job logs: Rustfmt errors on [#97](https://github.com/07egta/NODES/actions/runs/46109647862/job/140870098959), [#95](https://github.com/07egta/NODES/actions/runs/46109626497/job/140870073109), [#91](https://github.com/07egta/NODES/actions/runs/46109658155/job/140870158196), [#90](https://github.com/07egta/NODES/actions/runs/46109606470/job/140870010949), [#89](https://github.com/07egta/NODES/actions/runs/46109576496/job/140869877196), [#88](https://github.com/07egta/NODES/actions/runs/46109567179/job/140869845299), [#86](https://github.com/07egta/NODES/actions/runs/46109581090/job/140869907799). Later gates were skipped; compilation is not established. These active development branches are left to their owners to avoid concurrent overwrites.
- [#55](https://github.com/07egta/NODES/actions/runs/46107378653/job/140863046514): `random_in_range_mixes_never_panic`, MeshDimensions random round 53, 213.884436795s against existing 60s guard. The reported target is not necessarily the expensive upstream node. Reproduce with the failing SHA and graph/individual node timing before changing geometry; do not relax the guard.
- Historical G-code example fixture failure [46084455815](https://github.com/07egta/NODES/actions/runs/46084455815) followed by success [46085143861](https://github.com/07egta/NODES/actions/runs/46085143861); physics mesh tests failure [46084260874](https://github.com/07egta/NODES/actions/runs/46084260874) followed by success [46086926753](https://github.com/07egta/NODES/actions/runs/46086926753). These results apply only to their respective commits.

## Confirmed packaging defect

`.github/workflows/windows.yml` creates two different Windows artifacts. The first stages NODES.exe, nodes-cli.exe, Cargo.lock, all docs, examples, licenses, and BUILD-INFO.txt. The second constructs a separate ZIP from a smaller list: CLI, lockfile, build provenance and most docs are absent; some copy errors are suppressed. Both Linux and Windows release build steps compile binaries but do not invoke the packaged CLI. This can produce a successful build with an incomplete distribution.

Open PR #83 touches the workflow only to add an evaluator timing step; preserve it during integration. No other inspected open PR (first 100 changed files per PR) touched the workflow. Keep fixes on this separate branch and document exact CI SHA.

## Pending

Implement shared package-content verification, execute the packaged CLI on both OSs, and verify the Windows ZIP before uploading it. Existing Linux/Windows gates remain mandatory. Actual GUI/GPU/DLL and physical printer validation are separate; this work does not claim these. No new Rust dependency or test removal.
