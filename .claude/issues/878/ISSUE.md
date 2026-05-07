# Issue #878 (OPEN): DIM8-01: Per-frame full re-upload of material SSBO even when byte-identical to last frame

URL: https://github.com/matiaszanolli/ByroRedux/issues/878

---

## Description

`upload_materials` is called unconditionally every frame (`crates/renderer/src/vulkan/context/draw.rs:1029-1033`) and writes `count × 260 B` to the mapped staging buffer (`crates/renderer/src/vulkan/scene_buffer.rs:1017-1029`).

For static interior cells, `build_render_data` walks the same ECS queries in the same order frame-to-frame, so the resulting `materials` slice IS byte-identical between frames — the upload is pure waste in steady state.

## Evidence

```rust
// draw.rs:1029-1033 — unconditional upload, no dirty gate
if !materials.is_empty() {
    self.scene_buffers
        .upload_materials(&self.device, frame, materials)
        .unwrap_or_else(|e| log::warn!("Failed to upload materials: {e}"));
}
```

```rust
// scene_buffer.rs:1017-1030 — copy_nonoverlapping count*260 B every call
let buf = &mut self.material_buffers[frame_index];
let mapped = buf.mapped_slice_mut()?;
let byte_size = std::mem::size_of::<super::material::GpuMaterial>() * count;
unsafe {
    std::ptr::copy_nonoverlapping(
        materials.as_ptr() as *const u8,
        mapped.as_mut_ptr(),
        byte_size,
    );
}
buf.flush_if_needed(device)
```

## Why it matters

- ~8 KB/frame at 30 unique materials, ~150 KB/frame at 600 unique materials
- At 60 fps + 200 unique mats: ~3 MB/s sustained PCIe traffic for an unchanged buffer
- Below the signal floor today (mat upload dwarfed by 134 KB/frame instance upload that genuinely changes), but ratchets up if MAX_MATERIALS empirical use grows past current peaks (#779 right-sizing pending)

## Proposed Fix

64-bit content hash (xxh3 over the raw `materials` slice bytes) compared against last frame's hash; skip `copy_nonoverlapping + flush_if_needed` on a hit.

```rust
// On SceneBuffers:
last_uploaded_material_hash: [u64; MAX_FRAMES_IN_FLIGHT],
```

```rust
// In upload_materials:
let hash = xxh3_64(bytemuck::cast_slice::<GpuMaterial, u8>(materials));
if self.last_uploaded_material_hash[frame_index] == hash {
    return Ok(());
}
self.last_uploaded_material_hash[frame_index] = hash;
// ... existing copy_nonoverlapping + flush_if_needed
```

Mirrors the dirty-tracking pattern already in place at `scene_buffer.rs:1085` for terrain tiles.

**Naturally pairs with #781 (PERF-N4)** — both want the same xxh3 primitive on the same byte sequence; co-implement.

## Cost Estimate

~10 LOC. Steady-state savings ~3 MB/s PCIe. Today's signal floor; matters at scale.

## Completeness Checks

- [ ] **UNSAFE**: New `bytemuck::cast_slice` is safe (GpuMaterial is `bytemuck::Pod` per the existing dedup hash); the existing `copy_nonoverlapping` block is unchanged
- [ ] **SIBLING**: Apply the same dirty-gate to the GpuInstance buffer (#781 already proposes this); apply to terrain SSBO if not already gated
- [ ] **DROP**: N/A (no Vulkan object lifetime change)
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a regression test: render two consecutive frames with identical scene, assert `materials_uploads_skipped` increments by 1 (requires DIM8-04 telemetry — file as follow-up)

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (DIM8-01)
- Pairs naturally with: #781 (PERF-N4 — DrawCommand hash-cache, same xxh3 primitive)
- Follow-up telemetry: DIM8-04 (deferred until this lands)
- Related: #779 (PERF-N2 — MAX_MATERIALS right-sizing; DIM8-01 upper bound shifts after that)
