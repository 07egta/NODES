# API editing checkpoint (draft)

- Branch: `develop/nodes-ultimate-api-edits`
- Original base: `6349577399dd484a90fde5afe77ee173f8f9444a`
- Initial checkpoint: `eead4965f6d4741c3f83b5a764e7f6fe14be11dc`
- Integrated upstream snapshot: `c52e850ed01a82461557fab9182778092151db2d`
- Primary integration PR: https://github.com/07egta/NODES/pull/11

Implemented transaction-safe disconnect/reset/move/remove, alias lifetime and incident wire/group cleanup. Retained upstream canonical port validation, data-tree parameter validation, file safety and unrelated changes. Added four regression tests. No extra dependency, node kind, or geometry compatibility claim.

Validation: Rust and direct Git network unavailable in this session. Source review only; CI pending. Previous upstream results are not results for these edits.

Next: cargo fmt --all; cargo test --locked graph_api::tests; cargo test --all-targets --locked; review draft PR against develop/nodes-ultimate. Add file-edit CLI with explicit separate output and preserved document metadata after tests. Avoid main pushes or unvalidated merges.
