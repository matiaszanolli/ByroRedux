# #957 — REN-D8-NEW-13: instance_custom_index 24-bit overflow has no guard

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM8_v2.md`
**Dimension**: Acceleration Structures
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/957

## Location

`crates/renderer/src/vulkan/acceleration.rs:2047` — `vk::Packed24_8::new(ssbo_idx, 0xFF)` silently truncates above 2^24.

## Summary

`VkAccelerationStructureInstanceKHR.instanceCustomIndex` is 24 bits per spec. `ssbo_idx` is monotonic and unbounded; today the R16_UINT mesh_id ceiling caps visible instances at 32 767 (Dim 4), but that invariant lives in a different file and isn't tied to the 24-bit AS field.

## Fix (preferred)

`debug_assert!(ssbo_idx < (1 << 24), …)` at the push site, plus once-per-second `log::warn!` if instance_count is within 10% of 2^24.

## Tests

Optional unit test pinning the cap at the call site.
