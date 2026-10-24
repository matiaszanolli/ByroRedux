# PERF-D8-2026-10-23-02: bench_gpu_keys order test pins 10 of 24

- **GitHub**: #6337
- **Severity**: LOW
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-23.md

Found by `/audit-performance` 2026-10-23 (PERF-D8-02). Related: #3629.

## Location
`byroredux/src/main.rs` ~423-444 (`bench_gpu_keys_match_the_reported_bracket_order`), binding the order in `byroredux/src/app_events.rs` (the `[f32; 24]` ms array, the `[bool; 24]` active array, and the `gpu[0..=23]` format args).

## Description
Beyond the length (24) the test asserts only indices 4 and 11-18 (`main_render`, `presentation`, `tlas_build`, `caustic_splat`, `skin_palette`, `depth_history_copy`, `sky_cube`, `groundcover_scatter`, `exposure_meter`). Indices 0-3, 5-10 and 19-23 — including `shadow_filter` and `cloud_shadow`, the two newest nested children — are unpinned. `every_bench_gpu_key_is_printed_on_the_bench_line` checks presence, not order. Values come from three hand-maintained parallel lists of 24, each edited by hand on every append (four appends in five weeks).

## Evidence
The current order was verified correct by reading all three lists side by side. A transposition (e.g. `shadow_filter` ↔ `cloud_shadow`) would still pass the suite and silently mislabel a bench column and the `gpu_inactive=` token, which the harness extractors trust.

## Impact
Latent; mislabelled bench columns would corrupt cross-commit comparisons.

## Suggested Fix
Assert every index, or build the ms/active arrays from one `[(key, ms, active); N]` table so key, value and flag cannot drift; a source-scan test that the `gpu[i]` arg order matches `BENCH_GPU_KEYS` is a cheaper alternative (use `source_scan::production_text`).

## Completeness Checks
- [ ] **SIBLING**: Any other parallel-list pair in the bench line is checked
- [ ] **TESTS**: The new pin fails when two adjacent keys are swapped
