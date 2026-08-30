# CONC-D6-2026-08-30-02: `destroy_allocator_owned_resources` documents "reverse-creation order"; the block is neither reverse nor forward

**Issue**: #3658
**Labels**: documentation, renderer, low, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D6-2026-08-30-02 (LOW, D6 · Resource Lifecycle / GPU teardown ordering). Doc rot on a comment that **invites a specific harmful edit**.

**Location**: `crates/renderer/src/vulkan/context/teardown.rs:11-20` (doc), `:27-167` (body), `:172-174` (the same claim in `Drop`'s SAFETY comment).

## Description

`destroy_allocator_owned_resources` is documented as destroying subsystems "in reverse-creation order". The actual destroy sequence is neither reverse nor forward creation order — it **starts with the first-created subsystem**.

| destroy # | subsystem | `init.rs` creation line | creation rank |
|---|---|---|---|
| 1 | `texture_registry` | 349 | 1st |
| 2 | `scene_buffers` | 403 | 2nd |
| 6 | `accel_manager` | 429 | 3rd |
| 7 | `cluster_cull` | 562 | 4th |
| 8 | `skin_compute` | 602 | 5th |
| 9 | `ssao` | 864 | 12th |
| 10/11 | `placeholder_ao` / `placeholder_caustic_sink` | 746 / 761 | 9th / 10th |
| 12 | `exposure` | 910 | 13th |
| 13 | `frame_upscaler` (allocations) | 1341 | 23rd |
| 14 | `composite` | 1251 | 21st |
| 15 | `caustic` | 1114 | 19th |
| 16 | `volumetrics` | 937 | 14th |
| 17 | `bloom` | 1203 | 20th |
| 18 | `water_caustic_accum` | 777 | 11th |
| 19 | `svgf` | 1029 | 17th |
| 20 | `reservoir_buffers` | 1078 | 18th |
| 21 | `taa` | 1288 | 22nd |
| 22 | `gbuffer` | 977 | 16th |

True reverse-creation order would start with `presentation` / `frame_upscaler` and end with `texture_registry`.

## The order is nonetheless CORRECT — that is what makes the doc dangerous

After the `device_wait_idle` at `teardown.rs:176`, Vulkan imposes no cross-subsystem destroy ordering (a `VkDescriptorSet` may name a destroyed `VkImageView` as long as it is never used again, and every parent/child pair — framebuffer->render pass, view->image, image->memory, sets->pool — is contained inside a single subsystem's own `destroy`).

The four orderings that **are** load-bearing are all local and separately commented:
1. `skin_slots` before `skin_compute` (`:38-45`)
2. placeholders after the passes whose descriptors name them (`:98-107`)
3. `frame_upscaler::destroy_allocations` after `destroy_device_objects` (`:124-130`)
4. `exposure` before the `Arc::try_unwrap` (`:114-123`)

## Evidence

```rust
11      /// Destroy every subsystem whose resources are owned by the GPU
12      /// allocator, in reverse-creation order.
...
27      unsafe fn destroy_allocator_owned_resources(&mut self, alloc: &SharedAllocator) {
28          self.texture_registry.destroy(&self.device, alloc);   // created FIRST (init.rs:349)
29          self.scene_buffers.destroy(&self.device, alloc);      // created SECOND (init.rs:403)
```

## Impact

A maintainer who reads "reverse-creation order" as a **live invariant** and "restores" it would reshuffle the four local orderings above — in particular moving `skin_compute`'s pipeline/pool destroy ahead of the per-slot `free_descriptor_sets`, which is a real **`VUID-vkFreeDescriptorSets-descriptorPool-parameter`** violation.

## Folded-in LOW note (same family, no separate issue)

The #1483 block comment at `teardown.rs:189-208` and the #2158 comment at `:229-238` both justify their hoists by an *"allocator-`None` Drop path (#1426 early-return, or any future allocator-taken-early path)"*. **At HEAD that path does not exist**: grep over `crates/renderer/src` + `byroredux/src` finds no assignment of `VulkanContext::allocator` to `None` other than the final `self.allocator.take()` at `teardown.rs:346`, and `init.rs:1459` always constructs `allocator: Some(gpu_allocator)`. The hoists are correct and worth keeping as defence-in-depth, but the comments assert a live hazard that isn't.

## Related

#1749 / TD1-004 (the move that introduced the doc), #2406 / TD1-003, #1483, #2158, #1426.

## Suggested Fix

Replace "in reverse-creation order" with an **enumeration of the four real constraints**, and note that the remaining order is free once the device is idle. Optionally pin the four with an `include_str!` source-order test alongside the existing ones in `resize.rs`. Also soften the two "allocator-`None` path" comments to say the path is hypothetical, not live.

## Completeness Checks
- [ ] **DROP**: The four load-bearing local orderings are stated explicitly and unchanged — the point of this fix is to make them the documented contract instead of a false global claim
- [ ] **SIBLING**: `Drop`'s own SAFETY comment at `:172-174` repeats the same claim and must be corrected in the same edit
- [ ] **TESTS**: A source-order pin for the four constraints, in the style of `resize.rs`'s `old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`
