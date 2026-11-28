# PROJECT CONTINUITY — safe manual-save generations

## Scope

The existing v1 JSON `.morph` file format, legacy input normalization, graph
validation, and ordinary atomic save remain unchanged. Desktop Save and Save As
now use `persistence::save_with_backup`. When replacing an existing regular file,
the previous contents are copied byte-for-byte to a uniquely named sibling
`<name>.backup-<epoch-seconds>-<pid>-<sequence>.morph` (long names are shortened).
The UI reports its location. A backup can be opened normally if its contents
are a supported valid project. Use Save As to edit a recovered generation
without overwriting the source.

The new document is serialized into the existing bounded staging file. The
backup is opened with create-new semantics, written and synced before the
destination is replaced. Failure to read or back up the existing destination
aborts the save. If replacement fails after a successful backup, that backup
remains. Unknown, corrupt and newer-version files are never converted or silently
stripped: Open still fails closed, while an explicit overwrite through this
manual-save API retains their exact bytes. Oversized previous files and
nonregular destinations are refused; choose another filename.

Automatic recovery and exports keep their existing non-generational write path.
No dependency, schema or migration change is introduced.

## Limitations and verification

- Each overwrite adds a backup (up to the existing 8 MiB per-file limit). No
  automatic retention deletion is performed in this change; users must manage
  old generations. A crash during backup creation can leave an incomplete
  backup, but the original destination has not yet been replaced.
- Concurrent independent writers are not serialized. Unix syncs the backup
  directory entry; Windows syncs backup contents but directory durability and
  real power-loss behavior depend on the filesystem and OS.
- The new filesystem regression tests cover first save, multiple generations,
  exact-byte retention for future/unknown/corrupt documents, invalid and
  oversized input rejection, Data Tree re-evaluation and node IDs, ordinary
  autosave, Unix symlinks and Windows exclusive locks.
- Local test execution was blocked: Rust toolchain unavailable and direct Git
  clone DNS resolution failed. Require the repository Windows/Linux CI,
  formatting, lint and manual GUI save/reopen verification before merging.
- Other lifecycle work remains: independent multi-document recovery (coordinate
  with PR #180), recent projects, thumbnail, full 3D camera state, external
  references/relinking, a retention policy, multi-writer coordination, unknown
  node editable placeholders and future schema migrations. Existing v1 legacy
  aliases remain supported; no hypothetical v2 migration is claimed.
