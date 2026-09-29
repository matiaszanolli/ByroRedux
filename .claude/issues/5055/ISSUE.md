# #5055 — PERF-D4-2026-09-29-01: GpuLight.history_id is CPU-only identity shipped in the GPU struct (+16 B per light) and mirrored in four shaders that never read it

**Labels**: low, bug, performance, renderer, shaders

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D4-2026-09-29-01`

**Severity**: LOW

**Dimension**: SSBO Sizing & Upload

**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:362`; mirrors in `shaders/include/bindings.glsl:293`, `cluster_cull.comp:43`, `caustic_splat.comp:56`, `volumetrics_inject.comp:125`; producers `byroredux/src/render/lights.rs:183,244`

**Status in report**: NEW (`186234944`)

## Description

`LightHistory::remap` runs on the CPU and uploads its result as the header's `previous_to_current` table. No shader reads `history_id`: a grep of `crates/renderer/shaders` finds only the four struct declarations. The field still grows the light stride from 64 to 80 B, so each light upload carries up to 16 KiB of unused bytes at the 1023-light cap. The upload repeats every frame whenever any light animates, because the dirty gate hashes the whole light bytes. It also adds a fifth copy to the Shader-Struct-Sync lockstep set.

## Impact

small upload waste and maintenance surface. Shader fetch cost is essentially unchanged: member-wise SSBO loads skip the unused vec4.

## Suggested Fix

carry the identities in a CPU-side array parallel to `gpu_lights`, moved through the priority sort in the `light_sort_scratch` tuple. Then drop the field from `GpuLight` and the four GLSL mirrors.

Validated at HEAD 9fcfdc3fc: `GpuLight::history_id: [u32; 4]` is at `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`; the only shader hits for `history_id` are the four struct declarations (`include/bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp`, `volumetrics_inject.comp`); its only reader is CPU-side `scene_buffer/light_history.rs`.

## Completeness Checks
- [ ] **SIBLING**: every GLSL mirror of `GpuLight` (bindings.glsl + 3 standalone copies) is edited in lockstep, and the layout/size pins are updated
- [ ] **TESTS**: A regression test pins this specific fix (light-history remap still survives priority reordering with identities held CPU-side)
