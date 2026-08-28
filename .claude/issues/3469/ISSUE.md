# #3469 — PERF-2026-08-27b-03: vkGetBufferDeviceAddress is called per skinned draw per frame for an address that cannot change

**Labels**: medium, performance, renderer, vulkan, bug
**Filed**: 2026-08-27 from `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`
**HEAD at audit**: `969d81c8`

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` — finding `PERF-2026-08-27b-03`
**Severity**: MEDIUM · **Dimension**: Skinning & BLAS Cost
**Location**: `crates/renderer/src/vulkan/context/draw.rs:3007-3028` (inside the `for draw_cmd in draw_commands` loop opened at `:2833`); sibling sites `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:536-570`

## Description

The per-instance `GpuInstance` build loop resolves each skinned draw's deformed-vertex buffer address by calling into the driver — `vkGetBufferDeviceAddress` — every frame, for every skinned draw. A `VkBuffer`'s device address is fixed for the buffer's lifetime once bound (Vulkan spec), and `SkinSlot`'s own documentation relies on exactly that invariant elsewhere. The address is therefore computable once, at slot creation, and stored — which is precisely what the sibling `MorphSlot`, added in the same subsystem days later, already does.

## Evidence

In the hot loop (`crates/renderer/src/vulkan/context/draw.rs:3007-3028`):

```rust
let slot_address = (draw_cmd.bone_offset != 0)
    .then(|| self.skin_slots.get(&draw_cmd.entity_id))
    .flatten()
    .filter(|slot| skin_slot_backs_mesh(slot.vertex_count(), mesh.vertex_count))
    .map(|slot| {
        unsafe {
            self.device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default()
                    .buffer(slot.output_buffer.buffer),
            )
        }
    });
```

`SkinSlot` (`crates/renderer/src/vulkan/skin_compute.rs:83-148`) stores `output_buffer`, `output_size`, `descriptor_sets`, `vertex_count`, `last_used_frame`, `has_populated_output` and `descriptor_bindings` — **no address field** (`grep -n output_address crates/renderer/src/vulkan/skin_compute.rs` returns nothing) — while its own `descriptor_bindings` doc states the justifying invariant in as many words: *"`output_buffer` isn't tracked — it's a function of the slot itself, so once any FIF has been written it stays correct (the buffer doesn't move)."*

The immediately following block, `#3231`'s morph lookup (`draw.rs:3033-3054`), reads `slot.delta_address()` / `slot.weight_address()` — plain field reads of `vk::DeviceAddress` members cached at construction (`crates/renderer/src/vulkan/morph_compute.rs:41,50,82-88`, populated once at `:148,154`). The two adjacent code paths do the same job two different ways.

The same pattern repeats in the refit path: `refit_skinned_blas` re-queries the vertex, index and scratch addresses on every refit (`crates/renderer/src/vulkan/acceleration/blas_skinned.rs:538,544,567`), gated by `pose_dirty` so it costs one call per *moving* actor per frame rather than per draw.

## Impact

Derived, not measured. `vkGetBufferDeviceAddress` is a driver entry point reached through ash's loaded function table — tens of nanoseconds each, not free, and it sits in the innermost O(visible-instance) loop of `draw_frame`. `skyrim_se-WhiterunDragonsreach` reports `skin_pool_live = 83` (`.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`); a Skyrim NPC contributes several draws (head, body, hands, feet, hair, worn ARMA meshes — and `#3357` has since increased that per-NPC mesh count), so the order is several hundred driver calls per frame in that cell, scaling linearly with crowd size. On a machine where a CPU bottleneck is a bug by definition, this is pure avoidable per-frame driver traffic on the exact path `#1195`/`#1196`/`#1197` were written to keep lean.

## Related

`#2219` (introduced the draw-loop lookup), `#3231` (the `MorphSlot` sibling that caches), `#2402` (the `skin_slot_backs_mesh` filter that must stay in front of any cached read), `#1797` (the shared-scratch serialization ceiling on the same refit path — quantify with `skin.coverage` before touching that one).

## Suggested Fix

Add `output_address: vk::DeviceAddress` to `SkinSlot`, populated once in `SkinComputePipeline::create_slot` next to the existing buffer creation, with a `pub fn output_address(&self)` accessor mirroring `MorphSlot::delta_address`. Keep the `skin_slot_backs_mesh` filter exactly where it is — the cached address must still be suppressed for a slot that no longer backs the mesh. The refit path's three queries can move to the same cached field plus one address on the shared scratch buffer.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (`blas_skinned.rs`'s three refit-path queries, and any other `get_buffer_device_address` call reachable per frame)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
