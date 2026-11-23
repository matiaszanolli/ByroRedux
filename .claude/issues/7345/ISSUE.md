# #7345: GP-09: `stance_use_system`'s `Access` row was not updated by #7281 and no acquisition guard covers the skill-use exclusives

- **Severity**: LOW
- **Dimension**: 4 (Oblivion skill-use seam) / 7 (stage order and access)
- **Location**: `byroredux/src/boot/schedule/late/scripted_combat.rs:38-52` (row), `byroredux/src/skill_use_emit.rs:195-203` (body), `byroredux/src/systems/character.rs:164-197` (`player_accepts_movement_input`, `player_movement_input_drives_body`); guard list `byroredux/src/boot/schedule/mod.rs:1226-1260`
- **Status**: NEW (sibling of closed #7319, whose fix named a fixed list of exclusives and did not drive the guard from every `add_exclusive_with_access` row)
- **Trigger**: Oblivion (the only game with skill-use leveling), any cell; static declaration only, no runtime effect today.
- **Description**: 6e6fdb58b (#7281) made `stance_use_system` call `player_movement_input_drives_body`, which acquires resource `PlayerMode`, component `Dead`, resource `PlayerControlState`, component `ActorControlState` and resource `VatsState` (through `player_accepts_movement_input`). The row declares none of them (it lists StanceTimers, PendingSkillUse, CharacterRuleset, PlayerEntity, ActionState, PlayerDetection, Sneaking, CharacterController, WaterContact). The 8 acquisition guards in `system_access_declaration_tests` (`detection_tick`, `sneak_stance`, `player_cast`, `player_bow`, `player_seat`, `combat_input`, ...) do not include `stance_use_system`, `skill_use_system` or `oblivion_level_system`, so the test stayed green (verified: `system_access_declaration` filter passes).
- **Evidence**: row at `scripted_combat.rs:41-52` ends in `.reads::<byroredux_physics::CharacterController>().reads::<...WaterContact>()`; `rg stance_use_system byroredux/src/boot/schedule/mod.rs` has no hit; `skill_use_emit.rs`: `let drives_body = crate::systems::player_movement_input_drives_body(world, player);`.
- **Impact**: exclusives are not paired by the analyzer, so nothing misbehaves today; but `sys.accesses` reports a hold stack that is short by five types, and promoting the system to a parallel lane would be unsound. The same drift will recur for any later edit to the three skill-use exclusives.
- **Related**: #7319, #6546, #7281.
- **Suggested Fix**: add the five types to the row and add `stance_use_system` / `skill_use_system` / `oblivion_level_system` to the acquisition guard (ideally derive the guard from every exclusive row, as #7319 suggested).
- **Dimension agent ID**: GP-D4-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
