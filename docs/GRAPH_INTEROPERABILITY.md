# Mission 09: graph interoperability (draft, v1)

## Status and scope

A standalone GHX inspector and an intentionally narrow all-or-nothing converter; neither requires Rhino, GH_IO.dll or a network. The report is an inert JSON artifact with exact UTF-8 source, instance/component IDs, coordinates, port records, ordered sources, group membership, slider value, toggle value and panel text. Raw per-object/port details retain tree flags, defaults, preview, script text and unsupported parameters. No imported scripts execute. This is **not** a GH/GHX implementation or Grasshopper numerical compatibility claim.

Only the following GUIDs are candidates for scalar conversion:

| GH name | Component GUID | Native kind | Limits |
| --- | --- | --- | --- |
| Number Slider | 57da07bd-ecab-415d-9d86-af36d7073abc | number | Current value only, native range; GH min/max/step UI retained in inspection, not .morph |
| Construct Point | 3581f42a-9592-4549-bd6b-1c0fc39d067b | point | XYZ scalar inputs, path {0}, explicit defaults or one slider source |
| Panel | 59e0b89a-e487-49f8-bab8-b5bab16be14c | none | inspect only |
| Boolean Toggle | 2e78987b-9dfb-42a2-8b76-3923ac8bd91a | none | inspect only |
| Group | c552a431-af5b-46a9-a8a4-0fcbc27ef596 | none | inspect only |

Unknown components and source references stay in the report; conversion refuses them rather than creating a misleading executable .morph. No canvas placeholder UI yet. GH binary, multi-output and multi-source conversion, nontrivial tree matching, embedded clusters, GH export, native Boolean/string/vector/plane types, exact NURBS conversion and Rhino differential tests remain unsupported. Scalar arithmetic and geometry beyond the two mappings are available through the independent graph API, not GHX conversion.

## Commands

From the project root (Linux/macOS CLI; Windows GUI-subsystem binaries may not display stdout, so prefer library APIs or CI until a console tool is provided):

    cargo run --bin Morphlet -- --inspect-ghx examples/interop/scalar-point.ghx report.json
    cargo run --bin Morphlet -- --convert-ghx examples/interop/scalar-point.ghx sample.morph
    cargo run --bin Morphlet -- --node-catalog
    cargo run --bin Morphlet -- --build-graph examples/interop/ai-add.json sum.morph
    cargo run --bin Morphlet -- --validate sum.morph

The GHX fixture is synthetic (no McNeel code or third-party archive copied). Expected point: (7,2,3); graph API fixture: 7+5=12. These are analytically derived test expectations; not Grasshopper runtime recordings.

## Local graph API

Version 1 JSON request supports add (alias, kind, position), connect (from, to, input), set (node, input, value). The catalog emits actual NodeKind and input keys/types/default/range metadata. Edits apply to a candidate graph, commit only after validation. Existing numeric IDs are accepted as aliases; new aliases are bounded ASCII identifiers. Build, evaluation with cancellation, validated save and error outputs are available as Rust functions. CLI rejects graph evaluation errors before writing the destination. Explicit positive allowlist prevents script, expression, file-import, cluster and newly added unreviewed kinds from executing through this interface. No server or cloud API.

## Security and dependency notes

8 MiB UTF-8 GHX cap; 64 XML depth, 100k elements, max 32 attributes per element, existing graph object count, 32k wires and 64 MiB serialized report cap. DTD/ENTITY and processing instructions are rejected, no resolver or external loading. GUID uniqueness, reference ordering, missing references, cycles, invalid positions, finite scalar numbers and unsupported port layouts are validated. Unsupported structures are fail-closed at conversion. Reports may include private source code and paths: do not publish them automatically. Local AI JSON: 1 MiB/8192 edits, graph limits, parameter range and type checks, cycle detection, existing evaluator item/mesh budgets. No hard process-wide memory or wall-clock guarantee. xml-rs 0.8.29 was already in Cargo.lock, is used directly under its MIT license; no GPL/LGPL/AGPL or proprietary DLL added.

Existing Rhai engine has no_module, disabled eval, operation/call/expression/array/string/map/geometry limits, cooperative cancellation and 2-second progress deadline. These are not a hard memory sandbox. Existing native .morphpart imports may contain scripts; do not open untrusted native parts as though they were safe GHX reports. Existing clusters have max depth 4, size 256, max 64 definitions, recursion and reference validation. Existing schema is unchanged.

## Extension roadmap

Use declarative node packs first, backed by existing .morphpart definitions; versioned manifests, explicit enable/disable, validate all nested parts and reject script/import nodes for untrusted packs. Do not grant DLL loading or arbitrary process access. Wasm is a later optional runtime (separate feature, pinned permissively licensed runtime; no WASI by default, limited linear memory/table/instances, fuel and epoch interrupts, output validation, no native host IO, isolate traps). Measure Windows build size and performance before choosing a runtime. Neither a plugin registry nor a Wasm host is shipped by this change.

## Sources (public behavior/structure only)

- McNeel GH_Archive methods: https://developer.rhino3d.com/api/grasshopper/html/Methods_T_GH_IO_Serialization_GH_Archive.htm
- Public component GUIDs: https://discourse.mcneel.com/t/rhino-inside-plagiarism-check-for-grasshopper-files/142581
- Public GHX structural example: https://github.com/jhorikawa/GrasshopperProgrammingTutorials/blob/master/0004%20Load%20Grasshopper%20file%20with%20compute.rhino3d%20Python/voronoi.ghx
- Panel, toggle, group structural example: https://github.com/architecture-building-systems/dewbee/blob/main/installer.ghx
- XML dependency: https://docs.rs/crate/xml-rs/0.8.29
- Rhai limits: https://rhai.rs/book/safety/index.html
- Wasmtime resource limiting: https://docs.wasmtime.dev/api/wasmtime/struct.StoreLimitsBuilder.html

## Verification

Added Rust tests for synthetic scalar conversion and evaluation, unsupported IDs, retained ordered wires, dangling references, cycles, corrupt/oversized XML, XXE/DTD, deep nesting, NaN, non-default tree settings, multi-source refusal, API arithmetic, forbidden capability and version rejection, bounds, and atomic rollback. This authoring environment has no Rust toolchain or direct git/network access; these tests have **not** been executed locally. Run cargo fmt, cargo test --locked, and Windows integration checks before merge. Real GHX import/evaluation equivalence must be tested with Rhino-generated fixtures. Do not treat this draft as production-ready.


## Transactional editing extension (draft)

API v1 now also accepts:
- `disconnect`: `to`, `input` (requires an existing connection).
- `reset`: `node`, `input` (restores the declared numeric default, preserves the connection).
- `move`: `node`, `position` (finite coordinates in the native canvas range).
- `remove`: `node` (removes incident wires and group membership; node IDs are never reused).

Only one explicit wire edit per canonical target port is accepted per request. Use connect directly to replace a connection; disconnect and reconnect the same port in separate transactions. A deleted alias cannot be reused within the same request. Existing numeric IDs identify existing nodes; returned aliases include only surviving nodes. Any failure rolls back all graph and annotation changes. The Rust apply API supports existing graph edits; the existing CLI still creates new graphs only. No script, file import or plugin capability is granted.

Added regression tests for edit success, rollback, alias lifetime, bounds and canonical wire identity. Pending Rust/CI verification for this branch.
