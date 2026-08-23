# 3239: SAFE-D4: two unsafe blocks carry safety reasoning but skip the SAFETY: label convention

**Severity**: LOW · **Dimension**: Safety Dimension 4 (Unsafe-Block Discipline) · **Report**: `docs/audits/AUDIT_SAFETY_2026-08-23.md` (SAFE-D4-2026-08-23-01)

## Description

Two `unsafe` blocks carry a prose comment stating the actual safety reasoning, but neither uses this codebase's near-universal `SAFETY:` label convention (unlike every neighbouring block in the same file doing the identical pattern):

- `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:249` (`collect_rt_lod_telemetry`) — comment: "The per-FIF byte stride is Vulkan-aligned, but the mapped slice's Rust pointer has no typed-alignment guarantee." Same justification as its sibling `collect_selected_ray_probe` seven lines below, which *does* say `SAFETY: the length check above covers the complete repr(C), Copy record...`.
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs:917` — comment: "Same raw-handle ownership window as the DALC seed above." Correct and sufficient by reference, but again doesn't restate `SAFETY:`.

## Evidence

```rust
// crates/renderer/src/vulkan/scene_buffer/descriptors.rs:247-249
// The per-FIF byte stride is Vulkan-aligned, but the mapped slice's
// Rust pointer has no typed-alignment guarantee.
let budget = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<GpuRayBudget>()) };
```
```rust
// crates/renderer/src/vulkan/scene_buffer/buffers.rs:916-917
// Same raw-handle ownership window as the DALC seed above.
unsafe {
    device.destroy_descriptor_pool(descriptor_pool, None);
    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
}
```

## Impact

None on soundness — both invariants hold at their call sites (`read_unaligned` correctly sidesteps the alignment gap the comment names; the descriptor pool/layout are freshly created and unreferenced by any command buffer at the point of the early return). Purely a labeling-consistency gap that could make a future reader skim past the reasoning during a `grep SAFETY` sweep.

## Suggested Fix

Prefix both comments with `SAFETY:` for grep-ability; no logic change needed.

## Completeness Checks
- [ ] **UNSAFE**: Confirm no other block in the same two files has the same gap
