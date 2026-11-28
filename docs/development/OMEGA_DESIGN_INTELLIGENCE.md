# OMEGA 04 — AI Design Intelligence

2026-11-28 JST. Base: ec3b840e81c76aa7e426da5dc47b716ba975a98e.
Reviewed AGENTS.md, ARCHITECTURE, STATUS, Graph API, CLI and open PRs #52, #53, #55, #57, #65, #80, #81, #83, #84. Existing restricted Graph API, diagnostics and semantic diff must be reused, not replaced.

## First slice

Implement an offline bounded parameter exploration library and CLI: grid and seeded random search, explicit scalar objectives and constraints, Pareto front of evaluated feasible candidates, and replayable Graph API edits. Retain scalar data, not geometry per candidate. No new dependencies, saved-project schema changes, AI execution, network services or changes to main. Existing evaluator and SolidCraft-backed graph nodes provide geometry and metrics.

Public behavior references (independent implementation):
- Galapagos: slider genome and numeric fitness, https://www.cs.cmu.edu/afs/cs/academic/class/15394-s24/lectures/galapagos/galapagos.html
- Design Space Exploration: separate variables, objectives, solution history and Pareto front, https://grasshopperdocs.com/components/designspaceexploration/mOO.html

## Validation and handoff

Local container does not have cargo/rustc or direct GitHub DNS; GitHub connector used for publication. No subagent tool is available in this chat. Do not claim tests or speed measurements until CI is verified. Start with library and CLI tests, fmt, strict Clippy, and full CI. Native Design Explorer UI, thumbnails, natural language/vision/local LLM/MCP, adaptive optimizers (GA/DE/PSO/SA/NSGA-II/Bayesian) are subsequent independent slices.
