# Data Tree Engine (MISSION 03)

Status: implementation branch, **not a certified GH1/GH2 clone**. No Rust
compiler was available in the authoring environment. GitHub CI and GH1
oracle comparisons must pass before this is considered release-ready.

## Sources and scope

- McNeel GH1 [data trees and Python](https://developer.rhino3d.com/guides/rhinopython/grasshopper-datatrees-and-python/):
  sorted paths, dense lists, sparse paths, possible null items.
- McNeel GH1 [GH_Path SDK](https://developer.rhino3d.com/api/gh1/type/grasshopper.kernel.data.gh-path/):
  paths contain nonnegative signed 32-bit integers and are not empty.
- McNeel [Advanced Data Structures, §3.3](https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/advanced-data-structures/):
  branch and item matching, repeat the last branch or item for longer inputs.
- David Rutten's [branch order explanation](https://discourse.mcneel.com/t/data-matching-tree/59746)
  and [matching pseudocode](https://discourse.mcneel.com/t/c-matching-data-trees-in-grasshopper/77164/4):
  branch matching is ordinal, not a join on path identity.
- GH1 [Sets component catalog](https://grasshopperdocs.com/addons/grasshopper-sets.html),
  community-generated component descriptions/ports, not an official executable oracle.
- GH2 [data model design discussion](https://discourse.mcneel.com/t/datatrees-in-grasshopper-2-0/24020)
  and [July 2026 beta announcement](https://discourse.mcneel.com/t/rhino-beta-feature-grasshopper-2-beta/221336):
  redesigned immutable trees, metadata, new solver. Do not conflate GH1
  matching with GH2. No GH2 import or metadata compatibility is claimed.

## Architecture

Keep existing Tree (BTreeMap<Vec<u32>, Vec<Value>>) and graph schema v1.
BTreeMap gives deterministic lexicographic branch order and Vec preserves
item order, including explicit empty branches. Paths are bounded to depth 32
and nonnegative i32 range in the new API. Geometry values still share Arc
payloads; Number, Point, Curve, Mesh stay distinct. Slot<T> sketches the
future Missing/Null/Item interface; **it does not add runtime Null**.

Public entry points:
- data_tree::{flatten, graft, simplify, trim, merge, entwine, branch, item,
  statistics, prune_empty, unflatten, flip_matrix, remap_paths, path_mapper,
  partition, split_list, dispatch, cross_reference, validate_topology}.
- data_tree::{match_branches, ItemMatches, MatchMode, PortTransform, apply_transform}.
- data_tree_nodes::{spec, is_tree_kind, is_list_kind, access_label,
  supports_matching, supports_transform}.

The graph currently has one output per NodeId and one source per input.
Multi-output operations return multiple trees/lists via API only; do not
encode outputs as arbitrary additional branches. Native output IDs and
variadic input wiring need an explicit migration shared with the other
mission branches.

Graph persistence stores topology and node settings, not the evaluated tree.
New numeric options use reserved keys in the existing params map:
- __data_matching: 0 longest, 1 shortest, 2 cross. Absence preserves the
  legacy exact-path/single-branch-broadcast evaluator for old item nodes.
- __tree_port:<input-key>: 0 unchanged, 1 flatten, 2 graft, 3 simplify.
- Path Mapper's rule text is stored in the existing script field. It is
  parsed by a separate restricted parser, never passed to Rhai.
- Old v1 projects without these options load unchanged in the new version.
  Old executables may reject projects saved with new nodes/settings; no
  forward compatibility promise. Undo/Redo and cache keys already track
  params; path mapping rule text is now included in the extra key.

### Matching policy

For selected item-mode matching and new branchwise list nodes, pick the input
with the most branches, then the deepest path, then the earliest input as
the output topology (a NODES choice, GH1 mixed-depth output path construction
not fully specified by the tutorial). Pair branches by sorted ordinal,
repeating the final branch of a shorter tree. Empty tree or empty required
branch produces no tuples, retaining the output branch. Longest repeats the
last item; Shortest stops at the smallest length; Cross enumerates
Cartesian tuples with the final port varying fastest. Cross operates per
matched branch. Existing list nodes still receive branch lists and scalar
settings; mode selection is not exposed for list/tree/custom nodes.

Example:
- A {0;0}: [1,2], {0;1}: [], {1;0}: [3]
- B {8}: [10], {9}: [20]
- Longest Add => {0;0}: [11,12], {0;1}: [], {1;0}: [23].
- A [1,2,3], B [10,20]: Shortest [11,22], Longest [11,22,23],
  Cross [11,21,12,22,13,23] for addition.
- Cross cardinality uses checked multiplication before emitting any tuple;
  output combination iterator stores borrowed slices, not a tuple matrix.
- MAX_ITEMS = 16,384 branches/items; depth = 32; per-tree weight remains
  MAX_TOTAL_WEIGHT / 2. API cross-reference also checks aggregate output
  weight. No dependency added.

### Component correspondence

The following inputs/outputs use GH1 public catalog descriptions where
available. Defaults are **NODES defaults**, not verified GH1 defaults.
GH1 defaults, exact edge behavior and output path construction require
manual/automated comparison against a licensed GH1 installation.

| Component | GH1 inputs -> outputs (conceptual order) | NODES implementation / defaults |
|---|---|---|
| List Item | List, index, wrap -> element | existing; index 0, wrap true |
| List Length | List -> length | existing; branchwise |
| Reverse List | List -> list | existing |
| Sort List | keys, associated lists -> sorted keys/data | existing numeric keys only |
| Shift List | List, shift, wrap -> list | existing; 1, true |
| Split List | List, split index -> two lists | split_list API only |
| Partition List | L, S -> chunks | new; size 2, child paths |
| Sub List | List, domain, wrap -> subset | existing start 0/count 3, no domain/wrap |
| Cull Index | List, indices, wrap -> list | new; index 0, wrap true |
| Cull Pattern | List, pattern -> list | existing repeating bitmask |
| Cull Nth | List, frequency -> list | new; N=2, one-based frequency |
| Dispatch | List, pattern -> A, B | dispatch API only |
| Weave | Pattern, lists -> list | not implemented |
| Sift Pattern | List, pattern -> multiple lists | not implemented |
| Replace Items | List, replacement, indices, wrap -> list | new; port order list/index/value/wrap; 0,false; required data |
| Insert Items | List, items, indices, wrap -> list | new; list/index/value, index 0, original indices; no wrap |
| Duplicate Data | Data, N, order -> data | new; repeat whole list twice; order option not implemented |
| Repeat Data | Data, length -> data | new; length 10 |
| Combine Data | inputs -> result, source index | not implemented (requires null) |
| Mass Addition | values -> sum, partial sums | new; total only |
| Mass Multiplication | values -> product, partial products | new; total only |
| Member Index | set, member -> index data | new; exact equality, all indices; concatenated results |
| Random Reduce | list, reduction, seed -> list | new; count 1, seed 1; stable survivors |
| Jitter | list, strength, seed -> values, indices | new; strength 1, seed 1; value output only, own PRNG |
| Cross Reference | multiple lists -> multiple streams | API + item-mode cross; no dedicated graph multi-output |
| Flatten Tree | tree, target path -> tree | existing; target {0} fixed |
| Graft Tree | tree -> tree | existing; child index, keep empty branches |
| Simplify Tree | tree, front-only flag -> tree | new; all invariant columns at equal depth; common prefix for mixed depth |
| Entwine | multiple streams -> flattened tree | new two-port prefixing; manually flatten inputs to flatten, differs from GH default |
| Merge | multiple streams -> result | new MergeTree, two ports; equal path append; old Merge remains mesh join |
| Tree Branch | tree, Path -> branch | new; numeric list path, keep path |
| Tree Item | tree, Path, index, wrap -> item | new; numeric list path, index 0, no wrap |
| Tree Statistics | tree -> paths/counts/etc. | new per-branch count only; API returns paths/counts |
| Explode Tree | tree -> dynamic output ports | not implemented |
| Construct Path | integers -> path | numeric lists accepted for tree selection, no distinct path Value |
| Deconstruct Path | path -> integers | not implemented |
| Replace Paths | tree, source paths, target paths -> tree | remap_paths API / restricted Path Mapper; no distinct node |
| Path Mapper | tree, mapping expression -> tree | new safe subset {A;B}->{B;A}, literals/A-Z, collisions append, unmatched paths preserved |
| Flip Matrix | matrix-like tree -> tree | new rectangular sibling branches only; ragged rejected |
| Trim Tree | tree, depth -> tree | new; depth 1; retain at least one index |
| Split Tree | tree, path masks -> two trees | not implemented |
| Clean Tree | tree, clean invalid, clean empty -> tree | not implemented; prune_empty API only |
| Prune Tree | tree, min, max -> tree | new minimum 1; no max |
| Unflatten Tree | data, guide -> tree | new; exact count required, guide empty branches preserved |

## Tests and next steps

tests/data_tree_tests.rs adds expected-value cases for hierarchical paths,
unequal lists/trees, mismatched path identities, empty branches, collision
safety, invalid mapper rules, 10,001 values, Cartesian limits, graph
arithmetic and points, save/reopen, Undo/Redo, cache invalidation. A Slot
test is not a runtime null test. Tests written but not executed here.

Run: cargo test --all-targets --locked; cargo build --release --locked --bin Morphlet.
Then compare GH1 native component outputs including mixed depths, ragged
matrix, null/default insertion, seeded random indices and branch error
behavior. UI was not launched in this environment. Need native null,
typed path values, output-port migration, access scheduling for multi-valued
scalar settings, variadic ports, and remaining operations before complete
compatibility. No subagent execution or 60–120 minute session was available.
