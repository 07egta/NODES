# OMEGA 33: regression forensics — reroute geometry and alignment

## Scope / start (2026-11-28 JST)

- Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (merge PR #80).
- Prior main: `ec3b840e81c76aa7e426da5dc47b716ba975a98e`.
- Last successful main workflow observed: [46086964638](https://github.com/07egta/NODES/actions/runs/46086964638) at `95382a69b5a8a49ecd17e73b1f51570ed600aa0b`; later main workflows were cancelled and the base workflow was still running when inspected. This is not a behavioral good-SHA.
- Read AGENTS.md, CONTRIBUTING.md, architecture and development status; checked open PRs, including #107 (different GUI issues).
- Goal: retain the existing right-edge/center alignment behavior when integrating the new compact reroute node. Add regression cases using real GraphCanvas methods. Do not merge to main.
- GitHub API source/diff analysis is available, but this chat environment has no Rust toolchain or direct GitHub git-clone access. Local cargo checks, executable git bisect and native screenshots are not available; distinguish source-level evidence from CI/runtime verification.

## Finding R33-001: mixed reroute/card edge alignment

- Previous main `ec3b840e81c76aa7e426da5dc47b716ba975a98e` predates Reroute (not a runnable good SHA for this new mixed-node case).
- First affected main by source comparison: `c1213c885647c2e708017a63f6b7bdc8f9b66620`. PR #68 added edge alignment using WIDTH; PR #80 introduced DOT_W=34 while the existing alignment kept WIDTH=218.
- Reproduction: create Number and Reroute at different positions, select both, choose right edge or horizontal center alignment. Their displayed edges differ by 184 or 92 world units, respectively. This is a source-derived prediction, not an executed GUI observation.
- Fix: use node_size for both axes in align_selected_edge. Existing premium_tests now exercises six axis/edge combinations with mixed card/dot. No dependencies, persistence or evaluator changes.
- Fix commit: [0fc441d](https://github.com/07egta/NODES/commit/0fc441d54d88dfe38c0fad9cdbba7938ef47863d).
- Required verification: cargo test --locked --bin NODES mixed_card_and_reroute_alignment_uses_visible_bounds; cargo fmt --all -- --check; strict clippy and all-target tests on Windows/Linux, then native screenshot. Not executed locally. For red/green verification apply only the test to the base, then test the fixed head.
- Git bisect not executed: no clone/toolchain; the relevant feature first appears in the inspected merge, and the prior main lacks the reroute node.
- Merge remains reserved for MISSION 21 after CI and review.
