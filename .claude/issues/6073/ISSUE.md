# GAME-SD-03: A corpse's position is never carried across eviction; it comes back at its authored placement and collapses again

Labels: medium,gameplay,save-load,combat,bug

**Severity**: MEDIUM · **Dimension**: Death reconciliation across the stream boundary (Dim 4 producers in area, Dim 7)
**Location**: `byroredux/src/cell_loader/stream_snapshot.rs:96-103` (`has_package_state`), `:156-259`; `byroredux/src/combat.rs:1075-1076` (`reconcile_dead_actor` removes `AmbientPackageRuntime`); `reference_state.rs:325-341`

## Description
`capture_actor_snapshots` keeps a row only `if snapshot.has_package_state()` (`active_package_form_id`, travel, traveled or seat). `reconcile_dead_actor` strips `AmbientPackageRuntime` and the ambient behaviors, so a corpse has none of them and produces no row. The `ReferenceState` row records `dead: true` but no position. The respawn builds the actor at the REFR placement, `restore` marks it `Dead`, and the Late sink runs `activate_ragdoll` from the spawn pose. The ragdoll never moves the actor root (`ragdoll.rs:571`: "the entity's own `GlobalTransform` never moves during ragdoll"), so the pre-eviction root `Transform` *is* the death position that would have been captured.

Trigger: the player kills an NPC away from its authored spot (an NPC that walked to a Travel/Wander destination, or was chased), then streams the tile out (leaves the radius, or takes an exterior->interior door, which also calls `drain_streaming_state`) and returns.

Status note: the continuity doc's keep-list (`docs/engine/stream-boundary-state-continuity.md:115-118`) includes "live position/orientation if diverged"; the implementation gates that on package state.

## Evidence
No `Dead` / `corpse` mention anywhere in `stream_snapshot.rs`; its tests only exercise Travel and Seated. `StreamStateSnapshots` is also cleared wholesale by `drain_streaming_state` (`streaming_helpers.rs:602-606`) and is not saved (`registry_completeness_tests.rs:492`), so even a fix inside the snapshot would not survive a door transition or a save; the parked `ReferenceState` row is the store that does.

## Impact
Corpses teleport back to where the actor was authored and visibly die a second time on tile re-entry; a body the player lost track of in the wasteland is no longer where it fell. Loot is unaffected (the inventory row persists). Authored "Starts Dead" corpses are unaffected (their XRGD pose is re-applied).

## Related
#5267 (the queued Late reconcile), #5015 (XRGD decode for Skyrim+/FO4, open), GAME-SD-01, GAME-SD-02.

## Suggested Fix
Add the root `Transform` (translation + rotation) to the `dead` `ReferenceState` row (alongside GAME-SD-01/02's version bump) and apply it in `restore` before queueing the reconcile, only when it diverges from the authored placement by `POSITION_DIVERGENCE_EPSILON`. A test evicts a killed, displaced actor and asserts the respawned corpse's root position.

Source: docs/audits/AUDIT_GAMEPLAY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
