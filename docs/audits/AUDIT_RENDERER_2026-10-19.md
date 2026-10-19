**HEAD**: 75b921cf4 · **Baseline**: AUDIT_RENDERER_2026-10-18.md (exterior-scoped; the last full renderer audit is 2026-10-14) · **Audited**: Dim 1 (static-BLAS lifecycle slice), Dim 5 (mesh / texture / terrain-tile / geometry-pool admission and release slice), Dim 9 (skin-slot and skinned-BLAS unload slice); scope=streaming-deep AREA-SCOPED (renderer contract with `byroredux/src/streaming/`, `npc_spawn/`, `cell_loader/`) · **Unchanged since baseline (skimmed)**: the acceleration, texture-registry, mesh-registry and skin-slot code is untouched since 2026-10-18 apart from `a5958adf3` (starved LOD verdict), `4a6128389` (shadow filter submit failure) and `cf1a234d0` (volumetrics sweep); none touch the contract audited here.

**AREA-SCOPED (streaming-deep). This is NOT a full-codebase renderer audit.** Do not use it as the next run's full baseline. The renderer is a neighbour of this area, not an owner: only the contract between the area and the renderer was audited, namely what the cell loader, NPC spawn and streaming code upload and release (mesh, BLAS, texture, terrain-tile slot, geometry pool admission, skin slots) and in what order.

# Renderer Audit — 2026-10-19 (streaming-deep contract slice)

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | none |
| HIGH | 0 | none |
| MEDIUM | 3 | REN-SD-01, REN-SD-02, REN-SD-04 |
| LOW | 3 | REN-SD-03, REN-SD-05, REN-SD-06 |

The release side of the contract is sound. A cell unload frees terrain-tile slots first, drops the static BLAS of exactly the meshes whose last holder leaves (pre-pass `refcount == drop count`), then the mesh refs, then queues skin and morph slot victims for the renderer's post-fence-wait eviction, then drops texture refs, and only then despawns. Every GPU free on that path is deferred through a countdown queue, or is synchronous only behind the both-slots fence wait. No use-after-free window was found.

The problems sit on the upload and accounting side:
- A cell loaded in the same crossing as an unload cannot use the space that unload freed, and a cell refused at the geometry pool is never retried (REN-SD-01).
- The NPC spawn path builds a static BLAS for every skinned mesh although the TLAS never reads one (REN-SD-02).
- Two texture-refcount accounting gaps: particle greyscale LUTs are never released (REN-SD-03), and the ObScript `SwapTexture` helper acquires one ref for N holders and displaces the originals without releasing them (REN-SD-04).

**Method.** Static review only. The engine, any Vulkan device and game data were not used (suite rule). Nothing needing RenderDoc or `BYRO_VALIDATION=1` was attempted.

**Guards run** (repo `target/`, all green, none `#[ignore]`d):
- `cargo test -p byroredux-renderer --lib -- acceleration static_blas_recovery build_tlas_is_preceded skin_slot_drain planning_compaction in_place_compaction out_of_order_layout chunked_rebuild live_budget_gates texture_registry drop_ release`: 238 passed. This names `build_tlas_is_preceded_by_an_unconditional_as_write_to_as_read_barrier`, `static_blas_recovery_covers_both_sources_and_protects_current_draws` and `skin_slot_drain_sits_outside_the_skin_compute_guard`.
- `cargo test -p byroredux --bin byroredux -- static_blas_recovery_runs unload_ release_ drain_ queue_skin skin_cleanup rapier_release inventory_release`: 74 passed (includes `static_blas_recovery_runs_between_frames_not_in_the_render_driver`, `rapier_release_tests`, the unload greyscale-LUT tests).
- `scripts/check-shader-artifacts.sh`: clean (37 shaders plus the early variant).

## RT Pipeline Assessment (area slice)

