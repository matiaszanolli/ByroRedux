# CONC-D6-2026-08-30-01: `skin_slots` teardown nested under `skin_compute.is_some()` — the exact shape #3374 un-nested for `morph_slots`

**Issue**: #3657
**Labels**: bug, renderer, medium, memory, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D6-2026-08-30-01 (MEDIUM, D6 · Resource Lifecycle / GPU teardown ordering).

**Location**: `crates/renderer/src/vulkan/context/teardown.rs:46-51`.

**The exact shape #3374 un-nested for `morph_slots` — the skin half still carries the coupling.**

## Description

The per-skinned-entity `SkinSlot` drain in `Drop` runs **only when `skin_compute` is `Some`**, because `destroy_slot` is a method on the pipeline (it must `free_descriptor_sets` back into the pipeline's `FREE_DESCRIPTOR_SET` pool).

`MorphSlot`'s sibling drain immediately below was **deliberately taken out** of the analogous `(skin_compute, accel_manager)` guard by #3374 for exactly this reason — see the long comment at `skinned_blas_refit.rs:774-797` (*"That is the #2494 mistake one nesting level out"*).

The consequence if the gate ever goes false is **worse than a plain leak**: each `SkinSlot::output_buffer` is a `GpuBuffer` holding its own `Arc<Mutex<Allocator>>` clone (`vulkan/buffer.rs:633`). Skipping `destroy_slot` means those clones are released only by the *natural* Drop pass that runs **after** `VulkanContext::drop` returns — i.e. after the `Arc::try_unwrap` at `teardown.rs:346-384` has already given up — which takes the #665 leak-guard branch that **intentionally leaks the device, surface, instance and debug messenger**.

## Evidence

```rust
// crates/renderer/src/vulkan/context/teardown.rs
46          if let Some(ref skin) = self.skin_compute {
47              let slots = std::mem::take(&mut self.skin_slots);
48              for (_eid, slot) in slots {
49                  skin.destroy_slot(&self.device, alloc, slot);
50              }
51          }
52          // #3231 — MorphSlot owns plain buffers with no descriptor sets
53          // or pipeline dependency (unlike SkinSlot above), so it can be
54          // torn down unconditionally.
55          for (_eid, mut slot) in std::mem::take(&mut self.morph_slots) {
56              slot.destroy(&self.device, alloc);
57          }
```

## Trigger Conditions

Shutdown (`VulkanContext::drop`) in any future configuration where `self.skin_compute` is `None` while `self.skin_slots` is non-empty.

**Not reachable at HEAD** — `skin_compute` is assigned exactly once (`context/init.rs:600` / `:647`, via `couple_skin_compute_to_palette`) and never re-assigned, and every `skin_slots.insert` site (`context/skinned_blas_refit.rs:315`) sits inside a `skin_compute` guard. **This is a defence-in-depth gap, not a live leak.**

## Verification Path

`cargo test` cannot reach it (needs a live device). A **source-shape regression test** in the style of `skinned_blas_refit.rs`'s existing #3374 pin is the practical guard.

## Impact

None today. Under a future edit that nulls `skin_compute` at runtime (a device-lost recovery path, an RT-optional configuration, a pipeline-rebuild-on-resize), shutdown silently leaks every live skinned output buffer **and** trips the allocator-outstanding-reference guard, leaking the `VkDevice`/`VkInstance` too.

## Related

#3374 (the `morph_slots` half), #2494, #665 / LIFE-L1, #927.

## Suggested Fix

Make the drain **unconditional** and let `destroy_slot` become a free function (or an inherent `SkinSlot::destroy(device, allocator)` plus an `Option`-guarded `free_descriptor_sets` when the pipeline exists) — the descriptor sets are freed implicitly by pool destruction anyway, so **the buffer half never needs the pipeline**. Add the same source-shape pin #3374 added.

## Completeness Checks
- [ ] **DROP**: The reverse-order teardown contract still holds after un-nesting; `skin_slots` must still drain before `skin_compute`'s pipeline/pool destroy (that local ordering *is* load-bearing — `VUID-vkFreeDescriptorSets-descriptorPool-parameter`)
- [ ] **UNSAFE**: If the split introduces a new `unsafe` destroy path, the safety comment states which handle is still alive
- [ ] **SIBLING**: Every other `Drop` drain in `teardown.rs` nested under an `Option`-gated pipeline audited for the same coupling
- [ ] **TESTS**: A source-shape pin, mirroring #3374's, that fails if the drain is re-nested
