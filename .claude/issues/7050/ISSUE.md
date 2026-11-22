# Oblivion skill-use leveling Phase 1: experience and rank-up

Part of #7047. PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/oblivion-skill-use-leveling.md#9-delivery-sequence). Depends on the CHARAL ruleset and the Oblivion actor values (both built) and on #7049's V1, V2, V5, V10, V11 and V14.

## Acceptance Criteria
- [ ] OSL-XP-001: one entry point banks `uses` of an action against an Oblivion skill and returns the ranks reached, in order; skill use, `AdvancePCSkill` and later trainers all go through it
- [ ] OSL-XP-002: the cost from skill value `L` follows the formula with the five `fSkillUse*` settings from the load order (defaults 1.25, 0.35, 1.5, 0.75, 0.75); base, specialization, major and both classes pinned at 5, 25, 50, 75, 99
- [ ] OSL-XP-003: surplus experience rolls over; a skill never exceeds 100 by use
- [ ] OSL-XP-004, OSL-CLS-001, OSL-CLS-002: a skill's class comes from the player's class record, read at each skill use; a test edits the record's major skills between two uses and sees the cost change; `levelup` names the class it read
- [ ] OSL-CLS-003: a script may set the class through the same record data
- [ ] OSL-XP-005, OSL-SAVE-001 (experience): banked experience persists across a save and a load; any new column is a declared-storage component and the format major and schema fingerprint bump in the same commit
- [ ] OSL-XP-006: `LevelingModel::SkillUse` data overlaid with the load order's settings (`with_gmst` arm), no `if Oblivion` branch in the applying system
- [ ] OSL-XP-007: emitters append to one pending resource drained by a single system; no emitter locks the player's `ActorValues`; the drain's scheduler access is declared
- [ ] OSL-LVL-001: a rank-up raises the skill's base actor value by 1 and does not go through the permanent-modifier layer
- [ ] OSL-LVL-002: a major rank-up counts toward the level; any rank-up adds to its governing attribute's tally
- [ ] OSL-API-001, OSL-API-006: `AdvancePCSkill` and `ModPCSkill` handlers (unit per V14; `ModPCSkill` sets the banked experience to 0.0, Q5); the `docs/engine/scripting.md` row updated
- [ ] OSL-API-003: `levelup` reports, for Oblivion, the class and its major skills, per skill the experience banked and its cost, the major increases, and each attribute's tally (the pending levels arrive in Phase 3)

**Exit:** PRD §7.2 scenarios 1, 2 and 3 pass: the cost table (`cargo test -p byroredux-core`), the five settings resolving to their defaults against `Oblivion.esm` with the cost at skill 5 equal to 2.89 (ignored real-data test), and the headless banking and rank-up script `docs/smoke-tests/oblivion-level-up.sh`.

