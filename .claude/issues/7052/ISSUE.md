# Oblivion skill-use leveling Phase 3: levels

Part of #7047. PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/oblivion-skill-use-leveling.md#9-delivery-sequence). Depends on #7050, on the ObScript runtime for the commands, on #7049's V6 to V9, V12 and V13, and (for the native page, not the console surface) on #7048. Closes #7044.

The stored Health layer already exists (#7044, save format v136): `ActorValue`'s saved `stored` layer, which `undamaged()` counts and the per-frame refresh never rewrites, and `apply_oblivion_level_up_health`, which nothing calls yet.

## Acceptance Criteria
- [ ] OSL-LVL-003: the tenth major increase makes a level pending, carrying the attribute tallies frozen at that moment (V13 settles whether the tenth increase is in them)
- [ ] OSL-LVL-004, OSL-SLP-001, OSL-SLP-002: only a completed sleep (script path and bed activation) applies a pending level; waiting does not; an interrupted sleep does not
- [ ] OSL-LVL-005: several levels may be pending, each using its own frozen tallies
- [ ] OSL-LVL-006: no level when every attribute is at 100
- [ ] OSL-ATT-001: a level-up offers three attributes with the `oblivion_attribute_bonus` of their tallies, applies the three chosen up to 100, and rolls nothing over
- [ ] OSL-ATT-002: a test raises Endurance, Intelligence and Strength and sees Health, Magicka and Fatigue read their formulas at the new values
- [ ] OSL-HP-001: a level-up adds `floor(0.1 × Endurance)` to the stored Health term (V7, V8 settle which value)
- [ ] OSL-HP-002: the stored term survives `player_derived_stats_system`, a Fortify Health ability added and removed, a save and a live load
- [ ] OSL-HP-003: no Magicka or Fatigue stored-term change unless V9 found the original writing them
- [ ] OSL-API-002: `AdvancePCLevel` makes a level pending or opens the level-up surface; `SetLevel` on the player keeps changing the level and no stat
- [ ] OSL-API-004: applying a level pushes one line naming the new level to the notification queue (text per V12)
- [ ] OSL-API-005: `levelup attr <attribute>` spends one of a pending level's three picks; `levelup` reports the pending levels (OSL-API-003)
- [ ] OSL-SAVE-001 to OSL-SAVE-003: the major-increase count, tallies, pending levels and their tallies are saved; a pre-change save is refused by the `FORMAT_MAJOR` bump, never loaded with invented state; a mid-level save resumes identical and re-captures equal

**Exit:** PRD §7.2 scenarios 4 to 7 pass (level-up and Health in `oblivion-level-up.sh`, Health survives, a pre-feature save is refused, waiting does not level), and #7044 closes with them.

