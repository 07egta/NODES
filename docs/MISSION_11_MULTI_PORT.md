# Mission 11 — stable port architecture (draft)
 
## Scope
Independent Rust implementation, without Rhino runtime or SDK. Base integration checkpoint: `2e6534f6fd9e4358ea330e5f1f0ff27e9a7274cb`. This branch is not a claim of Grasshopper parity.

GH1 public API references:
- [IGH_DataAccess](https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_IGH_DataAccess.htm): read/write by parameter, including individual item, list and tree.
- [GH_ParamAccess](https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_ParamAccess.htm): item/list/tree access.
- [GH_InputParamManager](https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_Component_GH_InputParamManager.htm), [GH_OutputParamManager](https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_Component_GH_OutputParamManager.htm): separate registration of input/output parameters.
- [IGH_VariableParameterComponent](https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_IGH_VariableParameterComponent.htm): dynamic parameter insertion/removal and maintenance.
- [List components guide](https://developer.rhino3d.com/guides/grasshopper/list-components/).

GH2 API ideas are not treated as a frozen GH1-compatible contract. Exact per-build oracle fixtures for both families remain needed.

## Model and dataflow

Input keys are persistent strings; output IDs are u32. Port 0 is the permanent primary output, independent of display order. A wire endpoint is `PortRef { node, port }`. Input fan-in is ordered and bounded to 64 sources, deduplicated by endpoint. Multiple source trees merge before the input's Flatten/Graft/Simplify transformation; evaluation still uses existing tree matching rules.

Fixed ports may be renamed/reordered, not deleted. StreamMerge has dynamic inputs backed by 32 permanent keys (lifetime allocation limit). StreamGate has at most 32 active output ports with monotonically increasing IDs below 65536. Its gate is the persistent ID, not an index: intentionally differs from Grasshopper's index-oriented UI. Connected dynamic ports cannot be deleted. Unknown, duplicate or dangling IDs fail validation; never remap them to port 0.

`Evaluation::output(PortRef)` resolves a specific output. Cache entries retain separate trees/errors per port under a shared component calculation key/revision and count all port weights. Downstream keys include the source endpoint, so rewiring from one port to another invalidates the consumer even when the source node is unchanged. Independent per-port dirty propagation is not yet implemented.

## Components
- DeconstructPoint / DeconstructVector: XYZ (current vector representation is Point).
- DeconstructDomain: two numbers per branch, not a typed interval object.
- CurveEvaluate: point and unit tangent; old primary point retained.
- EvaluateSurface: point, normal, U derivative, V derivative; old primary point retained.
- DeconstructMesh: vertex list and flat triangle vertex indices, one branch per mesh. No MeshFace/color/normal parity claim.
- Dispatch: numeric truth pattern, cyclic, A/B trees.
- SortListPaired: stable numeric-key sorting, optional paired values; equal keys preserve input order. Old SortList unchanged.
- StreamGate: persistent-ID routing, unselected ports empty.
- StreamMerge: ordered variable input trees.

DeconstructPlane, distinct vector/domain/face types, further GH output variants (e.g. curve angle and surface frame), arbitrary variable ports for custom components and true multi-output cluster interfaces remain out of scope of this checkpoint. Cluster inner graphs support secondary wiring; legacy grouping refuses selections it cannot preserve. Legacy script still returns one output. GH/GHX export/import mappings require a later update.

## Persistence/UI
Schema 1 files load with implicit primary port 0. Schema 2 saves preserve the compact primary-wire form and explicitly encode nonprimary/multiple wires and port layouts. Unknown future versions and unknown endpoints are rejected. Undo/redo checkpoints and copy/paste retain the layout and wire data. Canvas supports per-port wire drag, Shift-append, output row labeling, per-wire removal in inspector and port edits. Use a downstream Output node for secondary output preview/export; old direct node export remains primary-oriented.

## Verification plan and status
The authored regression suite `tests/multi_port_integration.rs` covers independent outputs, rename/reorder, warm cache and invalidation, ordered fan-in, deletion, transactional cycle/type/unknown endpoint checks, dynamic ID allocation, gate routing, dispatch, stable paired sort, v1 load, v2 disk save/load, undo/redo, malformed endpoints and cluster inner/outer secondary wiring. Inventory now exports declared port IDs/access, not verified GH contracts.

No local compiler, direct Git network, or graphical environment was available to this authoring session. Tests have been written, not passed. Draft PR CI should run:
```sh
cargo test --locked --test multi_port_integration
cargo test --all-targets --locked
cargo build --release --locked --bin Morphlet
```
Do not merge or declare completion without reviewing build failures, verifying the GUI, and running exact GH1/GH2 oracle fixtures.
