# Issue #643: MEM-2-1: M29 SkinSlots and skinned_blas leak across cell transitions

**File**: `crates/renderer/src/vulkan/context/mod.rs:1306-1311` + `crates/renderer/src/vulkan/context/draw.rs:460-478`
**Dimension**: GPU Memory

`skin_slots: HashMap<EntityId, SkinSlot>` is only ever inserted (draw.rs:469); only ever drained inside Drop (mod.rs:1307). Same for `AccelerationManager::skinned_blas`: `drop_skinned_blas` exists (acceleration.rs:898) and is wired into the Drop chain, but there is no runtime call site.

On cell transitions / actor despawns, the per-entity skinned-vertex output buffer (`vertex_count × 84 B` + a refit-allowed BLAS at ~2× vertex bytes) and 2 descriptor sets per slot stay resident. A long session that streams through several worldspaces will retain SkinSlots for every NPC ever rendered, eventually exhausting the FREE_DESCRIPTOR_SET pool (`max_slots × MAX_FRAMES_IN_FLIGHT = 64` sets at default — far below the cumulative population of FNV exteriors).

**Fix**: Wire a per-frame check (or dedicated despawn hook) that drops both the SkinSlot and the matching `skinned_blas` entry when the entity is no longer in `draw_commands` for N consecutive frames. Mirror `evict_unused_blas`'s LRU pattern with a deferred-destroy queue so the buffers outlive any in-flight command buffer.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
