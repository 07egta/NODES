# OMEGA 44 — evaluation cancellation and worker lifecycle

- Start: 2026-11-28 JST; base `c1213c885647c2e708017a63f6b7bdc8f9b66620`.
- AI: ChatGPT. Do not merge this branch; MISSION 21 owns integration.
- Read AGENTS.md, CONTRIBUTING.md, ARCHITECTURE.md and development STATUS.md. Main uses a latest-only worker mailbox and scoped parallel evaluation (up to eight threads).
- Focus: cooperative cancellation around cache-only/disabled levels and final publication; deterministic single/multi-thread comparisons; worker disconnect handling where it can be kept independent of UI work. Related open PRs #83, #107, #110 change app/viewport code; avoid unrelated shared changes and coordinate with integration.
- Local environment has no Rust/Cargo on PATH and cannot resolve github.com for git checkout; use authorized GitHub API and CI. No successful Rust tests claimed yet.
