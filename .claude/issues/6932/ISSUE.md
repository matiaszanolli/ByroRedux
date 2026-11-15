# #6932 — INC-2026-11-14-20: The explicit-LOD shader guard scans only `ray_hit.glsl` and `triangle.frag`

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-20). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`explicit_bindless_lods_are_view_relative_under_streaming`)
- **Changed in**: `shader_contract_tests.rs` (commit `f754eee28`, Refs #6629)

## Description
The guard checks `textureLod(textures[...], …, lod)` only in `include/ray_hit.glsl` and `triangle.frag`. Other explicit-LOD samplers of the streamed `textures[]` array are unscanned: `include/material_sampling.glsl`, `include/sky.glsl` (two sites), `include/clouds.glsl`, `volumetrics_inject.comp`. All are correct today (view-relative, LOD 0, or a coarsest-mip clamp). `sky.glsl`, `volumetrics_inject.comp` and `ui.frag` declare `textures[]` without `bindings.glsl`, so they could not call `residentLod` if they needed it.

## Impact
None today. A future full-chain absolute LOD at one of those sites would read up to `first_level` mips too coarse under streaming with no validation error.

## Related
#6629

## Suggested Fix
Scan every shader source for `textureLod` / `texelFetch` on `textures[` and allow-list the reviewed sites.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
