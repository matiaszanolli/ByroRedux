# PERF-D4-NEW-02: upload_lights silently truncates past MAX_LIGHTS (512) — no overflow warn, no proximity prioritization

**Issue**: #1808
**Labels**: low,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-02)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-02)

## Location
`crates/renderer/src/vulkan/scene_buffer/upload.rs:25`; producer `byroredux/src/render/lights.rs:73-177`

## Description
`collect_lights` appends every `LightSource` with no cap, sort, or camera-proximity priority; `upload_lights` clamps to 512 silently (`let count = lights.len().min(MAX_LIGHTS);`). Every sibling overflow path (instances, indirect draws, terrain tiles, material intern) warns; lights only get a once-per-session info-log on the first frame, before dense cells load.

## Evidence
`upload.rs:25` `let count = lights.len().min(MAX_LIGHTS);` — no `log::warn!` on truncation.

## Impact
On content with >512 live lights (plausible on Skyrim radius-3+ exterior grids), lights past index 511 in storage-iteration order vanish with zero telemetry — a light adjacent to the camera can be the one dropped.

## Related
#279, #797.

## Suggested Fix
Add the same `log::warn!` pattern on `lights.len() > MAX_LIGHTS`; optionally sort by distance-to-camera before the clamp.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

