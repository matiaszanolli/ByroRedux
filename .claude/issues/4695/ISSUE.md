# GAME-D7-2026-09-21-01: Pickup tombstones of placements resident at save time are never re-applied on load — the item returns pickable and duplicates

**Issue**: #4695
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: HIGH
**Dimension**: 7 — Gameplay-State Coverage (plus Dim 2, loot persistence)
**Location**: `byroredux/src/inventory.rs` (`pickup_loot`, marker + `mark_picked_up`); `byroredux/src/cell_loader/reference_state.rs` (`without_parked_state`, `mark_picked_up`, `restore`); `byroredux/src/save_io.rs` (reload sequencing); `byroredux/src/save_io/registry_completeness_tests.rs` (allowlist entry)

## Description
`pickup_loot` stamps `PickedUp` on the resident root (not serialized) and parks a `picked_up: true` row in `PersistentReferenceStates` (saved). On load, the cell reload runs inside `without_parked_state`, which removes that resource for the reload's duration — so the respawned placement's `reference_state::restore` finds no store and does nothing. `restore_resources` then installs the saved store as a plain overwrite afterward, but nothing walks the now-resident entities to consume rows that now match them. The allowlist's justification ("`reference_state::restore` re-stamps the marker at load") holds only for the ordinary eviction/respawn case, not for the saved cell itself.

## Evidence
```rust
// save_io.rs — respawn happens with PersistentReferenceStates absent
let outcome = without_parked_state(world, |world| { reload_interior_session(...) });
// saved store arrives only after the respawn above
byroredux_save::restore_resources(world, &registry, &snapshot)
```
No test combines pickup with save/load.

## Impact
After save → load in the cell where the item was picked up (the ordinary case), the item is back — interactive and pickable — while the loaded inventory already holds it. Duplicate item. Stale row stays parked until the cell is evicted and revisited.

## Related
#4571 (closed; separate defect — render-skip marker placement, already fixed); #4465 (`picked_up` field / FORMAT_MAJOR 25); GAME-D2-2026-09-21-02 (pickup has no player input path today, limiting but not eliminating reachability). **Cross-report**: shares its root mechanism with `AUDIT_SAVE_2026-09-22.md`'s SAVE-D5-2026-09-22-01 (not yet published — expect a follow-up comment from that report's publish pass with the `without_parked_state` root-cause addition, not a duplicate issue).

## Suggested Fix
Either register `PickedUp` as a zero-field delta column (the `Dead` pattern), or after `restore_resources` consume `picked_up` rows for FormId-matched resident placements. Add a pickup → save → load round-trip test.
