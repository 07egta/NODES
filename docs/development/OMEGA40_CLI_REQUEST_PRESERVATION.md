# OMEGA 40 — CLI request preservation

- Owner: ChatGPT / OMEGA 40, 2026-11-28 JST.
- Baseline: `c1213c885647c2e708017a63f6b7bdc8f9b66620`.
- Branch: `fix/omega40-cli-request-preservation-20261128`.
- Scope: `src/bin/nodes-cli.rs` (build/apply request-destination guard only), new `tests/cli_request_preservation.rs`.
- Coordination: #57 adds GHX commands to the same CLI file; preserve both changes. #108 owns native persistence and #105 owns artifact CI; do not change their files. Export verification is separate.
- Finding: build/apply read valid request JSON then atomically save the project to the given output path without preventing output from naming the request file. A typo can destroy a reusable input request. The intentional apply-to-input-project flow must remain supported.
- Plan: refuse same request/destination paths, including canonical aliases, before graph evaluation or save; add executable CLI regressions for original bytes and valid in-place project editing.
- Validation: Rust/Cargo and direct GitHub access from shell unavailable; GitHub connector available. All Rust checks and actual CLI runs pending CI. No main merge.
