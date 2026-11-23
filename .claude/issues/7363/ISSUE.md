# #7363 — CONC-2026-11-23-04

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-04).

- **Severity**: LOW
- **Dimension**: 4 (scheduler proof soundness, guard blind spot)
- **Location**: `byroredux/src/boot/schedule/mod.rs:957-989` (the `player_controller_system` entry of `PARALLEL_SYSTEMS`); `byroredux/src/boot/schedule/early.rs:34-46` (the rows the hops need); callers at `byroredux/src/systems/character.rs:437` (`skill_use_emit::on_jump`) and `:1757` (`skill_use_emit::on_fall`, `detection::actor_value` at `:1751`)
- **Status**: NEW (same class as closed #4994, #5641, #6217, #6447, and the LimbEffects gap fixed in dfcbcafd5)
- **Description**: `player_controller_system` is one of two parallel systems in `Stage::Early` (the other is `timer_tick_system`). Since the baseline it reaches callees outside its listed source files:
  - `crate::skill_use_emit::on_jump` / `on_fall` -> `byroredux_scripting::oblivion_skill_use::queue_skill_use` (`crates/scripting/src/oblivion_skill_use.rs:479`): reads `CharacterRuleset`, writes `PendingSkillUse`.
  - `fall_landing` -> `crate::detection::actor_value` (`byroredux/src/detection/tick.rs:386`): reads `ActorValueNames`, then `ActorValues`.
  The row today declares `.writes_resource::<PendingSkillUse>()`, `.reads_resource::<ActorValueNames>()`, `.reads_resource::<CharacterRuleset>()` (and `ActorValues` as a write), so the declaration is correct. But `PARALLEL_SYSTEMS` lists no entry for `OBLIVION_SKILL_USE_SRC` / `queue_skill_use` or for `detection/tick.rs` `actor_value`, and the scan follows calls only inside listed files, so two of the new rows are held up by nothing: `PendingSkillUse` and `ActorValueNames` are acquired by no listed body (`CharacterRuleset` is read directly at `character.rs:186`, and `FallModel` / `FallTracker` in `character.rs`, so those three are guarded). A later change to `queue_skill_use` (a new gate resource, a notice push) would make `known_conflict_count() == 0` unsound for the Early pair with no test failing, which is the exact failure the table exists to prevent.
- **Evidence**:
  ```
  // character.rs:434-438
  if jump_fired && swim.is_none() {
      // #7051 ...
      crate::skill_use_emit::on_jump(world);
  }
  // character.rs:1749-1757
  let damage = model.damage(seconds, crate::detection::actor_value(world, player, "Acrobatics"));
  ... crate::skill_use_emit::on_fall(world);
  ```
  `PARALLEL_SYSTEMS["player_controller_system"]` hops: `queue_dead_actor_reconciliation`, `set_kinematic_translation`, `set_linear_velocity`, `weather_wave_adjustment`, `raise_kill_story_event`, `raise_story_event`, `note_death`, `locomotion_mult`, `crippled_legs`. No skill-use or detection entry.
- **Trigger Conditions**: none today (the declaration is complete, verified by hand against the two callee bodies). It becomes a live hole when either callee gains an acquisition.
- **Impact**: guard gap only. If it drifts, a data race is impossible (RwLock) but a cross-thread ABBA between `player_controller_system` and `timer_tick_system` becomes undetectable statically.
- **Verification Path**: add `(OBLIVION_SKILL_USE_SRC, "queue_skill_use")` and `(DETECTION_TICK_SRC, "actor_value")` to the entry, and extend `cross_file_hops_and_get_forms_reach_their_acquisitions` to pin `PendingSkillUse=Write` and `ActorValueNames=Read` on `player_controller_system`; that pin fails today without the hops (neither `PendingSkillUse` nor `ActorValueNames` appears in any listed body).
- **Related**: #4994, #6447, #5641, #6217 (all closed, same class).
- **Suggested Fix**: add the two hop entries and the pin above.
- **Dimension agent ID**: CON-D4-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record-time latches, other temp stems, other exclusive rows)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
