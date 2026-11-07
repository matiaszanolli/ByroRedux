# REN-D12-01: the overlay's "GPU passes Σ" double counts `volumetrics_local_sim` (missing from `NESTED_GPU_BRACKETS`)

Labels: low,renderer,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6754

**Severity**: LOW · **Dimension**: Debug/Telemetry

## Location
`crates/debug-ui/src/panels/studio.rs` (`NESTED_GPU_BRACKETS`, `gpu_pass_upper_bound_ms`)

## Description
The local-grid sim bracket is recorded inside the `volumetrics` bracket (`vol.dispatch` -> `grid.record_frame(..)` between the `Volumetrics` start and end), so its time is already in `volumetrics`. The table lists only `volumetrics_inject`, `volumetrics_integrate`, `shadow_filter`, `cloud_shadow`; `volumetrics_local_sim` is absent, so on every cycle where both ran the Σ adds the sim's ms a second time. `GpuTimerSnapshot.volumetrics_local_sim_ms`'s own doc says never to sum them.

## Evidence
`NESTED_GPU_BRACKETS` has four entries; `volumetrics_local_sim` is in `byroredux/src/main.rs` `BENCH_GPU_KEYS` and published as a `gpu_pass_ms` row by `systems/metrics.rs`; `gpu_pass_sum_excludes_nested_child_brackets` covers only inject / integrate and the hover text names only those.

## Impact
The overlay's headline GPU total is overstated by the sim's cost whenever a local-grid box is live (fire scenes). Bench-line and per-row figures are unaffected.

## Suggested Fix
Add `("volumetrics_local_sim", "volumetrics")` to `NESTED_GPU_BRACKETS`; extend the test and the hover text. Better: a test asserting every bracket documented as nested in `GpuTimerSnapshot` appears in the table.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Neighbouring unpinned contracts checked
- [ ] **TESTS**: The new pin fails when the wiring is removed (mutation-check it)
