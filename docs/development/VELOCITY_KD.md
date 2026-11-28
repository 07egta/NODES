# PROJECT VELOCITY — adaptive CPU point-cloud search

Base: 8cac838229071509156d560391c3d1ba5237d761 (main, PR #52 merged).
AI: ChatGPT. Focused development branch; no direct main changes.

## Scope
- `src/point_cloud.rs`: existing implicit median KD tree, exact nearest-distance
  and original-index tie rules. Target elongated/planar clouds and small inputs.
- A standalone Rust release benchmark exercising the public API with analytic
  and brute-force oracles. Compare the same workload on the base and candidate.
- No change to file formats, resource bounds, node IDs, public API or dependencies.
- No Rayon yet: the public cancellation callback is not Sync and existing
  graph-level parallelism needs coordination to avoid oversubscription.

## Coordination
Read AGENTS.md, architecture, status, performance protocol and relevant open
issues/PRs. Checked file lists for #87, #99, #132, #133, #139, #150, #152, #153,
#156, #159, #162, #175, #94 and #85; no point_cloud.rs overlap observed.
#87 owns Data Tree batching; #139 owns NURBS work; #132 owns schedule reuse;
#175 and #163–#171 own GPU-related work. Do not duplicate those changes.

## Validation
Local Git transport cannot resolve github.com and rustc/cargo are unavailable.
GitHub connector can read and commit text. No Rust timing, compilation,
formatting, CI or Intel/Grasshopper results have been obtained.
Keep this PR draft until checked; no unmeasured speedup claim.
