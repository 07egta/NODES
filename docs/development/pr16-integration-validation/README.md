# Evidence for the held PR16 / PR18 integration candidate

This is an isolated proposed combination, not a merged GitHub pull request.
Input heads and the exact one-line semantic repair are explained in
[the integration note](../OMEGA_PR16_INTEGRATION_20261127.md).
It includes PR16's inherited PR15 engine/model/canvas work as well as its own
numerical, file-output and help changes.

The automatic combined runtime failed all-target check with E0425 because its
diagnostic stale guard still referenced `before`, removed by the EditTracker
change. `before-check.json` and `before-check.log` preserve that controlled
failure. The recorded tree identifies the local `git merge-tree` result, whose
command and exact inputs are provided in the note. It is reproducible from the
input commits, not claimed as a separately published commit.

After replacing that reference with `self.edits.previous.graph`, all-target
check succeeded. Its tool-output observation is recorded explicitly without
claiming a separately retained raw log. The subsequently saved fmt, strict
all-target Clippy, full tests, debug and release logs all completed successfully.
Tests report **567 passed, 0 failed, 3 ignored across 57 suites**. The ignored
manual performance/stress cases are not counted as executed.

The exclusive target copied previously built dependencies, then cleaned the
project package before compiling. The parent worktree and target were not used
as a writable integration environment. `results.json` records the initial
candidate tree, inputs, actual commands, profile, durations and release binary
fingerprints. It is not a benchmark. Compiled binaries are not committed.

`source-sha256.json` identifies the exact runtime/test/example/Cargo bytes used
for the run. Those bytes were rechecked after tests, native GUI use and evidence
collection. The final evidence commit adds documentation without changing them.
Parent-specific historical records remain labeled as such rather than adding
their counts to the combined run.

The three real release CLI responses retain expected exit codes and report
counts. `release-fixtures.json` records identical before/after input hashes.
Transient timing fields in those JSON files do not measure performance changes.
The integration note records native root recovery, undo/redo and explicit GUI
limits separately from the automated counts. The candidate has no Windows CI
or new distribution upload at this checkpoint.

SHA256SUMS covers each file in this directory except itself.
