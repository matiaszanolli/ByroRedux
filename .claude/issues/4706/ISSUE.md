# GAME-D2-2026-09-21-05: Pickup always grants one item — the REFR item count (XCNT) is not decoded

**Issue**: #4706
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 2 — Pickup
**Location**: `byroredux/src/inventory.rs` (`ItemStack::new(base, 1)`); no `XCNT` arm anywhere in `crates/plugin/src`

## Description
xEdit defines REFR `XCNT` as a placed-stack count. The parser drops it entirely; `pickup_loot` hard-codes 1.

## Evidence
Census: FNV 33, FO3 186, Skyrim 118, Oblivion 29, FO4 97 placed stacks with non-1 XCNT (values 2-50).

## Impact
A placed stack of 20 arrows yields 1 on pickup. Latent while pickup has no input path (#4697).

## Related
GAME-D2-2026-09-21-02 (#4697); `/audit-esm` owns the decode half.

## Suggested Fix
Decode `XCNT` into `PlacedRef.item_count`; use `count.max(1)` in `pickup_loot`.