- **BLAS release ordering.** `release_entities_timed` drops a static BLAS only for handles whose refcount equals this batch's drop count, before `drop_meshes`. Both feed a `DEFAULT_COUNTDOWN` deferred queue (`drop_blas` in `blas_static.rs`, `release_mesh_ref` in `mesh.rs`) ticked after the both-slots fence wait. `drop_blas` also sets `needs_full_rebuild` on both TLAS slots and bumps `blas_map_generation`, so no TLAS BUILD or UPDATE can name a dropped address. Cost: a flag and a counter, no allocation.
- **LOD paths.** `release_lod_gpu_resources` and `unload_lod_block` call `drop_blas` unconditionally (not refcount-gated). That is safe: global-only LOD meshes are never registered in `geometry_cache`'s dedicated bucket (`register_scene_geometry_for_sharing` is called only for dedicated-buffer meshes), so no cell placement can share a LOD handle.
- **Skinned release.** Unload queues `pending_skin_unload_victims` and `pending_morph_unload_victims`. The renderer drains them post-fence-wait in `record_skinned_blas_refit`'s eviction block, outside the `skin_compute` guard (guard test green). Slot destroy is synchronous, which is legal only because `sync_and_acquire_frame` waits both slots' fences. The skinned BLAS goes through `drop_skinned_blas` into `pending_destroy_blas`. `failed_skin_slots` and `failed_skin_blas` are pruned or cleared on the same path.
- **TLAS churn at a crossing.** Unload sets `needs_full_rebuild`; the instance set changes anyway, so BUILD would be forced regardless. TLAS capacity is `max(2 × count, 8192)`. Shrink only records `tlas_shrink_pending` and the allocate-then-swap path performs it. `ensure_tlas_state` issues a defensive `device_wait_idle`, which is a no-op behind the both-slots wait. No thrash found.
- **Recovery.** `restore_missing_static_blas_for_draws` filters `bone_offset != 0`, so skinned draws are never restored (relevant to REN-SD-02).

## GPU-Struct & Memory Assessment (area slice)

