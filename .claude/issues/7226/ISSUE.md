# #7226: GAME-2026-11-22-03: A door saved or parked mid-swing keeps `is_animating = true` forever, so it can never be activated again

**Labels**: medium,gameplay,save-load,bug,game:oblivion,game:fo3,game:fnv

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-03**), audited at HEAD 2b40f76b6.

- **Severity**: MEDIUM
- **Dimension**: 2 (interaction, in-cell doors M66, persistence)
- **Domain**: gameplay · **Game**: Oblivion, FO3, FNV (any door whose model authors `Open` + `Close` sequences)
- **Trigger**: A save (quicksave, autosave) taken within about 1–2 s of activating a door, while its clip plays, then a live load of that save. Also: a scripted door whose cell unloads mid-swing, so its parked `scripts` row is consumed on re-entry.
- **Location**: `byroredux/src/systems/door.rs:143-173` (`door_motion_system`: the `settled` / `stale` loops), `crates/scripting/src/vm_state.rs:166-170` (`two_state_activator_system` skip), `byroredux/src/interaction.rs:1556-1560` (prompt filter), `byroredux/src/save_io/registry_completeness_tests.rs:1034` (the `Door` NOT_SAVED reason)
- **Status**: NEW (#7059 DRT-SAVE-001/-002 specify only the resting open pose)
- **Description**: `TwoStateActivator.is_animating` is saved: it is a live-save column, and `ScriptState.two_state` carries it on eviction. The paired bookkeeping, `Door.remaining`, is re-derived as `0.0` at spawn and is NOT_SAVED. `door_motion_system` clears `is_animating` only in the `settled` path, which needs `remaining` to step from `> 0` to `<= 0`. A restored door has `remaining == 0` and `posed == None`, so it takes the `stale` branch. That branch only re-poses the `AnimationPlayer` and never clears the flag. The registry reason ("presentation bookkeeping that the first tick re-establishes") is true for the pose and false for the flag.
- **Evidence**:
  - The `stale` loop calls `set_player` only.
  - `two_state_activator_system` does `if state.is_animating { continue; }`.
  - The interaction snapshot filters out an animating `TwoStateActivator`.
  - `restore_resident` runs before `apply_deltas`, so the saved `true` is the final value.
  - Every door test builds `is_animating: false`.
  - ObScript `GetOpenState` (`obscript_effects/getters.rs:425`) reports such a door as permanently opening or closing.
- **Impact**: The door is stuck for the rest of that save, with no prompt and every activation ignored. A door saved while closing stays shut, which can block quest paths. An unscripted door recovers on cell re-entry. A scripted door keeps the stuck state through eviction, and only a script `SetOpen` clears it.
- **Related**: #7059, #7062, #6071
- **Suggested Fix**: In `door_motion_system`, enforce the invariant the system itself creates (`is_animating` only while `remaining > 0`): clear the flag for a door with `remaining == 0`, in the `stale` branch or before the `posed` test. Add a test beside `a_door_at_rest_in_a_state_its_pose_does_not_show_snaps` with `TwoStateActivator { is_open: true, is_animating: true, .. }` and a fresh `Door`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
