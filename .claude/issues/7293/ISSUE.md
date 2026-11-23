# #7293: SCR-D6-01: Object scripts' `Begin MenuMode` blocks never run on non-pausing menu-mode frames

- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_objects.rs:287-295` (doc claim) and `:429`; `byroredux/src/boot/schedule/update/quest_scene/obscript_objects.rs:8-20`; `byroredux/src/menu_stack.rs:179-183`; `byroredux/src/app_events.rs:951-956`
- **Status**: NEW (introduced by 3ef0a2cb2, 2026-11-16, M48 Phase 1 / #6653; not in the 11-15 baseline)
- **Description**: GUI-EVT-006 made `collect_due` skip every object `GameMode` block while the registry holds any menu-mode menu, and added `obscript_object_menu_mode_system` to run `MenuMode` blocks instead. The doc says it is "Scheduled beside [`obscript_object_system`], and run by hand on the frames an open menu pauses the simulation". Only the second half is true. The scheduler registers `obscript_object_system` and `mark_cell_reset` only; the menu-mode system is called solely from `run_menu_mode_frame`, which `run_paused_frame` calls on frames whose simulation is paused. The quest sibling `obscript_quest_menu_mode_system` is registered in `obscript_quests.rs:23`; the object one is not. Oblivion/FO3/FNV `DialogMenu` is `MENU_MODE | CLAIMS_INPUT` with no `PAUSES_GAME` (the 3ef0a2cb2 commit message says "the dialogue menu keeps the world running"), so during every conversation object `GameMode` is suppressed and object `MenuMode` never runs.
- **Evidence**:
  ```rust
  // obscript_objects.rs:429
  BLOCK_GAME_MODE if !menu_mode && !is_stub(entity) => runs.push(Run { block, who: None }),
  // boot/schedule/update/quest_scene/obscript_objects.rs:15-19  (the whole registration)
  scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_state::mark_cell_reset);
  scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_object_system);
  // menu_stack.rs:179-183 — the only non-test caller
  pub(crate) fn run_menu_mode_frame(world: &World, dt: f32) { ... obscript_object_menu_mode_system(world, dt); ... }
  // menu_registry/tests.rs:57
  assert_eq!(fallout("DialogMenu").flags, MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT);
  ```
  `rg obscript_object_menu_mode_system byroredux` finds `menu_stack.rs` only. The unit test `menu_mode_replaces_game_mode_while_a_menu_mode_menu_is_open` calls both systems by hand, so it cannot see the gap.
- **Impact**: any scripted reference with a `Begin MenuMode` block (written for dialogue, `MenuMode 1009`, or "any menu") is dead during conversations and every other non-pausing menu-mode menu. Quest `MenuMode` blocks run in the same situation, so the two script kinds now disagree. `ui.md:886-890` documents the object path as live.
- **Related**: GUI-EVT-006; `/audit-ui` owns the registry and pause policy.
- **Suggested Fix**: register `obscript_object_menu_mode_system` in `register_obscript_objects_systems` right after `obscript_object_system`, as `obscript_quest_menu_mode_system` is, and add a schedule-level assertion (the `boot::SOURCES` style used by `scheduler_access_tests.rs`) that both menu-mode systems are registered.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
