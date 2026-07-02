# ECS-2026-07-02-03: clear_entities poison panic drops the component type name (bypasses the #466 side-table)

**Labels**: low, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1836
**Source**: docs/audits/AUDIT_ECS_2026-07-02.md

**Severity**: LOW
**Dimension**: 1 — Lock Ordering & Deadlock (poison-on-panic resolution)
**Location**: `crates/core/src/ecs/world.rs:227-233`

## Description
`World::despawn` resolves type-erased poison through the `type_names` side-table so the panic names the offending component (#466, `world.rs:121-136`). `clear_entities` — used on the M45 restore path, run on every full snapshot load — still iterates `self.storages.values_mut()` and panics with the generic message `"storage lock poisoned during clear_entities"`, discarding the TypeId key it could use to resolve the name. It still fails loud (the policy's core guarantee holds), but loses the diagnostic the side-table was built to provide.

## Evidence
```rust
// world.rs:230
.unwrap_or_else(|_| panic!("storage lock poisoned during clear_entities"))
```
vs. `despawn`'s `self.storages.iter_mut()` + `self.type_names.get(type_id)` + `storage_lock_poisoned_erased(type_name)`.

## Impact
On a poisoned-lock cascade during a save load, the panic cannot say *which* storage poisoned — the exact "10× harder bisect" #466 fixed for `despawn`. Post-panic-only path, so no runtime correctness impact.

## Related
#466; `despawn_poisoned_lock_panics_with_type_name` (`world_tests.rs:1256`). Same finding as `ECS-2026-07-01-02` (2026-07-01 report) — unfixed, carried over. Neither day's report had previously been filed as a GitHub issue.

## Suggested Fix
Iterate `self.storages.iter_mut()`, resolve the name via `self.type_names.get(type_id)`, and call `storage_lock_poisoned_erased(name)` — a three-line change mirroring `despawn`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (`despawn`'s existing poison-resolution as the reference implementation)
- [ ] **TESTS**: A regression test pins this specific fix (mirroring `despawn_poisoned_lock_panics_with_type_name`)
