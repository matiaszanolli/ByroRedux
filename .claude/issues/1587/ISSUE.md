**Severity**: LOW (no-op on the dev RTX 4070 Ti; real only on non-coherent host-visible memory) · **Dimension**: SSBO Sizing & Upload · **Status**: NEW
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F8)

## Description
The direct-copy upload paths write only the live prefix (`byte_size = sizeof(T) * count`) but `flush_if_needed` (`crates/renderer/src/vulkan/buffer.rs:629-652`) flushes `aligned_flush_range(alloc.offset(), alloc.size())` — the entire allocation (29.4 MB for instances) — regardless of bytes written. Callers: `scene_buffer/upload.rs` `upload_lights:74`, `upload_instances:514`, `upload_materials:572`, `upload_indirect_draws:619`, `upload_bone_worlds:201`. The sibling `write_mapped` (`buffer.rs:683-711`) was explicitly fixed under #301 to flush only `len`, and its comment names this exact waste.

## Evidence
Verified live (`buffer.rs:635-649`): the non-coherent branch flushes `aligned_flush_range(alloc.offset(), alloc.size())` — full allocation, independent of bytes actually written.

## Impact
ZERO on the dev GPU (NVIDIA `CpuToGpu` is HOST_COHERENT → early-return). On AMD/Intel/mobile non-coherent memory, `upload_lights`/`upload_indirect_draws`/`upload_bone_worlds` (no dirty gate) flush the full allocation every frame they run; instances/materials pay it on every gate miss.

## Suggested Fix
Have the `upload_*` callers flush via the already-existing `flush_range(device, 0, byte_size)` (`buffer.rs:722`) instead of `flush_if_needed`. No layout/shader change; pure flush-range narrowing.

## Completeness Checks
- [ ] **SIBLING**: Apply the narrowed flush to all five `upload_*` callers, not just one
- [ ] **TESTS**: A test (or debug assert) pinning that the flushed range == written `byte_size`, not allocation size
