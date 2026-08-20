# PERF-D2-01: reemit_water_planes' O(N_draws x W) linear scan rests on an invalidated premise

**Issue**: #3141 — https://github.com/matiaszanolli/ByroRedux/issues/3141
**Labels**: `low,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: Draw & Instancing
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D2-01)

## Location

- `byroredux/src/render/water.rs:61-67` (the doc comment), `:147-152` (the scan)
- The invalidating spawn path: `byroredux/src/cell_loader/spawn/mesh_instance.rs:691, 724-748`

## Description

`reemit_water_planes` finds each water entity's already-emitted draw with `draw_commands.iter().position(|c| c.entity_id == entity)` and its doc justifies the O(N×W) cost as:

> *"typical N is ~thousands of draws and W is ≤ ~3 water planes per cell, so this is well under a microsecond. A `HashMap<EntityId, usize>` would be premature for the expected scale."*

**That premise held while water came only from CELL `XCLW`/`XCWT`. It no longer does.** `mesh_instance.rs:724-748` now spawns a `WaterPlane` (with its own 436 B `WaterMaterial`) for **every mesh sub-shape whose material carries `is_water_shader`**, so a Skyrim/FO4 exterior with authored rivers, waterfalls and pond meshes can hold considerably more than three. The `LodWaterPlane` annulus (`streaming.rs:438`) adds one more.

## Evidence

Both the comment and the scan are verbatim at HEAD:
```
byroredux/src/render/water.rs:63:/// Linear scan over `draw_commands` per water entity is O(N×W);
byroredux/src/render/water.rs:64:/// typical N is ~thousands of draws and W is ≤ ~3 water planes per
byroredux/src/render/water.rs:147:        let Some(idx) = draw_commands.iter().position(|c| c.entity_id == entity) else {
```

`MAX_WATER_DRAWS = 186` (`crates/renderer/src/vulkan/water.rs:172`) is derived from the portable `maxUniformBufferRange` floor rather than an observed count, so it does not *by itself* prove W is large — but it is the ceiling the pass is built to tolerate, and the scan is O(N × W) up to it. Against the FO4 baseline's `bench_draws_cmds = 3440` (`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`), W = 40 would be ~138 k `entity_id` comparisons per frame.

## Impact

**Sub-millisecond in every configuration reachable today**, so this is a *stale-justification* finding rather than a measured regression. It is reported because the comment now asserts a bound the code no longer enforces, and the next reader will trust it — the same failure mode that lets a real O(N×W) blowup ship unnoticed once mesh-bound water becomes common in a target game.

## Suggested Fix

Either:

- Correct the comment to state the real bound (`MAX_WATER_DRAWS`, and that mesh water contributes), **or**
- Cheaper than a map: set `is_water` and capture the index during the existing static-mesh emit loop, which already visits every entity exactly once, and have `reemit_water_planes` consume a small `Vec<(EntityId, u32)>` instead of rescanning.

## Related

- #1026 / `F-WAT-05` (the no-resort contract this function depends on — **intact**; the `water_commands_match_draw_slots` debug assert is still in place, and `sort_draw_commands` still runs before `reemit_water_planes`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — other `draw_commands.iter().position(...)` rescans in `byroredux/src/render/`
- [ ] **TESTS**: A regression test pins this specific fix (if the index is captured during emit, pin that `water_commands_match_draw_slots` still holds for a multi-plane mesh-water cell)
