# #7229: GAME-2026-11-22-06: `ActiveProjectiles` survives an in-process load, so the outgoing session's arrows and spell bolts land in the loaded world

**Labels**: low,gameplay,combat,save-load,bug

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-06**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 7 (session transient not dropped by the load drain)
- **Domain**: combat · **Game**: all with projectiles; Oblivion arrows and Target spells make the window common
- **Trigger**: An in-process load while a shot is in flight.
- **Location**: `byroredux/src/systems/projectiles.rs:48-76` (`Projectile`, `ActiveProjectiles`), `:408-546` (`projectile_flight_system`); `byroredux/src/save_io/live_load.rs:480-624` (`discard_outgoing_session_requests`, not named); `registry_completeness_tests.rs:561`
- **Status**: NEW
- **Description**: Each `Projectile` carries `shooter: EntityId`, `launcher_bodies` (Rapier handles of the outgoing physics world), `spell`, `arrow` and `damage`. Neither the drain nor the cell reload touches the vector. The allowlist reason ("a saved game restarts with none in the air") holds only for a cold start. After the load, the old shots keep flying through the new physics world. A hit becomes a `TrapHit` against a loaded-world actor, and a spell bolt calls `player_cast::land_target`, which starts its effects and feeds the school. The player's id survives the load, so the player's own shot lands as the loaded player's.
- **Evidence**: `rg ActiveProjectiles byroredux/src/cell_loader byroredux/src/save_io` finds nothing.
- **Impact**: Phantom damage, a Marksman or school skill use, and spell effects for the remaining second or two of flight. This is the class #6239/#6508/#6574/#6927 closed for other transients.
- **Related**: #6574, #6508, #6927, #7055, #7080
- **Suggested Fix**: Clear `ActiveProjectiles` in `discard_outgoing_session_requests` and reword its NOT_SAVED reason to "a load discards", so the drain guard covers it.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
