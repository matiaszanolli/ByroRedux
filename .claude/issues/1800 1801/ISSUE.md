# #1800: PERF-D5-NEW-02: One-bounce-GI hit irradiance samples the first 8 lights in upload order, not the 8 relevant to the hit point

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-02)
**Location**: `crates/renderer/shaders/include/lighting.glsl:178-192` (`giHitIrradiance`
fixed-prefix loop + post-hoc `dist > radius` skip), `include/shader_constants.glsl:39`
(`GI_HIT_LIGHT_CAP = 8u`); light upload order `byroredux/src/render/lights.rs:73-160`

## Description
`giHitIrradiance` loops `count = min(lightCount, GI_HIT_LIGHT_CAP)` over the global
light SSBO prefix in upload order. `collect_lights` uploads the cell directional
first, then point lights in arbitrary ECS sparse-set iteration order —
proximity-blind. A `dist > radius` skip exists but only after the fixed-prefix
selection, so lights past index 8 are never considered. In any cell with >8
lights (taverns, Whiterun/Dragonsreach-class interiors), the GI bounce
permanently ignores every light past index 8 while still firing up to 8 shadow
rays against a fixed prefix that may be entirely out of range of the hit point.

## Impact
Two-sided — quality (bounce lighting in >8-light cells is systematically
wrong/dim and can flicker across cell reloads as ECS iteration order changes
which 8 lights exist for GI) and efficiency (up to 8 ray-query traces per lit
fragment spent on an unprioritized set).

## Suggested Fix
Prioritize the prefix CPU-side (sort `gpu_lights[1..]` by intensity·radius, one
small sort/frame) so "first 8" approximates "8 most influential"; or select
per-hit by distance with an early-out after 8 contributing lights. Keep the
ray-count cap unchanged.

---

# #1801: PERF-D1-NEW-01: about_to_wait runs a full MeshHandle+TextureHandle dedup walk every frame for on-demand-only telemetry

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-01)
**Location**: `byroredux/src/main.rs:2262-2286`

## Description
Every frame, `about_to_wait` iterates the entire `MeshHandle` and `TextureHandle`
storages and inserts each non-zero handle into a persistent `HashSet`
(allocation-free since #1584) to compute `meshes_in_use`/`textures_in_use` dedup
counts. The only consumers are the `stats` console command and the debug-server
entity evaluator — both on-demand, neither per-frame; `log_stats_system` (1 Hz)
doesn't print these fields.

## Impact
CPU cost scaling linearly with mesh-entity count — plausibly ~1 ms/frame on a
dense exterior grid or Skyrim city, spent on a stat nobody reads that frame.

## Suggested Fix
Throttle to the diagnostics cadence (every-16-frames or 1 Hz, matching
`log_stats_system`), or compute lazily inside the two on-demand consumers.
