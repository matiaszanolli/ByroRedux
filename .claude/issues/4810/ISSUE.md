# #4810: PERF-D8-2026-09-23b-01: Three `draw_frame` CPU spans fall outside every `cpu_ms:` sub-bucket or into the wrong one; the lazy blend-pipeline compile is the largest

**Severity**: LOW
**Labels**: low, performance, tech-debt, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D8-2026-09-23b-01)

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:802` (`ssbo_build` closes) → `:808-897` (blend-variant compile + `save_pipeline_cache_if_grown`) and `:900-1189` (UBOs, composite ×2, terrain re-upload), before `cmd_t0` (`draw.rs:2109`);
  - `draw.rs:2109-2312` (`cmd_record` contains `build_fog_volume_clusters` + the fog upload);
  - `draw.rs:2523+` (post-present shrinks);
  - bucket docs `crates/core/src/ecs/resources/mod.rs:820,870-874`.
- **Status**: NEW
- **Description**:
  - The multi-second first-sight blend compile (the code comment records "26 s single frames") lands only in the `rof_draw_call` residual, which the doc tells readers to treat as driver or host wait.
  - The fog-cluster build and upload (PERF-D4-2026-09-23b-02) reads as command recording.
  - #4767 budgeted `draw_frame`'s *line count*, not its time. The extracted shrinks still sit in the undocumented residual.
- **Impact**: Hitch triage blames the driver for a pipeline compile. No runtime cost.
- **Related**: #4208
- **Suggested Fix**: Add `pipeline_compile_ms` and `fog_cluster_ms` sub-buckets (and log each variant compile with its duration), and rewrite the `cmd_record_ms` / `rof_draw_call_ms` docs.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
