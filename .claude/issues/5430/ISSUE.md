# #5430: GAME-D5-2026-10-08-04: `forcegreet_system` moves NPCs outside the `BYRO_NO_AI_LOCOMOTION` kill switch, and the kill-switch guard cannot see `eat_sleep_system`

**Labels**: low,gameplay,ai,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5430

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D5-2026-10-08-04` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Dim 5
- **Location**: `byroredux/src/boot/schedule/update.rs:442`; `byroredux/src/boot/schedule/post_update.rs:90-136`; `byroredux/src/boot/schedule/mod.rs:304-345`
- **Status**: NEW
- **Description**: The skill's contract is that `BYRO_NO_AI_LOCOMOTION=1` gates exactly the motion systems. `forcegreet_system` steps NPCs toward the player but is registered unconditionally in Update. Since `00f580e09`, ambient selection installs it on its own, so a locomotion-isolated run still has NPCs walking. `eat_sleep_system` is correctly inside the block. However, `locomotion_kill_switch_gates_exactly_the_motion_systems` only collects `crate::systems::make_*` names, so `crate::systems::eat_sleep::eat_sleep_system` is invisible to it: moving it out of the block would fail no test. `walk_animation_registers_after_all_six_movers` likewise lists neither new mover.
- **Suggested Fix**: Gate the force-greet walk step on the switch (keep the open path ungated) and extend both guards to the new movers.

## Completeness Checks
- [ ] **SIBLING**: any other mover registered outside the locomotion block (or not via make_*) checked
- [ ] **TESTS**: A regression test pins this specific fix
