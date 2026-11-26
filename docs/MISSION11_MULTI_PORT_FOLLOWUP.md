# Mission 11 — Multi-port architecture follow-up

## Scope and provenance

The existing feat/nodes-multi-port-components implementation was inspected at cf1933cb2a13a7c332a457a4e9821c5c1ba20d88. The initial develop/nodes-ultimate comparison was 83 commits ahead of evolve/all (df77c542473a5c637f3353dec48e367dd67db027); the feature and integration branch had diverged by one commit each at that snapshot, common ancestor 2e6534f6fd9e4358ea330e5f1f0ff27e9a7274cb. PRs #1–#11 were inspected through their metadata and relevant source, not rebuilt individually. This follow-up branch deliberately targets the feature branch (draft PR #13); no other branch was rewritten or merged. The current feature branch already defines persistent PortRef(node, port), PortLayout, ordered input links, PortResults, per-port evaluation, graph validation, and schema v2. Preserve these rather than building a competing engine.

## Public reference design

GH1:
- https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_IGH_DataAccess.htm
- https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_Component_GH_InputParamManager.htm
- https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_Component_GH_OutputParamManager.htm
- https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_ParamAccess.htm
- https://developer.rhino3d.com/api/grasshopper/html/Methods_T_Grasshopper_Kernel_IGH_VariableParameterComponent.htm
- https://developer.rhino3d.com/guides/grasshopper/list-components/

Separate typed input/output registration, parameter-level item/list/tree access, and variable-parameter lifecycle are useful design references. GH1 allows mixed access; NODES has component-specific evaluation and is not an implementation of the GH SDK.

GH2:
- https://developer.rhino3d.com/api/gh2/type/grasshopper2.components.component/
- https://developer.rhino3d.com/api/gh2/type/grasshopper2.components.idataaccess/
- https://developer.rhino3d.com/api/gh2/type/grasshopper2.parameters.access/

GH2 separately declares inputs and outputs and uses Item/Twig/Tree; Twig carries metadata and null state beyond a plain list. NODES does not claim that its List is equivalent to GH2 Twig. No proprietary runtime, SDK or code dependency is introduced.

## Existing multi-port engine

- Stable numeric output IDs and stable string input keys; legacy primary port 0.
- Ordered sources, bounded at 64 connections per input, typed validation and cycle rollback.
- StreamMerge variable input and StreamGate variable output layouts, maximum 32 active ports; Merge input identity allocation has a finite lifetime bank of 32 keys.
- New kinds inherited from the feature: DeconstructPoint, DeconstructVector, DeconstructDomain, DeconstructMesh, Dispatch, SortListPaired, StreamGate, StreamMerge. CurveEvaluate and EvaluateSurface have additional outputs while retaining primary output behavior.
- Per-component cache includes source endpoint and layout ordering. It conservatively invalidates on upstream component revision rather than tracking independent revisions per port.
- Old schema v1 loads with primary-output links; schema v2 persists explicit port links/layout. Save and reload, invalid endpoint rejection and Undo/Redo require execution evidence.
- Point/Vector and Domain representations remain limited by the actual model types. Mesh faces are numeric triangle-index streams, not native GH face objects. Cluster boundary remains single-output, and unsupported grouping is refused rather than silently discarding wires.

## Follow-up changes

1. Canvas uses PortRef for click/drag and wire-drop search. Output rows are independently typed and hit-tested, linked inputs show their actual source, dynamic cards grow, wire shapes use the selected port's result, auto-arrange considers all edges. Shift adds a wire; ordinary connection replaces the input; right-click disconnects all wires on an input.
2. Inspector lists actual input endpoints and supports individually disconnecting a source, displays every output, and exposes variable input/output addition/removal for StreamMerge/StreamGate. Per-port values are visible in the inspector and separate value window. Use the graph API to rename/reorder port layouts; no graphical reordering/renaming UI is claimed.
3. Clipboard/duplicate retains port layouts and remaps each internal source endpoint; paste drops external edges, duplicate retains external edges. Calculation-change detection includes explicit links and ordered layouts. Display-only label changes do not require recalculation.
4. Seven native integration tests plus two app tests were authored. They cover XYZ output and cache, dispatch and ordered wiring, stable dynamic output ID/reorder/migration/Undo/Redo, old schema and invalid endpoints, cycles/type/wire caps, cancellation and disabled/removed ports, paired sort and dynamic input order, clipboard remapping, recalculation detection. None was run locally.

## Verification and remaining work

Local cargo/rustc and direct GitHub clone were unavailable (DNS error). The Windows PR workflow is the build/test gate; do not count a test as passing before logs for the exact head SHA confirm it. No GUI interaction, screenshot, benchmark, Rhino oracle, independent agent, or new Windows release was performed. Check both the base feature and this follow-up for compile errors and old test assumptions. No dependency additions.

Run:
```sh
cargo test --locked --test mission11_multi_port_contracts
cargo test --all-targets --locked
cargo build --release --locked --bin Morphlet
```

Next: reconcile active integration changes (notably node_access metadata, legacy input alias migration, branch matching, mathematical and surface fixes); verify actual full graph with multiple output sockets; review dynamic port renaming/reordering UI; extend safe offline Graph API and GHX import/export to explicit endpoints without permitting executable nodes; support multiple exposed cluster outputs; design a mixed access scheduler with null-state semantics. Current geometry preview/export still selects primary output unless the user wires a secondary output to a downstream display/output component. Ensure old files round-trip, independent port errors are surfaced, and no silent data loss occurs. Do not merge or claim complete Grasshopper compatibility until these gates pass.
