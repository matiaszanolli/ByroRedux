# Renderer Audit — 2026-05-11 (Dimension 3 focus)

**Scope**: Dimension 3 — Pipeline State (graphics + compute pipeline create, descriptor set layouts, vertex input, push constants, dynamic state, render-pass compatibility).
**Depth**: deep.
**Method**: orchestrator + single dimension agent.

## Executive Summary

- **Findings**: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW, 2 INFO.
- **Pipeline areas affected**: bindless descriptor flags (forward-looking); SAFE-25 #950 premise re-evaluation.
- **Net verdict**: **CLEAN.** Vertex input is pinned by `offset_of!` tests + stride assertion; every descriptor set layout (set 0 bindless, set 1 scene, all 9 compute pipelines, composite) is reflected against its SPIR-V at startup; dynamic state is comprehensively declared and matched per-frame; render-pass attachment count agrees with every pipeline's color-blend state in opaque, blend, and UI variants.

## Pipeline State Assessment (positive checks)

1. **Vertex input table consistency** — `vertex.rs:131-213` declares 9 attributes (locations 0..=8) post-M-NORMALS. Stride = `size_of::<Vertex>()` = 100 B, asserted at `vertex.rs:267`. Offsets `0/12/24/36/44/60/76/80/84` match `offset_of!` results pinned at `vertex.rs:275`. Formats: position/color/normal = `R32G32B32_SFLOAT`; uv = `R32G32_SFLOAT`; bone_indices = `R32G32B32A32_UINT`; bone_weights/tangent = `R32G32B32A32_SFLOAT`; splat_0/1 = `R8G8B8A8_UNORM`. Matches `triangle.vert:4-22` `in` declarations exactly.
2. **Push constant budget** — only block in the shader tree is `skin_vertices.comp:66` (12 B per `skin_compute.rs:56`, asserted at `skin_compute.rs:531`). No graphics push constants. The system-prompt's "mat4 viewProj + mat4 model = 128 B" description is stale; all per-instance data lives in the GpuInstance SSBO at set 1 binding 4.
3. **Dynamic state always re-emitted** — `pipeline.rs:284-292` declares VIEWPORT/SCISSOR/DEPTH_BIAS/DEPTH_TEST_ENABLE/DEPTH_WRITE_ENABLE/DEPTH_COMPARE_OP/CULL_MODE dynamic on opaque + blend; `draw.rs:1407,1413,1511,1656,1806,1811` fires the matching `cmd_set_*` per frame and per draw transition. UI pipeline declares VIEWPORT+SCISSOR dynamic and re-sets them in `draw.rs:1806-1811`.
4. **Render-pass vs color-blend attachment count** — render pass (`helpers.rs:105-112`) has 6 color refs (HDR/normal/motion/mesh_id/raw_indirect/albedo) + 1 depth. Opaque (`pipeline.rs:253-260`) provides 6 `color_blend_none` entries. Blend (`pipeline.rs:441-443`) provides `[hdr_blend, overwrite ×5]`. UI (`pipeline.rs:618-625`) provides `[ui_hdr_blend, ui_noop_blend ×5]` with empty `color_write_mask` on attachments 1-5. All three pipelines match the 6-color render pass. mesh_id and motion correctly have `blend_enable(false)` everywhere.
5. **Subpass index = 0 on every pipeline** — opaque `pipeline.rs:329`, blend `pipeline.rs:500`, UI `pipeline.rs:657`, composite `composite.rs:771`.
6. **G-buffer formats match render pass formats** — `gbuffer.rs:37-48`: `NORMAL_FORMAT=R16G16_SNORM`, `MOTION_FORMAT=R16G16_SFLOAT`, `MESH_ID_FORMAT=R16_UINT`, `RAW_INDIRECT_FORMAT=B10G11R11_UFLOAT_PACK32`, `ALBEDO_FORMAT=B10G11R11_UFLOAT_PACK32`. HDR is slot 0 with `HDR_FORMAT = R16G16B16A16_SFLOAT` (`composite.rs:108`).
7. **Composite pipeline descriptor layout** — 8 bindings declared at `composite.rs:471-520` matching `composite.frag:20-58`'s set 0 bindings 0..=7 + set 1 binding 0 (bindless). `validate_set_layout` fires at `composite.rs:521`. Pool sizes at `composite.rs:562-572` count 7 COMBINED_IMAGE_SAMPLER × MAX_FRAMES_IN_FLIGHT + 1 UNIFORM_BUFFER × MAX_FRAMES_IN_FLIGHT. The typed `[vk::WriteDescriptorSet; 8]` array in `recreate_on_resize:1019` catches divergence at compile time (#905).
8. **SSAO compute** — `ssao.rs:267-294`: 3 bindings (COMBINED_IMAGE_SAMPLER, STORAGE_IMAGE, UNIFORM_BUFFER) matching `ssao.comp:11-14`. `validate_set_layout` at `ssao.rs:284`.
9. **Cluster cull compute** — `compute.rs:117-148`: 4 bindings (STORAGE_BUFFER lights, UNIFORM_BUFFER camera, STORAGE_BUFFER grid, STORAGE_BUFFER indices) matching `cluster_cull.comp:52-78`. `validate_set_layout` at `compute.rs:139`.
10. **Bindless set (set 0)** — `texture_registry.rs:200-247`: single binding `COMBINED_IMAGE_SAMPLER[max_textures]` FRAGMENT-stage, `PARTIALLY_BOUND | UPDATE_AFTER_BIND` flags, `UPDATE_AFTER_BIND_POOL` on layout, `UPDATE_AFTER_BIND` on pool. `max_textures` derived from `maxPerStageDescriptorUpdateAfterBindSampledImages` clamped at the R16_UINT mesh-id ceiling — cannot underflow at low device limits.
11. **Scene set (set 1) binding flags** — `scene_buffer.rs:693-704` marks bindings ≥ 5 `PARTIALLY_BOUND` to tolerate the cluster-cull-fails-to-create case. TLAS (binding 2) listed in `optional_bindings` when `rt_enabled=false` (`scene_buffer.rs:712`).
12. **SPIR-V reflection coverage** — every compute pipeline (`bloom`, `caustic`, `cluster_cull`, `composite`, `ssao`, `svgf`, `taa`, `volumetrics`, `skin_compute`) calls `validate_set_layout`. Main raster set 0 reflected via `texture_registry.rs:221`; set 1 reflected via `scene_buffer.rs:713`.

