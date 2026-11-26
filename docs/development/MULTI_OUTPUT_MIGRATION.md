# Next migration: multiple outputs, opaque native nodes and Null

Status: **original proposal; first endpoint implementation in draft PR #14, validation pending**. See [Mission 11](../MISSION_11_MULTI_PORT.md). Opaque nodes and runtime Null remain proposed. Reviewed against source `2f94df459533774f83593835ba1e22d4924a3c42`. The current runtime still has one output per node and no `Value::Null`. This document is a resume guide, not a compatibility claim.

## Why this is the next foundation

`NodeSpec.output` is one `DataType`, `Node.inputs` maps a port key to a source `NodeId`, and `Evaluation.outputs` / cache results hold one `Tree` per node. Cluster definitions likewise expose one source node. Adding multi-output components only to the registry would leave connections, caches, serialization and the UI unable to represent their results.

## Small, reviewable implementation units

| Order | Files / contract | Required evidence |
|---|---|---|
| 1 | `model.rs`, `persistence.rs`: separate strict v1 DTOs from runtime data; introduce a stable string output key in source endpoints; design v2 envelope with bounded opaque records | All 37 v1 examples and legacy port aliases load without changing values, wires or settings; v1 integer source IDs migrate to an explicit default output, proposed key `result` |
| 2 | `engine.rs`, `node_access.rs`: output-key → Tree results; adapt existing kernels to the default output; include output key in input resolution and cache identity | Independent downstream connections to two outputs, unchanged cached results, deterministic evaluation, errors and cancellation; account for all outputs in resource budgets |
| 3 | `canvas.rs`, `cluster.rs`, `graph_api.rs`, CLI summary and `coverage_inventory.rs`: display and expose typed output endpoints consistently | Connect/save/reopen/Undo a two-output graph; preserve API v1 default-output behavior; add an explicit new API version for named outputs |
| 4 | Native opaque nodes: preserve original kind, fields, payload, ports and wires; display an unsupported placeholder without executing it | Unknown kinds, unknown known-node fields, unknown output keys and script payload round-trip through save/Undo/copy; unrelated supported graph remains inspectable |
| 5 | `Value::Null`, tree matching, data view and CLI value encoding | Distinguish unconnected input, absent branch, empty branch, Null item and evaluation error; retain Null positions through tree/list transforms |

The writer can emit v2 after its complete reader/writer contract exists. Keep v1 reading; reject unsupported future schema versions explicitly. Do not loosen serde validation by silently ignoring unknown content. An opaque node is preserved document data, not an allowed Graph API execution target.

## First practical two-output proof

Use Dispatch A/B. The existing `data_tree::dispatch` already computes two lists. With items `[1, 2, 3, 4]` and an alternating true/false pattern, independently expect A=`[1, 3]` and B=`[2, 4]`. Connect each to a different downstream node, then edit the pattern, save/reopen, test cache invalidation, Undo/Redo and error propagation. Preserve existing single-output nodes through an adapter rather than rewriting all kernels at once.

Initially keep node-level revisions and invalidation; per-output revisions are a later optimization. On cancellation or resource failure, do not mix old and new output sets. Validate declared output keys and types before publishing a node result. Specify disabled-node output behavior in the schema and fixtures.

## Opaque data boundary

Separate “document can be preserved and saved” from “node can be evaluated.” Unknown nodes produce a clear unsupported error for their downstream dependencies. Keep unknown JSON bounded, retain unknown numerical/field representations where required, and never activate script/import/plugin capabilities merely by opening the file. Editing a known position or name must not discard the opaque payload.

## Null boundary

`data_tree::Slot<T>::Null` is only a helper today. Add `Value::Null` as a real item; do not convert it to zero or an empty branch. Tree/list operations should preserve its position unless the operation explicitly filters Null. Define arithmetic, statistics, sorting, geometry and script input policies separately, with source-specific GH evidence before claiming compatibility. Keep Boolean/Vector/Domain type work separate until Null travels consistently through evaluation, display, serialization and API summaries.

## Commands and checkpoints

Start from the current development branch and `STATUS.md`. Run the existing sample/compatibility tests before migration, then add independent endpoint fixtures and retain old v1 fixtures. For each unit, run relevant tests, formatting and strict Clippy, save the commit to GitHub, verify its remote SHA, and update STATUS. Finish each complete source checkpoint with the normal Linux/Windows Check. Do not merge an intermediate schema migration into main.
