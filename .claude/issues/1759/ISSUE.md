# TD7-002: NON_COHERENT_ATOM_SIZE=256 hardcodes a device limit instead of querying PhysicalDeviceLimits

_Filed 2026-06-26 as #1759 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1759` for live state)._

**Severity**: LOW · **Dimension**: 7 — Magic Numbers & Hardcoded Constants
**Location**: `crates/renderer/src/vulkan/buffer.rs:389`
**Status**: NEW (prior audit REN-D2-NEW-03 logged INFO, never ticketed) · **Audit**: TD7-002

## Description
`aligned_flush_range` (used for `vkFlushMappedMemoryRanges` on non-coherent host-visible buffers) rounds offset/size to a hardcoded `NON_COHERENT_ATOM_SIZE = 256` instead of the device-reported `VkPhysicalDeviceLimits::nonCoherentAtomSize`.

## Why LOW (not HIGH)
256 is the spec's largest realistic atom size; every real device reports `nonCoherentAtomSize <= 256` (typically 64). Rounding to 256 is always a valid *superset* of the device requirement → it can only over-align (wastes O(192 B) per per-frame mapped flush, a few KB/frame), never under-align → no spec violation, no stale-data hazard. The doc comment at buffer.rs:375-388 already explains this is the deliberate conservative fallback.

## Suggested Fix
When `PhysicalDeviceLimits` is plumbed onto `VulkanContext`, replace the const with the queried value and pass it into `aligned_flush_range`. Until then, add a debug assertion at device-create time that `limits.non_coherent_atom_size <= 256` so a future exotic device fails loudly rather than under-aligning. (Or explicitly WONTFIX with the debug-assert as the guard.)

## Completeness Checks
- [ ] **TESTS**: a debug-assert pins the `<= 256` assumption at device create
