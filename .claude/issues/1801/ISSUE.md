# PERF-D1-NEW-01: about_to_wait runs a full MeshHandle+TextureHandle dedup walk every frame for on-demand-only telemetry

**Issue**: #1801
**Labels**: low,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-01)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-01)

## Location
`byroredux/src/main.rs:2262-2286`

## Description
Every frame, `about_to_wait` iterates the entire `MeshHandle` and `TextureHandle` storages and inserts each non-zero handle into a persistent `HashSet` (allocation-free since #1584) to compute `meshes_in_use`/`textures_in_use` dedup counts. The only consumers are the `stats` console command and the debug-server entity evaluator — both on-demand, neither per-frame; `log_stats_system` (1 Hz) doesn't print these fields.

## Evidence
`main.rs:2266-2281` runs unconditionally before the scheduler each frame.

## Impact
CPU cost scaling linearly with mesh-entity count — plausibly ~1 ms/frame on a dense exterior grid or Skyrim city, a double-digit share of the frame budget at the 300+ FPS bench rates this engine targets, spent on a stat nobody reads that frame. No quantitative guard exists for this site.

## Related
#1584, #637.

## Suggested Fix
Throttle to the diagnostics cadence (every-16-frames or 1 Hz, matching `log_stats_system`), or compute lazily inside the two on-demand consumers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

