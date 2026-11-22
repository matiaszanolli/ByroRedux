# #7196 CHAR-2026-11-21-M-04: Oblivion's level-up settings are hard-coded: `iLevelUpSkillCount`, the ten `iLevelUpXXMult` attribute bonuses and `fStatsHealthLevelMult` never reach the leveling model

**Labels**: medium,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D3-01
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `crates/core/src/character/leveling.rs:117-130` (`with_gmst`, `SkillUse` arm overlays only the five `fSkillUse*`), `leveling.rs:219-222` (`major_skill_ups_per_level: 10`), `crates/core/src/character/tes.rs:187-196` (`oblivion_attribute_bonus` literal bands), `tes.rs:216` (`0.1 * f32::from(endurance)`); consumers `components.rs:46` (`PendingLevel::bonus`), `oblivion_skill_use.rs` (`apply_level`, `pick_attribute`, `per_level_of`)
- **Status**: NEW (same class as closed #6756 and #5915; the skill-use PRD OSL-XP-006 says "the rules MUST be `LevelingModel::SkillUse` data overlaid with the load order's settings")
- **Source**: CS wiki `ILevelUpSkillCount.wiki` ("Number of skill increases necessary to advance to next level", default 10); `ILevelUpXXMult.wiki` (ten settings 01..10, defaults 2,2,2,2,3,3,3,4,4,5, "determine how many potential attribute points the player will have, depending how many times they have used a linked skill"); `FStatsHealthLevelMult.wiki` ("The number by which the player's endurance is multiplied to calculate the amount of health to be added when the character levels up ... rounded down"); `charal-oblivion-ruleset.md:1001-1005` already names `iLevelUpSkillCount` and `iLevelUp01Mult`..`iLevelUp10Mult` as the settings. Oblivion.esm authors none of them (byte scan: all 12 absent), so vanilla is unaffected; the value of each default is correct (see table).
- **Description**: `LevelingModel::SkillUse::with_gmst` resolves the five `fSkillUse*` settings (PASS) and nothing else. The three other leveling settings the CS wiki documents are baked in: the 10-major-increase threshold is a literal in `LevelingModel::OBLIVION`, the +1..+5 bands are a `match` in a free function with no access to the ruleset, and the per-level Health gain is a literal `0.1`. `PendingLevel` stores *tallies*, not bonuses, so a retune would take effect at apply time without a save-format change; only the readers need to go through the model.
- **Evidence**: `with_gmst` arm: `costs: SkillUseCosts { factor: gmst("fSkillUseFactor")..., spec_mult: gmst("fSkillUseSpecMult")... }` with `major_skill_ups_per_level` and `level_cap` passed through unchanged; `oblivion_attribute_bonus(governed_skill_ups: u16) -> u8 { match ... 0 => 1, 1..=4 => 2, 5..=7 => 3, 8..=9 => 4, _ => 5 }`; `oblivion_health_gain_per_level(endurance) = 0.1 * f32::from(endurance)`. The test `skill_use_costs_overlay_the_load_orders_settings` (`leveling.rs:495`) pins only the five.
- **Impact**: Leveling-rule mods (a lower skill-increase count per level, flat +5 attribute bonuses, a retuned per-level Health) are a long-standing Oblivion mod category and change nothing here; no log line, no test. The engine's own mod-compatibility rule (every number a load order can author resolves GMST first) is met for the cost and violated for the level, which the same mods retune together.
- **Related**: #6756 (derived-stat rows, closed), #5915, OSL-XP-006, CHAR-D3-02. Dim 2 should confirm the Oblivion derived rows `fPCBaseHealthMult` (exe default 2, CS `FPCBaseHealthMult.wiki`) and `fPCBaseMagickaMult` (Oblivion.esm authors 1.0, scan) in `oblivion_ruleset(resolve)` have the same gap: `profile.rs:880` calls `oblivion_ruleset(resolve)` with no `gmst`, unlike the FNV/FO4 `_with_gmst` builders.
- **Suggested Fix**: Carry `iLevelUpSkillCount`, an `[u8; 10]` bonus row (index = tally 1..=10; tally 0 stays the engine's +1, which no setting covers) and the Health multiplier on `LevelingModel::SkillUse`, overlay them in `with_gmst`, and have `PendingLevel::bonus`/`apply_level`/the menu feed read them through the ruleset; extend `skill_use_costs_overlay_...` with a retuned-GMST test and fix the three docs in the same commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
