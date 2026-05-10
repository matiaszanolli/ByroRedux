# #900 — FNV-D5-NEW-01: skin_compute descriptor pool exhausts on M41-EQUIP Prospector

**Severity**: MEDIUM
**Domain**: Vulkan renderer · skinned-mesh GPU compute path
**Source audit**: `docs/audits/AUDIT_FNV_2026-05-08.md` § Dimension 5
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/900
**Status**: NEW · CONFIRMED at HEAD `318fcaf`

## Locations

- `crates/renderer/src/vulkan/skin_compute.rs:248-260` — descriptor-pool sizing comment "max_slots == 32 covers every realistic interior cell" (stale post-#896 B.2)
- `crates/renderer/src/vulkan/context/mod.rs:1102` — `const SKIN_MAX_SLOTS: u32 = 32;` constant
- `crates/renderer/src/vulkan/context/draw.rs:506-512` — retry-every-frame WARN site

## Summary

Prospector at HEAD spawns enough skinned meshes (NPC body + per-slot armor pieces from #896 Phase B.2) that two entities (1088, 1095) overflow the 32-slot descriptor pool. `vkAllocateDescriptorSets` returns `OUT_OF_POOL_MEMORY`. The retry path at `draw.rs:506-513` has no per-entity sticky-failure marker, so each frame re-fires `create_slot` on the failing entities — 58 WARN lines / 300 frames in the captured bench.

RT shadows silently disabled on the overflow entities (raster path correct). Same call site as #871 (which fixed the buffer leak) but cap-and-retry semantics untouched there.

## Fix path

Pick one or both:

1. **Capacity bump.** `SKIN_MAX_SLOTS` 32 → 64 (or scale at cell-load from actor count). Each slot = 3 storage-buffer descriptors × `MAX_FRAMES_IN_FLIGHT`. Validate against `physical_device_properties.limits.max_descriptor_set_storage_buffers`.
2. **Retry suppression.** `failed_skin_slots: HashSet<EntityId>` cache; clear on cell unload via the existing `unload_cell` victim walk. WARN fires once per cell-load instead of per frame.

## Related

- #871 — same call site, distinct defect (`output_buffer` leak)
- #896 — M41-EQUIP parent feature track
- #902 / FNV-D5-NEW-03 — bench-staleness rationale that masked this
