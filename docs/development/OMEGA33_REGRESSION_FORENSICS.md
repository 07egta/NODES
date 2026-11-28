# OMEGA 33: regression forensics — reroute geometry and alignment

## Scope / start (2026-11-28 JST)

- Base: `c1213c885647c2e708017a63f6b7bdc8f9b66620` (merge PR #80).
- Prior main: `ec3b840e81c76aa7e426da5dc47b716ba975a98e`.
- Last successful main workflow observed: [46086964638](https://github.com/07egta/NODES/actions/runs/46086964638) at `95382a69b5a8a49ecd17e73b1f51570ed600aa0b`; later main workflows were cancelled and the base workflow was still running when inspected. This is not a behavioral good-SHA.
- Read AGENTS.md, CONTRIBUTING.md, architecture and development status; checked open PRs, including #107 (different GUI issues).
- Goal: retain the existing right-edge/center alignment behavior when integrating the new compact reroute node. Add regression cases using real GraphCanvas methods. Do not merge to main.
- GitHub API source/diff analysis is available, but this chat environment has no Rust toolchain or direct GitHub git-clone access. Local cargo checks, executable git bisect and native screenshots are not available; distinguish source-level evidence from CI/runtime verification.
