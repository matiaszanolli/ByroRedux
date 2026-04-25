# SAFE-21: acceleration.rs:694 lifetime transmute comment is imprecise

State: OPEN

**Severity**: LOW | **Dimension**: Unsafe Blocks | **Audit**: docs/audits/AUDIT_SAFETY_2026-04-23.md § SAFE-21

## Summary

The batched BLAS builder at `crates/renderer/src/vulkan/acceleration.rs:694-696` has a SAFETY comment referencing a \"transmute to 'static\" but the actual lifetime lie happens on the struct field at `:610` (`geometry: vk::AccelerationStructureGeometryKHR<'static>`). The comment's reasoning is compressed and partially misleading.

## Evidence

\`\`\`rust
// crates/renderer/src/vulkan/acceleration.rs:610
struct PreparedBlas {
    // ...
    geometry: vk::AccelerationStructureGeometryKHR<'static>,   // ← the lifetime lie
    // ...
}

// :694
// SAFETY: We transmute the lifetime to 'static because the triangles_data
// vec lives for the duration of this function. The geometry struct just
// holds a copy of the union data, not a reference.
let geometry = vk::AccelerationStructureGeometryKHR::default()
    .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
    .flags(vk::GeometryFlagsKHR::OPAQUE)
    .geometry(vk::AccelerationStructureGeometryDataKHR {
        triangles: triangles_data[idx],
    });
\`\`\`

The comment mixes two ideas:
1. \"triangles_data vec lives for the duration of this function\" — implies a borrow dependency
2. \"just holds a copy of the union data, not a reference\" — implies no borrow

Only #2 is accurate. The `AccelerationStructureGeometryTrianglesDataKHR` struct holds `u64` device addresses by value, so the `'static` annotation is sound regardless of `triangles_data`'s scope. The ash builder's phantom lifetime is vestigial here.

## Impact

No live bug — the lifetime lie is safe because all fields are value-typed. The risk is future edits: if someone adds a Rust reference to the geometry union (e.g. host pointers for CPU builds), the `'static` claim becomes real UB with no compiler warning, and the current comment won't flag that.

## Suggested fix

Rewrite the SAFETY comment to state the actual invariant:

> `vk::AccelerationStructureGeometryKHR<'a>` carries a phantom lifetime from the ash builder API. All union fields used here (`DeviceOrHostAddressConstKHR::device_address`, `u64`) are value-typed, so no real borrow is held. The `'static` annotation is sound as long as every `.geometry()`-reachable field remains value-typed — adding a host-pointer variant would make this UB.

Consider whether `PreparedBlas::geometry` could instead build the geometry lazily at submit time, eliminating the lifetime question entirely.

## Completeness Checks

- [ ] **UNSAFE**: New SAFETY comment names the real invariant (all union fields value-typed)
- [ ] **SIBLING**: Audit other `vk::*<'static>` annotations in `acceleration.rs` for the same pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — comment change
