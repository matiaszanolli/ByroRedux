# 3275: SPT-D3-2026-08-24-01: make_billboard_system's Access declaration still reads MeshHandle, a lock the system no longer takes

**Severity**: LOW · **Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-24.md` (SPT-D3-2026-08-24-01)

## Description

`4e1afcbe` deleted the geometry-tree loop that queried `world.query::<MeshHandle>()` and removed `MeshHandle` from `billboard.rs`'s `use` list entirely. The scheduler's declared `Access` for this system was not updated to match — mirror image of the already-fixed #3123 (an under-declaration, fixed by `5428e872` in the same block, four lines away, without touching this stale line).

## Location

`byroredux/src/boot.rs:1228-1236` (declaration), vs. `byroredux/src/systems/billboard.rs` (no `MeshHandle` reference anywhere)

## Evidence

`boot.rs:1235` — stale `.reads::<byroredux_core::ecs::MeshHandle>()`. `grep -n "MeshHandle" byroredux/src/systems/billboard.rs` → no hits.

## Impact

Documentation/analysis only — `add_exclusive_with_access` exclusives aren't paired against each other by the scheduler's analyzer, so no live scheduling consequence. `sys.accesses` and any future tooling/auditor reading this declaration is told the system reads `MeshHandle`, and it does not.

## Related

#3123 (sibling under-declaration, fixed by the same commit that left this one stale), #2391.

## Suggested Fix

Remove `.reads::<byroredux_core::ecs::MeshHandle>()` from `make_billboard_system`'s registration. Extend `scheduler_access_tests.rs` to assert the declaration does NOT mention `MeshHandle`.

## Completeness Checks
- [ ] **LOCK_ORDER**: Declaration matches actual read surface
- [ ] **TESTS**: Sibling test asserting `MeshHandle` absence from the declaration
