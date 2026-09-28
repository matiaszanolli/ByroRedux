# #4969: REN-D9-2026-09-27-01: The resize LRU rebase to the "never dispatched" sentinel makes a just-despawned entity's SkinSlot, skinned BLAS and MorphSlot permanently unreapable

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4969
- **Labels**: low,renderer,memory,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D9-2026-09-27-01**._

- **Severity**: LOW
- **Dimension**: Skinning (Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`finalize_screen_pass_state`: `self.frame_counter = 0;` then `for slot in self.skin_slots.values_mut() { slot.last_used_frame = 0; }` and the `morph_slots` sibling). `crates/renderer/src/vulkan/skin_compute.rs` (`should_evict_skin_slot`: `if last_used_frame == 0 { return false; }`). Guard `swapchain_recreate_rebases_skin_slot_stamps_when_it_zeroes_frame_counter` asserts the `= 0` choice.
- **Status**: NEW. The morph half was noted "not filed, needs a repro" in `AUDIT_RENDERER_2026-09-26.md` Needs-validation. This adds the SkinSlot and BLAS half and the mechanism. #2925 (closed) introduced the rebase.
- **Description**: `0` is the #643 sentinel that `should_evict_skin_slot` never evicts. After the rebase, a slot leaves the sentinel only when something re-stamps it:
  - a `SkinSlot` through the skin dispatch loop (the entity is drawn again);
  - a `MorphSlot` through `refresh_morph_slot_lru` (the entity still has a `MeshHandle`).

  An entity despawned outside `unload_cell` is never re-stamped. Examples are superseded outfit parts and other non-cell despawns, which the code comments say rely on "ordinary pool aging". If it was despawned within the `min_idle` (3-frame) window before the recreate, it is never re-stamped, and its slot is never evicted. `unload_cell` queues only its own victims, so nothing else reaches it. The #2925 comment claims the rebase "rescues … the idle slots — exactly the ones eviction exists to reclaim". It does the opposite: it exempts exactly those slots from eviction until a re-stamp that, for a dead entity, never comes. Before #2925 the stale stamp pinned a slot for "as many frames as the session had run". After it, a dead entity's slot is pinned forever.
- **Evidence**: `recreate_swapchain` → `recreate_screen_passes` → `finalize_screen_pass_state` zeroes the counter and rebases every stamp to 0. `set_upscaler_mode` also routes through `recreate_swapchain`, so an FSR preset switch triggers it too. In `record_skinned_blas_refit`'s eviction sweep, `should_evict_skin_slot(0, now, 3)` returns `false` forever, while `drop_skinned_blas` is reached only from that sweep, a remap, or a rebuild.
- **Impact**: Each occurrence leaks until shutdown:
  - one `SkinSlot` (output buffer of `vertex_count × 12 B` plus descriptor sets from the FREE_DESCRIPTOR_SET pool);
  - its skinned BLAS;
  - for morph entities, the weight buffer and a strong ref on the shared `MorphDelta`.

  The window is narrow (a despawn ≤3 frames before a resize or upscaler switch), but it recurs per occurrence and is invisible: `skin.coverage` just shows a higher slot count. No correctness or rendering impact.
- **Related**: #2925, #643, #4294. Dim 5 owns lifecycle and teardown.
- **Suggested Fix**: Rebase to a non-sentinel epoch value (e.g. `1`), not `0`. Live slots are re-stamped on the next frame by dispatch or `refresh_morph_slot_lru`. Dead and idle ones then age out on the normal `min_idle` threshold in the new epoch. Update the guard's assertion to pin "not the sentinel".

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
