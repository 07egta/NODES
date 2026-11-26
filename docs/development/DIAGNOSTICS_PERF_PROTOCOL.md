# Graph diagnostics performance protocol

The opt-in executable is [`examples/diagnostics_perf.rs`](../../examples/diagnostics_perf.rs).
It measures the public `diagnostics::diagnose` call using immutable output from a
real `Evaluator`. It does not evaluate geometry again during diagnostic samples.
Diagnosis does not modify the graph, input files, application settings or caches.

## Run and retain the evidence

Use the same machine, toolchain, optimization profile and workload selection when
comparing revisions. Keep full stdout JSON and the process exit code, build logs
and the exact source revision. Compilation is outside the example's timers.

```sh
NODES_SOURCE_SHA="$(git rev-parse HEAD)" \
NODES_RUSTC_VERSION="$(rustc --version)" \
cargo run --release --locked --example diagnostics_perf -- --quick \
  > diagnostics-quick.json

NODES_SOURCE_SHA="$(git rev-parse HEAD)" \
NODES_RUSTC_VERSION="$(rustc --version)" \
cargo run --release --locked --example diagnostics_perf -- --full \
  > diagnostics-full.json
```

For a matched three-repeat comparison of one workload:

```sh
cargo run --release --locked --example diagnostics_perf -- \
  --full --case prefix-4096 --repetitions 3 > diagnostics-prefix-4096.json
```

The default is `--quick`. Repetition counts are restricted to 3, 5 or 7; quick
defaults to 3 and full to 5. Case names must exist in the selected mode. Invalid
arguments or failed invariants return exit code 1 and `status: "FAILED"`; they
must not be presented as successful performance measurements. The executable
stops at the first failed case and preserves earlier results in its JSON.

Metadata also accepts `NODES_WORKTREE_STATE` (for example `clean` or `dirty`) and
`NODES_DIAGNOSTICS_SHA256` (SHA-256 of the actual `src/diagnostics.rs`). Record these
when measuring uncommitted source; a commit hash alone does not identify modified
code. Environment metadata is recorded as supplied, not independently verified
by the executable. Missing metadata is JSON null.

## Fixed workload matrix

| Case | Quick | Full | Analytic expectations |
|---|---:|---:|---|
| `success-1000` | 1,000 nodes | 1,000 nodes | Number 2.5 then Add(1); final value 1,001.5; no failures |
| `success-10000` | 10,000 nodes | 10,000 nodes | Final value 10,001.5; no failures |
| `failure-chain-1000` / `failure-chain-16384` | 1,000 nodes | 16,384 nodes | One Divide-by-zero root; every other node is blocked; root affects N−1 nodes |
| `balanced-128-tail` / `balanced-1024-tail` | 128 roots / 4,000 total nodes | 1,024 roots / 16,384 total nodes | Balanced binary joins, then a long single-input chain; all nodes fail |
| `prefix-512` | 512 roots / 1,023 nodes | — | One additional root joins the accumulated prefix per Add node |
| `prefix-1024` | 1,024 roots / 2,047 nodes | 1,024 roots / 2,047 nodes | Same incremental prefix construction |
| `prefix-2048` | 2,048 roots / 4,095 nodes | — | Same incremental prefix construction |
| `prefix-4096` | — | 4,096 roots / 8,191 nodes | Same incremental prefix construction |
| `prefix-8192` | — | 8,192 roots / 16,383 nodes | Same incremental prefix construction |

The fixtures retain the current 16,384-node graph limit. They use sequential IDs,
ordinary Number/Divide/Add prototypes from the public graph API, then one final
public graph validation. Avoiding per-insertion validation prevents fixture setup
from dominating the diagnostic measurement.

For a balanced graph with R roots and N nodes, there are R−1 merge nodes and
T = N−(2R−1) tail nodes. Each root affects exactly log₂(R)+T nodes. A merge at
level k has exactly 2ᵏ upstream roots, in a contiguous interval; every tail node
has R roots. The executable checks these formulas and the exact capped ID lists.

For a prefix graph, R roots have IDs 0 through R−1. Merge node R+j has j+2 roots.
Root r affects R−max(1,r) merge nodes. Expected IDs follow these construction
formulas independently of the diagnostic dependency traversal.

## What is measured

- `fixture_build_and_validate`: node construction and one public validation.
- `evaluation_wall`: exactly one cold evaluation with a new Evaluator, including
  construction and destruction of that Evaluator. `evaluation_reported` separately
  records the engine's internal wall-time field.
- `diagnostics_first_call`: the first diagnosis of this already evaluated graph.
- `diagnostics_warm_samples`: repeated diagnosis of the same graph and result;
  the report gives every sample and the median/minimum/maximum.
- `report_json_serialization`: serialization of the first report, measured
  separately. `serialized_json_bytes` records its size without printing the large
  complete report into benchmark output.

Diagnostic timers include report construction and the algorithm's normal checks.
They exclude fixture setup, evaluation, independent assertions, JSON serialization
and report destruction. Each report is dropped before the next sample; the first
report stays alive for final serialization and summary. Allocator and OS caches
are not reset. **First-call does not mean cold process or cold memory; warm means
repeated diagnostics, not cached geometry evaluation.**

Each run verifies every root/blocked ID, exact affected/upstream counts, bounded
ID previews, truncation flags, preserved ordinary error messages, completed-node
counts and real timing-record coverage. Success graphs also check the final
analytic arithmetic result. No wall-time pass/fail threshold is imposed.

JSON summarizes exact failure counts, preview lengths, truncated-record counts
and up to three root error messages. Relationship preview limits are read from
the actual diagnostic module, and the fixture oracle verifies those limits.
The measured module's own graph-validation cost remains inside each diagnosis.

## Comparing an optimization

Build the same executable source against the before and after modules in separate
clean worktrees, selecting identical cases and repetition counts. Do not substitute
mock failures or edit the baseline algorithm to make it callable from another crate.
Alternate run order when gathering more samples to reduce systematic thermal or
background-load bias. Retain actual numbers and source identifiers; avoid a single
universal speedup claim across these deliberately different graph shapes.

This harness does not measure GUI latency, frame rate, process startup, peak/RSS
memory, worker cancellation responsiveness, GPU work or Windows graphics behavior.
Those require separate measurements. Source presence and successful compilation
alone are not evidence that a workload has been run.