## Findings

### [LOW] Bindless texture descriptor lacks VARIABLE_DESCRIPTOR_COUNT flag
**Dimension**: Pipeline State
**Location**: `crates/renderer/src/texture_registry.rs:206-242`
**Severity**: LOW
**Observation**:
```rust
let binding_flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
    | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND];
let binding = vk::DescriptorSetLayoutBinding::default()
    .binding(0)
    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
    .descriptor_count(max_textures)
    .stage_flags(vk::ShaderStageFlags::FRAGMENT);
```
Fixed `descriptor_count = max_textures`; never sets `VARIABLE_DESCRIPTOR_COUNT`.
**Why bug**: Not a correctness bug today — pool sizes and descriptor-allocate-info match the fixed count; `PARTIALLY_BOUND` covers uninitialised slots in the runtime array. The flag is only required when allocation count differs from layout count. Worth raising because a future "shrink allocations below `max_textures` to save memory at low-RAM startup" path needs the flag added before it can compile.
**Fix**: Optional — add `VARIABLE_DESCRIPTOR_COUNT` to `binding_flags`. Zero behaviour change today.
**Confidence**: HIGH
**Dedup**: None.

### [LOW] #950 (SAFE-25) premise is partially stale
**Dimension**: Pipeline State
**Location**: `crates/renderer/src/vulkan/pipeline.rs:130-142`
**Severity**: LOW
**Observation**: `build_triangle_pipeline_layout` composes set 0 (bindless) + set 1 (scene), both already SPIR-V-reflected:
- set 0: `texture_registry.rs:221` calls `validate_set_layout` against `triangle.frag` + `ui.frag`
- set 1: `scene_buffer.rs:713` calls `validate_set_layout` against `triangle.vert` + `triangle.frag`

`PipelineLayoutCreateInfo` carries only `set_layouts` — no `push_constant_ranges`. Grep confirms zero `set = 2` / `set = 3` declarations in `triangle.vert/.frag`. The two real bindings are guarded; the issue body still describes a regression path that is structurally impossible without first adding a new set or a push-constant block that no current shader uses.
**Why bug**: #950's "main raster pipeline lacks reflection validation" is partly stale — the LIVE bindings are reflected. What's actually unguarded is (a) any future set ≥ 2 binding, (b) any future push-constant range. Both are structurally additive.
**Fix**: Either close #950 with a note that scene_buffer + texture_registry already cover the surface, or land a small defence-in-depth assertion in `pipeline.rs` that the SPIR-V never declares any descriptor set beyond 0/1 and never declares a push-constant block. Current state is not actively broken.
**Confidence**: HIGH
**Dedup**: #950 (open, partially stale premise).

### [INFO] Stencil state captured but no pipeline variant routes to it
**Dimension**: Pipeline State
**Location**: `crates/renderer/src/vulkan/pipeline.rs:300-311, 456-461`
**Severity**: INFO
**Observation**: Both opaque and blend pipelines hard-code `.stencil_test_enable(false)`; depth format prefers `D32_SFLOAT` (no stencil bits) at `helpers.rs:14`. `nif/import/material.rs` populates `MaterialInfo.stencil_state` from `NiStencilProperty` but it's deferred (see comments at `pipeline.rs:300-305` + `stencil_state_capture_tests.rs:81`).
**Dedup**: #337 (open, tracked).

### [INFO] Wireframe property parsed but no `PolygonMode::LINE` pipeline
**Dimension**: Pipeline State
**Location**: `crates/renderer/src/vulkan/pipeline.rs:222-228, 412-414, 580-585`
**Severity**: INFO
**Observation**: All three pipelines hard-code `polygon_mode(FILL)`; `MaterialInfo.wireframe` is read at NIF import but never feeds the cache key. Oblivion vanilla ships zero wireframe meshes; mechanical extension would add a `wireframe` axis to `PipelineKey`.
**Dedup**: #869 / O4-D4-NEW-01 (open, tracked).

## Prioritized Fix Order

Neither LOW is urgent.

1. **LOW** — Audit and either close or harden #950: add a defence-in-depth SPIR-V assertion in `pipeline.rs` that no descriptor set ≥ 2 and no push-constant block is declared, so the *next* shader refactor can't slip past reflection.
2. **LOW** — Add `VARIABLE_DESCRIPTOR_COUNT` to the bindless binding flags so future variable-count allocations are one line away. Zero behaviour change today.

## Notes

- Dim 14 (Material Table) heavy invariant-pinning via `gpu_material_size_is_260_bytes` and 65-field offset asserts overlaps with Dim 3 vertex/material checks. No duplication of findings — Dim 3 confirms input plumbing, Dim 14 owns struct contents.
- Dim 1, 3, 4, 8, 9 are now all CLEAN as of 2026-05-11; broad sweep on 2026-05-09 covered the other dimensions.
