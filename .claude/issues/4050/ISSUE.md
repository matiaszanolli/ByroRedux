# #4050 — REN-2026-09-06-D9-04: `SkinSlotPool::sweep` does not purge `pending_uploads`, so a requeued entry can outlive its slot's ownership

**Labels**: low, memory, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D9-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (latent — needs a persistent upload failure to reach)
- **Dimension**: Skinning
- **Location**: `crates/core/src/ecs/resources/skin_slot_pool.rs` (`SkinSlotPool::sweep`, `SkinSlotPool::requeue_pending`), consumed by `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::record_pending_bind_inverse_copies`)
- **Status**: NEW
- **Description**: `sweep` is careful about eviction hygiene — it drops the doomed entity
  from `entity_to_slot`, `last_seen_frame`, `last_pose_hash`, `pose_dirty` and
  `rollback_pose_hash` (each with its own comment explaining why) — but leaves
  `pending_uploads` untouched. Before #3569 that was nearly unreachable, because
  `drain_pending`'s cap (1366) exceeds the pool's own capacity (1364), so a pending entry
  never survived the frame it was created in. The new requeue path is the first thing that
  can hold an entry across frames. If the failure persists ≥ `min_idle` (3) frames while the
  entity leaves the draw list, `sweep` returns its slot to the free list, a different entity
  can `allocate` that slot, and the stale `(slot, old_entity)` entry is still queued —
  `app_frame.rs`'s filter only drops it if the *old* entity's `SkinnedMesh` component is
  gone, which an off-screen-but-alive NPC's is not.
- **Evidence**: The two entries then land in the same drain (cap ≥ capacity), so
  `record_pending_bind_inverse_copies` builds two `vk::BufferCopy` regions with the **same**
  `dst_offset` and issues them in a single `cmd_copy_buffer`. The Vulkan spec does not
  specify the order in which a copy command's regions are applied, so which entity's
  bind-inverse matrices survive in that slot is unspecified — a coin flip, not the
  list-order last-write-wins one might assume from reading the loop.
- **Impact**: One entity renders with another's inverse-bind matrices — a scrambled skin,
  and (through `skin_vertices.comp` → the skinned BLAS) a wrong-geometry AS entry for it.
  Bounded to the two entities sharing the slot, and self-heals on the next successful
  upload for the *live* tenant. Not reachable without `D9-03`'s persistent-failure
  precondition, which is why this is LOW rather than a sibling of `D9-01`.
- **Related**: #3569, #1192 / SAFE-D7-NEW-02 (the cap that used to make this unreachable),
  #1791 / D6-01. Same requeue as `D9-03`.
- **Suggested Fix**: In `sweep`'s per-doomed-entity block, add
  `self.pending_uploads.retain(|(_, e)| *e != entity);` alongside the five maps it already
  cleans — the slot is being handed back to the free list, so any queued upload for it is
  by definition stale. One line, and it makes the "eviction hygiene" comment block
  complete rather than five-sixths complete. A unit test in the existing
  `drain_pending_*` / `requeue_pending_*` family covers it with no Vulkan device.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
