# #2402 — CHAIN2-D2-03: `skinnedVertexAddress` can be emitted from a stale `SkinSlot` when a skinned entity's mesh becomes non-RT-capable

- **Severity**: LOW
- **Domain**: vulkan, safety
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2402


- **Severity**: LOW
- **Dimension**: 2 — Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2420-2436`; `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:109-142,196-236,608-649`
- **Status**: NEW

**Description**

`record_skinned_blas_refit`'s `dispatches` collection skips any skinned draw whose mesh is `!mesh.rt_capable` (`skinned_blas_refit.rs:124-126`); the per-entity capacity-stale reconciliation (`:213-236`) and LRU-liveness bump both live inside that loop. But `GpuInstance`'s builder (`draw.rs:2420-2434`) populates `skinned_vertex_address` from `self.skin_slots.get(&entity_id)` unconditionally for any `bone_offset != 0` draw, with no `rt_capable`/capacity cross-check. For up to `MAX_FRAMES_IN_FLIGHT + 1` frames (until the LRU sweep reaps the orphaned slot), a skinned draw remapped from an RT-capable mesh to a non-RT-capable one with a higher vertex count publishes a raw device address sized for the previous mesh, while `ray_hit.glsl` indexes it with the new mesh's index buffer (no descriptor range check on a `buffer_reference` load).

**Evidence** (re-confirmed at publish time against commit `79bfc76e`): `draw.rs:2420` looks up `self.skin_slots.get(&entity_id)` with no gate:

```rust
let slot_address = (draw_cmd.bone_offset != 0)
    .then(|| self.skin_slots.get(&draw_cmd.entity_id))
    .flatten()
    .map(|slot| unsafe {
        self.device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default().buffer(slot.output_buffer.buffer),
        )
    });
```

`skinned_blas_refit.rs:124`'s `if !mesh.rt_capable { continue; }` guard never runs for such a draw, so the slot is never invalidated in time.

**Impact**

If the new mesh has more vertices than the slot's allocation, the fragment shader reads past the end of a device allocation via a raw address — best case garbage normals, worst case a GPU page fault/device loss. Window is bounded (3 frames) and requires a specific remap direction.

**Trigger Conditions**: A skinned entity's `MeshHandle` is remapped (M41 equip/outfit swap/cell reload) from an RT-capable mesh to a non-RT-capable one (skinned effect-shader proxy or decal) with more vertices, drawn within the next `MAX_FRAMES_IN_FLIGHT + 1` frames.

**Verification Path**: `BYRO_VALIDATION=1` will not catch this (raw buffer-reference loads are outside descriptor validation). Instrument instead: assert in `draw.rs:2420` that the slot's `vertex_count()` is `>= mesh.vertex_count` for the draw's mesh, and run `docs/smoke-tests/m41-equip.sh` — a `debug_assert` trip confirms reachability. RenderDoc would show the buffer-size mismatch on the instance.

**Related**: `#1297`/`#1298` (the capacity-stale guard this path bypasses); `#2219`; CHAIN2-D2-01 (same consumer).

**Suggested Fix**: Gate `slot_address` on the slot's `vertex_count()` matching the draw's live `mesh.vertex_count` (already in hand at that site), falling back to `0`/bind-pose on mismatch — a pure CPU-side guard with no barrier implications.

## Completeness Checks
- [ ] **SIBLING**: Check other `buffer_reference` device-address emission sites for the same missing-capacity-gate shape
- [ ] **TESTS**: Add the `debug_assert` at `draw.rs:2420` and run `docs/smoke-tests/m41-equip.sh` to confirm reachability before landing the CPU-side gate; a `cargo test` regression test can pin the gate logic in isolation

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
