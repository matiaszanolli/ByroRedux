# #1188 — REN-D1-NEW-05: `recreate_in_flight_for_frame` leaves stale fence handles in `images_in_flight`

**Severity**: MEDIUM
**Dimension**: Vulkan Sync
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-05-19.md`
**Created**: 2026-05-19

## Locations

- `crates/renderer/src/vulkan/sync.rs:263-280` — the partial-recreation helper that misses the `images_in_flight` cross-ref wipe
- `crates/renderer/src/vulkan/context/draw.rs:231` — site that writes the old fence handle into `images_in_flight[img]` pre-submit
- `crates/renderer/src/vulkan/context/draw.rs:2486-2489` — submit-failure error arm that calls `recreate_in_flight_for_frame`
- `crates/renderer/src/vulkan/sync.rs:177-198` — `recreate_for_swapchain`, the reference pattern that does the right thing (line 182 zeroes the whole table)

## One-line summary

After today's #952 fix, a `queue_submit` failure recreates `in_flight[frame]` as a fresh fence but leaves `images_in_flight[img]` pointing at the destroyed old fence — next acquire on the same image index calls `wait_for_fences` on a dangling `VkFence`.

## Trigger

`queue_submit` fails (device-lost, host OOM, validation-driven abort) AND the next acquire on the same frame slot returns the same swapchain image index.

## Fix sketch

In `recreate_in_flight_for_frame`, between `mem::replace` and `destroy_fence`:

```rust
let old = std::mem::replace(&mut self.in_flight[frame], new_fence);
for slot in &mut self.images_in_flight {
    if *slot == old {
        *slot = vk::Fence::null();
    }
}
device.destroy_fence(old, None);
```

Add a unit test seeding a sentinel handle into both `in_flight[frame]` and `images_in_flight[k]`, calling the helper, asserting the slot was nulled.

## Related

- #952 (parent fix, closed today)
- #908 (resize-path sibling — correct pattern)
- #910 (`recreate_image_available_for_frame` sibling helper — semaphores have no cross-ref table, no hazard)

## Completeness Checks (copied to GH issue)

- [ ] **UNSAFE**: Helper is already `unsafe fn`; new loop is safe Rust. Verify safety docstring covers the new loop.
- [ ] **SIBLING**: `recreate_image_available_for_frame` has no cross-ref table (no hazard). `recreate_for_swapchain` already does the wipe. No additional siblings.
- [ ] **DROP**: No type changes; `Drop` unaffected.
- [ ] **LOCK_ORDER**: No `RwLock`.
- [ ] **FFI**: No cxx-bridge.
- [ ] **TESTS**: Unit test for the new invalidation. Can use `vk::Fence::from_raw(sentinel)` since the loop is pure pointer-comparison.

## Next step

```
/fix-issue 1188
```
