# OMEGA GHX migration — 2026-11-27

Base: main 7eb924ad011d8db630c7e91d1564ec5bd02805ee. Branch: feat/omega-ghx-migration-20261127.
PR #6's historical head 63abf8f had successful CI 45819461295, but is far behind main and main already includes its initial GHX inspector. Do not merge the old branch wholesale. Current main has nodes-cli and no pack registry. Other active PRs cover NURBS (#48), standard components (#50), biological patterns (#52), fabrication (#22), diagnostics (#18/#20), multi-port (#14/#17), semantic diff (#21). This branch owns focused GHX migration, CLI and tests only.

Checkpoint 1: expose existing bounded GHX inspector/converter in the supported console binary, with evaluation-before-save for conversion and integration tests of successful import and preserved destination on refusal. No schema or dependency changes. Local cargo/rustfmt unavailable; container git network DNS unavailable; GitHub connector available. CI is required, not yet verified. Next: selectively adapt modern ParameterData/arithmetic mapping from #6 to main without losing file input guards or current graph semantics.
