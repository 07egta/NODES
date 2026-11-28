# Omega curve precision checkpoint — 2026-11-27

Base: main at 7eb924ad011d8db630c7e91d1564ec5bd02805ee. Branch: fix/omega-curve-precision-20261127.
Scope: numerical correctness and bounded curve improvements in the native curve engine. Do not edit main directly or merge without verification.
PR #7's original 60-node branch passed CI, but main currently includes only the initial 40 curve adapters. PR #48 already integrates SolidCraft kernels; do not duplicate that work. Other open expansion and multi-port PRs remain independent.

## Checkpoint 1: rational degree-one arc length
The old adaptive Gauss–Legendre estimates can agree while missing an extremely concentrated speed near a tiny rational weight. Independent JavaScript reproduction for a unit segment with weights [1, 1e-12]: coarse integral ~6e-11, two half integrals ~1.21e-10; the existing 1e-9 absolute criterion accepts the latter despite exact length 1. Degree-one rational spans are straight in geometry: compute endpoint distance and use closed-form weighted parameter inversion. This also removes quadrature from polyline length and inverse. A normalized interval that cannot represent separate knot boundaries is rejected explicitly. No measured Rust performance result yet.

Added three Rust regression tests. Native compiler unavailable in this chat container; CI required. No dependencies or file format changes. No subagents exposed in this environment. Next: inspect CI, then improve curve node behavior without colliding with other PRs.
