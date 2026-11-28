# PROJECT QUANTUM — adaptive compute policy (#164)

Base: `8cac838229071509156d560391c3d1ba5237d761` (main). Existing #175 owns the CPU reference and accelerator contract (#163); #132 owns the typed IR/schedule. This branch will implement an independent, pure Rust decision policy, not a second graph evaluator or GPU adapter. Keep `eframe/glow`, graph persistence and Cargo dependencies unchanged.

Auto selects GPU only with a matching measured end-to-end profile, valid resources and an explicitly validated approximate kernel. Exact f64 CAD remains CPU-only. Include cold/warm setup, transfer/readback, validation and wait costs in measured end-to-end observations. No guessed speedups. Existing GPU commands cannot necessarily be preempted; worker integration must discard stale/cancelled completions and account for in-flight buffers until completion.

Local rustc/cargo and direct Git network access are unavailable in this session. Draft pending CI, integration with #175 and real adapter measurements (#171).

## Increment 1

Added pure Rust `compute_policy` with a precision barrier, availability and memory gates, exact workload/profile matching (including adapter generation and cold state), five-sample minimum and 20% wall-time reduction margin. Explicit PreferGpu bypasses timing only, not safety. Five unit tests cover routing, stale profiles, resource rejection, empty work and extreme durations. No GPU dispatch, graph integration or profile collection yet. Thresholds are initial policy choices, not measured crossover points. CPU/GPU timing must use matched representative data, and the caller must invalidate profiles on changed assumptions. Rust compilation, fmt, clippy and tests not run locally. Keep draft until CI. No new dependencies.
