# #5187 — REN-D9-2026-10-03-01: The skin compute dispatch and first-sight BLAS BUILD ignore geometry residency. Streamed-in skinned actors are skinned from past the bound vertex SSBO's end and BLAS-built from the result.

**Labels**: critical,renderer,vulkan,shaders,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: CRITICAL. This is the special-rule floor "BLAS/TLAS build with wrong geometry or address". It also involves an out-of-bounds storage-buffer read with `robustBufferAccess` off, the same bug class #4829 fixed on the palette side.
- **Dimension**: Skinning
- **Location**:
  - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`, `record_skinned_blas_refit`: the `dispatches` collection loop, the first-sight `first_sight_builds.push`, and the steady-state `skin_pipeline.dispatch`.
  - `byroredux/src/app_frame.rs`: the `is_geometry_dirty()` masking loop (`command.in_raster = false; command.in_tlas = false;`).
  - `crates/renderer/src/mesh/geometry_ssbo.rs`, `MeshRegistry::is_geometry_resident`.
  - `crates/renderer/shaders/skin_vertices.comp`, `main`: the `src_base` reads.
- **Status**: NEW. Searched "non-resident skinned", "geometry_batch_in_progress skin", "in_tlas skin dispatch", "appended geometry skin compute", and the open list. No match. The 2026-09-27 renderer report's compaction note covers static/LOD BLAS restore only.
- **Description**:
  - **Why meshes are non-resident.** While `WorldStreamingState::geometry_batch_in_progress()` is true (exterior streaming apply / pending / LOD reconcile), the frame driver defers `rebuild_geometry_ssbo`. Appended scene meshes then carry CPU-side `global_vertex_offset`s that lie beyond the bound global vertex buffer. That buffer is created at exactly `pending_vertices.len()` with no slack.
  - **The mask covers raster and TLAS only.** `app_frame.rs` handles this by setting `in_raster = false` and `in_tlas = false` on every draw whose mesh fails `is_geometry_resident`. The doc on `is_geometry_resident` says those draws "must remain out of raster/TLAS until then or they index past the old buffer tail".
  - **The skin chain reads neither flag nor residency.** `record_skinned_blas_refit` collects every draw with `bone_offset != 0`, an existing mesh and `rt_capable`. For a first-seen entity it then does three things:
    1. It creates the `SkinSlot` and queues a first-sight BUILD.
    2. It dispatches `skin_vertices.comp` with `vertex_offset = mesh.global_vertex_offset`, against `input_buffer` = the *bound* global vertex buffer (`(b.buffer, b.size)`).
    3. It records the BUILD from that output.
  - **The shader read is out of bounds.** The shader reads `inputVertexData[(vertex_offset + vid) * VERTEX_STRIDE_FLOATS + …]`, which is past the bound buffer's end. Only `vid < vertex_count` is checked.
  - **The #3372 compaction latecomer case is milder but still wrong.** There the offset lands *inside* the old buffer, so the shader reads another mesh's vertices: valid memory, wrong geometry.
- **Persistence**:
  - The dispatch is the first one for the slot, so `has_populated_output` is promoted at submit.
  - From the next frame the skip gate `slot.has_populated_output && !is_dirty` suppresses both the dispatch and the refit whenever the pose hash is unchanged.
  - Nothing re-arms a slot when the geometry becomes resident. Nothing in `skin_compute.rs` or the context touches `has_populated_output` on a geometry-generation change; `input_generation` only keys the descriptor cache.
  - So a skinned actor whose pose is static from its first frame keeps the out-of-bounds-derived positions indefinitely. Examples: an un-animated corpse, a posed mannequin, a creature with no idle playing.
  - Once the geometry is resident, `in_tlas` is true. The TLAS then instances that BLAS, and `GpuInstance.skinnedVertexAddress` publishes the same output buffer to `ray_hit.glsl` and `caustic_splat.comp` for hit reconstruction.
  - A forced rebuild never happens, because `refit_count` advances only on refits.
- **Animated actors** self-heal on the next dirty frame, since the UPDATE refit rewrites positions. Two side effects remain:
  - **Possible VUID-03663 violation.** If the out-of-bounds read produced NaN X components, those triangles were *inactive* in the BUILD, and an UPDATE with real positions makes them active. That violates `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03663` ("inactive primitives in its srcAccelerationStructure member must not be made active"). They would stay ray-invisible or undefined until the #679 rebuild, about 600–660 dirty frames later.
  - **Degraded BVH.** The BVH topology was built from garbage, so traversal quality is degraded until that rebuild.
- **Evidence**:
  ```rust
  // app_frame.rs — the only residency consumer on the per-frame path
  if ctx.mesh_registry.is_geometry_dirty() {
      for command in &mut self.draw_commands {
          if !ctx.mesh_registry.is_geometry_resident(command.mesh_handle) {
              command.in_raster = false;
              command.in_tlas = false;
          }
      }
  }
  // skinned_blas_refit.rs — dispatch collection: no in_tlas / in_raster / residency test
  for dc in draw_commands.iter() {
      if dc.bone_offset == 0 { continue; }
      if !seen.insert(dc.entity_id) { continue; }
      let Some(mesh) = self.mesh_registry.get(dc.mesh_handle) else { continue; };
      if !mesh.rt_capable { continue; }
      ... SkinPushConstants { vertex_offset: mesh.global_vertex_offset, vertex_count: mesh.vertex_count, .. }
  ```
  ```glsl
  // skin_vertices.comp
  if (vid >= vertex_count) return;
  uint src_base = (vertex_offset + vid) * VERTEX_STRIDE_FLOATS;   // no bound vs. inputVertexData.length()
  ```
  `grep -rn "is_geometry_resident\|in_tlas" crates/renderer/src/vulkan/context/skinned_blas_refit.rs` returns nothing.
- **Impact**:
  - Every exterior streaming transaction that brings in skinned actors (FNV, FO3, Skyrim and FO4 grid crossings with NPCs or creatures) runs the skin compute out of bounds for each such actor's first frames, and records a BLAS BUILD from that output.
  - Out-of-bounds device reads are undefined. Usually they return garbage; possibly they fault the device.
  - Static-pose actors keep a garbage skinned BLAS in the TLAS and garbage hit positions for the rest of their residency. The visible result is wrong or missing RT shadows, reflections and GI and stray ray hits.
  - Animated actors get one corrupted BUILD, plus the 03663 hazard when NaNs are produced.
  - Raster is unaffected: it is masked, and it skins inline from the resident bind pose.
- **Related**:
  - #3372 and SAFE-2026-08-27-01: the residency gate's origin, raster/TLAS only.
  - #3976: the same "never build a skinned BLAS from undefined memory" principle, applied there to `bind_inverses`.
  - #2402 and #3231 add `skin_slot_backs_mesh` / `morph_slot_backs_mesh`. They are slot-sizing guards, not residency guards.
  - #4829 fixed the out-of-range storage-buffer reads with `robustBufferAccess` off on the palette side.
  - #1195 / #1196: the pose-dirty skip that makes the corruption persistent.
- **Suggested Fix**:
  - In the `dispatches` collection loop, skip any draw whose mesh is not `is_geometry_resident`. Skipping it there, before slot creation, also skips the BUILD, as #3976 argues for its own gate.
  - Alternatively, gate on `dc.in_tlas` if every non-RT reason for `in_tlas == false` is acceptable to skip.
  - Either way, add a source-position pin next to `bind_inverse_upload_failure_blas_gate_tests`.
  - Optionally re-arm `has_populated_output = false` for every slot when `geometry_generation` changes, as belt-and-braces.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
