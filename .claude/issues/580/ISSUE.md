---
issue: 580
title: "SAFE-21: acceleration.rs:694 lifetime transmute comment is imprecise"
labels: low, safety, renderer, documentation
state: OPEN
audit: docs/audits/AUDIT_SAFETY_2026-04-23.md § SAFE-21
---

## Summary

SAFETY comment at `crates/renderer/src/vulkan/acceleration.rs:694` mixes two reasonings — one claiming `triangles_data` must stay alive, one claiming no borrow exists. Only the second is accurate. The real lifetime lie is on the struct field at `:610`:

```rust
geometry: vk::AccelerationStructureGeometryKHR<'static>,
```

The `<'static>` is sound *because* the union fields are value-typed (`u64` device addresses), not because of `triangles_data`'s scope.

## Fix
Rewrite the SAFETY comment to name the real invariant: `<'a>` is a phantom lifetime from the ash builder; `DeviceOrHostAddressConstKHR::device_address` is `u64`, so no borrow is held; `'static` becomes UB the moment a host-pointer variant is added.

## Completeness
- [ ] New SAFETY comment names the value-typed union invariant
- [ ] Other `vk::*<'static>` fields in the file audited for the same pattern
