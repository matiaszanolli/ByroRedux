# #1193 — SAFE-D7-NEW-03: `record_pending_bind_inverse_copies` has no slot-bounds debug_assert

**Severity**: LOW
**Dimension**: D2 — Vulkan spec compliance (VUID-vkCmdCopyBuffer-dstOffset-00114)
**Source audit**: `docs/audits/AUDIT_SAFETY_2026-05-19.md`
**Introduced**: `5be66790` (M29.6, this session)

## One-line

Pool capacity drift past `(MAX_TOTAL_BONES / MBPM) - 1` would write past the persistent SSBO end via `cmd_copy_buffer`. Contract is convention-only today.

## Site

`crates/renderer/src/vulkan/scene_buffer/upload.rs:260-266`

## Fix recipe

```rust
for (i, &slot_id) in pending_slots.iter().take(capped).enumerate() {
    debug_assert!(
        ((slot_id as usize + 1) * MAX_BONES_PER_MESH) <= MAX_TOTAL_BONES,
        "M29.6 contract: slot_id {slot_id} would write past bind_inverses_persistent end \
         ({MAX_TOTAL_BONES} bones). SkinSlotPool capacity must be \
         ≤ (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1"
    );
    copies.push(...);
}
```

Also update the `SkinSlotPool::new` docstring (`resources.rs:580-584`) to call out the renderer-side `(MAX_TOTAL_BONES / MBPM) - 1` ceiling.

## Test recipe

`#[should_panic]` test on a CPU mock would require a Vulkan mock; cleanest is to just rely on the debug_assert firing during a future hypothetical regression. Optional.

## Next step

```
/fix-issue 1193
```
