# #1927: REN-D8-02 — #865 XCLL cubic-fog was never reachable for the interiors it targets

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/composite.frag:510,533-540` (as of the audit date)

## Description
The #865 XCLL cubic fog was authored for FNV interiors (Doc Mitchell's House,
Goodsprings Source Pump), but lived inside the exterior-only aerial-perspective
branch (`depth_params.x > 0.5`) and mixed toward `compute_sky()` — meaningless
for interiors (no sky). Independent of the (separately reported, #1926) dead
`z` gate, this feature could never have applied to the interiors it targets,
and would have fogged toward the wrong color if it had.

## Note
By the time this issue was picked up, #1926 (fixed earlier in the same
session) had already removed the entire aerial-perspective fog fallback
branch — the specific broken code this issue points at no longer exists.
The underlying design flaw is still worth recording so a future revival
doesn't repeat it: documented on `RenderData::fog_clip`/`fog_power`
(`draw.rs`) and the `fog_params` UBO comment (`composite.frag`) that any
future interior XCLL cubic-fog consumer needs its own interior-scoped branch
mixing toward `fog_color`, not the exterior/sky-haze path that was removed.

---

# #1928: REN-D10-01 — GpuCamera.render_origin doc says "w = unused" but the name is overloaded elsewhere

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:259-285` (`GpuCamera.render_origin` doc)

## Description
`GpuCamera.render_origin`'s doc says "w = unused" — accurate for that struct —
but `VolumetricsParams::render_origin.w` (a separate struct) packs
`is_exterior`. A future dev skimming only the `GpuCamera` doc could assume the
w-slot is free to repurpose without checking the sibling struct.

## Suggested Fix
Amend the doc to note the field name is separately overloaded in
`VolumetricsParams`, so it isn't naively reclaimed.
