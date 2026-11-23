# #7342: GP-06: Doors in the exterior streaming ring park their saved open state in `scripts`, where only a script attach ever consumes it

- **Severity**: LOW
- **Dimension**: 2 (reference_state, save-load park)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:1135-1148` (`park_unresolved_snapshot_rows`, #5054); consumers `:1034` (`restore_resident`) and `references/attach.rs:220` (only inside `if attached`)
- **Status**: NEW
- **Trigger**: an exterior save (Oblivion/FO3/FNV worldspace) with an unscripted sequence door in the hysteresis ring (`radius_load + 1`, not respawned by the reload) left open; load; walk to it.
- **Description**: #7305 reasoned that an unscripted door never consumes a parked `TwoStateActivator` because `restore_script_state` runs only after a script attaches. The save-reload park path builds the same kind of row from the snapshot (`two_state: two_states.remove(&old)`) for every unresolved reference with no `Door` test (the entity is not live, so there is no `Door` to read). For an unscripted door that row is never consumed: the door streams in at `Open by Default` instead of its saved state (a resident door gets the saved state through `apply_deltas`), and the row is never removed, so it is re-saved forever (the leak #7305 fixed for eviction).
- **Evidence**: `restore_script_state` is called only from `reference_state.rs:1034` (`restore_resident`, resident entities at load time) and `attach.rs:220` inside `if attached { .. }`. `parked_scripts` keeps any non-empty `ScriptState` whether or not the reference will ever attach a script.
- **Impact**: a saved open (or closed, for an open-by-default door) unscripted door state in the ring is lost on load, and one dead row per such door sits in `PersistentReferenceStates.scripts` and in every later save.
- **Related**: #5054, #7305.
- **Suggested Fix**: consume the parked `two_state` where `attach_door` stamps the `Door` (restore after `attach_door`), or park it in `rows` with a consumer on the door spawn path.

---
- **Dimension agent ID**: GP-D2-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
