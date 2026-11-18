# Oblivion skill-use leveling Phase 0: decode, census, decisions

Part of #7047. PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/oblivion-skill-use-leveling.md#9-delivery-sequence). No engine code.

## Acceptance Criteria
- [ ] OSL-XP-002: V1 answered: whether the minor multiplier is `fSkillUseMinorMult` (CS) or `fSkillUseMinor` (UESP formula), from `Oblivion.esm`'s `GMST` records and the executable's string table
- [ ] OSL-XP-003: V2 answered: whether a skill at 100 keeps banking experience
- [ ] OSL-USE-001: V3 answered: whether UESP's experience per action are the executable's skill-use increments (`GetSkillUseIncrement` read by script)
- [ ] OSL-USE-002 to OSL-USE-005, OSL-USE-007: V4 answered: the census of actions (which armor piece takes a hit's experience, which jump, fall and dodge events count, whether a pickpocket counts), and UESP's action count recorded for §8's fed-actions metric
- [ ] OSL-LVL-001: V5 answered: whether a rank-up writes the base skill or a separate stored value
- [ ] OSL-SLP-001, OSL-SLP-002: V6 answered: what ends a sleep as interrupted, whether the engine can interrupt one, whether `SetPCSleepHours` applies a level
- [ ] OSL-HP-001: V7 and V8 answered: which value the executable truncates, and whether the gain reads Endurance before or after the level's attribute bonus (starting point: the level-up routine near `0x66c3e0`)
- [ ] OSL-HP-003: V9 answered: whether the level-up writes the stored terms of Magicka or Fatigue, and whether the load path zeroes the `Player` record's stored pools (`0x5240b2`)
- [ ] OSL-CLS-001: V10 answered: the vanilla `Player` record's class and its seven major skills
- [ ] OSL-API-001, OSL-API-006: V11 answered (how many vanilla scripts call `AdvancePCSkill`, `ModPCSkill`, `AdvancePCLevel`, `SetLevel` on the player) and V14 answered (the unit of `Amount`)
- [ ] OSL-API-004: V12 answered: where the level-up message's text lives
- [ ] OSL-LVL-003: V13 answered: whether the tenth major increase counts toward its attribute's tally
- [ ] OSL-NG-005: V15 answered: whether the live UESP *Oblivion:Health* page documents a 2024 formula beside the 2006 one
- [ ] OSL-XP-007: `docs/engine/skill-use.md` merged, with the emitter contract (one pending resource, one draining system, scheduler access declared) and the action census of V4
- [ ] The "Leveling — LOCKED" paragraph of `docs/engine/charal-oblivion-ruleset.md` corrected: `fSkillUseCurve` 1.95 is Skyrim's, and UESP counts all governed skills toward the attribute bonus
- [ ] The whole `ModPCSkill` / `GetAV` / `SetAV` / `ModAV` row of `docs/engine/scripting.md` corrected (or its stale actor-value half filed against the Oblivion Actor Values PRD)
- [ ] The menu request #7048 filed against the Game UI Integration PRD

**Exit:** every verify item closed or filed as an issue, the ruleset and scripting documents corrected, `skill-use.md` merged, the menu request filed, and every §10 decision recorded (done: Q1 to Q5 decided 2026-11-18). Then the PRD goes ACTIVE with this issue and #7047 on its `**Tracking**:` line.
