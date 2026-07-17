# SAVE-D2-06: ItemInstancePool has no round-trip test but currently holds no real data

**Labels**: low, ecs, bug

**Severity**: LOW (informational — test-coverage note, not a live risk)
**Dimension**: Registry & (De)serialization Fidelity
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`crates/core/src/ecs/resources/mod.rs:689-693,707-709`; registered at `byroredux/src/save_io.rs:193`

## Description
The doc comment asserts `ItemStack.instance` safety depends on `ItemInstancePool` round-tripping as a resource, but no test backs that claim. `ItemInstance` is currently a placeholder (`_reserved: ()`), so this is vacuously true today.

Verified current: `ItemInstance` (`crates/core/src/ecs/resources/mod.rs`) still has only a `_reserved: ()` field; no round-trip test for `ItemInstancePool` found in `crates/save/tests/*.rs`.

## Suggested Fix
No action required now; add a round-trip test in the same commit that gives `ItemInstance` real fields.

## Completeness Checks
- [ ] **TESTS**: A regression test to add once `ItemInstance` gains real fields (tracked here so it isn't forgotten)
