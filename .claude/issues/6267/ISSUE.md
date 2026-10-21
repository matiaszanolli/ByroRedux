# #6267: SCR-2026-10-21-10: The new request markers are not in `cleanup.rs`'s drain table

**Labels**: low,scripting,documentation,doc-rot,test-gap

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: LOW · **Dimension**: Event Runtime · **Location**: `crates/scripting/src/cleanup.rs` module-doc table; markers `FleeRequest`, `PickIdleRequest`, `PushActorRequest`, `ResetInventoryRequest`, `ReviveRequest`, `UseWeaponRequest`, `OnResetEvent` (consumers: `systems/{flee,cinematic,knockdown,script_requests,revive}.rs`, `obscript_objects.rs:264-275`)

**Audit status**: NEW

## Description

All seven drain unconditionally at their consumers' heads; I checked each. Because they are undocumented, `every_documented_pattern_b_marker_drains_in_its_named_consumer` does not pin them, and a later early return placed before a drain would go unnoticed.

## Suggested Fix

Add the seven rows, each naming its consumer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other Pattern-B request marker added since the table was last synced)
- [ ] **TESTS**: A regression test pins this specific fix
