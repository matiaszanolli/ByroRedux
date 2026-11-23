# #7348: SAVE-03: the `Background` NOT_SAVED reason is still false for `class_form_id`: `SetClass` writes the player's class and no save keeps it

- **Severity**: MEDIUM
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:487` (`Background`); writer `crates/scripting/src/obscript_more/tail.rs:474-483` (`cmd_10f0`, registered `tail.rs:764` for Oblivion); reader `crates/scripting/src/oblivion_skill_use.rs:107-116` (`player_class`); partial fix `byroredux/src/systems/character.rs:131-153` (`player_identity_sync_system`).
- **Status**: NEW
- **Dimension**: Snapshot Completeness
- **Data-Loss Class**: silent-drop
- **Where**: `byroredux/src/save_io/registry_completeness_tests.rs:487` (`Background`); writer `crates/scripting/src/obscript_more/tail.rs:474-483` (`cmd_10f0`, registered `tail.rs:764` for Oblivion); reader `crates/scripting/src/oblivion_skill_use.rs:107-116` (`player_class`); partial fix `byroredux/src/systems/character.rs:131-153` (`player_identity_sync_system`).
- **Premise (quoted)**: reason = "derived character-creation metadata: re-created with the actor, and on the player rewritten from the saved CharacterIdentity by player_identity_sync_system ... so the saved column is the source of truth". `CharacterIdentity` is `{ name, female, race_form_id }` (`crates/core/src/character/components.rs:343-349`) — no class. `player_identity_sync_system` writes only `race_form_id` (and inserts `class_form_id: 0` when no Background exists). `cmd_10f0` does `background.class_form_id = class` on `caller_or_player` (GECK/CS *SetClass*: "Changes the characters class of the actor"). `player_class()` reads `Background.class_form_id` first and falls back to the NPC record's class; that class's seven majors decide which rank-ups count toward a level (`SkillUseLevel`, saved).
- **Problem**: the 2026-11-17 SAVE-9 fix replaced "no live production mutator" with a reason that is true for race and still false for class. After a cold-start load the player's `Background` is re-stamped from the `NPC_` record, so a script-set class (a mod's `player.SetClass`, the OSL-CLS-003 case the skill-use docstring promises) reverts and the saved `SkillUseLevel` tallies/pending levels are then judged against the wrong majors. After an in-process live load the process-lifetime player keeps whatever class the outgoing timeline set (cross-timeline leak, the inverse failure). Same hole for any actor whose class a script changed (NPC `Background` is not parked either). Vanilla Oblivion scripts have zero `SetClass` calls (cs-uesp-wiki "List of function appearances in scripts"), hence MEDIUM, not HIGH.
- **Fix sketch**: carry the player's class in the saved identity (`CharacterIdentity.class_form_id`, a v144 bump, restored by `player_identity_sync_system`) or register `Background` as a replacing column; then restate the reason.
- **Dedup**: no open or closed issue (`gh issue list --state all --search "SetClass Background class save"` empty; not in issues.json). Regression note on SAVE-9 (fixed 110790dce, partial).
- **Dimension agent ID**: SAVE-D1-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
