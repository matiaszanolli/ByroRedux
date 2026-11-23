# #7305: SCR-D4-01: Door runtime parks a script-state row for every unscripted in-cell door that nothing ever consumes, and a scripted door ignores the re-entry default

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/reference_state.rs:499` (`store.scripts.extend(scripts)`), `:505-:547` (`capture_script_states`), `:552` (`restore_script_state`); sole spawn-time consumer `byroredux/src/cell_loader/references/attach.rs:216-220`; test `byroredux/src/systems/door.rs:390-402`
- **Status**: NEW, but overlaps the open #7059. Its acceptance criterion, "a door returns to its default on same-session re-entry", is not met on the real unload path. I treat the leak and the test gap as findings, not as an already-listed gap.
- **Description**: Before door-runtime commit `50abfd008`, a `TwoStateActivator` existed only on a reference with an attached script. That script's attach consumed the parked row through `restore_script_state`. `attach_door` (`byroredux/src/systems/door.rs:193`) now stamps a `TwoStateActivator` on every in-cell door whose model authors `Open` and `Close` sequences, with or without a script. The rest of the path is unchanged:
  - `capture` (called from `unload_cell` and `evict_actor`) parks `ScriptState { two_state: Some(..) }` for every victim that has a `FormIdComponent`. The `states.retain(|(_, s)| !s.is_empty())` filter keeps the row.
  - `restore_script_state` is called only inside `if attached {` in `attach_script_for_refr`. An unscripted door never attaches, so its row is never removed. `store.scripts.remove` at `reference_state.rs:558` is the only removal in the tree.
  - The `scripts` map is serialized (`SavedPersistentReferenceStates.scripts`, `reference_state.rs:208` and `:247-249`), so every visited unscripted door leaves a permanent saved row.
  - A door that does attach a script, such as an ObScript `SCRI` or a recognized or VM Papyrus script, gets the parked `is_open` restored over the `Open by Default` seed. That is the #6071 behaviour. It contradicts the door runtime promise and the Phase 1 acceptance criterion of #7059. So re-entry default behaviour differs between scripted and unscripted doors.
- **Evidence**:
  ```rust
  // attach.rs:216-220 — the only spawn-time consumer, gated on a script having attached
  if attached {
      // #6071 — the state an earlier visit left, over the authored defaults ...
      crate::cell_loader::reference_state::restore_script_state(world, entity);
  ```
  ```rust
  // door.rs:394-398 — the re-entry test never runs capture / restore_script_state
  world.query_mut::<TwoStateActivator>().unwrap().get_mut(first).unwrap().is_open = !default_open;
  world.despawn(first);
  let second = placed_door(&mut world, default_open);
  ```
  The `a_door_returns_to_its_default_state_on_reentry` test does a bare `despawn`. It bypasses `reference_state::capture` and the script attach, so it passes only because it does not exercise the parking path.
- **Impact**: Unscripted doors give a bounded but permanent growth in the saved `PersistentReferenceStates.scripts`. It is one small row per distinct door ever evicted, bounded by the number of doors, which is about 1.7k sequence placements in Oblivion. Scripted doors are not reset on re-entry while unscripted ones are, so one design promise gives two behaviours. For a scripted door the original behaviour is unsourced (#7062 lists the default-state question), so I rate this LOW and not MEDIUM.
- **Related**: #6071 (closed, the parking design), #7226 (closed, a door parked mid-swing, which can only happen on the scripted path), #7059, #7062.
- **Suggested Fix**: Drop the `two_state` capture for a door that carries a `Door` component, or have `capture_script_states` skip an entity whose `TwoStateActivator` has no script attached. Alternatively consume the row for it at `attach_door` time. Then make the re-entry test go through `capture` and the real attach, so it pins the decision either way.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
