# #5056 — SAVE-D5-2026-09-29-02: Cinematic state survives the save-load teardown — the player stays glued to a retained ghost vehicle, and retained FormID-bearing entities are duplicated by the reload

**Labels**: high,save-load,gameplay,bug

**Source report**: `docs/audits/AUDIT_SAVE_2026-09-29.md`
**Severity**: HIGH
**Dimension**: Live Load-Apply & Frame Boundary (GPU/physics teardown completeness) — data-loss class: corruption-on-load / reference-break

## Location
- `byroredux/src/cell_loader/unload.rs` — `cinematic_retained_entities`, applied by every `unload_cell`/`unload_cells` (`victims.retain(|e| !retained.contains(e))`), including the load path's `unload_current_interior` and `drain_streaming_state`
- `byroredux/src/save_io.rs` — `ActorCinematicState`/`HorseTetherState` registered but excluded from `MUTABLE_DELTA_COLUMNS`; `validate_cinematic_entity_refs`
- `crates/scripting/src/fragment/effects.rs` — lazy insert and `SetVehicle` on any resolved actor, including the player
- `byroredux/src/systems/cinematic.rs` — `vehicle_attachment_system`
- `crates/save/src/driver.rs` — `pair_to_live` collected into a `HashMap`

## Description
1. **Player state leaks.** The process-lifetime player's `ActorCinematicState` is neither overlaid nor cleared on load. Loading during the MQ101 cart ride keeps `vehicle = Some(cart)` (saving is chargen-disabled then, loading is not).
2. **Teardown retains instead of despawning.** Because of that state (and `HorseTetherState`, never removed), the teardown keeps the cart, horse, every rider and their subtrees, stripping `CellRoot` rather than despawning. After the convoy has started once in a process, cart and horse are retained on every later load.
3. **The reload duplicates them.** Fresh copies of the same REFR/ACHR records spawn, so two live entities share a `FormIdPair`; `build_form_id_remap`'s `HashMap` collect keeps one arbitrarily, so saved deltas can land on the ghost.
4. **The player is pinned to the ghost.** Every frame `vehicle_attachment_system` overwrites the player's `Transform` and kinematic body from the ghost cart, defeating `apply_player_pose`. A restored quest state past MQ101 will never fire ExitCart.
5. **Validation cannot see it.** Stale ids pass post-load validation and every later pre-save gate, because entity ids are never reused and the dangling test is `>= next_entity`.

## Evidence
`unload_cell_inner` → `victims.retain(|entity| !retained.contains(entity))`. No production site removes either cinematic component (re-confirmed by grep for `remove::<ActorCinematicState>` / `remove::<HorseTetherState>`). `vehicle_attachment_system` writes `Transform` and `set_kinematic_translation` for every `ActorCinematicState` whose `vehicle` still has a `Transform`.

## Impact
Loading during or after the Helgen convoy in the same process can pin the player to a moving ghost cart, and the world can hold duplicate cart/horse/(mid-ride) rider entities with ambiguous remap targets. No in-session recovery if the loaded quest state is past MQ101.

## Related
#3817 (open: cinematic states never terminate — covers permanence across ordinary unloads, not the load teardown, the stale player state, or the duplicate FormIDs), #3254, #2380, #2535; ECS-2026-09-29-D1-02 (lock-order hygiene in `cinematic_trio_survives_save_load_round_trip`); save-load-teardown theme: GAME-D7-2026-09-29-01 (#5027), SAVE-D1-2026-09-29-01 (#5052), SAVE-D1-2026-09-29-02 (#5058), GAME-D1-2026-09-29-02 (#5034), SAVE-D5-2026-09-29-01 (#5054).

## Suggested Fix
Before the load's teardown, remove `ActorCinematicState`/`HorseTetherState` from live entities (at least the player's) so `unload_cell` retains nothing. After the overlay, clear the player's `vehicle` when the saved row is absent, or register the pair as replacing for FormID-matched entities. Test: live player `vehicle` + tether → after load, no entity outside `CellRootIndex` and no player `vehicle`.

Validated at HEAD 9fcfdc3fc: `cinematic_retained_entities` still feeds `victims.retain` in `unload.rs`; both components registered full-only (not in `MUTABLE_DELTA_COLUMNS`); no production removal of either.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
