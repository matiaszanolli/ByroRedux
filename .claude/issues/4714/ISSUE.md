# GAME-D7-2026-09-21-05: armor_covers_main_body / main_body_bit are test-only but documented as used by the spawn pipeline

**Issue**: #4714
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 7 — tested-but-unwired
**Location**: `crates/plugin/src/equip.rs`

## Description
`armor_covers_main_body`'s doc claims spawn-pipeline use; no non-test caller exists. Production instead reimplements the check independently via `main_body_covered` (`npc_spawn.rs`), which calls the sibling `main_body_bit` directly against `EquipmentSlots`, not `armor_covers_main_body`.

## Evidence
`grep -rn "armor_covers_main_body\|main_body_bit"`: `armor_covers_main_body` appears only in its own tests.

## Impact
Stale API/doc; #4074's provisional FO76/Starfield bit table feeds an unwired function.

## Related
#4074.

## Suggested Fix
Delete `armor_covers_main_body` + its tests, or correct the doc to name `main_body_covered` as the real consumer.
