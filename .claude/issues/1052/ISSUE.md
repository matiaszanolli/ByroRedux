# Issue #1052: Tech-Debt: 8 long-function extractions [batch]

**State:** OPEN  
**Labels:** enhancement, nif-parser, renderer, low, vulkan, tech-debt

## Status (2026-05-15)

| Finding | Status |
|---|---|
| TD9-008 draw_frame (2322 LOC) | DEFERRED — requires RenderDoc baseline |
| TD9-009 SceneBuffers::new (323 LOC) | **FIXED** — extracted to allocate_scene_render_buffers + create_scene_descriptors; new() = 64 LOC |
| TD9-010 VulkanContext::new (745 LOC) | DEFERRED — complex init chain, needs dedicated session |
| TD9-011 build_blas_batched | DEFERRED — requires RenderDoc baseline |
| TD9-012 build_tlas | DEFERRED — requires RenderDoc baseline |
| TD9-013 extract_local_bound | DONE — already 31 LOC (split in prior sessions) |
| TD9-014 walk_controller_chain | DONE — already 62 LOC (split in prior sessions) |
| TD9-015 parse_render_debug_flags_env | DONE — already 24 LOC (split in prior sessions) |
