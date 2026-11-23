# #7306: SCR-D4-02: A recognized `default2StateActivator` script overwrites the `Open by Default` state `attach_door` just seeded

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Untrusted-Input**: Yes. The recognizer reads `isOpen` and `doOnce` from modded VMAD or `.pex` property values, but this is no new decode path.
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:917` (`attach_door`, primary-synth block) versus `:980` (`attach_script_for_refr`); `crates/scripting/src/translate/recognizers/two_state_activator.rs:44`
- **Status**: NEW (hedge: overlaps the #7059 acceptance criterion that `SetOpenState`, Papyrus `SetOpen` and an activation drive one transition, so a maintainer may fold it in)
- **Description**: `attach_door` seeds `TwoStateActivator { is_open: default_open, .. }` from the REFR's `ONAM`. About 60 lines later the same function attaches scripts. A recognized `default2StateActivator` runs `world.insert(entity, state)`, and `state.is_open` comes from the script's `isOpen` property, defaulting to `false`. The insert replaces the whole component. A door that is `Open by Default` and carries that script therefore spawns closed. The `doc` comment says the seed and the recognizer coexist ("the Papyrus recogniser keeps filling it"), but nothing orders them. The overwrite also replaces the seed's `is_open` before `door_motion_system` poses the door, so the pose follows the wrong state. I could not find a vanilla door that combines both. That is unverified, and the severity reflects it.
- **Evidence**:
  ```rust
  // synth_child.rs:915-918
  if let Some(door) = stat.door.filter(|_| placed_ref.teleport.is_none()) {
      crate::systems::door::attach_door(world, placement_root, door, record_index.game, placed_ref.open_by_default);
  }
  ...
  // synth_child.rs:980 -> attach_vmad_scripts -> (recognized.spawn)(world, entity)
  // two_state_activator.rs:44
  world.insert(entity, state);   // is_open = script prop, default false
  ```
- **Impact**: The `Open by Default` placement contract (DRT-REC-004, DRT-STATE-001) breaks for scripted doors of that one archetype. The door's pose follows the script's default instead.
- **Related**: #7059, #7060 (Skyrim, where `default2StateActivator` is the common Papyrus door/lever script).
- **Suggested Fix**: Run `attach_door` after the script attach and merge it (set `is_open` only if the script did not author `isOpen`), or have the recognizer preserve an existing seeded state.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
