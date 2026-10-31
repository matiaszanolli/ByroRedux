# GAME-D4-2026-10-31-01: `mine_system` and `attraction_system` ignore the reference enable ledger, so a detonated mine comes back with its cell and disabled mines and objects still react

Labels: medium,gameplay,combat,bug,game:fo4

Filed from `docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (finding `GAME-D4-2026-10-31-01`).

- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4, M43 mines); NPC producers (Dim 5, attraction); state saved or re-derived (Dim 7)
- **Location**: `byroredux/src/systems/mines.rs:229-248` (the `placed` scan) and `:261-331` (`detonate`); `byroredux/src/systems/attraction.rs:173-190` (the object/actor gather)
- **Status**: NEW (`c36ab4125`, `ccccbaf03`, 2026-10-30)
- **Trigger**: Fallout 4. (a) Detonate a placed mine, then leave and re-enter its cell, or save and load. (b) Enter a cell whose mine or attraction object is "Initially Disabled", has an `XESP` parent that is off, or was `Disable()`d by a script.
- **Description**: A disabled REFR still gets a placement root. The loader (`spawn.rs:787-797`) spawns the root with no mesh or collider, marks it `PlacementContentWithheld`, and `synth_child.rs:83` still stamps `SceneAliasCandidate` on it. Both scanners enumerate every `SceneAliasCandidate` whose base is in their book and read `GlobalTransform`; neither asks `byroredux_scripting::reference_enabled(world, reference_form_id)` or checks the withheld marker. Compare `random_conversation::complex_scene_active` (`random_conversation.rs:94-107`), which fixes the same trap for the complex-scene marker ("the vanilla marker beside Whiterun is parented to a quest-driven reference, and counting it while dormant held back every conversation"). `detonate` records the mine as spent only by inserting the session component `SpentMine` and by `ReferenceEnableState::set_enabled(form, false)`. The ledger is saved and honoured on the next load, which withholds the root's content, but `SpentMine` is not saved. After a reload the root is a fresh "waiting" mine, so the next actor within the record's proximity re-trips and re-detonates it. The module doc says the ledger entry means the mine "does not come back with its cell". The invisible, non-solid mine does.
- **Evidence**:
  ```rust
  .filter(|(entity, _)| !world.has::<SpentMine>(*entity) && !world.has::<ArmedMine>(*entity))
  .filter_map(|(entity, spec)| Some((entity, spec, world.get::<GlobalTransform>(entity)?.translation)))
  ```
  There is no `reference_enabled` or `PlacementContentWithheld` test in `mines.rs` or `attraction.rs` (`grep` finds none).
- **Impact**: A spent mine explodes again on every cell reload, hurting the player and re-raising `TMEE`, which can restart its quests. An authored dormant mine field (quest-staged, enable-parented) detonates before the quest enables it. `attraction_system` raises `AOBJ` for disabled workshop and comment objects that nothing renders. The number of vanilla `Fallout4.esm` mines or attraction objects that are initially disabled or enable-parented was not measured.
- **Related**: #3278, #4698, #4820, #6077 (the loader and interaction honour the same ledger); `complex_scene_active` (005aa8a08) is the in-tree model for the fix.
- **Suggested Fix**: In both scans, skip a candidate when `candidate.reference_form_id != 0 && !reference_enabled(world, candidate.reference_form_id)` (or when it carries `PlacementContentWithheld`). Add a case to `mines.rs` tests for a disabled reference and for a spent mine after a simulated reload.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
