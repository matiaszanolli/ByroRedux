# Investigation: #52 — No instanced drawing for repeated mesh+texture

## Scope Assessment
This is a significant architectural change that touches:
1. context.rs — draw loop (sort by mesh+texture+pipeline, batch instances)
2. pipeline.rs — pipeline layout (add instance SSBO binding)
3. scene_buffer.rs — new per-instance model matrix SSBO
4. triangle.vert — replace push constant model matrix with SSBO[gl_InstanceIndex]
5. main.rs/cell_loader.rs — draw command generation (group by mesh+texture)

**6+ files affected.** Requires user confirmation per scope check protocol.

## Risk
Changes the entire per-frame draw path. Shader recompilation needed.
Must maintain push constants for viewProj (can't instance those).

## Recommendation
Defer to dedicated milestone (M25: Vulkan Compute + Batching).
The current 85 FPS at 789 entities on RTX 4070 Ti is not bottlenecked here.
