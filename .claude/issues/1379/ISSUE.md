# PERF-D7-NEW-01: skinning bone-world upload + palette dispatch sized to monotonic max_used_slot high-water (never contracts)

**Severity**: LOW · **Dimension**: TAA & GPU Skinning Cost (PERF-D7-NEW-01)
**Location**: `crates/core/src/ecs/resources.rs:876-878` (max_used_slot = next_slot-1); consumed at `byroredux/src/render/skinned.rs:131` (resize) + `crates/renderer/src/vulkan/scene_buffer/upload.rs:164` (upload count) + `crates/renderer/src/vulkan/context/draw.rs:728` (palette bone_count)
**Status**: NEW (distinct from PERF-DIM7-09 MBPM per-slot zero-pad)

`sweep` returns freed slots to the free_list but never lowers `next_slot`, so `max_used_slot` is a session high-water. After a high-skinned-NPC scene unloads to a low-NPC one, the bone_world host→device copy + skin_palette dispatch keep covering the peak slot count for the renderer's lifetime (e.g. ~200×144×64B ≈ 1.8 MB/frame copy + dispatch over ~28.8K bone slots vs ~720 live). Palette shader early-returns past bone_count so GPU ALU is cheap; the PCIe copy bandwidth + dispatch granularity are paid in full. LOW on a 12 GB/PCIe4 rig; matters on bandwidth-starved configs.

**Fix**: track a contractible `high_used_slot` — after sweep, if the top of the issued range is now free, lower next_slot to the highest still-live slot + 1 (contract-only variant; never moves live slots, so the persistent bind_inverses SSBO + descriptor_bindings cache stay valid). ~30-50 LOC.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
