**HEAD**: `f002763b4` · **Baseline**: `AUDIT_RENDERER_2026-09-29.md` (HEAD `9fcfdc3fc`) · **Audited**: Dims 1–12, delta-scoped to `9fcfdc3fc..f002763b4` (202 commits; every dimension had commits on its Paths, Dim 9 on its refit/AS neighbours only) · **Unchanged since baseline (skimmed)**: Dim 9 core (`skin_compute.rs`, `morph_compute.rs`, `render/skinned.rs`, `skin_slot_pool.rs` — no commits); FSR core (`crates/fsr3-sys`, `frame_upscaler.rs`, `upscaling.rs`, `presentation.rs` — no commits)

# Renderer Audit — 2026-10-03 (all 12 dimensions, delta)

This is a standalone `/audit-renderer` run. It audited the 202 commits since the 09-29 baseline. The renderer-relevant delta is:

- **Fix wave.** The follow-up wave to the 09-29 audit, all closed: #5018, #5020, #5022, #5023, #5026, #5028, #5030.
- **Hardening batch.** A large batch of older issues (#4599, #4633, #4722, #4726, #4778–#4784, #4843–#4845, #4874–#4897, #4902–#4925, #5053–#5072, #5087–#5106).
- **Unnumbered feature commits:**
  - Creation-era LSCR loading-cover model stage (`e60911864`);
  - Starfield CDB material Phase 2 (`18fce7e43`);
  - interior exposure retuning (`7d99ba7f0`);
  - two clippy sweeps (`3f0852e08`, `4ad847a81`).

**Method.** The orchestrator dispatched one sub-agent per dimension, at most three concurrently. It verified every CRITICAL and HIGH finding itself against the code before merging (noted per finding), and ran the guard suites itself. **No engine, GPU process or `BYRO_VALIDATION=1` run was launched.** Every Vulkan-facing conclusion comes from source and guard-test reading, and anything needing a device is listed under *Needs-RenderDoc*.

## Executive Summary

| Severity | Count | Findings |
|---|---|---|
| CRITICAL | 2 | D9-01 (skin compute + first-sight skinned BLAS read non-resident geometry past the bound vertex SSBO), D4-01 (failed geometry-SSBO rebuild leaves scene-set and caustic descriptors pointing at freed buffers) |
| HIGH | 2 | D10-01 (Oblivion `__MAX_Default_Light` artifact becomes a scene-wide shadowed white directional), D6-01 (Starfield CDB `TextureReplacement` collapsed slot-agnostically into albedo tint) |
| MEDIUM | 9 | D2-01, D10-02 (both edges of #5018's origin rule), D5-01 (loading-cover stage teardown copy leaks Rapier bodies + failure-arm GPU handles), D9-02 (#679 forced rebuild drops live BLAS first), D9-03 (`updateScratchSize` never read), D6-02, D6-03 (CDB flags inert; keyword classifier re-opened for Starfield), D6-04 (`perturbNormalGrad` Path 2 NaN), D11-01 (0.5 s adaptation never takes effect) |
| LOW | 22 | doc rot, test gaps, ledger rows, hardening (see Findings) |

**Two CRITICALs, both on paths the guards cannot see.**

- **D9-01: the skin chain ignores geometry residency.**
  - `record_skinned_blas_refit` builds its dispatch list from every draw with `bone_offset != 0`. It never consults `in_tlas` or `is_geometry_resident`. The static-BLAS restore path does consult residency (`resources.rs`).
  - During an exterior streaming transaction, a newly streamed skinned actor is therefore skinned from `global_vertex_offset` past the end of the bound vertex SSBO. `skin_vertices.comp` has no length check, and `robustBufferAccess` is off.
  - The actor's first-sight BLAS is built from that output. A static-pose actor keeps the corrupt BLAS, because the pose-dirty skip never re-dispatches it.
  - Orchestrator-verified: the dispatch loop, the shader indexing, and the absence of any residency gate.
- **D4-01: a failed geometry rebuild leaves descriptors naming destroyed buffers.**
  - The dimension agent found this as a HIGH, caustic-only residual of #5064: the latch is one-way.
  - The orchestrator traced it further. The same `if let` with no else arm leaves the *scene* set's bindings 8/9 naming the destroyed buffers. The RT flag drops only on a TLAS-build failure, not on missing geometry.
  - So `triangle.frag`'s ray hits keep reading freed memory through `ray_hit.glsl`. Escalated to CRITICAL as a GPU use-after-free.

**Two HIGHs at translation boundaries.**

- **D10-01 (Oblivion).**
  - 48 vanilla Oblivion meshes, including every ears mesh and so the player body, carry the Max exporter's `__MAX_Default_Light`.
  - Gamebryo scopes it to its node's `effects` subtree. The engine ignores `affected_node_names`, so one full-white, shadow-traced directional lights every cluster.
  - #5123's fix keeps the first copy deliberately, and the runtime baseline now encodes it.
- **D6-01 (Starfield).**
  - The CDB `TextureReplacement` is keyed by object, dropping its slot index. It is applied as `diffuse_color` over whatever colour texture is bound.
  - Orchestrator-verified in `capture_instance` and `apply_cdb_material`.
  - Per-slot semantics come from external CE2 material knowledge; a slot census is pending.

**#5018's light-side origin settles one convention and leaves two edges.**

- **Open geometry (D2-01).** The terminator band now leaks front-lobe light from behind the plane, because the lobes gate on the shading normal and the origin side on Ng.
- **Closed meshes (D10-02).** The wrap, back and thick-translucency lobes are still zeroed by the far wall.
- Both want the same fix: split reflection from transmission visibility.

**The fix wave is sound.** Each 09-29 issue's fix was re-checked against its premise:

- #5018: verified, with the residual edges D2-01 and D10-02.
- #5020, #5022, #5023, #5026, #5028 and #5030: verified.
- #4956/#4957 regression guards hold.
- **No regression of a closed issue found.**

Three new findings are residuals of closed fixes: D4-01 (#5064), D5-03 (#4886) and D7-01 (#903).

**Guard posture: green.**

| Suite | Passed | Failed | Ignored |
|---|---|---|---|
| Renderer lib | 1359 | 0 | 1 (device-only `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`) |
| Bin crate (toolchain 1.96.0) | 2611 | 0 | 48 (data/bench-gated) |
| Core `--features inspect` | 791 | 0 | 0 |
| `byroredux-fsr3-sys` | 8 | 0 | 0 |

- The real-data `cross_game_translation_completeness` was run explicitly (`--ignored`) and **passes** (2.85 s).
- `scripts/check-shader-artifacts.sh`: 36 shaders plus the opaque early-test variant match glslang 11:16.2.0.
- Every skill-named `Guard:` test was found in the test logs and passed, **except `gpu_light_is_80_bytes`, which no longer exists**. It was renamed to `gpu_light_is_64_bytes` when `GpuLight` shrank to 64 B (#5055). This is a stale premise, not a failure.

**Performance.** This report gives no FPS or ms figures. ROADMAP's live Bench-of-record is the stepped-camera refresh at `a37fcba3c`. Its one open regression, **R6a-regress-22**, was re-stated by #5128 as a camera move plus a small residual, and is outside this audit's correctness scope.

## RT Pipeline Assessment

- **Acceleration structures (Dim 1): contracts hold; the new edge cases are lifecycle.**
  - `instance_custom_index` == SSBO slot, and the map is capped at `instance_map_cap` (#4833). `decide_use_update` still keys on addresses plus entity IDs. Both TLAS barriers are present.
  - The new raster-side capacity twins are sound: #4726 `clamp_batches_to_instance_capacity` and the #4722 UI re-clamp.
  - #4884's allocate-before-retire holds at all three scratch sites. #4633's finiteness gate works, but its drop cause is invisible to `rt.integrity` (D1-01).
  - The AS one-time-submit arms ignore #4891's maybe-in-flight policy (D1-02).
- **SSBO indexing and ray queries (Dim 2): clean indexing; one convention edge.**
  - Delta: #5018's origin change (D2-01), #5062's reservoir publish (verified `writeonly` plus `SHADER_WRITE|SHADER_READ`), and #5055's CPU-side identities.
  - The coordinate-space split and the depth convention are unchanged.
- **Skinned AS (Dim 9): CRITICAL D9-01 plus two lifecycle and spec edges.**
  - The forced rebuild drops the live BLAS before its replacement exists (D9-02).
  - The UPDATE scratch is sized from `buildScratchSize` (D9-03; also the TLAS update path).
- **Denoiser and resolve (Dim 7): healthy.**
  - #5087's `frame_params.rs` extraction is behaviour-identical (normalised line-set diff). The post-pass order matches the docs. #4780 and #5064's skip paths clear correctly.
  - SVGF's nearest-tap fallback re-admits the NaN #903 drops (D7-01, dormant).
- **Participating media (Dim 8): healthy.**
  - The #4784 occupancy mask is conservative: same predicate, one-cell dilation bounded by the 256 BU camera-cut.
  - Its sync contract rests on an incidental barrier (D8-02), and two of its guards cannot fail (D8-03).

## GPU-Struct & Memory Assessment

- **Struct layout (Dim 3): no drift.**
  - `GpuLight` 80 → 64 B (#5055, `history_id` gone from all four GLSL mirrors).
  - `GpuTerrainTile` 160 → 176 B (#4903/#4907).
  - `GroundCoverCell`'s pads became live lanes.
  - All guarded; mirror set unchanged (`GpuInstance` 1 + 5, `GpuLight` 1 + 3, `CameraUBO` 5, `GcCameraUBO` prefix).
  - The prose left behind is in D3-01 plus the #5172 addendum. The light ↔ identity lockstep is enforced at three sites, but only one is tested (D3-02).
- **Memory and lifecycle (Dim 5): teardown order and `AllocatorResource` removal hold on every exit path, including `engine.quit`.**
  - The loading cover introduced a second, partial copy of cell teardown (D5-01, D5-02).
  - #4886's transactional recreate leaks samplers on failure (D5-03).
  - Volumetrics' host-visible SSBOs, including the new occupancy masks, are unledgered (D5-04).
- **Pipeline and sync (Dim 4): sound apart from D4-01.**
  - #5057's early-test widening to kinds 0–16 is sound: 6 `discard` sites, all certified out; no `gl_FragDepth`; the early pipeline clones pipeline 0.
  - #4890's immediate old-swapchain retirement and #5072's egui reseed are correct.
- **Presentation (Dim 11): FSR core unchanged; #5030 holds.**
  - #4840 and #5154 mirrors match.
  - 7d99ba7f0's 0.5 s adaptation is overridden every frame by `ExposureTuning`'s 0.2 s (D11-01).

## Findings


### CRITICAL

#### REN-D9-2026-10-03-01: The skin compute dispatch and first-sight BLAS BUILD ignore geometry residency. Streamed-in skinned actors are skinned from past the bound vertex SSBO's end and BLAS-built from the result.


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

#### REN-D4-2026-10-03-01: A failed geometry-SSBO rebuild leaves the scene set's bindings 8/9 and the caustic set's 9/10 pointing at the destroyed global vertex/index buffers. Ray hits keep reading them, because #5064's `geometry_bound` latch is one-way and the per-frame re-point has no `None` arm.
- **Severity**: CRITICAL. This is a GPU use-after-free (undefined behaviour) on the main shading path, not only in the caustic splat. The trigger is narrow: an allocation failure inside `rebuild_geometry_ssbo`. But the low-memory reclaim arm that makes the failure likely is the arm that frees the old pair *first*. The dimension agent filed this as HIGH and caustic-only. The orchestrator traced the scene-set half and escalated it (see Evidence).
- **Dimension**: Sync/Barriers (descriptor validity at draw/dispatch). Cross-ref Dim 5, memory lifecycle.
- **Location**:
  - `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs`: the per-frame re-point `if let (Some(vb), Some(ib)) = (global_vertex_buffer, global_index_buffer) { scene_buffers.write_geometry_buffers(..); caustic.write_geometry_buffers(..) }`. It has no else arm.
  - `crates/renderer/src/vulkan/caustic.rs`: `CausticPipeline::write_geometry_buffers` is the only writer of `geometry_bound[frame_index]`, and it stores `true` only. The reader is `CausticPipeline::geometry_bound`.
  - `crates/renderer/src/mesh/geometry_ssbo.rs`: `rebuild_geometry_ssbo_atomic_fallback` (reclaim arm: `device_wait_idle` → `global_vertex_buffer.take()` + `destroy`; non-reclaim arm: `deferred_destroy.push(.., DEFAULT_COUNTDOWN)`) → `build_geometry_ssbo`, where the `create_device_local_buffer(..)?` failure leaves both fields `None`.
  - `byroredux/src/app_frame.rs`: the `rebuild_geometry_ssbo` `Err` arm only logs `warn!`.
  - `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs`: `patch_camera_rt_flag(.., 0.0)` runs only on a *TLAS build* failure, never on missing global geometry.
  - `crates/renderer/src/vulkan/context/post_passes.rs`: `record_caustic_splat_pass` `match (tlas_handle, geometry_ready)`.
- **Status**: NEW. The caustic half is a residual of CLOSED #5064, whose fix covers only "never written". The scene-set half predates the window. No issue covers the rebuild-failure path; searched `rebuild_geometry_ssbo`, "geometry SSBO rebuild failure descriptor" and "caustic geometry_bound".
- **Description**:
  - **The trigger.** When `build_geometry_ssbo` fails after the old pair was taken, the session continues with `global_vertex_buffer == None`. On the reclaim arm the old pair is already destroyed. On the deferred arm it is destroyed `MAX_FRAMES_IN_FLIGHT` ticks later.
  - **No descriptor re-points or invalidates.** From that frame on, `sync_and_acquire_frame` skips both re-points. Both descriptor sets keep naming the freed `VkBuffer`s:
    - set 1 bindings 8/9 (`GlobalVertices` / `GlobalIndices`, read by `ray_hit.glsl` for every committed hit);
    - the caustic set's bindings 9/10.
  - **Scene shading keeps tracing.** The RT flag stays 1.0, because the TLAS still builds: static BLAS are self-contained and survive the geometry rebuild by design. So `triangle.frag`'s reflection, GI and glass hits reconstruct UVs and normals by indexing `vertexData[...]` through the stale binding. Bindings 8/9 are `PARTIALLY_BOUND`, but that only exempts descriptors that are *never dynamically used*. The code comment says "the None case — no geometry yet / headless — leaves them validly unbound", which is true only for the never-written case, not for a once-written descriptor whose buffer was destroyed.
  - **The caustic splat keeps dispatching.** `geometry_bound(frame)` is still `true`, and `tlas_handle(frame)` is still `Some` (#2673 keeps it).
  - **Contrast:** `record_volumetrics_pass` re-derives the live globals each frame and skips on `None`, which is the safe pattern.
- **Evidence**:
  - `grep geometry_bound crates/renderer/src` finds only `store(true` in `write_geometry_buffers`, plus the reader.
  - The re-point block in `sync_and_acquire_frame.rs` is a bare `if let` with no `else`.
  - In `dispatch_skin_and_cluster.rs`, the only `patch_camera_rt_flag(.., 0.0)` sits in the TLAS-build-failure arm.
  - `ray_hit.glsl` reads `vertexData[(vOff + i0) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS]` on every committed hit.
  - The geometry pass already handles `None` for raster (`global_bound = false`), which shows the state is expected to be survivable.
- **Impact**:
  - Reads of freed device memory from the main fragment shader and the caustic compute, every frame until a retried rebuild succeeds.
  - Possible page fault → `VK_ERROR_DEVICE_LOST`; at best garbage RT hit attributes.
  - This violates VUID-vkCmdDispatch-None-08114 and its draw-time twin (descriptors must be valid when dynamically/statically used).
- **Related**: #5064, #2374, #3443 (reclaim arm), #2673 / #4779 (TLAS handle kept), #4843 (rt-flag lifecycle).
- **Suggested Fix**:
  - Add an else arm to the re-point. On `None`, clear `caustic.geometry_bound[frame]` and drop the frame's RT flag (`patch_camera_rt_flag(.., 0.0)`) so no shader traces against geometry it cannot reconstruct. Alternatively, keep the old pair alive until a replacement exists: build-then-swap, falling back to *not* reclaiming when the replacement allocation fails.
  - Pin both halves with `production_text` scans.
  - Confirm with a fault-injected `build_geometry_ssbo` allocation failure under `BYRO_VALIDATION=1` (Needs-RenderDoc).


### HIGH

#### REN-D10-2026-10-03-01: Oblivion's `__MAX_Default_Light` exporter artifact still reaches the GPU as one full-white, shadow-traced directional that lights the whole scene. Gamebryo scopes it to the ear/hair/statue subtree its root node lists it under.

- **Severity**: HIGH. This is a rendering-correctness problem. In every Oblivion session the first carrier mesh to load adds an unauthored, unit-white (`radiant=[1,1,1]`, `dimmer=1.0`) directional key light to every cluster. That is as bright as the authored XCLL key (`directional_color × 0.6`) or brighter. The player body's `earshuman.nif` is a carrier, so it is present from boot. #5123, also HIGH, was the 4× version of this.
- **Dimension**: Light Animation (light canonical translation)
- **Game Affected**: Oblivion only. A content scan of the meshes archive finds `NiDirectionalLight` in 48 Oblivion meshes, all named `__MAX_Default_Light`, and 0 in FNV or FO3. An `NiPointLight` sanity check on the same scanner found 38 in FNV.
- **Location**: `byroredux/src/cell_loader/spawn.rs` `spawn_nif_lights`. Also `is_known_exporter_artifact_light_name` and the dedup on `world.find_by_name`, which keeps the *first* artifact. Further down: `byroredux/src/render/lights.rs` `collect_lights` → `gpu_light_from_emitter` (`LightKind::Directional` → type `2.0`), `crates/renderer/shaders/cluster_cull.comp` (`lightType > 1.5` → "always affects all clusters"). The scope source: `crates/nif/src/import/types.rs` `ImportedLight::affected_node_names`.
- **Status**: NEW. This is the residual of #3557 and #5123, both CLOSED. Both fixed only the *multiplicity*. #3557 took "the intended synthetic contribution" as its premise, and #5123's fix (`c6dd169f8`) deliberately keeps "only the first artifact light in the process". It also regenerated the Gilded Carafe baseline to `light_count_directional 1`, so the gate now encodes the artifact as expected. #335 (CLOSED) imported the affected-node list and explicitly deferred renderer-side scoping. No issue questions whether the artifact should light the scene at all. Searched: "max_default_light", "exporter artifact light", "NiDirectionalLight", "directional light NIF", "light_count_directional".
- **Description**:
  - **Gamebryo semantics.** A dynamic effect's scope *is* its affected-node list. `gamebryo-v32/Documentation/Programmer/General_Topics/Introduction_to_Dynamic_Effects.htm` says: "Objects that are to be affected by a dynamic effect are registered with that effect using the AttachAffectedNode method. This method causes the effect to affect the entire subtree rooted at the given object."
  - **The vanilla carrier.** `meshes\characters\imperial\earshuman.nif` is NIF v10.0.1.0. Per nif.xml, that version has no on-light `Affected Nodes` list (it is absent between 4.0.0.2 and 10.1.0.0), so the scope is serialized on the node side. Byte-decoding the root `NiNode "EarsHuman"` gives `children (1, 6, 7)` and `effects (6, 7)`. Blocks 6 and 7 are the two `NiDirectionalLight`s: dimmer 1.0, ambient 0, diffuse (1,1,1), specular (1,1,1). In the legacy engine they affect the ear mesh subtree at most.
  - **What the engine does instead.** The importer leaves `affected_node_names` empty. Its doc then states the opposite of Gamebryo: "An empty `Vec` means "no restriction" (the light affects every nearby surface)". `spawn_nif_lights` ignores the list anyway. So the surviving artifact becomes a `LightSource` with `VisibilityMask::for_legacy_local_light()` (= `FULL`, so shadow-traced). `collect_lights` uploads it as a type-2.0 GpuLight in the *point-light suffix*, and `cluster_cull.comp` puts it in every cluster.
  - **Direction is arbitrary.** Its direction is whichever half of the ± key/fill pair loaded first, at identity placement on the loose/actor path. #5123's dump shows `[-0.4467, 0.7444, 0.4963]`, i.e. lit from above. It does not follow any actor.
  - **Collateral.** The `collect_lights` comment "the directional light (if present) is always exactly one entry ... everything from here on is the point-light suffix" is false whenever this light exists. The renderer's `take_while(color_type[3] > 1.5)` in `assemble_camera_and_lights.rs` can also absorb it into the "pinned" prefix when no scene key is present. Both are ordering-only side effects; there is no further correctness impact.
- **Evidence**:
  ```text
  earshuman.nif (Oblivion - Meshes.bsa), header "NetImmerse File Format, Version 10.0.1.0"
  [0] NiNode "EarsHuman"  children (1, 6, 7)  effects (6, 7)
  [6],[7] NiDirectionalLight "__MAX_Default_Light"  dimmer 1.0  diffuse (1,1,1)  spec (1,1,1)
  Carriers (48): characters\{imperial\earshuman, highelf\earshighelf, woodelf\earswoodelf, darkelf\earsdarkelf}.nif,
    14+ hair styles (style01-03, emperor, nordfemalebunches, ...), clutter\key\key.nif,
    architecture\statue\statueimperial02-05 / thesentinel / statueleyawiin01, daedric shrines, priory doors, citadel pieces, menus\*.
  ```
  ```rust
  // crates/nif/src/import/types.rs, ImportedLight::affected_node_names
  /// ... An empty `Vec` means "no restriction" (the light affects every nearby surface).
  ```
  Method: `target/debug/examples/bsa_extract_one` + `dump_nif` (prebuilt), plus a read-only Python BSA v103/v104 content scan and a byte decode of the root NiNode, all in the session scratchpad. No cargo build.
- **Impact**: Oblivion interiors and exteriors get a white key light from a fixed arbitrary direction on top of authored lighting. It casts RT shadows, and it adds a ReSTIR candidate and a GI light to every pixel. Gilded Carafe's runtime baseline now treats this as correct.
- **Related**: #3557, #5123, #335, #4395, #4972. `NiAmbientLight` was checked and dropped: the 17 Oblivion and 8 FNV carriers sampled (`weynondoor01`, `vine01`) have zero diffuse, so `is_spawnable_nif_light` already skips them.
- **Suggested Fix**: Do not spawn NIF-embedded *directional* lights as scene lights. They have no LIGH authority, and every vanilla instance is the Max artifact. At minimum, drop `is_known_exporter_artifact_light_name` matches outright instead of deduplicating them. Fix the `affected_node_names` doc: empty means "scope carried elsewhere (NiNode effects, pre-10.1) or none", never "unrestricted". Reset the Gilded Carafe `light_count_directional` baseline to 0 and confirm it with a live `light.dump` (`/audit-runtime`).

#### REN-D6-2026-10-03-01: CDB `TextureReplacement` is collapsed slot-agnostically into `diffuse_color`, so any slot's flat replacement (normal, roughness, AO, …) tints the albedo, even when a colour texture is also bound

- **Severity**: HIGH. The floor applies: a wrong `Material` comes out of the boundary on Starfield. Confidence in the per-slot semantics is code-structural plus external, and is stated below.
- **Dimension**: NIFAL Material
- **Location**: `crates/sfmaterial/src/index.rs` (`MaterialIndex::capture_instance` `"BSMaterial::TextureReplacement"` arm; `MaterialIndex::collect_slots`); `byroredux/src/asset_provider/material/merge.rs` (`apply_cdb_material`)
- **Status**: NEW. Related: #3398 (open umbrella). No issue mentions TextureReplacement or the CDB flat colour.
- **Description**:
  - The CDB `Components` table carries an `Index` per `(object, type)`. For `MRTextureFile` that `Index` is the texture slot. The code uses it that way (`self.textures…push((row.index, path))`), and census 1 of the `cdb_join_probe` example confirmed slot meaning by filename suffix.
  - A `TextureReplacement` lives on the same texture-set object with its own `Index`. The synthetic fixture models this as `(13, 13, 0)` beside `(13, 10, 0/1/3)`. But `capture_instance` drops `row.index` for this class and stores `flat_colors: HashMap<u32 /*object*/, (rgba, enabled)>` with `or_insert`. The first replacement on the texture set wins, whatever slot it replaces.
  - `apply_cdb_material` then writes it unconditionally: `material.diffuse_color = [r, g, b]`. It does this even when the same lookup also filled `textures.base_color` (`SLOT_COLOR`).
  - `triangle.frag` multiplies `albedo *= vec3(mat.diffuseR, mat.diffuseG, mat.diffuseB)` (the same product appears at the RT hit sites). The flat colour is therefore applied as a tint over the texture, not "INSTEAD of any texture" as the field doc and the comment in `apply_cdb_material` claim.
  - In Starfield's CE2 material model, each slot can carry its own replacement colour (fo76utils/NifSkope `CE2Material::TextureSet::textureReplacements[]` plus `textureReplacementMask`). That source is external and not checked into `/mnt/data/src/reference`. A flat-normal (≈0.5, 0.5, 1.0), roughness or AO replacement therefore becomes a blue, grey or white albedo tint.
- **Evidence**: `capture_instance` reads `"BSMaterial::TextureReplacement" => { … self.flat_colors.entry(row.object).or_insert((color, enabled)); }`, and `row.index` is unused in that arm. The guarding test does not catch it: `mat_path_merges_cdb_authored_textures_when_indexed` asserts that `base_color` is bound **and** that `diffuse_color == [0.25, 0.5, 0.75]`, which pins the texture × flat-colour product.
- **Impact**: For Starfield only, any material whose first-walked texture set carries a non-colour-slot replacement gets a fabricated albedo tint. The commit's own census counts 36,866 replacement instances. Their slot distribution is unmeasured.
- **Trigger / verification**: Extend census 4 of the `cdb_join_probe` example to key `TextureReplacement` by `Components.Index`. If any instances sit on slot ≠ 0, or the slot-0 replacements coexist with a slot-0 `MRTextureFile`, the finding is confirmed. This audit did not run the probe: available RAM was about 7 GB, and the memory note says to avoid big-RSS probes in-session.
- **Related**: REN-D6-2026-10-03-02, #3398, nifal.md parked-slot table (#4429).
- **Suggested Fix**: Capture `(row.index, color, enabled)` and keep the replacement per slot. Translate only the `SLOT_COLOR` replacement, and only when no colour texture landed in that slot. Park the other slots like the parked texture kinds, or route them to their role once the role exists. Rewrite the test to cover a non-zero-slot replacement.


### MEDIUM

#### REN-D2-2026-10-03-01: #5018's light-oriented origin removes the geometric-horizon self-occlusion the old viewer-side origin supplied. In the shading-terminator band (N·L > 0 ≥ Ng·L), an opaque open or two-sided surface now takes front-lobe light from a light behind its own triangle plane

- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**:
  - `crates/renderer/shaders/triangle.frag`, ReSTIR finalize: `vec3 rayOrigin = offsetRayOriginForDirection(fragWorldPos, geometricNormal, L);`. The compiled-out legacy-WRS arm has the same line.
  - `crates/renderer/shaders/include/lighting.glsl`, `shadowableLightRadiance`: `rawNdotL = dot(N, L)` against the shading normal; there is no geometric-normal term.
- **Status**: NEW. This is a side effect of the CLOSED #5018 fix (`b1c619fa7`), not a regression of an earlier fix. Before the change the origin was always on the viewer side: `fragWorldPos + N_bias * 0.05` until `2d904acd7`, then `shadowOffsetNormal`. Searched "shadow terminator" and "light leak wall": no issue.
- **Description**: The #5018 commit (and #5018's own suggested fix) says the direction-aware origin is "identical to the old viewer-side offset for front-lit surfaces". That holds only when "front-lit" is measured against the geometric normal `geometricNormal` (the `dFdx`/`dFdy` triangle plane), which is what `offsetRayOriginForDirection` tests. The lobes in `shadowableLightRadiance` are gated on the normal-mapped, viewer-flipped shading normal `N`, so there are two cases:
  - **N·L > 0 but Ng·L < 0** (light just behind the triangle plane; the normal map or the smooth vertex normal tilts N toward it). Before the change, the viewer-side origin made the ray cross its own triangle, so visibility was 0. That self-hit acted as a de facto geometric-horizon clamp.
  - **Same case after the change.** The origin is placed on the back side and the ray moves away from the plane. On closed meshes the far wall still occludes. On open geometry, such as two-sided cards and single-sided shells with nothing behind them within the light's reach, the ray reaches the light. The diffuse, specular and rim lobes are then added for a light that is geometrically behind the surface.

  `shadowableLightRadiance` takes no geometric normal, and nothing in the ReSTIR finalize gates on `dot(geometricNormal, L)`; grep finds no horizon test. The secondary-hit NEE paths stay consistent: `reflectionHitIrradiance` and the GI-hit loop offset along the oriented face normal and gate `giLightSample` on `dot(n, L) > 0` with that same normal. Only the primary direct path now offsets by Ng and shades by N.
- **Evidence**:
  ```glsl
  // triangle.frag (post-b1c619fa7)
  vec3 rayOrigin = offsetRayOriginForDirection(fragWorldPos, geometricNormal, L); // side chosen by Ng·L
  // lighting.glsl shadowableLightRadiance
  float rawNdotL = dot(N, L);            // N = normal-mapped shading normal, flipped to the viewer
  float NdotL = max(rawNdotL, 0.0);      // front lobes live wherever N·L > 0, regardless of Ng·L
  ```
  Under ray queries without a facing-cull flag (#4580), back faces of closed meshes still occlude, so the leak is confined to open or two-sided geometry.
- **Impact**: Visual only. Bumps on a back-lit opaque two-sided card, fence or sign, or on a single-sided shell piece with a light behind it, pick up speckled front-lobe light inside the shadow-fade range. The amount is bounded by how far N tilts past the plane and by the light's attenuation. It is not quantified: no capture was possible. The ReSTIR pHat already counted this band, so selection is unchanged and only its visibility flipped from 0 to roughly 1.
- **Related**: #5018, #4946, #4580. The thick-object and `MAT_FLAG_SOFT_LIGHTING` wrap note below is a separate observation, not filed.
- **Suggested Fix**: Restore the horizon clamp for the lobes that should not cross the plane. For example, zero the non-back lobes when `dot(Ng_viewer, L) < 0`: pass the viewer-oriented geometric normal into `shadowableLightRadiance`, or split its result so only the translucency/back-light terms use the light-side origin. Then pin it with a shader-contract test. Accept on a before/after capture of an opaque, normal-mapped, two-sided card lit from behind.

#### REN-D10-2026-10-03-02: The soft-lighting wrap, back-light and thick-translucency lobes are binary-traced against the fragment's own closed body, so they are zero for every traced light inside `shadowFade` and switch on, unshadowed, past it. This is the "separate decision" #5018 deferred.

- **Severity**: MEDIUM. This is a BSDF/visibility convention mismatch. It is visual only, and limited to flagged materials. Those include Skyrim skin and the tint family (`SLSF2_Soft_Lighting`, see #3458) and FO4 BGSM `translucency_thick_object` skin.
- **Dimension**: Disney BSDF / Soft Shadows
- **Location**: `crates/renderer/shaders/include/lighting.glsl`:
  - `shadowableLightRadiance`: `bethesdaDiffuseLightFactor` (wrap non-zero for `rawNdotL ∈ (-width, 0)`), `bethesdaBackFactor` (`max(-rawNdotL, 0)`), and the `MAT_FLAG_TRANSLUCENCY` block's `MAT_FLAG_TRANSLUCENCY_THICK_OBJECT` arm.

  `crates/renderer/shaders/triangle.frag`, ReSTIR finalize:
  - `offsetRayOriginForDirection(fragWorldPos, geometricNormal, L)` (#5018);
  - `visibility = mix(vec3(1.0), transmissionFrame, shadowFade)`.

  `crates/renderer/src/vulkan/acceleration/predicates.rs`, `shadow_mask_for_instance`: actors stay in `VISIBILITY_LAYER_DYNAMIC_ACTOR`, which is inside the FULL mask.
- **Status**: NEW. This is not a delta regression: before #5018 the fragment's own triangle zeroed these lobes, and now the far wall does. Dim 2 routed it here (`dim_2.md`, "Thick-object back lobes and the soft-light wrap still self-occlude"). No issue tracks it. Searched: "soft lighting", "thick object", "self-occlusion", "translucency", "back lighting", "wrap lighting", "subsurface". #5018's suggested fix said "Thick objects would still self-occlude ... whether their SSS should skip the own instance is a separate decision".
- **Description**:
  - Each of these lobes is non-zero only where `rawNdotL < 0` (the wrap only in `(-width, 0)`). There, on a watertight mesh, `Ng·L < 0` too, unless a normal map tilts N away from Ng. So #5018's direction-aware origin starts the ray just inside the body, and the ray travels through the interior toward the light.
  - Ray queries carry no facing-cull flag (#4580). So the far wall commits as an opaque hit in `VISIBILITY_MASK_ALL_OPAQUE`, and `traceShadowTransmittanceDetailed` returns `vec3(0.0)`. Visibility is therefore 0 for exactly the configurations these lobes model: the soft terminator past N·L = 0, back-lit skin and wax.
  - This holds for every light whose `visibilityMaskNeedsTrace` is true. That is every legacy light (`for_legacy_local_light()` = FULL) plus the sun and the XCLL key, and it holds inside `SHADOW_FADE_START` (8 000 BU). Between 8 000 and 12 000 BU the `mix(1, V, shadowFade)` blend fades the lobes in unshadowed, so they switch on with distance.
  - In the reference content model most local lights are not shadow casters. FO3/FNV author zero projection/shadow flags, and Skyrim+ shadow-cast only flagged LIGHs. So these lobes were authored to be visible under ordinary room lights.
  - Secondary effect: ReSTIR pHat includes these lobes. A light that sits just past the terminator, or behind the body, is selected in proportion to a contribution its ray always zeroes. The estimate stays unbiased, but the reservoir sample is spent on a guaranteed-dark candidate, which adds noise on characters.
- **Evidence**:
  ```glsl
  // lighting.glsl — bethesdaDiffuseLightFactor
  float wrapped = max((rawNdotL + width) / (1.0 + width), 0.0);   // > 0 for rawNdotL in (-width, 0)
  // bethesdaBackFactor / translucency
  return max(-rawNdotL, 0.0) * clamp(strength, 0.0, 4.0);
  float backDotL = max(-rawNdotL, 0.0);
  // triangle.frag — ReSTIR finalize (#5018)
  vec3 rayOrigin = offsetRayOriginForDirection(fragWorldPos, geometricNormal, L); // inside a closed body when Ng·L < 0
  visibility = mix(vec3(1.0), transmissionFrame, shadowFade);                     // lobe returns unshadowed past 12 000 BU
  ```
- **Impact**: Skyrim characters lose the authored soft terminator (`lightingEffect1` wrap, with the slot-2 mask) under every light at any practical viewing distance. FO4 thick-object skin SSS renders no back-lit transmission. Thin open cards (foliage, hair) are unaffected, since #5018 already fixed those.
- **Related**: #5018, #4946, #3574, #3458, #4580; REN-D2-2026-10-03-01 (the opposite-sign leak on *open* geometry from the same origin rule).
- **Suggested Fix**: Make the visibility convention per-lobe instead of one binary ray for the whole BRDF. Split `shadowableLightRadiance`'s return into a reflection part and a transmission part (back-light, translucency, and the wrap's `rawNdotL < 0` excess). Trace the transmission part with the fragment's own instance skipped (candidate-loop instance-id test), or leave it unshadowed with the existing `shadowFade` semantics. Then pin the split with a `shader_contract` test. Accept on a before/after capture of a Skyrim soft-lit head beside a cell light, and of an FO4 thick-translucency surface lit from behind (Needs RenderDoc). This pairs naturally with REN-D2-2026-10-03-01's fix, which needs the same split.

#### REN-D5-2026-10-03-01: the loading-cover model stage has its own copy of the cell-teardown release path, and the copy misses Rapier bodies and skin/morph slots; its post-spawn failure arm frees no GPU handles at all

- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle
- **Location**: `retire_stage` and `LoadingScreen::spawn_model_stage` (`byroredux/src/loading_screen.rs`); the canonical path is `release_entities` / `release_entities_timed` (`byroredux/src/cell_loader/unload.rs`).
- **Status**: NEW. This absorbs Dim 1's REN-D1-2026-10-03-03, which independently found the `stage_camera == None` arm leaking the stage's mesh, BLAS and texture refs; that ID is retired into this one. The orchestrator confirmed `retire_stage` never calls `release_victim_rapier_bodies`. Cross-owner with `/audit-physics`, since the leaked objects include Rapier bodies.
- **Description**:
  - e60911864 (2026-10-02 16:35) added `retire_stage`, a hand-written subset of cell teardown: it collects victim GPU handles, drops BLAS for meshes whose last holder goes, then calls `drop_meshes` and `drop_textures`.
  - Four and a half hours later, f78e018ac (#5028) extracted that same teardown into `cell_loader::unload::release_entities`, so that "gear release and cell teardown share one despawn + GPU-handle path". `retire_stage` was never moved onto it.
  - Next to `release_entities_timed`, the copy omits:
    1. `release_victim_rapier_bodies`. The stage NIF is spawned through `load_nif_bytes` → `spawn_nif_nodes`, which inserts `CollisionShape` + `RigidBodyData` on any node with bhk collision. `physics_sync_system` registers newcomers before it steps, and the scheduler keeps running at `dt = 0.0` while the cover is up (`app_events.rs` `about_to_wait`: `if self.loading_screen.active() { 0.0 }`). The stage's fixed bodies are therefore created. `collect_newcomers` has no stage filter, and `crates/physics` has no orphan sweep, so the despawn leaves them in the `PhysicsWorld`.
    2. `queue_skin_unload_victims` and `pending_morph_unload_victims`. Skinned or morphed stage meshes are reclaimed only by the idle-eviction pass, not by the same frame's drain.
    3. `finish_unload_batch`: `shrink_storages`, plus a BLAS-scratch shrink once the stage's BLAS are gone.
  - Separately, `spawn_model_stage`'s `stage_camera(...) == None` arm runs *after* `load_nif_bytes` has registered meshes and textures and built their BLAS (`count > 0`). That arm calls `despawn_subtree`, whose own doc says it is for spawn failure paths "before any mesh was registered". Every mesh, BLAS and texture refcount the stage took is leaked.
  - That arm is reachable from data: `stage_pose` passes `trns.scale` / `initial_scale` through unvalidated, so a zero or non-finite authored scale, a non-finite translation, or point-sized geometry gives `radius <= EPSILON`. The `count == 0` arm has the same shape for any particle-emitter textures `spawn_nif_particle_emitters` acquired.
- **Evidence**:
  ```rust
  // loading_screen.rs retire_stage — the whole release:
  let (mesh_drops, texture_drops, _terrain_slots) =
      crate::cell_loader::collect_victim_gpu_handles(world, &victims, fallback_tex);
  world.despawn_batch(victims);
  /* … drop_blas for freed handles … */
  ctx.mesh_registry.drop_meshes(&mesh_drops);
  ctx.texture_registry.drop_textures(&ctx.device, &texture_drops);
  // spawn_model_stage, after load_nif_bytes returned count > 0:
  let Some(camera) = stage_camera(&posed, fov_y) else {
      despawn_subtree(world, root);   // no GPU release
  ```
- **Impact**:
  - Each Skyrim or FO4 door or save cover that shows a collision-bearing model leaves its fixed Rapier bodies behind. They are invisible but still collide, at the stage pose (menu-space translation converted to Y-up) in the destination world. They also grow the broad-phase without bound over a session.
  - This is a per-user-action leak, not per-frame.
  - The degenerate-pose arm leaks one model's worth of mesh, BLAS and texture refs per occurrence.
  - Not done: a vanilla census of which LSCR `NNAM` / `TNAM` models carry bhk collision. That needs an archive extract, and building is out of scope for this run.
- **Related**: #5028 (the extraction this copy predates); #1520 (the Rapier-on-despawn rule); #1003 / #3231 (skin/morph unload queueing).
- **Suggested Fix**:
  - Route `retire_stage` through `cell_loader::unload::release_entities`, adding a log label parameter in place of its hard-coded "gear release". Delete the copy.
  - Release the stage subtree through the same call in the `stage_camera == None` arm, or validate the pose (finite, non-zero scale) before `load_nif_bytes`.
  - Optionally exclude `LoadingModelStage` subtrees from physics registration altogether.

#### REN-D9-2026-10-03-02: The #679 forced rebuild drops a working skinned BLAS before its replacement exists. A failed rebuild leaves the actor out of the TLAS until an unrelated eviction.


- **Severity**: MEDIUM. The error path is recoverable but unhandled. The effect is visual only: one actor's RT shadows, reflections and GI are lost.
- **Dimension**: Skinning
- **Location**:
  - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`, `record_skinned_blas_refit`: the `if accel.should_rebuild_skinned_blas(entity_id) { … accel.drop_skinned_blas(entity_id); }` arm, the `failed_skin_blas.insert` in the build-result `Err` arm, and the `failed_skin_blas.clear()` in the eviction branch.
  - `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`, `build_skinned_blas_batched_on_cmd`: the Phase 1 result-buffer / AS-create `Err` arms, the Phase 2 scratch-grow `Err` arm, and the Phase 4 `self.drop_skinned_blas(p.entity_id)` before insert.
- **Status**: NEW. Searched "skinned BLAS rebuild drop", "should_rebuild_skinned_blas", "failed_skin_blas", "refit threshold rebuild fails". #4884 (CLOSED) fixed the same shape for the scratch buffer only.
- **Description**:
  - **Drop happens before the attempt.** Once `refit_count >= skinned_blas_refit_limit(entity)`, the first-sight loop calls `drop_skinned_blas` immediately. The old BLAS goes onto `pending_destroy_blas` with `DEFAULT_COUNTDOWN`, so its memory is still held for two more frames. The entity is then queued as a fresh BUILD.
  - **What happens on failure.** If that BUILD fails (result-buffer allocation, AS create, or the scratch grow #4884 just made non-destructive), three things follow:
    - the entity is inserted into `failed_skin_blas`;
    - the refit loop skips it (`!accel.has_skinned_blas`);
    - `build_tlas` omits it (`missing_skinned_blas`).
  - **Retry is gated on an unrelated event.** The retry is suppressed until `failed_skin_blas.clear()`, which runs only when the SkinSlot eviction pass evicts *something*. In a stable interior where every actor stays in view, that never happens.
  - **Why the BUILD is likely to fail here.** The replacement must be allocated while the dropped BLAS's memory is still pending destroy. Under VRAM pressure, an allocation that the in-place UPDATE never needed is exactly the one likely to fail.
  - **The phases already support build-then-swap.** Phase 4 already calls `drop_skinned_blas` just before inserting the new entry (#2481), so the call-site pre-drop is not needed for correctness on success.
  - **The rebuild is routine.** With #3669's jitter, every moving actor passes through this window every 600–660 dirty frames.
- **Evidence**:
  ```rust
  if accel.should_rebuild_skinned_blas(entity_id) {
      log::info!(…"refit chain reached {} frames, dropping for fresh BUILD (#679)"…);
      accel.drop_skinned_blas(entity_id);              // live BLAS gone here
  }
  let needs_blas = accel.skinned_blas_entry(entity_id).is_none();
  …
  Err(e) => { log::warn!(…"first-sight BLAS build failed"…); self.failed_skin_blas.insert(entity_id); }
  ```
  The #4884 commit message names the outcome it was fixing ("freezing RT shadows/reflections/GI on animated NPCs under exactly the VRAM pressure that triggers the shrink"). The forced-rebuild path reaches the same outcome by a different route.
- **Impact**: Under VRAM pressure, a moving NPC that hits its periodic rebuild can permanently (per cell stay) lose its RT shadow, reflection and GI contribution. Without the rebuild it would only have a degraded-quality BVH. Raster is unaffected. Telemetry shows it as `missing_skinned_blas` plus one WARN.
- **Related**: #679 (forced rebuild), #3669 (jitter), #4884 (scratch allocate-before-retire), #2481 (Phase 4 drop-before-insert), #2802 (`failed_skin_blas`).
- **Suggested Fix**:
  - Do not pre-drop on the rebuild arm. Queue the entity for BUILD while its old entry stays live, and let Phase 4's existing `drop_skinned_blas` retire the old BLAS only after the new one is recorded.
  - On `Err`, keep refitting the old BLAS (reset or saturate `refit_count`) instead of recording it in `failed_skin_blas`.
  - Add a source-order pin.

#### REN-D9-2026-10-03-03: The skinned UPDATE refit assumes `updateScratchSize ≤ buildScratchSize`, which the spec does not guarantee; `updateScratchSize` is never read


- **Severity**: MEDIUM.
  - This is a latent spec violation. If a driver returns `updateScratchSize > buildScratchSize`, every skinned refit violates VUID-…-pInfos-12259 and overruns the shared scratch: HIGH floor, plus a GPU-side overrun.
  - No such driver has been observed. The dev 4070 Ti and RADV values have not been dumped.
- **Dimension**: Skinning (AS scratch; the TLAS UPDATE sibling is Dim 1)
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`:
    - `build_skinned_blas_batched_on_cmd`: `max_scratch_size = max_scratch_size.max(sizes.build_scratch_size)`, and `BlasEntry { build_scratch_size: p.build_scratch_size, … }`.
    - `refit_skinned_blas`: `debug_assert!(scratch_buffer.size >= entry.build_scratch_size)` and the "UPDATE scratch ≤ BUILD scratch" comment.
  - `crates/renderer/src/vulkan/acceleration/memory.rs`, `shrink_blas_scratch_to_fit`: `shared_blas_scratch_peak(… e.build_scratch_size …)`.
  - Sibling: `crates/renderer/src/vulkan/acceleration/tlas.rs`, comment "Scratch is sized for BUILD which is >= UPDATE per Vulkan spec".
- **Status**: NEW. Searched "updateScratchSize", "update_scratch_size", "update scratch", "UPDATE scratch". The only hit is #247 (TLAS BUILD-vs-UPDATE mode, unrelated). AUDIT_PERFORMANCE_2026-08-12 PERF-D3-01 (#2460) questioned the shrink's static-only peak, not this premise.
- **Description**:
  - **The query result is discarded.** `vkGetAccelerationStructureBuildSizesKHR` returns both `buildScratchSize` and `updateScratchSize`. The skinned path keeps only the former: it sizes `blas_scratch_buffer` from it, stores it in `BlasEntry::build_scratch_size`, the shrink peak walks it, and the refit's only size check (a `debug_assert!`) compares against it.
  - **What the spec requires.** The UPDATE's requirement is VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-12259 (from `/usr/share/vulkan/registry/validusage.json`): for mode UPDATE, `[scratch, scratch + N)` must lie in one buffer, "where N is given by the updateScratchSize member … returned from a call to vkGetAccelerationStructureBuildSizesKHR with an identical VkAccelerationStructureBuildGeometryInfoKHR".
  - **The premise is undocumented.** Nothing in the spec relates the two sizes. Three code comments state "UPDATE scratch ≤ BUILD scratch" as fact, one of them "per Vulkan spec".
  - **Inconsistent with the rest of the file.** The surrounding code is careful about the other scratch VUIDs: alignment (#1386 / 03710), the serialize barrier (#1790), and allocate-before-retire (#4884).
- **Evidence**: `grep -rn "update_scratch_size\|updateScratchSize" crates/ byroredux/ docs/` returns nothing. `sizes.build_scratch_size` is the only `sizes.*scratch*` field read in `blas_skinned.rs`.
- **Impact**:
  - On a driver where the premise fails, every skinned refit overruns `blas_scratch_buffer`, a GPU-side write past the allocation.
  - The shrink can compound it, since it sizes to the build-size peak.
  - It is invisible to `cargo test`, and invisible to the validation layer on hardware where the premise happens to hold.
- **Related**: #2460 (union peak), #1386 (alignment padding), #4884, and Dim 1's TLAS UPDATE path (same assumption in `tlas.rs`).
- **Suggested Fix**:
  - Store `max(build_scratch_size, update_scratch_size)` from the query that already runs, either in the existing field or in a new `scratch_requirement`. Use it for the batch max, the shrink peak and the refit `debug_assert!`.
  - Correct the three comments.
  - The TLAS site can take the same one-line change.

#### REN-D6-2026-10-03-02: The CDB settings arm lands three flags their consumers cannot act on: `IsGlass` → `thin_glass` only, `UseSSS` → a zero-colour translucency lobe that still buys back-light rays, and alpha test ignores `HasOpacity`

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/asset_provider/material/merge.rs` (`apply_cdb_material`); consumers are `byroredux/src/helpers.rs` (`classify_glass_into_material_with_provenance`), `crates/renderer/shaders/triangle.frag` (`isThinGlass`, `sssGate`) and `crates/renderer/shaders/include/lighting.glsl` (translucency block).
- **Status**: NEW (related #3398)
- **Description**:
  1. **Glass.** `cdb_mat.is_glass == Some(true)` sets only `material.thin_glass`.
     - `thin_glass` is not a glass-classifier input. `classify_glass_into_material_with_provenance` takes `bgem_glass`, the keyword match and the provenance pair.
     - The shader honours `MAT_FLAG_THIN_GLASS` only under `isGlass` (`bool isThinGlass = isGlass && …`).
     - The result: an authored-glass CDB material without a glass keyword never becomes `MATERIAL_KIND_GLASS`, and the flag is inert.
     - One that is keyword-classified is forced onto the *thin-shell* variant. In the BGEM arm that variant means `non_occluder` (`bgem_uses_thin_glass_behavior`), not "is glass". A closed bottle therefore renders as a thin sheet.
  2. **Translucency.** `UseSSS` sets `has_translucency` and `translucency_transmissive_scale`, but leaves `translucency_subsurface_color` at the default `[0,0,0]` and `mix_albedo` false.
     - `lighting.glsl` multiplies the lobe by `subsurfaceCol`, so it evaluates to exactly 0.
     - `triangle.frag`'s `sssGate = max(-rawNdotL, 0) * translucencyTransmissiveScale` ignores the colour, so every back-facing light passes the early-out and traces visibility for a zero term.
  3. **Alpha.** Alpha test is enabled whenever `AlphaTestThreshold > 0`. `CdbMaterial::has_opacity` (`AlphaSettingsComponent.HasOpacity`) is parsed, but no code in `byroredux/src` reads it, so an explicit `HasOpacity = false` does not gate the test. The test samples base-colour alpha, while Starfield's opacity source (slot 2) is parked.
- **Evidence**: grep shows `thin_glass` has no reader in `helpers.rs`; `has_opacity` has zero hits under `byroredux/src`. None of the three paths has a test: the synthetic CDB fixture carries no alpha, effect or translucency component, and `starfield_mat.rs` asserts none of them.
- **Impact**: Authored Starfield glass is mostly not glass. Authored SSS shades nothing but costs rays. Opaque materials can enter the alpha-test path.
- **Suggested Fix**: Route `IsGlass` to the classifier's positive glass signal, as `bgem_glass` does, and set `thin_glass` only from an authored thin/non-occluder fact. Do not set `has_translucency` until a subsurface colour source is translated, or have `sssGate` include the colour. Gate `alpha_test` on `has_opacity != Some(false)`. Add fixture components for all three.

#### REN-D6-2026-10-03-03: CDB hits now feed the CDB colour path into `classify_pbr_keyword`, so Starfield metalness and roughness become filename guesses (eyes, creatures and fruit bins included) where the source authors them

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`translate_material`: `texture_path = textures.base_color`, NaN sentinel), `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`, `classify_pbr_keyword`), `byroredux/src/asset_provider/material/merge.rs` (`apply_cdb_material`)
- **Status**: NEW. This is the #4941 shape on a new producer; related #3398 and #2707.
- **Description**:
  - Starfield `material_reference` stubs leave both overrides `None`, which reaches `translate_material` as NaN, and `resolve_pbr` then runs the keyword classifier on `texture_path`.
  - Before 18fce7e43 a stub had no texture path, so it landed on the terminal arm.
  - Now a CDB hit fills `base_color` from the CDB, and the keyword arms fire on it. `metal/steel/iron` gives metalness 0.9 and roughness 0.55. `gold/silver/bronze/copper` gives 0.95 and 0.25.
  - This happens although Starfield authors metalness and roughness explicitly, in the metal and rough slots (parked) and in `MaterialParamFloat` (deliberately untranslated). The `apply_cdb_material` doc refuses to guess a param index because "a guess would poison … every Starfield surface", yet the classifier guess now lands anyway.
  - The merge comment at the `.mat` gate still says Phase 2 "should overwrite … with CDB-authored data". Phase 2 does not.
- **Evidence**: A census of `Starfield - Textures*.ba2` `*_color.dds` (prebuilt `ba2_grep`, read-only) found 275 of 12,581 colour maps on a metal arm: 226 metal-word and 49 gold/silver-word. They include clear misfires: `actors\human\faces\eyes\iris_iron_color.dds` (eyes become a 0.9 conductor), `bipeda_silverfish_*_color.dds` (a creature becomes a 0.95 conductor), and `ak_bin_metal01_fruits_color.dds`. Per #4941, a keyword conductor with no specular authority renders as roughly 10% diffuse with no highlight.
- **Impact**: About 2% of Starfield colour maps resolve to fabricated conductors. This is the "chrome" class, produced by the boundary rather than by missing textures.
- **Suggested Fix**: On a CDB hit (`apply_cdb_material`), set dielectric-neutral overrides (`metalness_override = Some(0.0)`, roughness at the classifier's no-data neutral) until `MaterialParamFloat` or the metal/rough roles are translated. Alternatively, have `resolve_pbr`'s backstop skip keyword arms when an external material resolved. Pin it with a CDB fixture whose colour path contains `iron`.

#### REN-D11-2026-10-03-01: the #5158 change of the adaptation constant from 0.2 s to 0.5 s has no effect; `ExposureTuning::default()` still holds 0.2 s and overwrites the renderer every frame

- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation
- **Location**:
  - `ExposureTuning::default` in `byroredux/src/components.rs` sets `adaptation_seconds: 0.2`.
  - The per-frame push in `App::render_one_frame`, `byroredux/src/app_frame.rs`: `ctx.exposure_adaptation_seconds = exposure.adaptation_seconds;`.
  - `VulkanContext::new` in `crates/renderer/src/vulkan/context/init.rs` sets `exposure_adaptation_seconds: 0.5`.
- **Status**: NEW. It was introduced by 7d99ba7f0. Related to the OPEN #5158, which this commit was partly meant to fix.
- **Description**:
  - 7d99ba7f0 raised the renderer's eye-adaptation constant from 0.2 s to 0.5 s. Its stated aim was to stop per-frame meter jitter from the MC residual pumping the exposure (#5158).
  - The engine does not run with the renderer's own field, though:
    - `boot::build_world` always inserts `ExposureTuning::default()`.
    - `App::new` reseeds only `auto` and `agx` from `RendererConfig`.
    - `render_one_frame` copies `ExposureTuning.adaptation_seconds` into `ctx.exposure_adaptation_seconds` unconditionally on every frame, including frame 0.
  - The live constant is therefore still 0.2 s, and the 0.5 s in `init.rs` is dead.
  - The `ExposureTuning` docstring says "the resource defaults match the renderer's own", which is no longer true.
  - Two comments now describe a default that does not run:
    - the #4590 comment in `build_and_upload_instances.rs`: "(1.0 s at the 0.5 s default, N = 2)";
    - the loading-cover comment in `app_frame.rs`: "auto adaptation resumes (0.5 s τ)".
- **Evidence**:
  - `components.rs`: `impl Default for ExposureTuning { … adaptation_seconds: 0.2, … }`. Blame: d54382415, which predates 7d99ba7f0.
  - `app_frame.rs`: inside `if let Some(exposure) = self.world.try_resource::<ExposureTuning>()`, the line `ctx.exposure_adaptation_seconds = exposure.adaptation_seconds;`.
  - `main.rs` `App::new`: only `tuning.auto = …; tuning.agx = …;`.
  - The `exposure` console status prints `speed = 0.20 s` at boot.
  - No test ties the two defaults together. `git grep adaptation_seconds -- byroredux/src` finds only the struct, the console command and the push.
- **Impact**:
  - The anti-pumping half of the #5158 tuning never shipped.
  - Any A/B or live calibration that credits its result to the "0.5 s τ" measured 0.2 s.
  - Workaround: run `exposure speed 0.5` each session.
  - `fixed_exposure: 0.85` is a second hand-typed copy of `DEFAULT_EXPOSURE`. It matches today, but it is the same drift class.
- **Related**: #5158 (OPEN), #4590 (per-FIF alpha), d54382415.
- **Suggested Fix**:
  - Add a `DEFAULT_ADAPTATION_SECONDS` const in `crates/renderer/src/vulkan/exposure.rs`. Use it in both `init.rs` and `ExposureTuning::default`, and make `fixed_exposure` read `DEFAULT_EXPOSURE`.
  - Add a bin test that asserts `ExposureTuning::default()` matches the renderer constants.
  - Correct the two comments.

#### REN-D6-2026-10-03-04: `perturbNormalGrad` Path 2 (screen-derivative TBN) has no zero-length guard, and the comment justifying that misses the T_raw = 0 case

- **Severity**: MEDIUM (defence-in-depth; NaN propagation)
- **Dimension**: Tangent-Space
- **Location**: `crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormalGrad`, Path 2)
- **Status**: NEW. #2815 guarded Path 1 only; #3984 fixed the anisotropic builder.
- **Description**:
  - Path 2 computes `T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y)` and then `normalize(T - dot(T,N)*N)`, with no length check.
  - The Path-1 comment says Path 2 "needs no equivalent guard because its derivative-built T is already tangent-plane by construction". That only covers T ∥ N. When V is constant across the pixel quad (`dUVdx.y == dUVdy.y == 0`, for example a U-only strip mapping or float-equal V at large tiled UVs), the raw numerator is the zero vector and `normalize` returns NaN.
  - The NaN reaches the shaded normal, the G-buffer `octEncode`, RT origins, SVGF/TAA history and the bloom pyramid.
  - The sibling derivative builder `parallaxDisplaceUV` guards the same quantity (`dot(T, T) < 1e-8 … return uv`). `getRayHitTangentFrame` guards it as well.
- **Trigger Conditions**: A normal-mapped draw whose vertex tangent is zero-length reaches Path 2. That covers empty synthesis output, which `synthesize_tangents_yup` documents as falling back to Path 2, and Starfield BSGeometry without UDEC3 tangents. Hitting the bug also needs constant V over a quad. Degenerate-UV triangles get a permutation fallback tangent (#3176) and do not reach Path 2.
- **Suggested Fix**: Add `if (dot(Traw, Traw) < 1e-8) return N;` before the first normalize, and the same guard after projection. Correct the Path-1 comment. Add a `shader_contract` source pin next to the #2815 pin.


### LOW

#### REN-D1-2026-10-03-01: #4633's non-finite TLAS drops are counted nowhere in `TlasIntegritySnapshot`, so `rt.integrity` reports FAIL with every cause counter at zero

- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`: the `non_finite_transform` arm and the `self.tlas_integrity = TlasIntegritySnapshot { .. }` assignment); `crates/renderer/src/vulkan/acceleration/mod.rs` (`TlasIntegritySnapshot`); `crates/core/src/ecs/resources/mod.rs` (`RtIntegrityStats`, `RtIntegrityStats::verdict`, `machine_line`); `crates/renderer/src/vulkan/context/telemetry.rs` (`fill_rt_integrity_stats`)
- **Status**: NEW. This is a gap left by #4633, which is closed and whose fix is in place. It is not a regression.
- **Description**: #4633 (`6e3ca2716`) added a fourth way an eligible draw can be dropped from the TLAS: `tlas_instance_transform` returns `None` for a non-finite model matrix.
  - The new counter `non_finite_transform` reaches only the rate-limited `log::warn!`.
  - `eligible_instances` is incremented before that arm, and the instance is not emitted.
  - `TlasIntegritySnapshot` has fields only for `missing_skinned_blas`, `missing_rigid_blas` and `missing_ssbo_instance`.
  - Result: the snapshot has `emitted < eligible` while all three cause counters are 0. `RtIntegrityStats::verdict` correctly turns FAIL, because `tlas_emitted == tlas_eligible` is false. But `machine_line` has no field that explains the gap.
  - The warn's sample suffix also misreports. The `"; ..."` overflow marker compares `missing_blas_total > missing_samples.len()`, which excludes the non-finite count.
- **Evidence**:
  ```rust
  let Some(transform) = tlas_instance_transform(draw_cmd) else {
      non_finite_transform += 1;   // only consumer: the warn below
      ...
      continue;
  };
  ...
  self.tlas_integrity = super::TlasIntegritySnapshot {
      frame, eligible: eligible_instances as u32, emitted: instance_count,
      missing_skinned_blas, missing_rigid_blas, missing_ssbo_instance, // no non-finite field
  };
  ```
  `grep -rn non_finite crates/renderer crates/core` finds only `tlas.rs`.
- **Impact**: Diagnostics only. The AS itself is correct: the instance is dropped, not corrupted.
  - The bench harness and the console report `verdict=FAIL` with `missing_*=0`. That is the "FAIL, no cause" shape #1228 and #3999 removed for the other causes.
  - It fires only when corrupt transforms get past the #4549/#4633 import gates. Those are exactly the frames where the operator most needs to know why.
- **Related**: #4633, #4549, #1228, #3833/#3999 (the integrity chain).
- **Suggested Fix**: Add `non_finite_transform: u32` to `TlasIntegritySnapshot` and to `RtIntegrityStats` (with a `machine_line` field). Fold it into `verdict` beside the other three, and count it in the warn's overflow-marker comparison. Extend `every_blas_residency_accessor_reaches_the_rt_integrity_snapshot`, or add a sibling test, so it pins the field through the whole chain.

#### REN-D1-2026-10-03-02: `build_blas_batched` destroys prepared and compacted BLAS on a `MaybeInFlight` one-time-submit failure, against #4891's policy

- **Severity**: LOW. This is defence in depth, at the same severity #4891 itself was filed. The hazard is reachable only when `vkWaitForFences` with infinite timeout fails with an OOM, or when `vkQueueSubmit` fails. After a true `VK_ERROR_DEVICE_LOST`, destroying child objects is valid per spec §"Lost Device".
- **Dimension**: AS Correctness (Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`, `AccelerationManager::build_blas_batched`:
  - the `if let Err(e) = build_result` arm, which calls `unwind_prepared(.., Some(query_pool))`;
  - the `if let Err(e) = copy_result` arm, which destroys both the `prepared` originals and the `compact_accels`.
- **Status**: NEW. It is a sibling gap of #4891 (closed, `9cc77cec0`). #4891 scoped the policy to "the upload orchestrators", and #4883 (`628913593`), landed the same day, re-documented the AS arm with the pre-#4891 premise.
- **Description**: #4891 split one-time-submit failures into `OneTimeCommandError::NotSubmitted` and `MaybeInFlight`.
  - `MaybeInFlight` means `queue_submit` or the fence wait failed, so "the commands may be pending".
  - The documented rule is "destroy when `may_be_in_flight` is false, leak when it is true".
  - `buffer.rs` and `texture_registry/upload.rs` follow that rule. Both BLAS one-time submissions go through the same helper (`submit_one_time` → `with_one_time_commands*`) but never consult it.
  - Their SAFETY comments state a premise the typed error now contradicts:
    - "the build submission failed, so no in-flight command buffer references `prepared` or `query_pool`"
    - "the copy submission failed, so no in-flight command buffer references `p.accel`"
  - On a fence-wait failure the build or compaction-copy command buffer may still be executing. The arms then destroy every AS it writes, plus its query pool. This is the "freeing an AS that in-flight work still reads" class, gated behind an already-failing device.
- **Evidence**: `grep -rn may_be_in_flight crates/renderer/src` finds hits only in `buffer.rs`, `texture_registry/upload.rs` and `texture.rs`, and none in `acceleration/`. `with_one_time_commands_inner` returns `OneTimeCommandError::maybe_in_flight(e, "wait for one-time commands")` from the `wait_for_fences` error arm.
- **Impact**: The whole renderer is degraded by the time this fires. The risk is a host-side destroy racing a still-pending AS build or copy on an OOM-from-wait path, which is undefined behaviour per spec, rather than a clean leak. It is also a policy inconsistency: the same helper has two failure contracts depending on the caller.
- **Related**: #4891, #4883, #1097, #2926.
- **Suggested Fix**: In both arms, branch on `OneTimeCommandError::may_be_in_flight(&e)`. Unwind on `NotSubmitted`. On `MaybeInFlight`, `mem::forget` (leak) the AS handles and buffers, as the upload orchestrators do. Correct the two SAFETY comments. A source pin like `one_time_failure_class_tests`' needle scan can cover `blas_static.rs`.

#### REN-D1-2026-10-03-04: `docs/engine/renderer.md` describes the BLAS budget with the pre-#3839 formula

- **Severity**: LOW (doc-rot)
- **Dimension**: AS Correctness (documentation)
- **Location**: `docs/engine/renderer.md`:
  - the feature bullet "LRU eviction (budget = `device_local / 3`, floored at 256 MB)";
  - the Acceleration Structures section's "**BLAS LRU eviction**: budget is `device_local / 3`, floored at `MIN_BLAS_BUDGET_BYTES = 256 MB`".
- **Status**: NEW. #3866 fixed four other sites of this exact staleness (`mod.rs`, `constants.rs`, `predicates_tests.rs`, `memory-budget.md`). `renderer.md` was not in its scope.
- **Description**: The real rule is `blas_budget_for_heap(heap, reserved) = ((heap − reserved) / 3).clamp(MIN_BLAS_BUDGET_BYTES, MAX_BLAS_BUDGET_BYTES)`.
  - `reserved = screen_scaled_reservation_bytes(extents, volumetrics, upscaler_sdk_bytes)`, which is re-derived on resize by `recompute_blas_budget_for_current_state`.
  - `MAX_BLAS_BUDGET_BYTES` is 1 GiB.
  - The doc omits the reservation, the 1 GiB ceiling and the runtime re-derivation. `memory-budget.md` already states the correct formula.
  - The same section also says static BLAS are "built once when the mesh is uploaded". In fact they are built by `build_blas_batched` at cell or NIF load and restored after eviction by `restore_missing_static_blas_for_draws`.
- **Impact**: An operator sizing VRAM from `renderer.md` overestimates the BLAS budget, by a factor of 4 on a 12 GB card at the ceiling.
- **Related**: #3866, #3839, #3988.
- **Suggested Fix**: Restate both sites against `blas_budget_for_heap` and link `memory-budget.md` §Acceleration Structures as the ledger.

#### REN-D3-2026-10-03-01: #5055 shrank `GpuLight` to 64 B, but four texts still describe the 80 B identity-carrying struct, including the `NoUninit` SAFETY comment, which names a test that no longer exists

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: the stale texts are listed under Evidence.
- **Status**: NEW. This absorbs Dim 2's REN-D2-2026-10-03-02, which found the same `renderer.md` site; that ID is retired into this one. #5055 (CLOSED) is the change itself. #4952 (CLOSED) was the previous size-drift on the same struct. No open issue covers these sites.
- **Description**:
  - c705c310d moved the ReSTIR remap identity off the GPU struct and updated `gpu_types.rs`, the size pin (`gpu_light_is_80_bytes` → `gpu_light_is_64_bytes`), `shader-pipeline.md` and the `memory-budget.md` light row.
  - Four other texts still state the old layout:
    - Two are prose.
    - One is the SAFETY justification of an `unsafe impl`. It cites "80 B total", "one `[u32; 4]` identity" and the renamed test as the pin holding the layout fixed.
  - The invariant it relies on still holds: four `[f32; 4]`, no padding. The justification text and its named guard are wrong.
- **Evidence**:
  - `docs/engine/renderer.md` § Multi-light SSBO: "Each `GpuLight` is an 80-byte struct of four `vec4`s and a `uvec4` identity … `history_id` identifies the producer across light animation and priority reordering; zero declines selection reuse."
  - `MAX_LIGHTS` rustdoc in `crates/renderer/src/vulkan/scene_buffer/constants.rs`: "1023 lights × 80 bytes plus the 4112-byte remap header is about 84 KiB". The live figure is 1023 × 64 + 4112 = 69 584 B ≈ 68 KiB.
  - `hash_light_upload` rustdoc in `crates/renderer/src/vulkan/scene_buffer/descriptors.rs`: "`GpuLight` is `#[repr(C)]` with four `[f32; 4]` fields, one `[u32; 4]` identity, and no implicit padding."
  - SAFETY comment on `unsafe impl crate::vulkan::buffer::NoUninit for super::gpu_types::GpuLight` (same file): "four `[f32; 4]` fields and one `[u32; 4]` identity (16 B each, 80 B total) … `gpu_light_is_80_bytes` (in `gpu_instance_layout_tests.rs`) holds the layout fixed." `grep -rn "fn gpu_light_is_80_bytes" crates/` finds nothing.
- **Impact**: No runtime effect. A reviewer checking the `NoUninit` justification follows it to a test that does not exist. renderer.md tells readers the identity is GPU-resident, which is the design #5055 removed; its rustdoc says "Do not re-add identity data here".
- **Related**: #5172 (OPEN, the same class of drift for `GpuTerrainTile` 160 → 176 in the same window). Stale skill premises below (Dim 3 Guard line, Dim 10 `history_id` bullet).
- **Suggested Fix**:
  - Rewrite the four texts for the 64 B / four-`vec4` struct and name `gpu_light_is_64_bytes`.
  - In renderer.md, say the identity rides `FrameInputs.light_ids`, CPU-only.
  - The window produced two Gpu* size drifts and both left prose behind. Consider extending the `gpu_material_size_claims` scanner (which #4952 already suggested) to every struct with a size pin (`GpuLight`, `GpuTerrainTile`, `GpuInstance`, `GpuCamera`).

#### REN-D3-2026-10-03-02: the light ↔ identity parallel invariant that feeds the light-SSBO remap header is enforced at three sites, but only `collect_lights` is tested

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

#### REN-D4-2026-10-03-02: shader-pipeline.md still says only material kind 0 takes the early-test pipeline (stale since #5057), and names `context/draw.rs` as the uploader of `renderOrigin.w`

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (doc)
- **Location**:
  - `docs/engine/shader-pipeline.md` §Per-Frame Submission Order, step 6: "Certified opaque batches (`DrawCommand::allows_early_fragment_tests`: no blend, no alpha test, material kind 0, …) bind `pipeline_early`".
  - The same file, §Render-origin-relative: "`renderOrigin.w` … uploaded in `context/draw.rs`".
- **Status**: NEW. CLOSED #4958/#5023 covered recorder names and the reactive mask, not this.
- **Description**:
  - Since `3c197ed8c` (#5057), `DrawCommand::allows_early_fragment_tests` admits `material_kind <= MATERIAL_KIND_MAX_LIGHTING_SHADER` (16). The doc's "material kind 0" now under-describes the certificate. A reader auditing early-Z soundness for kinds 1–16 would wrongly conclude they still go through late tests.
  - The `renderOrigin.w` FSR-reset upload is written in `assemble_camera_and_lights.rs`, which carries the comment "FSR one-frame-reset flag, read by `triangle.frag`'s FSR-reset…". This stale pointer predates the window; it dates from the #3282 split.
- **Impact**: Documentation only. The doc is the "code-verified reference" that the skill audits against.
- **Suggested Fix**:
  - Step 6: "material kind 0..=`MATERIAL_KIND_MAX_LIGHTING_SHADER` (16, the reviewed BSLightingShaderProperty types; review pinned by `early_fragment_kinds_have_no_discard_or_depth_write_path`)".
  - Change the `renderOrigin.w` pointer to `context/assemble_camera_and_lights.rs`.

#### REN-D5-2026-10-03-02: switching from a retained image cover to a model cover overwrites `Artwork::Image` without `drop_texture`

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `LoadingScreen::spawn_model_stage` (`byroredux/src/loading_screen.rs`), the `self.artwork = Some(Artwork::Stage(..))` assignment.
- **Status**: NEW.
- **Description**:
  - `Artwork::Image` is deliberately retained across transitions ("reused on repeated transitions … avoids allocating a new non-reusable bindless slot at every door").
  - `present_image_artwork` releases the previous image (`drop_texture(old)`) when it replaces it.
  - `spawn_model_stage` only drains `retired_stage`, then assigns `self.artwork = Some(Artwork::Stage(...))`. If the previous artwork was a retained `Image`, its texture handle is dropped on the floor. Its refcount never reaches zero, so the registry never redirects or frees the slot.
- **Evidence**: The only release in `spawn_model_stage` before the assignment is `if let Some(stage) = self.retired_stage.take() { retire_stage(...) }`. Nothing inspects the existing `self.artwork`.
- **Impact**: One original-artwork texture and its bindless slot leak, resident until shutdown, per image→model switch. Bounded and rare: it needs a load order mixing image-only and model LSCRs, for example FO4 or Skyrim with a mod adding image screens.
- **Related**: REN-D5-2026-10-03-01 (same file).
- **Suggested Fix**: Before assigning the stage, `if let Some(Artwork::Image { texture, .. }) = self.artwork.take() { ctx.texture_registry.drop_texture(&ctx.device, texture) }`, mirroring `present_image_artwork`.

#### REN-D5-2026-10-03-03: #4886's "transactional" `recreate_descriptor_sets` still leaks the replacement samplers when pool creation or set allocation fails

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `TextureRegistry::recreate_descriptor_sets` (`crates/renderer/src/texture_registry/mod.rs`).
- **Status**: NEW. Residual of CLOSED #4886; the fix is otherwise in place.
- **Description**:
  - When the mip bias changed (an upscaler/preset switch or a resize under FSR), `replacement_samplers = Some(create_material_samplers(...)?)` creates four `VkSampler`s *first*.
  - The following `create_descriptor_pool(...)` returns through bare `?`. The `allocate_descriptor_sets` error arm destroys only `new_pool`.
  - On either failure the four new samplers are never destroyed: they are a local `Option<[vk::Sampler; 4]>` with no Drop. The doc's claim, "an `Err` return leaves the registry exactly as it was", holds for the registry's fields but not for the device.
  - The pin `recreate_descriptor_sets_allocates_the_replacement_before_the_old_pool_dies` checks pool/set ordering only. `create_material_samplers` itself unwinds correctly on a partial failure.
- **Evidence**: The sampler creation precedes `let new_pool = unsafe { device.create_descriptor_pool(&pool_info, None).context(...)? };`. The `Err(e)` arm of `allocate_descriptor_sets` destroys `new_pool` only.
- **Impact**:
  - Four sampler handles leak per failed recreate.
  - The #2156 `set_upscaler_mode` rollback re-enters this function, so a persistent descriptor-pool OOM can leak 4 more per retry.
  - Validation reports live samplers at device destroy.
  - Reaching it requires a descriptor-pool allocation failure.
- **Suggested Fix**: Create the replacement samplers after the pool and sets succeed, or destroy `replacement_samplers` in both error arms. Extend the #4886 pin to assert that one of these holds.

#### REN-D5-2026-10-03-04: volumetrics' per-slot host-visible buffers, including #4784's new occupancy masks, have no memory-budget.md row

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `VolumetricsPipeline` construction (`crates/renderer/src/vulkan/volumetrics/init.rs`): `fog_volume_buffers`, `fog_cluster_buffers`, `fog_cluster_index_buffers`, `combustion_light_moment_buffers`, `combustion_occupancy_buffers`. Ledger: `docs/engine/memory-budget.md` § "Volumetrics (M55)".
- **Status**: NEW.
- **Description**:
  - c705c310d (#4784) added a resource owner: `combustion_occupancy_buffers`, one `create_host_visible` buffer per frame-in-flight slot. Each is `FOG_VOLUME_CLUSTER_COUNT` (16³ = 4096) × 4 B = 16 KiB, so 32 KiB in total.
  - The commit edited memory-budget.md only for the `GpuLight` 80→64 B row. `grep -i occupancy` on the ledger finds nothing.
  - Checking the section showed the gap is older and larger. The Volumetrics section ledgers the six froxel volumes and the noise pair, but none of the per-slot host-visible SSBOs:
    - `fog_cluster_index_buffers`: 4096 × (`MAX_FOG_VOLUMES_PER_CLUSTER` 64 + `MAX_FOG_PORTALS_PER_CLUSTER` 128) × 4 B = 3 MiB per slot, 6 MiB in total. This is CpuToGpu memory, so it is BAR-eligible.
    - `fog_cluster_buffers`: 4096 × 16 B = 64 KiB per slot.
    - The fog-volume upload buffer.
    - The 8 KiB per slot moment readback.
- **Evidence**: `occupancy_buffer = try_or_cleanup!(GpuBuffer::create_host_visible(device, allocator, std::mem::size_of::<[u32; FOG_VOLUME_CLUSTER_COUNT]>() ...))`. memory-budget.md's volumetrics rows end at the volume table and the 294,912 B noise pair.
- **Impact**: Ledger completeness only. The skill rule is that every resource owner added since the baseline needs a row. The unledgered host-visible total (about 6.2 MiB of BAR-eligible memory, mostly the index lists) is invisible to anyone budgeting BAR pressure, which is the condition #4889 had to degrade around.
- **Suggested Fix**: Add a per-slot host-buffer sub-table to the Volumetrics section, with sizes derived from the named constants. Optionally pin it like `memory_budget_ledgers_the_sky_and_ground_cover_owners`.

#### REN-D5-2026-10-03-05: `VulkanContext::drop` still `expect`s the transfer-fence lock — the #4599 poison policy covers allocator locks only

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `impl Drop for VulkanContext` (`crates/renderer/src/vulkan/context/teardown.rs`): `.lock().expect("transfer fence lock poisoned")`. The lock holder is `with_one_time_commands_inner` (`crates/renderer/src/vulkan/texture.rs`), `fence_guard`.
- **Status**: NEW, a sibling of CLOSED #4599, whose scope was allocator locks: its issue body lists only allocator sites.
- **Description**:
  - #4599 established that a poisoned lock on a teardown path must be recovered, because a panic there skips `save_pipeline_cache`, `destroy_device` and `destroy_instance`, or aborts during unwind. It routed allocator locks through `lock_recovering` / `into_inner_recovering`.
  - `VulkanContext::drop` still acquires `self.transfer_fence` with `.expect`.
  - That mutex is held as `fence_guard` across reset, submit and wait in `with_one_time_commands_inner`. The one panicking call inside that window is `queue.lock().expect("graphics queue lock poisoned")`.
  - The pin `allocator_lock_direct_acquires_are_confined_to_allocation_and_reports` counts only `.expect("allocator lock poisoned")` / `.lock().unwrap()` spellings, so it cannot see this site.
- **Evidence**:
  ```rust
  let fence = *self.transfer_fence.lock().expect("transfer fence lock poisoned");
  self.device.destroy_fence(fence, None);
  ```
- **Impact**: Reaching it needs a chain: an earlier panic while the graphics-queue lock is held poisons that lock; a one-time submit then panics while holding the fence; the context then drops. This is hardening only, with no realistic trigger at HEAD, but it is the exact failure mode #4599 named.
- **Suggested Fix**: Use `allocator::lock_recovering(&self.transfer_fence)` in Drop. For consistency, also use it for the queue lock inside the fence window. Extend the #4599 allowlist scan to the `expect("… lock poisoned")` family within `teardown.rs`.

#### REN-D6-2026-10-03-05: Spec and doc rot from CDB Phase 2 and the guard's exemption list

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `docs/engine/nifal.md` (§ parked Starfield kinds; §3 "Drawn-surface exemptions"); `byroredux/src/asset_provider/material/merge.rs` (the `.mat` gate comment in `merge_external_material`); `byroredux/src/material_translate.rs` (`every_exterior_spawner_inserts_a_boundary_material` doc)
- **Status**: NEW
- **Description**:
  - nifal.md still says "zero Starfield texture roles are produced, so the gap is latent". It also names `mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands` as the pin that "gets rewritten", but that test no longer exists. 18fce7e43 forwards colour/normal/emissive/height and replaced it with `mat_path_lookup_miss_keeps_presence_only_and_no_textures` and `mat_path_merges_cdb_authored_textures_when_indexed`.
  - nifal.md records none of the CDB scalar translations: flat colour, alpha, glass, SSS.
  - The `merge_external_material` comment still describes Phase 1: "forwards no authored field. Phase 2 should return `Merged`…".
  - nifal.md §3 says there are "exactly four deliberate exemptions": Cornell, `crates/save`, ground cover, and mesh→participating medium (#5102). The spawner-guard doc lists Cornell, save, ground cover and `scene.rs` demo primitives. The two lists differ.
  - The guard doc names `npc_spawn/resumable.rs`, which is now `npc_spawn/resumable/mod.rs`. It names `cornell.rs`, but the Cornell `MeshHandle` inserts are in `cornell/builders.rs`.
- **Suggested Fix**: Rewrite the nifal.md Starfield paragraph to describe the forwarded roles and point at the live pins. Reconcile the two exemption lists. Update the guard-doc paths.
- **Folded in**: REN-D12-2026-10-03-03 (Dim 12). The `mat.set` glass-optics comment in `byroredux/src/commands/scene.rs` still says "`cornell.rs`'s `glass()`"; since #5090 (8f38df6df) `glass()` lives in `byroredux/src/cornell/builders.rs`. Fix the pointer in the same change.

#### REN-D7-2026-10-03-01: SVGF's nearest-tap fallback re-accepts the non-finite history texel that #903's guard just dropped, so with a parked camera one NaN/Inf indirect sample persists indefinitely

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` — `main()`, the bilinear-tap loop's `isnan(sInd)` guard and the `else if (length(motion * screen.xy) < 1.5)` nearest-tap branch (`nearID`)
- **Status**: NEW (gap in the closed #903 fix; #737 added the fallback before #903 landed, and #903 guarded only the bilinear loop)
- **Description**: #903 drops a non-finite history tap with `continue` inside the 4-tap bilinear loop. If every tap is dropped (or carries ~0 weight), `wTotal <= 0.01`, and the sub-pixel-motion fallback runs. That fallback picks `q = ivec2(round(prevPx))`. This is always one of the same four taps, so it is the one just rejected. It applies only the mesh-ID and normal tests, then reads `prevIndirectHistTex`/`prevMomentsHistTex` with **no** finite check. With a parked camera, `motion == 0`, so `prevPx == p` (within float error). The bilinear weight is ≈1 on the pixel itself and ≈0 on the other three taps, so a NaN in the pixel's own history always reaches the fallback and is taken back. `mix(NaN, currInd, alphaC)` then writes NaN to this frame's temporal output. That output *is* the next frame's history (à-trous does not feed back, per `svgf.rs`'s history wiring). The NaN therefore self-perpetuates for as long as the camera stays parked or moves under 1.5 px/frame.
- **Evidence**: bilinear loop: `if (any(isnan(sInd)) || any(isinf(sInd)) || any(isnan(sMom)) || any(isinf(sMom))) { continue; }`. Fallback branch: `histInd = texelFetch(prevIndirectHistTex, q, 0).rgb; histMom = sMom.xy; histAge = sMom.z; hasHistory = true;`, with no `isnan`. The current-frame sample has no guard either: the firefly clamp `if (currLum > maxL)` is false for NaN, so a single-frame NaN in raw indirect enters history directly. No test pins #903 on the SVGF side (`grep -n "isnan\|#903" crates/renderer/src/vulkan/svgf.rs` finds nothing).
- **Impact**: This is dormant defence-in-depth, and #903 itself records "no live NaN source today". But in the parked case, the one where history lives longest, the guard does nothing. A transient NaN from any future RT branch would become a permanent dark or garbage blob, spread each frame by the 3 à-trous iterations into composite until the camera moves. The `presentation.frag` `ImageHealth` non-finite counter (#2736) would report it but not clear it.
- **Related**: #903, #737, #1159 (all closed). #4782 (same class, V-buffer history, closed).
- **Suggested Fix**: Apply the same `isnan`/`isinf` rejection to the fallback's `sMom` and `histInd` before setting `hasHistory`. Better, sanitise `currInd` once at the top of `main()` so no non-finite value is ever written to history. Pin both with a GLSL source-scan test in `svgf.rs`.

#### REN-D7-2026-10-03-02: the scene-static signal's coverage of renderer-appended combustion lights rests on an unguarded variable shadow in `draw_frame`

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame`, `let lights = frame_lights.as_slice();` immediately before `self.build_and_upload_instances(…, lights, …)`; consumer `build_and_upload_instances.rs` (`caustic_scene_key` light-rig fold → `caustic_scene_static` → `next_svgf_temporal_alpha` and `scene_static_last_build`)
- **Status**: NEW (test gap)
- **Description**: `next_svgf_temporal_alpha`'s `caustic_history_valid` input, and ReSTIR's parked mode 2 through `scene_static_last_build`, see light changes only through the `caustic_scene_key` fold over the `lights` slice handed to `build_and_upload_instances`. That slice is correct today: `draw_frame` shadows the app's `FrameInputs.lights` with `frame_lights`, the merged list after `append_combustion_surface_lights` and the #5055 priority re-sort. So advected/cooling combustion surface lights do break parked accumulation, as they should. Nothing pins this, though. `svgf_temporal_alpha_is_fed_the_combined_camera_and_light_rig_signal` and `scene_static_signal_sees_rigid_instance_set_changes` scan only `build_and_upload_instances.rs`, and no test references `frame_lights` outside its producer. Passing the un-shadowed app slice (an easy slip: #5055 just threaded a parallel `light_ids` through the same call chain) would compile and pass every test. It would also silently drop renderer-derived fire lights from the key, so SVGF would keep its ~1/256 parked α and ReSTIR mode 2 over GI and direct lighting that a burning field keeps changing.
- **Evidence**: `grep -rn "frame_lights" crates/renderer/src` hits only `draw.rs`, `assemble_camera_and_lights.rs`, `mod.rs`, `init.rs`, `telemetry.rs` and `shrink_frame_scratch.rs` production code; there are zero test needles. The key fold is `for light in lights { … position_radius … color_type … direction_angle … params }` in `build_and_upload_instances`.
- **Impact**: No live defect. This is a regression path that no `cargo test` can catch, landing on the HIGH-adjacent SVGF/ReSTIR history signals (parked ghosting = MEDIUM floor).
- **Related**: #4046, #4943, #4942 (closed); #5055 (this window).
- **Suggested Fix**: Add a `production_text` scan of `draw.rs` asserting that the `build_and_upload_instances(` argument list passes the `frame_lights`-derived slice (or rename the shadow, e.g. `merged_lights`, and pin the name). Alternatively, return the merged slice through `CameraAssemblyOutput` and use it directly.

#### REN-D7-2026-10-03-03: denoise/resolve doc residue that #4874 and its predecessors missed — retracted Halton "LCM-6 period" rationale in `renderer.md`, and a phantom bindless STORAGE_IMAGE row in `shader-pipeline.md`

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `docs/engine/renderer.md` TAA "Per-frame flow" step 1; `docs/engine/shader-pipeline.md` `## Descriptor Sets` table, row `0 | 1`
- **Status**: NEW (residual of #3606 and #4019, both closed)
- **Description**:
  1. `renderer.md` still says the jitter has "period 16 — #1093 — chosen as the nearest power of two above the natural LCM-6 period". `taa_jitter`'s own doc in `frame_params.rs` carries the 2026-08-31 correction: Halton sequences are aperiodic, so there is no LCM-6 period and the real motivation for 16 is an open question. #3606 fixed the two code sites. #4874 rewrote steps 2–3 of this same paragraph but left step 1.
  2. The descriptor table lists `0 | 1 | STORAGE_IMAGE (bindless) | Per-pass read/write images | bloom, svgf, taa`. The bindless layout (`build_bindless_descriptor_bindings`) is two `COMBINED_IMAGE_SAMPLER` arrays: binding 1 is the environment cubemap array (`include/bindings.glsl`: `layout(set = 0, binding = 1) uniform samplerCube cubemaps[]`). Bloom, SVGF and TAA bind only their private set 0, where binding 1 is `uMotion`/`motionTex`/`dst`. #4019 corrected the caustic/volumetrics credits in this table but not this row, which dates to 78540d8ef.
- **Evidence**: `grep -n "LCM" docs/engine/renderer.md` → step 1. `build_bindless_descriptor_bindings` → `[0, 1].map(… COMBINED_IMAGE_SAMPLER …)`.
- **Impact**: Audit and onboarding only. This is the table the skills tell auditors to trust instead of re-deriving descriptor facts.
- **Related**: #3606, #4019, #4874.
- **Suggested Fix**: Replace step 1's rationale with "period 16 (#1093; motivation open, see `taa_jitter`)". Change row `0|1` to `COMBINED_IMAGE_SAMPLER` (bindless `samplerCube` array) used by `triangle` (and any other `bindings.glsl` includer).

#### REN-D8-2026-10-03-01: `shader-pipeline.md`'s inject binding table stops at 23; the live shader declares 26 bindings (0–25)

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `docs/engine/shader-pipeline.md` (the `volumetrics_inject.comp` table: "24 bindings — widened twice…"), against `crates/renderer/shaders/volumetrics_inject.comp` (`CombustionOccupancyOut` / `CombustionOccupancyIn`) and `crates/renderer/src/vulkan/volumetrics/init.rs` (layout bindings 24/25)
- **Status**: NEW (recurrence of the class closed by #3830)
- **Description**: c705c310d (#4784) added `layout(std430, set = 0, binding = 24) buffer CombustionOccupancyOut` and `binding = 25 readonly buffer CombustionOccupancyIn`. Both are in the descriptor-set layout and are rotated per FIF (out = slot `f`, in = slot `previous`). The same commit edited `shader-pipeline.md`, but only the `GpuLight` rows. The inject table still says "24 bindings", lists 0–23, and has no row for either occupancy buffer or its per-FIF rotation. Its own header warns: "verify against the source before relying on this table for a new binding".
- **Evidence**: `grep -n "binding 24\|24/25\|occupancy" docs/engine/shader-pipeline.md` returns nothing. `grep -n "binding = 2[45]" crates/renderer/shaders/volumetrics_inject.comp` returns both declarations.
- **Impact**: Documentation only. The next binding added to the inject set will be numbered against a stale table, which is how #3830 started.
- **Related**: #3830 (same table, 12 vs 24); REN-D5-2026-10-03-04 (the same buffers have no memory-budget ledger row).
- **Suggested Fix**: Add rows 24/25 (STORAGE_BUFFER, 16³ `u32` mask on the fog-cluster grid; 24 = this slot's atomicOr marks, 25 = previous slot's mask as the dilated skip gate). Update the count to 26.

#### REN-D8-2026-10-03-02: `combustion_occupancy_buffers` depends on the all-slots fence wait, but the `sync.rs` rider list does not name it; its previous-slot RAW is covered only by an unrelated barrier

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` (`VolumetricsPipeline::dispatch`: `self.combustion_occupancy_buffers[frame].write_mapped(..)`; the Stage B `history_barriers` array); `crates/renderer/src/vulkan/sync.rs` (the "Bumping this constant still requires:" rider list and `frames_in_flight_contract_names_every_dependent_resource`); `crates/renderer/src/vulkan/context/post_passes.rs` (`record_volumetrics_pass`, the "Compute→compute visibility" `memory_barrier`)
- **Status**: NEW (same class as #4988 and #4851)
- **Description**: Frame N, slot `f`, host-zeroes and seeds `combustion_occupancy_buffers[f]`. That buffer was last read on the GPU by frame N‑1 (slot `1-f`) through binding 25, so slot `f`'s own fence does not retire that read. Only the top-of-frame all-slots wait does. The field doc and the commit body both say so ("the all-slots fence wait at the top of `draw_frame` has retired the previous reader"). The resource is still missing from the `sync.rs` list that the #4601/#3643 rule calls load-bearing and that #5117 relies on before any wait narrowing.
  - The read-after-write half has a similar gap. Frame N‑1's atomicOr marks are read by frame N's inject through binding 25. Stage B publishes the previous slot's WRITE→READ for all five per-FIF history images, but includes no buffer barrier for this mask.
  - That RAW is currently ordered only by the global COMPUTE `SHADER_WRITE` → COMPUTE `SHADER_READ` `memory_barrier` in `record_volumetrics_pass`. That barrier exists for cluster_cull's buffers. Its first scope happens to reach the previous submission's inject.
  - The Stage F `SHADER_WRITE → HOST_READ` barrier carries an explicit comment on why a fence alone does not give a memory dependency. The occupancy mask has no equivalent stated dependency.
- **Evidence**: The `history_barriers` array lists `lighting_volumes`, `emission_history_volumes`, `combustion_state_volumes`, `combustion_dynamics_volumes` and `combustion_optical_volumes`, with no buffer barrier. `grep -n occupancy crates/renderer/src/vulkan/sync.rs` returns nothing.
- **Impact**: Nothing breaks today. If the cluster-cull barrier is narrowed to a buffer barrier, the RAW loses its only ordering. If the fence wait is narrowed per slot (#5117), the host write races frame N‑1's read of the mask.
  - Either change can produce a false-negative occupancy bit. A false negative skips the RK2 transport block where combustion actually sits, so a plume freezes in place.
  - `cargo test` cannot see either failure.
- **Related**: #5117, #4988, #4851, #4601; REN-D5-2026-10-03-04.
- **Suggested Fix**: Add the occupancy mask to the `sync.rs` rider list and its pinning test. Name the previous-slot mask in Stage B's barrier set, or document there that the cluster-cull barrier covers it. Stop at that: per the skill, any barrier edit is "needs syncval" (below).

#### REN-D8-2026-10-03-03: two of the #4784 guards cannot fail on the regressions they name

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`, `mod transport_occupancy_tests`: `build_marks`, `occupancy_marks_reset_between_builds`, `transport_occupancy_gate_is_wired_into_the_inject_pass`
- **Status**: NEW
- **Description**:
  1. `occupancy_marks_reset_between_builds` claims to pin "a fresh build must not inherit the previous frame's marks". However, `build_marks` allocates a new zeroed `Box<[u32; FOG_VOLUME_CLUSTER_COUNT]>` on every call, so the second build always starts from zero. The test also never reaches production's empty-volume arm, `self.fog_cluster_occupancy.fill(0)` in `dispatch`, because that arm does not call `build_fog_volume_clusters`. It stays green if either `occupancy.fill(0)` (in `build_fog_volume_clusters`) or the empty-branch `fill(0)` is deleted.
  2. The call-site check in `transport_occupancy_gate_is_wired_into_the_inject_pass` falls back to `shader.find("transportOccupied,")`. That needle also matches the parameter declaration `bool transportOccupied,` in `transportCombustion`'s signature. If the call site regressed to passing `true`, the gate would be bypassed and the assertion would still pass.
- **Evidence**: `fn build_marks(volumes: &[GpuFogVolume]) -> Box<[u32; FOG_VOLUME_CLUSTER_COUNT]> { … let mut occupancy = Box::new([0u32; FOG_VOLUME_CLUSTER_COUNT]); …`. The `.or_else(|| shader.find("transportOccupied,"))` fallback. `volumetrics_inject.comp`'s `bool transportOccupied,` parameter.
- **Impact**: Test gap only. A missing reset fails conservatively: stale marks keep transport running wherever fire ever burned, which costs performance but not correctness. A literal `true` at the call site silently undoes the #4784 performance fix.
- **Related**: #4784.
- **Suggested Fix**: Reuse one occupancy buffer across both `build_fog_volume_clusters` calls with a non-empty second volume list, and separately assert the `dispatch` empty branch's `fill(0)` via `production_text`. Drop the fallback needle, or match the call's argument list exactly.

#### REN-D9-2026-10-03-04: Doc rot in the skinning lane (bundle)


- **Severity**: LOW
- **Dimension**: Skinning
- **Location**:
  - (a) `crates/renderer/src/vulkan/context/init.rs`, step 12d comment above `SkinComputePipeline::new`.
  - (b) `crates/renderer/src/vulkan/morph_compute.rs`, rustdoc on `MorphSlot::last_used_frame`.
- **Status**: NEW. (a) was re-wrapped by `7f6ab8e8f` (#4894) without the number being fixed. (b) predates the window; #4294 changed the behaviour but not this doc. Searched "32 skinned", "SKIN_MAX_SLOTS", "MorphSlot last_used_frame".
- **Description**:
  - **(a)** The comment says the slot ceiling matches "`MAX_TOTAL_BONES / MAX_BONES_PER_MESH = 32` skinned meshes". The value is `SKIN_MAX_SLOTS = (196608 / 144) - 1 = 1364`, and the module-level const doc in `context/mod.rs` says so. 32 was the pre-#900 value.
  - **(b)** The rustdoc says the stamp is "bumped every frame this entity appears in `draw_commands` (including skip-path entries)". Since #4294 it is stamped from entity liveness by `refresh_morph_slot_lru` → `refresh_live_slot_stamps`, so that a non-recreatable MorphSlot is never reaped from a live entity. The dispatch loop carries an explicit "#4294 — no `MorphSlot` LRU bump here".
- **Impact**: (b) points a future fixer back toward the draw-list stamping that #4294 removed as a bug. (a) misstates a capacity by about 40×.
- **Suggested Fix**: (a) Replace "= 32" with a pointer to `SKIN_MAX_SLOTS`. (b) Restate the doc as "stamped from entity liveness each frame (`VulkanContext::refresh_morph_slot_lru`, #4294); `0` is the never-stamped sentinel (`skin_lru_stamp` never writes it, #4969)".

#### REN-D11-2026-10-03-02: `exposure_meter.comp` falls outside both the single-source constant net and the mirror pins, so the #5158 clamp-then-compensate order is guarded only on the Rust side

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - `exposure_meter.comp` `main`, in the single-thread epilogue.
  - The `EXPOSURE_CONSTANT` doc in `crates/renderer/src/vulkan/exposure.rs`.
  - The `auto_exposure` mirror and its test `envelope_caps_dark_scene_lift_and_compensation_biases_around_it`.
  - The source pins in `crates/renderer/src/vulkan/exposure_meter.rs` `mod tests`.
- **Status**: NEW.
- **Description**:
  - **Claim (a)**: "metering shader, chroma compress and this module cannot disagree".
    - The `EXPOSURE_CONSTANT` doc says it "Resolves to the shared `EXPOSURE_METER_NEUTRAL` (#5154) so the metering shader, the presentation chroma compress and this module cannot disagree". The #5154 commit message makes the same single-source claim.
    - `exposure_meter.comp` has no `#include "include/shader_constants.glsl"`. It hand-types `1.2 * exp2(-ev100)`, `* 8.0` (S/K) and the Rec.709 luma weights `vec3(0.2126, 0.7152, 0.0722)`.
    - A change to `EXPOSURE_METER_NEUTRAL` would move presentation's neutral point and the host mirror, but not the meter.
  - **Claim (b)**: the shader's clamp-then-compensate order is pinned.
    - The core of 7d99ba7f0 is this ordering: clamp the *metered* target, then apply `exp2(-params.mode.z)`.
    - Only the Rust `auto_exposure` mirror is tested for it.
    - No pin in `exposure_meter.rs` covers the shader's order. Its source tests cover the workgroup size, the #4597 divisor and the sample policy only.
    - Reverting the shader alone would leave every guard green while `exposure ev` goes dead again in clamped scenes.
- **Evidence**:
  - `exposure_meter.comp` contains the line `float target = 1.2 * exp2(-ev100);`.
  - Within it, `target = clamp(target, params.limits.x, params.limits.y);` precedes `target *= exp2(-params.mode.z);`.
  - `git grep` finds no Rust test that references `params.limits` or `exp2(-params.mode.z)`.
- **Impact**: Today there is no behaviour change, because the values agree. This is a latent drift path on the default render path's metering, and a code comment states a guarantee the code does not provide.
- **Related**: #5154, #5158, #4597.
- **Suggested Fix**:
  - Include `shader_constants.glsl` in the meter and use `EXPOSURE_METER_NEUTRAL` and `LUMA_REC709`.
  - Add a source-order pin in `exposure_meter.rs`: clamp before the compensation multiply.

#### REN-D11-2026-10-03-03: presentation and exposure docs went stale after #5154 and #5158 (the "16x clamp" text and `tonemap(graded * exposure)`)

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - The #5154 comment in `presentation.frag` `main`: "halving saturation at the 16x clamp".
  - The `adaptation_chroma_compress` doc in `crates/renderer/src/tonemap.rs`: "halving saturation at the meter's 16x clamp".
  - The `ADAPTATION_SAT_FALLOFF` comment in `crates/renderer/src/shader_constants_data.rs`.
  - `docs/engine/shader-pipeline.md` (the `presentation.frag` table row and pass 17b/presentation prose) and `docs/engine/renderer.md` (steps 21 and 24, plus the file-tree note). Both still describe presentation as `tonemap(graded * exposureTex)`.
  - The #4591 comment in `record_exposure_meter_pass` (`post_passes.rs`): "Fixed mode (the default)".
- **Status**: NEW (doc rot).
- **Description**:
  - Since 7d99ba7f0, `MAX_AUTO_EXPOSURE` is 2.0, not 16. The maximum auto-mode lift is now about 0.74 stops, which gives chroma of about 0.88, not "halved". The updated test comment in `tonemap.rs` already says this; the three prose sites were not updated.
  - Since 4bf2ec3a4, presentation's tonemapper input is `compressed * exposure`, the luma-preserving chroma compress. The engine docs omit that stage.
  - Auto exposure has been the boot default since a070baaad and 546e7fbc7, so "Fixed mode (the default)" in the meter gate comment is wrong.
- **Impact**: Readers are misled about how strong the #5154 compress is under the envelope, and about the presentation stage list. There is no runtime effect.
- **Suggested Fix**:
  - Restate the three comments as "about 0.88 chroma at the 2× envelope cap; `ev` compensation can lift further".
  - Add the chroma-compress step to the two engine docs.
  - Change "Fixed mode (the default)" to "fixed mode".

#### REN-D12-2026-10-03-01: `DebugStats::groundcover_model_{demanded,emitted}` (#4920) are written every frame but nothing reads them

- **Severity**: LOW
- **Dimension**: Debug/Telemetry (co-owner `/audit-exterior`)
- **Location**: `crates/core/src/ecs/resources/mod.rs` (`DebugStats::groundcover_model_demanded` / `groundcover_model_emitted`); writer in `byroredux/src/app_events.rs` (`ctx.groundcover_model_stats().unwrap_or_default()`); producer `GroundCoverModelTier::harvest` / `stats()` in `crates/renderer/src/vulkan/groundcover_models.rs`.
- **Status**: NEW. This is a residual of CLOSED #4920. Its suggested fix was "Log once when a cap is hit, and add both counts to `DebugStats`". Both were done, but no consumer was ever wired.
- **Description**: 4dfe97f3e added the two fields, and the commit message says the truncation is "reported as DebugStats::groundcover_model_{demanded, emitted}". The `app_events.rs` comment says it is "visible without `--bench-*`". A whole-tree grep finds the fields only at their definition, their `Default`, and that one writer. No surface reads them:
  - The `stats` console command (`commands/world_info.rs`) does not.
  - The debug server's `eval_stats` (`DebugResponse::Stats`) does not.
  - `log_stats_system` does not.
  - No debug-ui panel does.

  The visibility that does exist comes from the once-per-episode `log::warn!` in `harvest` and the bench-only `groundcover-models:` line.

  The value is also stale off-cover. `harvest` returns early unless `pending_stats[frame]` is set, and `record` sets it only on frames that dispatch. In an interior, or with no cover, `stats` keeps the last exterior placement indefinitely. The field doc says "read back one pipelined frame late", which does not describe that latch.
- **Evidence**: `grep -rn 'groundcover_model_demanded\|groundcover_model_emitted' crates byroredux tools` returns `resources/mod.rs` (definition and Default) and `app_events.rs` (write) only. `GroundCoverModelTier::prepare` calls `self.harvest(device, frame)`, and `harvest` begins with `if !std::mem::take(&mut self.pending_stats[frame]) { return; }`.
- **Impact**: The fields are dead telemetry. Someone triaging missing plants through `byro-dbg` has no way to see the truncation counts #4920 meant to expose, short of a bench run or catching the one-time warn. If a reader is added later, it will show a stale exterior count in interiors. No render impact.
- **Related**: #4920 (CLOSED); #4338 (the blade-tier precedent: log only).
- **Suggested Fix**: Either surface the pair (one `stats` line, or a `DebugResponse::Stats` field), zero or invalidate `stats` on a frame where `prepare` finds nothing to place, and fix the field doc; or delete the fields and leave the warn and bench line as the documented surfaces.

#### REN-D12-2026-10-03-02: `gpu_timers.rs` module header has two inaccuracies that the #4981 refresh missed

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs`, the module doc: the bracket-history paragraph and the "When the driver lacks timestamp support" section.
- **Status**: NEW. Neither item was in #4981's (a)–(e) list, which f4f280c9a closed in full.
- **Description**:
  - (a) "The original four brackets (skin dispatch / skin palette / BLAS refit / TAA) shipped with the #1194 perf-bisect work." The skin-palette bracket was added by #3676 (`6a605e7fc`, slot 28 / `BIT_SKIN_PALETTE = 0x4000`). That commit rewrote the sentence from "The original three brackets (skin / BLAS refit / TAA)" and moved its own bracket into the #1194 history.
  - (b) The no-timestamp section says "`DeviceCapabilities::timestamp_supported == false` skips creation entirely; `last_snapshot()` returns zeroed values." The real gate is `DeviceCapabilities::gpu_timers_supported()`, which is `timestamp_supported && timestamp_valid_bits > 0 && host_query_reset_supported` (#1478). `GpuPerFrameTimers::new` then returns `Ok(None)`. No timer object exists, so `last_snapshot()` cannot be called. The zeroing happens in `fill_skin_coverage_stats`'s `else` arm (`telemetry.rs`), which also clears every `_active` flag.
- **Evidence**: `git show 6a605e7fc -- crates/renderer/src/vulkan/gpu_timers.rs` shows `-//! The original three brackets (skin / BLAS refit / TAA)` replaced by `+//! The original four brackets (skin dispatch / skin palette / BLAS refit / TAA)`. `fn gpu_timers_supported` is in `crates/renderer/src/vulkan/device.rs`. `GpuPerFrameTimers::new` opens with `if !caps.gpu_timers_supported() { return Ok(None); }`.
- **Impact**: Doc only. (b) is the more useful fix. A timestamp-capable GPU without `hostQueryReset` gets no timers, and the header points a reader at the wrong predicate. That is the same misreading #1478 fixed in code.
- **Related**: #4981, #4811 (CLOSED, verified below); #1478.
- **Suggested Fix**: Restore "three original brackets (skin dispatch / BLAS refit / TAA)" and credit skin palette to #3676. In the no-timestamp section, name `gpu_timers_supported()` and say that consumers zero the stats and clear every `_active` flag.


#### Existing #5172: addendum (sites the issue does not list)
#5172 (OPEN, `GpuTerrainTile` 160 → 176 B doc drift) does not list these sites:
- The SAFETY comment in the `SceneBuffers` terrain-tile upload (`crates/renderer/src/vulkan/scene_buffer/upload.rs`) cites the dead `gpu_terrain_tile_is_160_bytes`; the live test is `gpu_terrain_tile_is_176_bytes`.
- `docs/engine/shader-pipeline.md` Scene Buffer Capacity table: "`MAX_TERRAIN_TILES` | 1 024 | 160 B each".
- `docs/engine/exal-groundcover.md`, two passages ("brought it to its current 160 B …", "is now 160 B with the Tier-3 …").
- `crates/renderer/shaders/include/terrain_sample.glsl`: "`GpuTerrainTile` (160 B)". This is routed by Dim 2 and owned by `/audit-exterior`.

Fold these into #5172 rather than filing them separately.

## Prioritized Fix Order

1. **D9-01.** Gate the skin dispatch collection on `is_geometry_resident` before slot creation. This is one condition, and it stops OOB reads and corrupt BLAS on every streamed NPC.
2. **D4-01.** Give the geometry re-point a `None` arm: clear the caustic latch and drop the RT flag, or keep the old pair until a replacement exists. Pin it.
3. **D10-01.** Stop spawning NIF-embedded directional artifacts. Fix the `affected_node_names` doc, and reset the Gilded Carafe baseline to 0 with a live `light.dump`.
4. **D6-01.** Keep CDB replacements per slot. Run the `cdb_join_probe` slot census first, on a quiet machine.
5. **D5-01 (+ D5-02).** Route `retire_stage` through `cell_loader::unload::release_entities`, and release on the framing-failure arm.
6. **D9-02, then D9-03.** No pre-drop on the forced rebuild. Size refit scratch from `max(build, update)` (also the TLAS update site).
7. **D11-01.** Single-source the adaptation constant (`DEFAULT_ADAPTATION_SECONDS`). Without this, any #5158 calibration measures 0.2 s.
8. **D2-01 + D10-02 together.** Split `shadowableLightRadiance` into reflection and transmission visibility. This needs a before/after capture.
9. **D6-02, D6-03, D6-04.** CDB flags routed to their consumers; dielectric-neutral PBR on a CDB hit; Path-2 TBN guard.
10. **LOW batch.** Hardening first (D1-02, D5-03, D5-05, D7-01), then test gaps (D3-02, D7-02, D8-02, D8-03), then telemetry (D1-01, D12-01), then doc rot (D1-04, D3-01, D4-02, D5-04, D6-05, D7-03, D8-01, D9-04, D11-02, D11-03, D12-02, #5172 addendum).

## Needs-RenderDoc / live validation

None of these was run: no engine or GPU process was launched in this audit.

- **D9-01.** A GPU-assisted-validation run on an exterior crossing that streams in NPCs (FNV or FO4 grid cross). Expect OOB reads before the fix and none after. Also run `rt.integrity` on a static-pose streamed actor (corpse or mannequin).
- **D4-01.** Fault-inject a `build_geometry_ssbo` allocation failure on the reclaim arm under `BYRO_VALIDATION=1`. Expect descriptor-validity VUIDs on the scene and caustic sets before the fix.
- **D10-01.** `light.dump` / `light_count_directional` on Oblivion `ICMarketDistrictTheGildedCarafe` before and after the fix (`/audit-runtime`).
- **D2-01 / D10-02.**
  - Capture an opaque, normal-mapped, two-sided card lit from just behind its plane (use `DBG_VIZ_NORMAL_DIVERGENCE` to locate the band).
  - Capture a Skyrim soft-lit head beside a cell light.
  - Capture an FO4 thick-translucency surface lit from behind.
  - Also the open #5018 acceptance capture.
- **D6-01 / D6-02.** A `cdb_join_probe` census of `TextureReplacement` by `Components.Index` (needs more than 10 GB free RAM). Then a Starfield albedo capture against NifSkope, an `IsGlass` capture, and the RT ray count for a back-lit `UseSSS` material.
- **D5-01.** A physics collider census before and after a Skyrim or FO4 model cover is dismissed. Also a census of which vanilla LSCR models carry bhk collision.
- **D9-02 / D1-02 / #4884 / #4882 / #4891.** Fault injection (allocation, bind, submit and fence-wait failures) under `BYRO_VALIDATION=1`.
- **D9-03.** Dump `updateScratchSize` against `buildScratchSize` on the 4070 Ti and on RADV.
- **D11-01.** The #5158 envelope on the FNV saloon and a night exterior, with `exposure speed 0.5` set by hand until the fix lands.
- **D7-01.** Inject one non-finite raw-indirect texel with a parked camera, then watch `ImageHealth`.
- **#5057.** A/B a Skyrim/FO4 interior with skin, hair, eyes and env-mapped actors against `BYRO_DISABLE_OPAQUE_EARLY_TESTS=1` (pixel-identical expected).
- **#5062.** A sync-validation run to confirm the binding-16 READ_AFTER_WRITE is gone.
- **c705c310d.** A validation run with an active combustion emitter (occupancy bindings 24/25).
- **GpuLight 64 B on device.** A RenderDoc buffer view of set 1 binding 0 (`ArrayStride 64` at 4112). Unlike `GpuTerrainTile`, `GpuLight` has no shipped-SPIR-V stride test.
- **Loading cover.** `docs/smoke-tests/p6-loading-model.sh` under `BYRO_VALIDATION=1`, with no AS VUIDs at the membership swaps and `rt.integrity` PASS on cover frames.
- **Carried from the baseline:**
  - #4890 rapid resize, #5072 surface-format change with the overlay open, #4889 under BAR pressure, #4885 resize during a streaming apply;
  - the `r.upscaler` switch, `BYRO_FSR_FORCE_DISPATCH_FAIL=1`, resize under FSR;
  - one validation run on the default FSR Quality path;
  - the FP32 SDK permutation, which is untested.

## Stale skill premises (for the next `/audit-renderer` sync)

**Cross-cutting**
- `context/frame_params.rs` (#5087) must join the Dim 4 and Dim 7 Paths. These moved there from `draw.rs`: `build_composite_params`, `pack_sky_dome`, `build_sky_cube_params`, `taa_jitter`/`halton`, `FrameInputs`, `needs_two_sided_blend_split`, `group_state` and `should_use_indirect_draws`. The tests `splits_when_glass_and_z_write_false`, `does_not_split_two_sided_blended_particles` and `taa_and_fsr_negate_jitter_y_the_same_way` live there now.
- `byroredux/src/loading_screen.rs` must join the Dim 1 and Dim 5 Paths. While a cover is up it decides the whole TLAS membership, drops BLAS and owns GPU handles. Dim 5 also needs `byroredux/src/cell_loader/unload.rs` (`release_entities`).
- The "run a bench under `BYRO_VALIDATION=1`" First steps (Dims 4 and 11) were flagged at 09-29 and are still unchanged. Replace them with "list under Needs-RenderDoc".
- The baseline's Dim 5 additions (`EguiPass::image_mirrors`, the player gear-import lifecycle) are still unapplied. The skill has had no edit since `9fcfdc3fc`.

**Dim 1**
- TLAS drop causes are now four: name `non_finite_transform` (#4633).
- Add #4884 allocate-before-retire at all three `blas_scratch_buffer` sites (`blas_scratch_realloc_order_tests`).
- Name `tlas_built_this_frame` (#4843) and `ray_query_tlas`.
- Add `clamp_batches_to_instance_capacity` (#4726) and the UI re-clamp (#4722) as siblings of `instance_map_cap`.
- `built_primitive_count` is enforced in `build_tlas`, not inside `decide_use_update`.
- Add the #4891 maybe-in-flight policy for AS submits.
- The pre-TLAS barrier test cuts its file before its own module rather than using `production_text`. It is non-vacuous today.

**Dim 2**
- "#4940 awaits A/B" is stale: #4940 is closed.
- Record the #5018 origin rule `offsetRayOriginForDirection(fragWorldPos, geometricNormal, L)`, pinned by `direct_shadow_rays_orient_their_origin_toward_the_light`, with its two edges (D2-01, D10-02).
- Add the #5020 pin `mixed_visibility_clusters_keep_a_documented_ratio_lag`.
- `ReservoirCurrBuffer` is `writeonly` (#5062).

**Dim 3**
- `gpu_light_is_80_bytes` → `gpu_light_is_64_bytes`; "80 B lights" → 64 B.
- Add the CPU-side identity (`FrameInputs.light_ids`, `[[u32;4]]`) and its three lockstep sites. `hash_light_upload` covers count, lights and remap.
- `gpu_material_size_claims` is a test *module* (fn `no_file_states_a_stale_gpu_material_size`) and covers `GpuMaterial` only.
- Add the semantic lanes:
  - `render_debug.w` = packed weather surface;
  - `GpuTerrainTile` = 176 B (`gpu_terrain_tile_is_176_bytes`, lanes @160/@164);
  - `GroundCoverCell` pads now live.

**Dim 4**
- `every_frame_recorder_is_documented` (#4958) covers `draw_frame` and 8 phase files. "`shader_pipeline_documents_every_record_pass_helper` covers `post_passes.rs` only" is incomplete.
- The early certificate admits kinds 0..=`MATERIAL_KIND_MAX_LIGHTING_SHADER` (16), guarded by `early_fragment_kinds_have_no_discard_or_depth_write_path`.
- Name the #4726 batch clamp and `record_groundcover_bench` in the frame shape.
- The caustic `geometry_bound` latch must also *close* (D4-01).

**Dim 5**
- Device init: `ray_query_supported == false` branches were *removed* by #4894, so the regression to look for is a reintroduced branch, not dead code.
- Add #4887 (square cubemaps), #4886 (transactional recreate, with the D5-03 caveat), #4889 (staging failures skip the overlay frame) and #4891 (`NotSubmitted` vs `MaybeInFlight`, `rollback_global_geometry`).
- Add the #4599 allowlist test, and note it covers allocator locks only.

**Dim 6**
- Add a CDB `.mat` checklist bullet: per-slot replacements, flags reaching their consumers, no keyword classifier after a CDB hit.
- The Guard-line exemption list does not match `nifal.md` §3.
- `cross_game_translation_completeness` is `#[ignore]`d. Run it with `cargo test -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness` (repeat from 09-29).
- The "post-projection zero-length guard" holds for Path 1 only.
- The First step omits `normal_transform.glsl`.
- Cross-skill: `/audit-starfield` still lists the deleted `mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands`.

**Dim 7**
- The caustic bullet should add the #5064 latch and `clear_for_skip` → `reset_parked_slot`.
- The scene key folds the merged post-combustion `frame_lights`; identities are not part of it.

**Dim 8**
- Add the #4784 occupancy mask: bindings 24/25, host zero-and-seed, the 27-cell dilated gate and the camera-cut precondition.
- Add the #4782 store clamp, the #4780 every-skip history reset and the #4909 `sun_illuminance` lane.
- Add `combustion_occupancy_buffers` to the `sync.rs` rider list.
- Add the guards `transport_occupancy_gate_is_wired_into_the_inject_pass`, `v_buffer_history_rejects_non_finite_and_the_store_clamps_to_fp16`, `every_skipped_frame_drops_the_temporal_history_not_just_the_first`, `caustic_dispatch_is_gated_on_geometry_bindings_written` and `volumetric_sun_uses_portal_lane_only_inside`.

**Dim 9**
- Add a residency / `in_tlas` gate item for the skin dispatch (D9-01).
- The rebuild must not destroy the live BLAS before its replacement succeeds (D9-02).
- Update builds use `updateScratchSize`, with no guaranteed ordering against build size (D9-03; Dim 1 too).
- "Pinned against LRU" → "never LRU candidates; leave only through `drop_skinned_blas`".
- The #4884 immediate destroy rests on two conditions: the both-fence wait, and the first-sight batch being the first recorder of `blas_scratch_buffer` in the frame.

**Dim 10**
- "unique `history_id`" is stale: identities are a CPU-side slice, and `remap` maps duplicates to INVALID.
- "Directional pinned at slot 0" holds for the scene key only (NIF directionals ride the point suffix).
- Record #4938 (NIF lights consume the canonical LIGH falloff), and that `affected_node_names` is never consumed.
- Since #4902 every interior has an outdoor `SkyParamsRes`.

**Dim 11**
- `tonemap(graded * exposure)` → `tonemap(compressed * exposure)`; add the `adaptation_chroma_compress` mirror.
- Add the exposure envelope order: clamp to [1/32, 2], then compensate.
- "Degrades to `DEFAULT_EXPOSURE`" needs three refinements:
  - an upload-failure latch freezes the last value;
  - fixed mode writes `ExposureTuning.fixed_exposure`;
  - the cover uses `STAGE_EXPOSURE_LINEAR`.
- The live adaptation constant comes from `ExposureTuning`.

**Dim 12**
- Paths and First step need `byroredux/src/cornell/`, not only `cornell.rs`. The `_audit-owners.md` row `byroredux/src/cornell.rs` no longer prefix-matches the split files.
- `timestamp_supported == false` → `gpu_timers_supported()`.
- The reader list omits `fill_skin_coverage_stats`, `SkinCoverageStats` and the bench format string.
- The #4868 housekeeping note can be dropped (closed).

## Guard posture

| Dim | Guards confirmed (exist, not `#[ignore]`d, ran green) | Notes |
|---|---|---|
| 1 | `acceleration` filter, TLAS barrier pin, static-BLAS recovery (lib + bin) | none sees D9-01 / D1-01 / D1-02 |
| 2 | `shader_contract` (116), #5018 pin, depth family, glass/IOR, `light_history`, ray-budget, ReSTIR pins | none sees D2-01 |
| 3 | size/offset/mirror/UBO/light-header, `gpu_material_size_claims` | `gpu_light_is_80_bytes` renamed → `_64_` |
| 4 | post-pass, egui dependency, depth capture, FIF contract, blend split (now `frame_params.rs`), descriptor reflection, #5057 / #5064 / #4958 pins | the #5064 pin is a `contains` check and cannot see the one-way latch |
| 5 | geometry compaction/rebuild, skin-slot drain, staging/image-chain allow-lists | allow-list exemptions are not coverage |
| 6 | spawner guard (non-vacuous after #4917), core PBR idempotence/glass overlay, corpus `cross_game_translation_completeness` (run, pass) | `mat_path_merges_cdb_authored_textures_when_indexed` pins D6-01's defect |
| 7 | TAA/SVGF/bloom/jitter/aperture | no SVGF NaN pin (D7-01), no `frame_lights` pin (D7-02) |
| 8 | water/volumetrics/caustic, #4782 / #4774 / #4780 pins | two #4784 guards vacuous (D8-03) |
| 9 | push-constant size, stride, `palette`, morph weak-ref, bin palette overflow + rollback | none sees D9-01..03 |
| 10 | BSDF/light source-shape pins, bin light-policy tests, overflow warn | the Gilded Carafe baseline encodes D10-01 |
| 11 | lib `exposure|tonemap|upscal|post_passes`, fsr3-sys 8/0, bin FSR default | no shader-side meter-order pin (D11-02) |
| 12 | `gpu_timers`, bracket coverage, debug-mode guards, bin bench keys, `mat_set_tests` | — |

Totals:
- renderer lib 1359 / 0 / 1;
- bin 2611 / 0 / 48;
- core `--features inspect` 791 / 0;
- fsr3-sys 8 / 0;
- corpus NIFAL 1 / 0;
- stale-SPIR-V gate clean.

## Process notes

- **Dedup.**
  - Open issues came from `gh issue list --limit 400` (144 open).
  - Each dimension searched closed issues by keyword (listed per finding).
  - All seven 09-29 REN issues (#5018, #5020, #5022, #5023, #5026, #5028, #5030) are CLOSED and their fixes were re-verified.
- **Cross-dimension merges.**
  - REN-D1-2026-10-03-03 (loading-cover framing-failure leak) → REN-D5-2026-10-03-01.
  - REN-D2-2026-10-03-02 (`renderer.md` 80 B `GpuLight`) → REN-D3-2026-10-03-01.
  - REN-D12-2026-10-03-03 (`mat.set` Cornell path) → REN-D6-2026-10-03-05.
  - Corroborations without a new ID: Dim 8 corroborated D4-01's latch; Dim 9 notes the TLAS half of D9-03 (Dim 1 scope).
- **Severity changes by the orchestrator.**
  - D4-01: HIGH → CRITICAL, after tracing the scene-set bindings 8/9 and the RT-flag lifecycle. The agent left that half untraced.
  - All other severities are as filed.
- **Orchestrator spot-verification.** D2-01 (no Ng term in `shadowableLightRadiance`), D1-03/D5-01 (`retire_stage` body; framing arm after `count > 0`), D4-01, D6-01 (`capture_instance` drops `row.index`), D9-01 (dispatch loop and shader indexing), D10-01 (no reader of `affected_node_names` under `byroredux/src`; first-copy retention), D11-01 (`ExposureTuning` 0.2 s pushed each frame).
- **Dropped candidates.**
  - The occupancy-mask binding 25 cross-submission barrier (Dim 4): covered by the global compute→compute barrier. Re-filed as the D8-02 documentation and contract gap only.
  - `NiAmbientLight` carriers (Dim 10): zero diffuse, already skipped.
  - The #4782 emission-history sidecar NaN (Dim 8): bounded to one froxel for one frame.
  - The `fixed_exposure` 0.85 duplicate (Dim 11): folded into D11-01's fix.
- **Out-of-scope pointers.**
  - The CDB index can still be built twice on concurrent first lookups (about 470 MB; `/audit-performance`).
  - `INTERIOR_AMBIENT_SCALE` is an untested constant (Dim 7/10).
- **Per-dimension scratch files** (`dim_1.md` … `dim_12.md`) held each agent's full Verified-OK list. They were reconciled against this report (38 scratch findings − 3 merges = 35 here) and removed at Phase 4 cleanup. Each verified-OK delta commit is summarised under *RT Pipeline* / *GPU-Struct & Memory Assessment* above.
