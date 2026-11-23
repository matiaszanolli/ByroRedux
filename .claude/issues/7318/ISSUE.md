# #7318 ECS-D5-01: `combat_damage_system` and `interaction_system` rows omit types their cross-file hops acquire

Labels: low,ecs,concurrency,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: LOW. Both are exclusive and never paired, so this matters only for the `sys.accesses` report and as a baseline if either is promoted to a parallel lane.
- **Location**: `byroredux/src/boot/schedule/update/player_animation/combat_damage.rs`; `byroredux/src/boot/schedule/update/interaction_and_crime/interaction.rs`.
- **Status**: NEW. It is the same class as #6546 and #5035.
- **Description**:
  - The kill branch of `combat_damage_system` reaches `raise_kill_story_event` (`CurrentCellContext` / `CurrentExteriorContext` reads), `raise_death_event` (`OnDeathEvent`) and `obscript_state::note_death` (`KillRecords` write, `PapyrusPlayerEntity` read). When the player is the killer it also reaches `MiscStats`, `ChallengeDefs` and `ChallengeState`. The row declares none of these. `player_controller_system`'s row declares the same hop (#5641).
  - `interaction_system` reaches `jail::player_in_jail`, which reads `ObScriptLedger`. That came in with #7212.
- **Suggested Fix**: declare the types and list the hops in the guard tables.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
