# #7212 CHAR-2026-11-21-L-15: Two consumer-side `GameKind` tests remain on the Oblivion leveling route: `install_skill_use_classes` and `oblivion_rest::supports_rest`

**Labels**: low,character,tech-debt,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: oblivion
- **Location**: `crates/scripting/src/oblivion_skill_use.rs:79-97` (`install_skill_use_classes`, `index.game == GameKind::Oblivion`); `byroredux/src/oblivion_rest.rs:47-51` (`supports_rest`, `ScriptGame.0 == GameKind::Oblivion`), callers `oblivion_rest.rs:111,331`, `interaction.rs:1512`, `menuxml_rest.rs:254`, `menu_stack.rs:415`
- **Status**: NEW (merges CHAR-D4-06 and CHAR-D1-04)
- **Description**: The skill-use runtime keys on `LevelingModel::SkillUse`, and its module doc says "there is no game test here" (`oblivion_skill_use.rs:9-11`). Two gates still test the game:
  - **The class table** is built only when `index.game == Oblivion`. That gate is redundant, because `ClassRecord::specialization` is decoded only for Oblivion (`actor/class.rs:119-141`).
  - **Sleep, wait and the bed prompt** are offered only on `GameKind::Oblivion`, and they lead to `apply_pending_level` and the level-up offer. So the entry to a data-driven level-up is a game test. It is partly a UI-routing fact, since only Oblivion ships these two MenuXml documents.
- **Evidence**: the two predicates above; the Dim 1 grep finds no other production `GameKind` hit in a CHARAL rule consumer.
- **Impact**: none today. The #5921 class: a second game with a classic-TES class row or a sleep/wait menu has no data row to flip and needs a code edit in each gate.
- **Related**: #5921, #7086 (Oblivion sleep and level-up menus), `docs/engine/skill-use.md:192`.
- **Suggested Fix**: key the class table on `class.specialization.is_some()` (or the installed leveling model); derive `supports_rest` from `leveling.skill_use_costs().is_some()` or a profile `rest` row, keeping the MenuXml document check on the UI side.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
