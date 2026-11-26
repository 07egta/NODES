# Cargo manifest declaration inventory

The new `docs/development/dependency-manifests.json` is generated evidence.
The historical recovery ledger `dependency-inventory.json` and existing notices
remain unchanged. The generator does not install, download, or execute packages.

## Reproduce

Use Python 3.11 or later in the repository checkout:

~~~sh
python3 scripts/audit_dependencies.py --self-test
cargo metadata --locked --format-version 1 > /tmp/nodes-cargo-metadata.json
python3 scripts/audit_dependencies.py \
  --metadata /tmp/nodes-cargo-metadata.json \
  --output docs/development/dependency-manifests.json
~~~

The generator reads the explicitly supplied JSON, the repository's `Cargo.lock`,
and the two fixed historical license ledgers. It only hashes license documents
listed by those ledgers if their paths are regular tracked files under
`licenses/`. It rejects traversal and symlinks. It never opens paths supplied by
Cargo's `manifest_path` or `license_file` fields. Relative license-file declarations
are recorded; external paths and raw package IDs containing runner paths are
omitted. Only the exact known crates.io source IDs
`registry+https://github.com/rust-lang/crates.io-index` and
`sparse+https://index.crates.io/` are preserved. Every other source ID, including
arbitrary public URLs, local URLs, credentials, queries, or fragments, is
represented by an opaque SHA-256 fingerprint.

The JSON has deterministic package/feature/evidence ordering, no generation
timestamp or absolute runner paths, and a SHA-256 of the actual lock file.
Checksums mean **a declaration in Cargo.lock**: registry archives have not been
rehashed. An optional metadata checksum must agree with Cargo.lock. Package
identity duplicates, absent resolution data (`--no-deps`), and lock/metadata
mismatches fail with a nonzero exit code. This generator requires full metadata
without `--filter-platform`: Cargo may leave packages in that array while
filtering resolution nodes, and this deliberate limitation produces an explicit
error rather than a misleading completeness claim. Lock packages outside the supplied
metadata resolution are listed explicitly; this is not an all-features,
all-platforms, or linked-binary completeness claim.

## License meaning

Each package records its manifest license expression, license-file declaration,
GNU-family and MPL tokens, resolved features, and any checksum-attributed
checked-in text evidence. Existing `MPL-2.0+` declarations remain visible.

The expression parser checks bounded **operator structure**, not complete SPDX
syntax, identifier validity, exception applicability, or legal meaning. The
`spdx_operator_structure_only` status makes that limit explicit. A `+` is allowed
only once at the end of a license token; repeated or embedded `+` and `+` on an
exception token are rejected. It reports malformed or unsupported expressions as
ambiguous, and missing declarations as missing. GNU-family declarations always
receive a review flag. An `OR` expression such as `MIT OR GPL-3.0-only` reports
that a syntactic alternative without GNU-family tokens exists. It is never
automatically labelled forbidden or legally approved. An `AND` expression must
satisfy every conjunct for that same purely syntactic property. Exceptions and
custom license references are preserved for review.

Existing notices are attributed only when package name, version, and registry
checksum uniquely match the historical ledger. A matching text hash proves that
the tracked working copy agrees with the recorded text hash; it does not prove
that the upstream archive contains the same bytes, that the text covers every
feature, or that the application's distribution obligations are complete.
Unattributable, missing, changed, or untracked evidence remains explicit.
Runtime and separately distributed font notices are outside this Cargo-package
inventory and remain covered by the existing documentation.

No dependency, selected license, or license policy changes are made by this
script. Review reasons are evidence gaps or review requests, not automatic legal
decisions. Standard output contains only a short count summary.

## Validation state

After the local execution environment became available again on 2026-11-26,
the eight self-tests passed with Python 3.12.14 (exit code 0). A separate
temporary Git checkout check also passed through the actual command-line
entry point: it inspected tracked Cargo.lock, generated the default output for
two fixture packages, omitted private runner paths, and printed one summary line.

**Full NODES dependency metadata generation remains pending CI.** The fixture
checks do not establish the declarations or completeness of the real dependency
inventory. The self-tests cover malformed JSON, missing license data and MPL
preservation, duplicate package IDs/identities, deterministic output, GNU
`OR`/`AND` semantics, malformed `+` suffixes, strict source-ID redaction,
checksum-verified license text changes, traversal/symlink rejection, and
metadata/lock mismatch, including the explicit `--filter-platform` limitation.
The symlink assertion is skipped on platforms that cannot create a test symlink;
the other checks in that test still run.

## Primary format references

- Cargo metadata format and its scope:
  https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
- Cargo manifest license and license-file fields:
  https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields
- SPDX license-expression grammar:
  https://spdx.github.io/spdx-spec/v2.3/SPDX-license-expressions/
