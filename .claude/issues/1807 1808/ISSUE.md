# #1807: PERF-D3-NEW-03: memory-budget.md links the BGSM cache section to a path deleted by the asset_provider module split

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-03)
**Location**: `docs/engine/memory-budget.md:149`

## Description
The doc links `byroredux/src/asset_provider.rs`, which no longer exists — the
module split into `byroredux/src/asset_provider/{mod,archive,material,script,texture,tests}.rs`.
All documented values remain correct; pure link rot.

## Suggested Fix
Point the link at `byroredux/src/asset_provider/material.rs`, and sweep the doc's
other relative links.

---

# #1808: PERF-D4-NEW-02: upload_lights silently truncates past MAX_LIGHTS (512) — no overflow warn, no proximity prioritization

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-02)
**Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:25`; producer
`byroredux/src/render/lights.rs:73-177`

## Description
`collect_lights` appends every `LightSource` with no cap, sort, or camera-proximity
priority; `upload_lights` clamps to 512 silently
(`let count = lights.len().min(MAX_LIGHTS);`). Every sibling overflow path
(instances, indirect draws, terrain tiles, material intern) warns; lights only get
a once-per-session info-log on the first frame, before dense cells load.

## Impact
On content with >512 live lights (plausible on Skyrim radius-3+ exterior grids),
lights past index 511 in storage-iteration order vanish with zero telemetry — a
light adjacent to the camera can be the one dropped.

## Suggested Fix
Add the same `log::warn!` pattern on `lights.len() > MAX_LIGHTS`; optionally sort
by distance-to-camera before the clamp.
