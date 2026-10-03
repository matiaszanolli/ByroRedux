# #5204 — REN-D3-2026-10-03-02: the light ↔ identity parallel invariant that feeds the light-SSBO remap header is enforced at three sites, but only `collect_lights` is tested

**Labels**: low,renderer,test-gap,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (light SSBO header)
- **Location**:
  - `VulkanContext::assemble_camera_and_lights` in `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`: the post-combustion decorate re-sort through `light_resort_scratch`, and the `frame_light_ids.resize`.
  - The loading-stage filter compaction in `App::render_one_frame` (`byroredux/src/app_frame.rs`).
  - `SceneBuffers::upload_lights` in `crates/renderer/src/vulkan/scene_buffer/upload.rs`.
- **Status**: NEW (test gap / duplicated logic). No match in open issues or in a closed search for "light identity" / "history_id".
- **Description**:
  - Since #5055, `previousLightToCurrent[]` in the light-SSBO header comes from `LightHistory::remap(frame, &identities[..count])`. Every light's identity must take exactly the permutation and filtering its `GpuLight` took. Three sites permute or filter the pair:
    1. `collect_lights` decorate-sort. Pinned by the bin test `light_history_identity_survives_animated_priority_reordering`.
    2. The loading-stage `keeps_light` compaction in `app_frame.rs`. A hand-written index compaction, untested.
    3. The renderer's re-sort after `append_combustion_surface_lights`. A second, hand-copied decorate-sort over `(score, GpuLight, [u32;4])`, untested. It runs inside a device-owning method, so no unit test can reach it.
  - I read all three. They are correct today.
  - The only runtime check is `debug_assert_eq!(lights.len(), identities.len())` in `upload_lights`. It checks length, not order, and in release a shorter identity slice panics at `identities[..count]`.
  - The renderer's `frame_light_ids.resize(frame_lights.len(), [0; 4])` runs only inside `if let Some(ref mut volumetrics)`. With volumetrics absent, the lengths match only if the caller kept them matched.
- **Evidence**:
  - `resort.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));` followed by a zip write-back into `frame_lights[directional_count..]` and `frame_light_ids[directional_count..]`.
  - It duplicates `collect_lights`' `sort_scratch.sort_unstable_by(...)` plus its zip write-back.
- **Impact**:
  - Today: none.
  - If a future edit permutes or filters `frame_lights` without `frame_light_ids`, temporal and spatial ReSTIR reservoirs are remapped to the wrong lights. The result is silent shading or flicker; no layout test, lockstep test or validation layer sees it.
  - A length mismatch becomes a release-mode panic in the per-frame upload.
- **Related**: #5055, #4954 (light hash gate), #4942.
- **Suggested Fix**:
  - Extract the two decorate-sorts into one pure helper, e.g. `sort_lights_by_priority_with_ids(&mut [GpuLight], &mut [[u32;4]], directional_count, scratch)`. Unit-test it once (identities follow lights; directional prefix untouched) and call it from both sites.
  - Move the `resize` out of the `if let` so lengths are equal by construction before `upload_lights`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
