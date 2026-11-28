# OMEGA 27: Native data integrity

AI: ChatGPT. Base `ec3b840e81c76aa7e426da5dc47b716ba975a98e` (main).

## Scope

- Reject duplicate JSON keys in node parameter and input maps, including nested cluster graphs; do not silently choose the last value.
- Preserve floating-point bits in native JSON round trips.
- Use bounded same-directory temporary names for atomic saves, including long Unicode destination names.
- Preserve existing schema v1, NodeKind and port IDs. No new runtime dependency. Keep GHX changes separate from #57.

## Validation status

AGENTS.md, CONTRIBUTING.md, architecture, status, persistence, compatibility and current related PRs inspected. Local clone is blocked by network and Rust/Cargo are unavailable. Rust checks must be run by repository CI. No tests have been run in this session yet.

## Open work

Unknown node/field preservation as editable inert records, cross-instance recovery, backup lifecycle, and external-format semantic round trips require separate changes and validation. Unknown native data is currently rejected without modifying the original file. Do not claim full compatibility.
