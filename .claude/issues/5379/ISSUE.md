# #5379: SAVE-D5-2026-10-08-01: #3817's cinematic re-adoption stamps `CellRoot` on the process-lifetime player, so the next save-load teardown despawns the player; pending un-rooted convoy entities survive the load as `FormIdPair` ghost twins

**Labels**: high,save-load,gameplay,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5379

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `unplaced` filters only on missing `CellRoot`, `retry_cinematic_readoption` inserts `CellRoot` with no player check, and `purge_cinematic_retention_state` never touches `CinematicReAdoption`. Reopens hazard (2) of closed #5056 through the new #3817 path; the player-despawn half is new.

- **Severity**: HIGH
- **Dimension**: Live Load-Apply & Frame Boundary (teardown completeness)
- **Data-Loss Class**: corruption-on-load / reference-break
- **Location**:
  - `byroredux/src/systems/cinematic.rs:489-498` (riders = every actor whose `ActorCinematicState.vehicle` is the cart);
  - `byroredux/src/systems/cinematic.rs:524-561` (`unplaced` = every released entity without `CellRoot`, queued on `CinematicReAdoption`);
  - `byroredux/src/systems/cinematic.rs:574-640` (`retry_cinematic_readoption`: no player exclusion; `world.insert(*entity, CellRoot(*root))` at `:619` plus a `CellRootIndex` push);
  - `byroredux/src/app_step.rs:83-90` (the retry runs every exterior streaming frame);
  - `byroredux/src/cell_loader/unload.rs:77-86` (`purge_cinematic_retention_state` removes only the two state components and never touches `CinematicReAdoption`);
  - `byroredux/src/save_io.rs:1581-1589` (exterior reload: purge, then `drain_streaming_state`);
  - `byroredux/src/save_io/registry_completeness_tests.rs:476` (allowlist reason).
- **Status**: NEW. This is a regression introduced by the #3817 fix (`63bf3347f`). It is distinct from ECS-2026-10-08-D7-01, which covers subtree nodes adopted by *local* Transform. It reopens hazard (2) of #5056 for released entities.
- **Trigger**: Skyrim MQ101 opening convoy. The player is a `SetVehicle` rider; the `unload.rs:1046` test and #5056 both model `vehicle = Some(cart)` on the player. The tether releases at the authored route terminal while riders are still attached; per the release doc, riders keep `cart_seat` for the later scripted exit. After that, the player does any in-process load (F9, the pause menu, console `load`) or a cell transition.
- **Description**:
  1. **The player is adopted into a cell.**
     - `release_finished_tethers` collects riders without excluding the player.
     - `release_set` walks their `Children`, which include the player body root.
     - `unplaced` keeps every member without a `CellRoot`. The player never has one: it is process-lifetime, and the body root is "never `CellRoot`-owned" (`player_body.rs:74`). So the player and its subtree are queued every time.
     - On the next streaming frame, `retry_cinematic_readoption` finds the player's world `Transform` in a loaded grid cell (by definition the player's own cell). It inserts `CellRoot(cell_root)` and registers the player in `CellRootIndex`.
  2. **The save load then destroys the player.**
     - `reload_exterior_session` runs `drain_streaming_state`, which unloads every loaded cell through `CellRootIndex` victims. The player is now one of them and is despawned, along with its physics body and GPU handles.
     - Nothing respawns it, because the live path relies on the player outliving the reload.
     - `PlayerEntity` dangles, and `build_form_id_remap` finds no live `PLAYER_FORM_ID_PAIR`. The player's saved `Inventory`, `ActorValues`, `CharacterController` and the rest go unresolved, and #5054's `park_unresolved_snapshot_rows` parks them as a `ReferenceState` that nothing will ever respawn.
     - `apply_player_pose` has no body.
     - The same despawn happens without any load, as soon as the player walks out of the arrival cell's ring or takes a door.
  3. **Pending entities survive the load.**
     - Released entities that are still pending have no `CellRoot`, and no state component, since release cleared it. Examples: the horse that drove past the loaded ring (the #3817 test asserts it "stays pending"), and subtree nodes per ECS-D7-01.
     - The purge does not touch them and the teardown cannot enumerate them, so they survive the load.
     - The reload spawns fresh copies of the same REFR/ACHR, and ghost and fresh copy share a `FormIdPair`. `build_form_id_remap`'s `HashMap` collect then keeps one at random, which is #5056's hazard (2).
     - The un-purged pending list then adopts the ghost into the loaded session.
     - The allowlist reason ("a save/session replacement tears the whole world down anyway (purge_cinematic_retention_state)") is false on both counts.
- **Evidence**: `let unplaced: Vec<EntityId> = { let roots = world.query::<CellRoot>(); release_set.iter().filter(|e| roots…get(**e).is_none())… }` → `pending.pending.extend(unplaced)`. The retry runs `world.insert(*entity, CellRoot(*root)); idx.map.entry(*root).or_default().push(*entity)` with no `PlayerEntity` check. `rg 'Player' cell_loader/unload.rs` finds nothing, so the unload has no player guard. `tether_releases_at_the_authored_route_terminal_and_detaches_riders` uses a non-player rider, so it cannot see this.
- **Impact**: after the Helgen convoy arrives, every later in-process load produces a session with no player. Workaround: restart the process and fresh-load. Separately, every later streaming unload of the arrival cell deletes the player, which is the gameplay-breaking half and is owned by `/audit-gameplay` or `/audit-scripting`. Saves already on disk are intact.
- **Related**: #3817 and #5056 (both closed); ECS-2026-10-08-D7-01; #3254.
- **Suggested Fix**:
  - Exclude the player and its body subtree from `unplaced`; the process-lifetime player must never be cell-owned. Add a player-rider case to the #3817 test that asserts the player has no `CellRoot` after the retry.
  - In `purge_cinematic_retention_state`, also despawn (or hand to the teardown) every entity on `CinematicReAdoption.pending`, then clear the list.
  - Fix the allowlist reason.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other process-lifetime entities (player body subtree, camera) reachable from a cinematic release set)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
