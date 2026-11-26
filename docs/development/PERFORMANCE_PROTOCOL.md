# Performance workload protocol

Status: **source added; Cargo and execution validation pending in CI**.

The executable example is [coverage_perf.rs](../../examples/coverage_perf.rs).
It measures headless graph evaluation, Data Tree kernels, mesh kernels, and native
project serialization/file I/O. It does not measure the GUI, frame time, process
startup, idle CPU, resident memory, Windows 11 graphics, or printer hardware.

## Reproducible invocation

Use a clean checkout and retain the commit, compiler, workflow/run URL, command,
and complete JSON output. For comparable measurements, use the same profile and
machine configuration. Compilation time is outside the example's timers.

Bash:

```sh
NODES_SOURCE_SHA="$(git rev-parse HEAD)" \
NODES_RUSTC_VERSION="$(rustc --version)" \
cargo run --release --locked --example coverage_perf > performance.json
```

PowerShell:

```powershell
$env:NODES_SOURCE_SHA = git rev-parse HEAD
$env:NODES_RUSTC_VERSION = rustc --version
cargo run --release --locked --example coverage_perf > performance.json
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

The example writes exactly one JSON document to stdout. Successful measurements
have `status: "MEASURED"`; an operation error or invariant failure produces
`status: "FAILED"` and exit code 1. Panic diagnostics may also appear on stderr.
A failed run must not be reported as a performance result.

`source_revision` uses runtime `NODES_SOURCE_SHA`, then the same variable captured
at compilation, and otherwise JSON null. Both values are recorded separately.
Compiler version is recorded only when `NODES_RUSTC_VERSION` is supplied.
OS, architecture, pointer width, package version and the actual
`cfg!(debug_assertions)` value are recorded. Optimization settings are not inferred
from that flag; use the retained command and build logs.

## Fixed workloads and independent invariants

| Workload | Bounded input | Checks |
|---|---|---|
| Existing numeric graph samples | 10, 100, 1,000, MAX_NODES, 10,000 nodes; 50 source-slider edits for each | Warm cache hits equal node count; final result after every edit is slider value + node count - 1 |
| Data Tree | 128 × 96 matrix = 12,288 numbers, plus 8 explicit empty branches | Exact flattened sequence and sum 75,491,328; all grafted paths and values; all transposed values; empty branches; full unflatten equality |
| Mesh | 128 × 128 unit XY grid, 16,641 vertices / 32,768 triangles | Area 16,384 mm²; centroid (64,64,0); 512 boundary edges, all other indexed audit counts zero |
| Mesh subdivision | One midpoint subdivision; 66,049 vertices / 131,072 triangles | Same analytic area/centroid; exact vertex/triangle counts; bounds (0,0,0)–(128,128,0); 1,024 open boundary edges |
| Native project | 2,000 nodes, source Number followed by 1,999 Add(1) nodes | Full Project equality for in-memory and file roundtrips; preserved Japanese name, pan and zoom; changed parameter and name survive overwrite; independent outputs 2,016.25 and 1,995.5 |

The mesh is deliberately an **open surface**. The benchmark checks that it is not
reported closed. Its zero signed surface integral is not a claim of a valid
solid volume. Area/centroid comparisons require finite values and tolerance
`1e-10 × max(1, abs(expected))`. Counts, bounds and arithmetic sequences are exact.

Tree operations use `data_tree::validate_topology`, `flatten`, `graft`,
`flip_matrix` and `unflatten`. Only the rectangular matrix is passed to
`flip_matrix`; explicit empty branches are checked by the other operations.
Fixture construction is timed separately, and the kernels retain their normal
input/resource checks.

Mesh operations use `mesh_fabrication::audit`, `area_center`, `subdivide` and
`persistence::mesh_stats`. The checks use analytic grid geometry rather than a
second invocation of the same measured computation as its own oracle.

Project operations use `serde_json::to_vec_pretty`,
`persistence::decode_project`, `persistence::save` and `persistence::load`.
The native save timing includes project validation, pretty JSON writing,
temporary-file preparation, flush/sync and rename as implemented by the public
API. Load includes parsing, normalization and graph validation. Both initial save
and replacement of an existing project are measured and read back.

The file lives in a newly created private benchmark directory below the OS temp
directory. Cleanup removes only its one project file and the empty directory;
there is no recursive deletion or user-supplied output path.

## Measurement scope and interpretation

- Existing `samples` fields are preserved. Additional results are under
  `additional_workloads.data_tree`, `.mesh` and `.native_project`.
- `workload_schema_version: 2` identifies this additive extension.
- Each newly added operation is measured once. These samples are not statistical
  medians, throughput guarantees or cross-machine comparisons.
- The existing slider p50 and p95 use nearest-rank samples 25 and 48 of 50 sorted
  timings. Fixture time is separate from evaluation.
- Timed operations include their own input checks; independent result assertions
  run after the relevant timer. `total_workload_ms` also includes fixtures,
  assertions and temporary-file cleanup, and excludes printing the final JSON.
- Loads immediately follow writes. The OS page cache is not evicted, so these are
  **not cold-disk measurements**. Save timing is the observed public API duration,
  not proof that hardware write caches are durable.
- Fixed data sizes are selected to remain within current resource limits and to
  target less than one minute excluding compilation on CI. That duration is a
  target pending actual execution, not a measured claim. No wall-time assertion
  is imposed on slower machines.
- Preserve failed JSON and stderr when an invariant fails. Do not remove an
  assertion or shrink a failing workload merely to publish faster numbers.

## Next verification

Run the command above in diagnostic CI, check the process exit code, parse stdout
as one JSON object, and retain all fields. Confirm each sample and added workload
has `status: "MEASURED"`, and record actual wall time/profile before claiming
these categories have been measured. Follow with the repository's normal fmt,
check, test and Clippy gates.
