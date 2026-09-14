# #4294: REN-2026-09-14-D9-01: an LRU-evicted `MorphSlot` is never recreated — any entity that misses the skin dispatch list for more than `MAX_FRAMES_IN_FLIGHT` frames permanently loses morph-target deformation, including on the raster path

- **Labels**: medium,renderer,animation,memory,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4294
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: MEDIUM
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`record_skinned_blas_refit` — the MorphSlot LRU bump inside the dispatch loop, and the `morph_evictees` sweep); `byroredux/src/cell_loader/spawn/mesh_instance.rs` (`spawn_mesh_instance` — the sole `ctx.morph_slots.insert`)
- **Status**: NEW
- **Description**: `MorphSlot`s are created exactly once, at spawn (`create_morph_slot_for_mesh` → `ctx.morph_slots.insert(entity, slot)` is the only insertion site; `morph_compute.rs`'s module doc confirms "created once at spawn … rather than lazily on first dispatch"). But their eviction was modelled on `SkinSlot`, which *is* lazily recreated: the sweep drops any MorphSlot whose `last_used_frame` trails `frame_counter` by `MAX_FRAMES_IN_FLIGHT + 1` frames. The only place `morph_slot.last_used_frame` is refreshed is inside the per-entity skin dispatch loop, after these gates: the draw must have `bone_offset != 0`, its mesh must be `rt_capable`, the entity must have a live `SkinSlot` (`self.skin_slots.get_mut(&entity_id) else continue` runs *before* the morph bump), and the whole block needs a live `skin_compute` + `accel_manager` + global vertex buffer + bone buffer. Any entity that fails one of those for 3+ frames has its MorphSlot destroyed, and nothing ever creates a new one. Once the entity reappears, `build_and_upload_instances` finds no slot, `morph_gpu_fields_for_draw(None)` writes `morphDeltaAddress = 0`, and `triangle.vert` skips blending for the rest of the entity's life.
- **Evidence**: Paths that keep a MorphSlot entity out of the bump:
  - `static_meshes.rs` skips entities whose `AnimatedVisibility` is false (`if !visible { continue; }`), so the entity has no `DrawCommand` at all.
  - A full `SkinSlotPool` makes `allocate` return `None`, the draw goes out with `bone_offset = 0`, and the loop's `if dc.bone_offset == 0 { continue; }` skips it.
  - A failed `create_slot` (the `failed_skin_slots` path) means no `SkinSlot`, so the loop `continue`s before the morph bump. Raster would still deform here: the `GpuInstance` morph lookup is gated only on `bone_offset != 0`, not on a SkinSlot.
  - A skinned mesh uploaded `rt_capable = false` is skipped by `if !mesh.rt_capable { continue; }`. Its MorphSlot is evicted about 3 frames after spawn, even though raster reads morph for every `bone_offset != 0` draw.
  
  `grep "morph_slots.insert"` finds only `mesh_instance.rs`. `pending_morph_unload_victims` only removes.
- **Impact**: Visual only: blink/lip-sync/expression morphs (NiGeomMorpherController, #3231) stop for good on the affected NPC, with no log line (eviction is `log::debug!`). Frequency at runtime is unmeasured. The skin-pool-full and create-slot-failure triggers need memory pressure, and the visibility trigger needs a skinned morph mesh hidden for 3+ frames. Not reproduced here (no engine launch). `morph_memory_usage()`'s active-slot count dropping while the entities are still spawned would confirm it.
- **Related**: #3231 (morph path), #3374 (drain placement), #643 (the SkinSlot LRU this copied), #2925 (epoch rebase, covers morph correctly).
- **Suggested Fix**: Either exempt MorphSlots from idle eviction (release them only through `pending_morph_unload_victims` on despawn), or move the LRU bump to a place every live morph entity reaches, e.g. `update_morph_weights` / `flush_pending_morph_weights`, which already visit every slot each frame. Add a test pinning that a slot absent from dispatch for N frames survives.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **DROP**: If Vulkan objects change, teardown is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
