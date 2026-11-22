# #7217 CHAR-2026-11-21-L-20: The Oblivion capture and `skill-use.md` still describe a pre-wiring Oblivion: no ruleset, inert regen, skill-use "Not built", no level-up, 8 of 21 skills fed

**Labels**: low,character,documentation,doc-rot,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion
- **Location**: `docs/engine/charal-oblivion-ruleset.md`:
  - `:9-12` (header: "builder complete but unwired, `RulesetBuilder::None` … needs a legacy actor-value resolver first");
  - `:605-619` and `:765-766` (regen "no-ops today … inserted by no Oblivion path"; "`pool_regen_tick_system` always passes `false` today");
  - `:659` ("Oblivion has no wired ruleset today … it matters when #3768 lands");
  - `:1009` (skill-use "Not built: the PRD … owns the build");
  - `:1326` and `:1364` ("cannot happen until Oblivion has a level-up");
  - `:1366-1370` ("the engine reads no attack damage from this model").

  `docs/engine/skill-use.md:3` ("the emitters … feed 8 of the 21 skills").
- **Status**: NEW (merges CHAR-D5-03, items 1, 3 and 5 of CHAR-D3-05, and items d-e of CHAR-D4-08)
- **Description**: Each spot is contradicted by code at HEAD:
  - **Ruleset**: `RulesetBuilder::Oblivion` (#7010).
  - **Regen**: `CharacterRulesProfile::build_tes_pool_regen`, installed on the first cell load. The tick computes `stunted` from `stunted_magicka_avif`.
  - **Skill-use and level-up**: `oblivion_skill_use::apply_level`, `250a5ba67`.
  - **Attack damage**: `stamp_creature_attack` stamps `CreatureAttack` (since 2026-11-15).
  - **Fed skills**: `FED_SKILLS` holds 16 codes. The same document's later section says 16 of 21.

  The `:659` note matters beyond wording: the `.player_only()` scope of the three pool rows is now observable on a live actor path. `:1326` / `:1364` should also say that already-spawned PC-offset actors keep spawn-time stats until their cell reloads. That divergence is declared by the skill-use PRD (row 5, OSL-NG-003, owned by #7022/#7040).
- **Impact**: the capture opens by saying the layer it captures is unwired and contradicts itself about 1,000 lines later; `/audit-character` reads it first.
- **Related**: L-19, L-21, L-09 (the attack-damage gap itself), M-05 (the regen spelling), #7067 and #7094 (closed doc fixes of the same family).
- **Suggested Fix**: reword the header to "wired by #7010"; rewrite the regen paragraphs around `build_tes_pool_regen`; mark skill-use and the level-up built (#7050-#7056) and cite OSL-NG-003 at `:1326`/`:1364`; correct the attack-damage sentence; change `skill-use.md:3` to 16 of 21.

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
