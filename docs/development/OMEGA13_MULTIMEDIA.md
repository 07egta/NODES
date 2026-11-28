# OMEGA 13 — Multimedia & Data-Driven Geometry

2026-11-28 JST. Base: main ec3b840e81c76aa7e426da5dc47b716ba975a98e.
AI: ChatGPT. Related PR search for multimedia/image sampler/audio/height map returned no matches; main already uses image with PNG enabled and includes existing graph and mesh limits.
Read AGENTS.md, CONTRIBUTING.md, architecture and status; use existing single-output Tree/Value, stable IDs, Rust, existing mesh validation, no new dependencies. No direct main push.
Initial scope: bounded offline image/data/audio ingestion and geometry nodes, Japanese specs, independent tests and examples. No video or MP3 support initially. Prefer bounded embedded/sample pipelines or explicit local-file input; do not expose filesystem capability in untrusted Graph API. Preserve existing project schema and ports. Avoid overlapping common engine refactors. No local Rust compiler or direct GitHub clone DNS in this environment; CI is required. No performance claims without measurements.
