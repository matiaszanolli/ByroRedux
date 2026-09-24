# #4808: PERF-D5-2026-09-23b-05: `gpu_main` absorbs the sky-cube bake: the main-render timer starts at `TOP_OF_PIPE` right after `record_bake`, and the bench TSV does not extract `sky_cube_ms`

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D5-2026-09-23b-05)

- **Severity**: LOW (attribution; confirmed independently by the Dim 8 auditor)
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:694-708`; `context/geometry_pass.rs:33`; `context/build_and_upload_instances.rs:969-979` (the bake), `:731-738` (the controller input `max(main_render_ms, volumetrics_ms)`); `scripts/fsr-bench-matrix.sh:207,270-272`
- **Status**: NEW (the main-pass analogue of #4789; #2040 caveat class)
- **Description**:
  - A `TOP_OF_PIPE` start doesn't wait for prior compute, so the bake's drain lands in `main_render_ms` and is double-counted against `sky_cube_ms` (END at `BOTTOM_OF_PIPE`).
  - The adaptive ray-budget controller consumes the inflated value.
  - The bake landed between the two bench records (2026-09-13…14).
  - The TSV extracts 8 of the 19 brackets, and not `gpu_sky_cube`, although the `bench:` line prints it.
- **Impact**: Doesn't make the engine slower, but makes `gpu_main` and the controller input unattributable across records. It is a candidate partial cause of R6a-stale-22.
- **Suggested Fix**: Write the main-render START at a stage that waits for the bake. Add `gpu_sky_cube`, `gpu_tlas`, `gpu_cluster_cull` and the RT tier to the TSV, then re-bench both sides, since the harness changes.
- **Confidence**: Medium; `TOP_OF_PIPE` timestamp placement is implementation-defined, and this is not verified on NVIDIA.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
