# Issues #398 + #399 — Investigation + Scope

Both issues are HIGH-severity Oblivion compat gaps where data extracted into `MaterialInfo` never reaches the GPU. They share the same plumbing pattern (extend `GpuInstance` + lockstep-update three shader struct definitions per `feedback_shader_struct_sync.md` + thread through `DrawCommand`) so investigating them together makes sense, but the actual fixes are distinct.

## #399 — glow / detail / gloss texture slots

Three of the seven NiTexturingProperty slots populate `MaterialInfo` (and the ECS `Material` component) but no `GpuInstance` field exists for them.

**Scope estimate**:
1. `crates/renderer/src/vulkan/scene_buffer.rs` — add `glow_map_index`, `detail_map_index`, `gloss_map_index`. Struct grows 160 → 176 bytes (still 16-byte aligned).
2. `crates/renderer/shaders/triangle.vert` — mirror struct.
3. `crates/renderer/shaders/triangle.frag` — mirror struct + add sampling logic (glow → emissive multiply, detail → 2× UV scale base modulate, gloss → specular intensity scale).
4. `crates/renderer/shaders/ui.vert` — mirror struct (UI doesn't read these but the layout must match).
5. `crates/renderer/src/vulkan/context/mod.rs` — add 3 fields to `DrawCommand`.
6. `byroredux/src/render.rs` — thread `Material.{glow_map, detail_map, gloss_map}` → texture handles → `DrawCommand` indices.
7. Layout test in `scene_buffer::gpu_instance_layout_tests` — update offsets + size.
8. SPV recompile.

**~7 files modified.**

## #398 — z_test / z_write / z_function

Per-draw depth state needs to flow somewhere. Two architectural choices:

**(a) `VK_EXT_extended_dynamic_state` dynamic state** (audit-preferred):
- `vkCmdSetDepthTestEnable` / `vkCmdSetDepthWriteEnable` / `vkCmdSetDepthCompareOp` per draw.
- Avoids combinatorial pipeline explosion.
- Requires extension enable at device creation + per-draw-batch state changes in the draw loop.

**(b) Pipeline cache key extension**:
- Existing `PipelineKey` already keys on `(src_blend, dst_blend, two_sided, is_decal)` (per #392).
- Add `(z_test, z_write, z_func)` to the tuple — 8 × 2 × 2 = 32 new variants in the worst case (but most cells use only 2-3).
- Smaller code change than dynamic state, but the cache footprint grows.

**Scope estimate (option b — smaller change)**:
1. `crates/nif/src/import/material.rs` — extract `z_function` from `NiZBufferProperty` (currently only z_test + z_write extracted).
2. `crates/renderer/src/vulkan/context/mod.rs` — add 3 fields to `DrawCommand`.
3. `crates/renderer/src/vulkan/pipeline.rs` — add to `PipelineKey`, route to `vk::PipelineDepthStencilStateCreateInfo`.
4. `byroredux/src/render.rs` — thread from `MaterialInfo` → `DrawCommand`.
5. `crates/renderer/src/vulkan/context/draw.rs` — sort key includes new state so consecutive draws with same depth state batch correctly.
6. Tests — synthetic `DrawCommand` with z_write=false produces the right pipeline variant.

**~6 files modified.**

## Combined fan-out

If we land both in one PR: ~10-12 files. Significantly over the 5-file scope check threshold from the pipeline.

## Recommendation

Land them as **two sequential commits** in one session:
1. **#399 first** (texture slots) — no architectural decisions, just additive struct + shader sampling. Renderer doesn't need to gain new pipeline variants. Lower-risk.
2. **#398 second** (depth state, option b — pipeline variants) — simpler than dynamic state and follows the existing cache pattern from #392.

Each fix lands a focused regression test. Pipeline cache test counts grow as new variants exercise.

## SIBLING

Memory `feedback_shader_struct_sync.md` flags that GpuInstance lives in 3 shaders that must update in lockstep. Both fixes need the SPV reflection pass from #427 to verify the layout doesn't drift.
