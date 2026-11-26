# Coverage and workflow recovery after executor loss

- Recovery branch: `recovery/coverage-workflows`.
- Confirmed base: `6349577399dd484a90fde5afe77ee173f8f9444a`.
- Original local commits: GH2 generator/inventory `6f77c9f`; access regressions `f593f4a`; example workflows `949fcb5`.
- The executor became unavailable before the later checkpoint was confirmed remotely.
- Recovered Rust files use the exact pre-rustfmt authoring payloads retained in the conversation; run formatting again.
- No Rust build or test was run during recovery. Do not count these recovered tests as passed.

## Preserved work

1. `scripts/update_gh2_inventory.py`: deterministic public-read GH2 index collector, 375 lines.
2. `docs/compatibility/gh2_recovery_provenance.json`: source export identity and results observed before executor loss.
3. `tests/access_evaluation_regressions.rs`: five regression tests for empty list access, SingleItem constraints and canonical API port conflicts.
4. `tests/example_edit_workflows.rs`: 37 explicit example edits plus one inventory completeness test; eight STL roundtrips.

## Integration dependencies

- The full `gh2_official_inventory.json` could not be recovered from the lost filesystem. Regenerate using the pinned export; never replace its 1419 entries with guessed data.
- Access regressions require the reviewed engine empty-list handling and graph API canonical connection checks.
- The 37-example fixture map includes `examples/surface-curvature-study.morph`, absent from the confirmed base. Restore that separately together with its curvature implementation.
- The base repository still contains earlier documentation/test results; they are not evidence for the recovered changes.

## Resume commands

```sh
python scripts/update_gh2_inventory.py --self-test
python scripts/update_gh2_inventory.py --expect-count 1419 --expect-version 9.3.26329.6001
cargo fmt --all
cargo test --locked --test access_evaluation_regressions
cargo test --locked --test example_edit_workflows -- --test-threads=2
```

STL checks compare triangle counts and geometric bounds with a tolerance derived from f32 coordinates plus 1e-6 mm importer welding. They do not certify printability.
