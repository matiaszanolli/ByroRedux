# Issue #1640 — REG-08: App-Drop field order (#1477) has no test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-08 (PARTIAL hardening gap, LOW)

The fix for **#1477** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
`AllocatorResource` is removed before `renderer.take()` / `VulkanContext` drop on every teardown, including panic-unwind — closing the #1406 allocator-teardown hazard re-arm. Structural; the original issue listed TESTS as a wishlist item.

## Evidence
- `byroredux/src/main.rs` — `impl Drop for App`: allocator removed ahead of `renderer.take()` / `VulkanContext` drop.

## Impact
A revert re-arms the allocator-outlives-`VulkanContext` hazard on panic-unwind (allocator freed after the objects it owns).

## Suggested Fix
Drop-order tests are brittle; acceptable as structural. Leave/confirm the comment in `impl Drop for App` naming the "allocator before context" ordering invariant.

## Completeness Checks
- [ ] **DROP**: `App` field/teardown order keeps allocator alive until after `VulkanContext` drop, on normal and panic-unwind paths
- [ ] **TESTS**: Ordering rationale documented (Drop-order tests brittle)