- **Texture refcount ledger on unload.** `collect_victim_gpu_handles` sweeps `MeshHandle`, `TextureHandle`, `NormalMapHandle`, `WaterNoiseMapHandles`, `MaterialTextureHandles::secondary_values`, `AnimatedTextureFlip::all_handles`, and `TerrainTileSlot` (8 layer refs via `free_terrain_tile`). Two acquisition sites keep their refs outside that sweep (REN-SD-03, REN-SD-04).
- **Deferred destroy.** Textures (`release.rs`, age ≥ `MAX_FRAMES_IN_FLIGHT` frame ids), meshes (`DeferredDestroyQueue`) and BLAS use consistent countdowns. A dropped bindless slot is redirected to the fallback immediately for the current set and queued for the other (#92). A purged `pending_dds_uploads` entry on last release (#4879) is in place.
- **Terrain tiles.** `MAX_TERRAIN_TILES` = 1024 against 729 slots at radius 12 with the +1 unload hysteresis: fits. The tile SSBO is a single shared device-local buffer refreshed by a frame-recorded copy; the both-slots wait covers the WAR hazard.
- **Geometry pool.** Admission is judged on `pending_vertices.len()`, which includes dead spans until compaction (REN-SD-01). The pool caps are 4 M vertices and 16 M indices (`memory-budget.md`).

## Findings

### REN-SD-2026-10-19-01: Space freed by a crossing's own unload is unavailable to that crossing's loads, and a full-detail cell refused at the geometry pool is never retried or counted
- **Severity**: MEDIUM (a cell loses statics or its terrain mesh; the loss is silent and permanent for that residency)
- **Dimension**: Memory/Lifecycle
- **Location**:
  - `crates/renderer/src/mesh.rs` `accumulate_global_geometry` (admission tested against `pending_vertices.len()` / `pending_indices.len()` via `check_pool_growth`; closed latch checked first).
  - `crates/renderer/src/mesh/geometry_ssbo.rs` `plan_geometry_compaction` (the only place holes are squeezed out, and the only place `scene_geometry_admission_closed = false`, line 268).
  - `byroredux/src/app_frame.rs` (rebuild gate at the `defer_geometry_rebuild` / `rebuild_geometry_ssbo` block) and `byroredux/src/streaming/mod.rs` `WorldStreamingState::geometry_batch_in_progress`.
  - `byroredux/src/app_step.rs` `step_streaming` (sets `lod_reconcile_pending = true` at the crossing, line 152; the comment "Unload first to free GPU resources before kicking new loads", line 172).
  - Refusal sinks: `byroredux/src/cell_loader/spawn/mesh_instance.rs` `prepare_mesh_upload_range` (the "skipping remaining uncached geometry for this cell" arm) and `byroredux/src/cell_loader/terrain.rs` (`Failed to upload terrain mesh` → `return None`).
- **Status**: NEW. Related: EXT-D6-2026-10-19-01 names "the full-detail analogue (a cell loaded under starvation keeps its missing terrain / meshes until it is reloaded)" in its Related line and files only the LOD half; this is that analogue plus the ordering cause it did not examine.
- **Description**:
  - `drop_meshes` marks `geometry_has_holes` and `geometry_dirty` but leaves the dead spans inside `pending_vertices` / `pending_indices`. Admission counts those spans.
  - Compaction runs only inside `rebuild_geometry_ssbo`, which `app_frame.rs` starts only when `!geometry_batch_in_progress()`. That predicate is true whenever `pending` is non-empty, an apply job is active, or `lod_reconcile_pending` is set. The crossing that unloads sets `lod_reconcile_pending` itself (`step_streaming`, line 152) and queues the loads in the same call.
  - So the dead spans of the three cells unloaded at a crossing stay counted until every load and the LOD reconcile have drained. A load that crosses the cap in the meantime closes admission (`scene_geometry_admission_closed = true`), and every later fresh mesh of that and following cells is refused.
  - The refusal is not retried. `prepare_mesh_upload_range` returns `PreparedMeshUpload::Failed` for the meshes it could not admit, `spawn_mesh_instance` returns `false`, and the cell is inserted into `loaded` like any other. A refused terrain mesh returns `None` from the terrain spawn and the cell has no ground.
  - After the batch settles, compaction reopens admission, but nothing re-runs the refused cell. The only signal is a one-line `log::warn!` per placement batch. No telemetry counter exposes full-detail refusals; `rejected_for_capacity` (`streaming_helpers.rs`) diffs `scene_geometry_admission_rejections()` around the LOD rings only.
- **Evidence**:
  - `accumulate_global_geometry`: `let new_v_len = self.pending_vertices.len() + vertices.len();` then `check_pool_growth(..., VERTEX_POOL_HARD_CAP, ...)` → `scene_geometry_admission_closed = true` on `Err`.
  - `app_frame.rs`: `rebuild_in_progress || (ctx.mesh_registry.is_geometry_dirty() && !defer_geometry_rebuild)`.
  - `geometry_batch_in_progress`: `!self.pending.is_empty() || self.active_apply.is_some() || self.persistent_apply.is_some() || self.lod_reconcile_pending`.
  - `grep` for `plan_geometry_compaction` callers outside tests: only `rebuild_geometry_ssbo` (`geometry_ssbo.rs:490`).
- **Disproof attempted**:
  - Could compaction run mid-batch through another path? No: the only production callers of `rebuild_geometry_ssbo` are the gated `app_frame.rs` block, and `build_geometry_ssbo` (`scene.rs`, bootstrap) does not compact.
  - Could a refused cell be reloaded? Nothing in `streaming/` or `cell_loader/` re-queues a loaded cell; only a stream-out followed by a stream-in does.
  - Does the interior/exterior door path suffer the same way? Checked: after `drain_streaming_state` and `unload_current_interior` a frame normally renders (no streaming state, so no defer) and compaction runs before the interior apply job's first slice, so that path is not affected; the exterior arrival path loads synchronously in the same step as `unload_current_interior`, but the latch is already clear after the interior frames. Not filed.
- **Impact**:
  - The pool's effective capacity at a crossing is `cap − (volume unloaded at that crossing)`, not `cap`. For a radius-12 grid that is one or two rows, about 4–8 % of the resident set.
  - Whenever the live set sits inside that band, which EXT-D6-01 reports is reachable at the default radius on FNV and FO4, the newly entered cells lose their uncached meshes or their ground, and keep that loss until they are unloaded. A cell refused is also invisible to every smoke gate.
  - Not measured: no engine launch (suite rule).
- **Related**: EXT-D6-2026-10-19-01 (LOD analogue), #6054 (closed, `Starved` verdict), #2678 (hole flag), `docs/engine/exterior-grid-streaming.md` §"Geometry admission".
- **Suggested Fix**:
  - Smallest change: before queueing loads at a crossing that unloaded cells, run the compaction (`plan_geometry_compaction` only mutates the CPU pools; the GPU copy can stay resumable) so the freed space is counted. Alternatively count dead spans out of the admission test.
  - Independently: record full-detail refusals (a `MeshRegistry` counter diffed per cell apply) and put a refused cell on a retry list re-armed by the admission closed → open edge.
  - Neither touches a barrier or layout.

### REN-SD-2026-10-19-02: NPC and loose-NIF spawn builds a static BLAS for every skinned mesh, and the TLAS never reads it
- **Severity**: MEDIUM (VRAM billed against the BLAS budget, plus synchronous fence-waited builds on the streaming path)
- **Dimension**: AS Correctness · Skinning
- **Location**:
  - `byroredux/src/scene/nif_loader.rs` `spawn_nif_mesh` (`blas_specs.push((mesh_handle, num_verts as u32, mesh.indices.len() as u32))` gated only on `for_rt && fresh_source`, line 1168) and the caller's `ctx.build_blas_batched(&blas_specs)` (line 506).
  - `crates/renderer/src/vulkan/acceleration/tlas.rs` `build_tlas_instances` (skinned draws, `bone_offset != 0`, read only `self.skinned_blas`, line 556).
  - `crates/renderer/src/vulkan/context/resources.rs` `restore_missing_static_blas_for_draws` (filters `cmd.bone_offset != 0`, line 418).
- **Status**: NEW
- **Description**:
  - `spawn_nif_mesh` queues a static (mesh-handle keyed) BLAS for every freshly uploaded mesh with `for_rt`, including meshes that carry `mesh.skin`.
  - A skinned mesh that binds (`attach_nif_skin_binding` succeeds) is drawn with `bone_offset != 0`. `build_tlas_instances` takes the per-entity skinned BLAS for those draws and never consults `blas_entries[mesh_handle]`.
  - So the bind-pose static BLAS built at spawn is dead weight: the rigid recovery pass skips skinned draws and nothing else reads it.
  - The only case that would use it is a skinned mesh that fell back to rigid drawing (bone cap exceeded, or the skin-slot pool full), and for that case the recovery pass would build it lazily anyway.
- **Evidence**: `tlas.rs` `if draw_cmd.bone_offset != 0 { let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) ... } else { ... self.blas_entries.get_mut(mesh_handle) ... }`; `resources.rs` `if cmd.bone_offset != 0 || !draw_command_eligible_for_tlas(cmd) { return None; }`.
- **Disproof attempted**:
  - Is the static BLAS a fallback when the skinned BLAS is missing? No: the missing-skinned arm `continue`s and counts `missing_skinned_blas`.
  - Does `build_blas_batched` skip skinned meshes? It filters only on `mesh.rt_capable`.
  - Does cell-loader placement hit this? It attaches no `SkinnedMesh` (#2440), so its rigid builds are correct; only the `nif_loader` path is affected.
- **Impact**:
  - Every unique skinned NPC mesh (per-NPC FaceGeom heads, hair, shared bodies and outfits) carries a compacted BLAS that counts in `static_blas_bytes` and `shared_blas_scratch_peak`.
  - `build_blas_batched` is a synchronous submit with host fence waits and a compaction-size readback per call, and `spawn_nif_mesh` runs once per NPC part unit; `restore_missing_static_blas_for_draws` documents ~0.2–0.5 ms per mesh for the same primitive.
  - The skinned-handle BLAS never get their LRU stamp bumped, so under pressure they are evicted first (cheap), but eviction forces a full TLAS rebuild.
  - Not measured: no runtime. An NPC-dense interior with unique heads is the worst case.
- **Related**: #4180 (recovery between frames), #3840 (pending bytes), `crates/renderer/src/vulkan/acceleration/blas_static.rs` `shared_blas_scratch_peak` users.
- **Suggested Fix**: in `spawn_nif_mesh`, skip the `blas_specs.push` when `skin_attached` will be true (or `mesh.skin.is_some()` and the bone count fits), leaving the rigid-fallback case to the lazy recovery pass. Pass `for_rt` unchanged so skinned-output BLAS input flags stay as they are. Add a source-shape test that pairs the push with the skin gate.

### REN-SD-2026-10-19-03: Particle emitter greyscale-LUT texture refs are never released on unload
- **Severity**: LOW (the refcount leaks, bounded by the number of distinct LUT paths; no per-frame growth)
- **Dimension**: Memory/Lifecycle
- **Location**:
  - `byroredux/src/cell_loader/spawn.rs` emitter spawn (`greyscale_lut = em.greyscale_lut_map ... resolve_texture(...)`, line ~1310).
  - `byroredux/src/scene/nif_loader.rs` emitter spawn (`greyscale_lut = emitter.greyscale_lut_map ... resolve_texture(...)`, line ~1703).
  - `byroredux/src/cell_loader/unload.rs` `collect_victim_gpu_handles` (no `ParticleEmitter` read).
  - `crates/core/src/ecs/components/particle.rs` `ParticleEmitter::greyscale_lut_index`.
- **Status**: NEW (sibling of closed #1341, which fixed the same leak for `MaterialTextureHandles`; not a regression of it)
- **Description**:
  - Both spawn paths call `resolve_texture` for the emitter's authored greyscale palette. That is `acquire_by_path` or an enqueue, so it holds one ref per call.
  - The returned handle is written into the plain `u32` `ParticleEmitter.greyscale_lut_index` through `apply_emitter_overlays`.
  - `collect_victim_gpu_handles` does not read `ParticleEmitter`, so unload never decrements.
  - On top of that, the ref is acquired before the two early exits that spawn no emitter at all: the fog-volume replacement (`medium_from_particle` → `continue`/`fog_volume`) and the unresolvable-sprite skip. There the LUT ref is orphaned the moment it is taken.
- **Evidence**: `unload_greyscale_lut_tests.rs` pins only the `MaterialTextureHandles` role; `grep -n ParticleEmitter byroredux/src/cell_loader/unload.rs` is empty.
- **Impact**: a LUT texture (small, palette-sized) per distinct path stays resident with a growing refcount for the session; the FNV ownership soak cannot see it because FNV particles author no BGEM LUT. Skyrim and FO4 effect shaders do.
- **Related**: #1341, #1656, #3590 / #4044 (LUT introduction).
- **Suggested Fix**: either resolve the LUT only after the emitter is certain to spawn and store it in a swept handle component, or have `collect_victim_gpu_handles` read `ParticleEmitter::greyscale_lut_index` and pin it with a second case in `unload_greyscale_lut_tests`.

### REN-SD-2026-10-19-04: ObScript `SwapTexture` acquires one texture ref for N holders and leaks every displaced original
- **Severity**: MEDIUM (premature free of a shared texture, or a ref-underflow warning, on unload; plus leaked originals)
- **Dimension**: Memory/Lifecycle
- **Location**: `byroredux/src/asset_provider/texture.rs` `apply_texture_swap` (`let handle = resolve_texture(ctx, tex_provider, Some(&path));` line 1344, then `handles.insert(mesh, TextureHandle(handle))` for each matched mesh); driver `byroredux/src/app_step.rs` `step_texture_swaps`.
- **Status**: NEW
- **Description**:
  - One `resolve_texture` takes one ref. The handle is then assigned to every mesh under the target whose base texture path contains the slot name.
  - Unload releases one ref per `TextureHandle` holder, so M matched meshes release M refs against one acquired: net −(M−1).
  - The meshes' previous `TextureHandle` values were acquired at spawn and are overwritten without a `drop_texture`, so each original leaks one ref. Swapping the same slot twice leaks the earlier swap's ref too.
  - When M ≥ 2 and the texture is shared with other resident meshes, the extra releases reach zero early: the other holders' bindless slot is redirected to the checkerboard. When it is not shared, the surplus decrement logs `drop_texture(..) on already-released handle`.
- **Evidence**: `apply_texture_swap` body; `resolve_texture_view_with_clamp` doc: "each resolve pairs with one drop_texture on cell unload ... exactly one outstanding ref per caller".
- **Disproof attempted**: is the handle re-acquired per mesh somewhere? No, `resolve_texture` is called once before the loop and the loop only inserts the handle. Is the old handle released? The loop never reads the previous `TextureHandle`.
- **Impact**: wrong textures on unrelated meshes after the swap target's cell unloads, and the originals pinned. The command is rare (endgame slides), so blast radius is small.
- **Related**: #524 (refcount model), #4427 (flipbook frames, the same ledger done correctly).
- **Suggested Fix**: acquire once per matched mesh (loop `resolve_texture`, or bump the registry ref by M−1) and `drop_texture` the displaced handle before overwriting it.

### REN-SD-2026-10-19-05: A texture newly queued by `SwapTexture` is not flushed until some unrelated upload batch runs
- **Severity**: LOW (visible checkerboard until the next flush; not verified at runtime)
- **Dimension**: Memory/Lifecycle
- **Location**: `byroredux/src/app_step.rs` `step_texture_swaps` (no flush); flush callers are `flush_pending_cell_textures*` (cell apply), `flush_pending_lod_textures` (only when a LOD attempt ran), and the NPC loaders (`loot_appearance.rs`).
- **Status**: NEW
- **Description**: `apply_texture_swap` goes through `resolve_texture`, which on a miss reserves a bindless slot (descriptor = fallback) and queues the DDS. The frame renderer does not drain `pending_dds_uploads` (the app only reads the count for the loading-cover decision). In an interior, or an exterior with no LOD or cell work pending, the next flush can be arbitrarily far away.
- **Evidence**: `grep -rn flush_pending_uploads byroredux crates` shows no caller from `step_texture_swaps`, `draw_frame` or the idle path.
- **Impact**: the swapped surface shows the fallback texture until some other batch flushes the shared queue. Unverified at runtime (no engine launch).
- **Suggested Fix**: call the existing `flush_pending_cell_textures` helper at the end of `step_texture_swaps` when it applied anything.

### REN-SD-2026-10-19-06: `acquire_matching_geometry` carries a dead guard, `refcount(handle) == Some(0)`
- **Severity**: LOW (dead code; the intended property holds by another route)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh/geometry_sharing.rs:115` against `crates/renderer/src/mesh.rs` `refcount` (filters `rc > 0`).
- **Status**: NEW
- **Description**: `refcount` returns `None` for a zero count, so `== Some(0)` can never be true. The guard was meant to skip a released handle. Today that case is covered because `drop_meshes` prunes `geometry_cache` (`prune_shared_geometry`) and `self.get(handle)` returns `None` for a released slot, so no wrong geometry is shared. A change to either of those would find the guard silently inert.
- **Evidence**: `refcount` is `.get(handle).copied().filter(|&rc| rc > 0)`; introduced in `b9e961eeb`.
- **Impact**: none today; a misleading safety check on the sharing path that streaming exercises hard.
- **Suggested Fix**: test `self.refcount(handle).is_none()` and keep one of the two checks covered by a unit test that releases the last holder without pruning.

## Prioritized Fix Order

1. **Correctness**: REN-SD-01 (compaction before loads, retry or counter), REN-SD-04 (per-holder acquisition and displaced-handle release).
2. **Safety/accounting**: REN-SD-03 (sweep or release the LUT), REN-SD-05 (flush).
3. **Optimization**: REN-SD-02 (skip static BLAS for skinned meshes).
4. **Hygiene**: REN-SD-06.

## Dimension notes (no further findings)

- **Dim 2** (ray-query shading), **Dim 3** (GPU struct layout), **Dim 4** (sync/pipeline), **Dim 6** (material translation), **Dim 7** (denoise/resolve), **Dim 8** (volumetrics, caustics, water shading), **Dim 10** (lighting), **Dim 11** (FSR/presentation), **Dim 12** (debug/telemetry): out of area.
- **Checked and not filed**:
  - **#2030 bindless slots are grow-only.** Documented ceiling (`memory-budget.md`, 32,767 slots). The FNV ownership soak holds ~190 live textures, so roughly 170 full turnovers are needed to exhaust it; not reachable in a plausible session on vanilla FNV. Unmeasured for FO4 / mod-heavy loads.
  - **`TextureRegistry::tick_deferred_destroy` is O(slot count) per frame.** At the ceiling that is ~2 MB of cache lines per frame; unmeasured, below a filing threshold.
  - **`unload_lod_block` / `release_lod_gpu_resources` drop the BLAS unconditionally.** Safe (see RT Pipeline Assessment).
  - **Interior door load after a saturated exterior.** The admission latch is cleared by the first rendered frame (see REN-SD-01 disproof).
  - **Water meshes.** Upload with `rt_enabled = false`; no BLAS, and the LOD-water rebuild drops the old mesh after the new one is admitted (needs headroom for both; same class as REN-SD-01, not filed separately).
- **Skill premises to correct** (below).

## Needs-RenderDoc

None. Every finding here is host-side accounting or ordering and is visible to a unit test or a log.

## Not checked

- Anything needing the engine, a Vulkan device or game data: runtime size of REN-SD-01's band, the wasted-BLAS bytes in REN-SD-02, and whether REN-SD-05 shows the checkerboard.
- `cargo test -p byroredux-core --features inspect` and the shader-contract families (out of area).
- `BYRO_VALIDATION=1` sync-validation run.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 5**: add the geometry-pool admission/compaction contract with the streaming gate (`geometry_batch_in_progress` defers compaction; admission counts dead spans). The `Paths:` line should list `byroredux/src/streaming/mod.rs` and `byroredux/src/app_frame.rs` (rebuild gate) for that.
- **Dim 1**: note that the rigid recovery pass excludes `bone_offset != 0`, and that `spawn_nif_mesh` still queues static BLAS for skinned meshes (REN-SD-02).
- **Dim 5 guard list**: `refcount(handle) == Some(0)` in `geometry_sharing.rs` is dead (REN-SD-06); any text relying on it as the "released handle" skip is wrong.
- **Dim 5**: the unload texture-ledger list should name `ParticleEmitter::greyscale_lut_index` and `apply_texture_swap` as acquisition sites outside the victim walk.
