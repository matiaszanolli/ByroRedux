# CHAR-2026-10-16-M-08: Five new character-rule consumers select their per-game rules by `GameKind` instead of `CharacterRulesProfile` / parser data

Labels: medium,character,bug,tech-debt
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM (doctrine; no wrong value today)
- **Dimension**: Ruleset Seam & Doctrine
- **Game**: all
- **Location**: `byroredux/src/encumbrance.rs:84-91` (`install_encumbrance_rules`: `Fallout3NV → SpeedMult`, `Skyrim → NoSprint`, `_ → None`); `byroredux/src/barter.rs:92-111` (`BarterRules::from_index`: skill name, scale, cap, fallback GMST defaults, perk hooks, and a second `match` for `(barter_max, barter_min)`); `crates/scripting/src/bleedout.rs:49` (`ActorProtection::from_acbs_flags`: `!matches!(game, Oblivion | Fallout3NV)` decides whether the Protected ACBS bit exists) and `:143` (`engine_defaults`); `byroredux/src/combat.rs:506` (`install_experience_rules`: `index.game != Fallout3NV`); `crates/scripting/src/perk_entry_points.rs:151,209,239-251,284-342` (per-game entry-point tables)
- **Status**: NEW (all landed in the M47.2 wave: `ed59c81f7`, `61ad95cf3`, `dd843f9e6`, `dbe562c1f`, `0ea4a87d4`)
- **Source**: SKILL Dim 1 doctrine ("data in the tables, never a branch in a consumer"); precedents #4447 (`body_condition_base`) and #4679 (`vital_pools`), both of which moved a consumer-side `GameKind` list into the profile; `charal.md` format-translation rule.
- **Description**: Each is a per-game rule table written as a consumer `match`. The ACBS Essential/Protected decode is the clearest case: a record-format difference that belongs at the parser boundary (`NpcRecord.essential/protected`), not in a scripting-crate constructor taking `GameKind`. The DT and XP-modifier installs gate on authored data (AVIF/GMST presence), which is the preferred shape. `GameKind::Fallout3NV` also conflates FO3 and FNV, where only the profile row discriminates (kill-XP and encumbrance are documented for both, so no behavioural error today). FO4 encumbrance silently has `_ => None` with no capture note saying that is intended.
- **Evidence**: `grep -n GameKind` at the cited lines; none appears in any doctrine pin (`fallout_profiles_keep_roster_health_and_ruleset_in_lockstep` does not see them). Dim 1 re-checked `bleedout.rs:49` and `combat.rs:506` directly.
- **Impact**: A new game or an FO3/FNV divergence needs edits in five scattered places, and none is covered by the profile pins.
- **Related**: #4447, #4679, #4448.
- **Suggested Fix**: Decode Essential/Protected in `parse_npc` per game; move the encumbrance effect, kill-XP applicability, barter row and bleedout defaults onto `CharacterRulesProfile` rows; or at minimum add a test that fails when a new `GameKind` is added without a decision in each table.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
