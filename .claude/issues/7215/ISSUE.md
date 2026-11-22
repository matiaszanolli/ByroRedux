# #7215 CHAR-2026-11-21-L-18: `docs/feature-matrix.md` CHARAL section (and the ROADMAP residue bullet) still rates Oblivion "unwired", with no stealth, crime, disposition, cast, bow, block, fall or skill-use rows

**Labels**: low,character,documentation,doc-rot,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D5-01
- **Severity**: LOW (doc rot, but it is the project's public status table and every Oblivion cell in it is wrong)
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion (also fo3/fnv/fo4/skyrim for the missing rows)
- **Location**: `docs/feature-matrix.md` "Character / Progression (CHARAL)" table and the prose beneath it; the "What Doesn't Work Yet" rows "CHARAL: player actor-value seed" and "CHARAL: Oblivion ruleset built but unwired ..." (section header says "live gaps as of 2026-10-22"); `ROADMAP.md` "CHARAL runtime-dead surface" bullet ("FO3, FNV, Skyrim and FO4 reach actors")
- **Status**: NEW (same family as closed #2961, #3768, #6769; those were earlier wiring events)
- **Description**: The matrix was last touched by `e7e753505` (a video commit) and was not updated by #7009-#7011, #7037-#7056 or M65 Phase 5/6. Each claim below is contradicted by code:
  - "Ruleset wired Oblivion: ~ built, unwired" and the paragraph "`CharacterRulesProfile::OBLIVION` still maps to `RulesetBuilder::None`, so it never reaches a live actor". Contradicted by `CharacterRulesProfile::OBLIVION` (`ruleset: RulesetBuilder::Oblivion`), the `RulesetBuilder::Oblivion => oblivion_ruleset(resolve)` arm of `build_ruleset`, and the test `oblivion_builds_its_runtime_ruleset`.
  - "NPC actor-value population Oblivion: ✗" and "Creature (`CREA`) Oblivion: ✗ `CREA.DATA` layout unsourced". Contradicted by `npc_stats: NpcStatModel::OblivionNpc` and `creature_stats: NpcStatModel::OblivionCreature`, and `oblivion_population::derive` (ROADMAP's own Oblivion row reports 2 482 / 2 482 `NPC_` and 914 / 914 `CREA`).
  - "Runtime leveling Oblivion: ✗" and the prose "Oblivion, FO76 and Starfield still have no live leveling path". Contradicted by `crates/scripting/src/oblivion_skill_use.rs` (`apply_pending_level`, `apply_level_menu_picks`) and `byroredux/src/oblivion_rest.rs`.
  - "Pool regen tick Oblivion: ✗ inert" and "the Fatigue/Magicka lanes are inert everywhere". Contradicted by `build_tes_pool_regen` (`profile.rs`) installed at `cell_loader/references/mod.rs` for `NpcStatModel::OblivionNpc`.
  - "What Doesn't Work Yet: player actor-value seed ... Oblivion no" and "Oblivion ruleset built but unwired; Fatigue/Magicka regen ... inert everywhere". Same code.
  - The table has no row at all for: M65 stealth/detection (six `StealthKind` rows; line ~260 even says "Not yet: detection meter"), crime (`CrimeModel::OBLIVION` / `SKYRIM`), starting disposition (`build_disposition`), Oblivion player cast / bow / block / fall / armor-rating (`install_*_model`), or Oblivion skill-use leveling. `grep -i 'stealth\|crime\|disposition\|archery\|spellcast' docs/feature-matrix.md` finds none in the CHARAL section.
  - `ROADMAP.md` "CHARAL runtime-dead surface": "FO3, FNV, Skyrim and FO4 reach actors" omits Oblivion, whose runtime ruleset has reached actors since #7010; the same ROADMAP's Oblivion paragraph (`Oblivion's actor values are live`) says so.
- **Evidence**: see the symbols above.
- **Impact**: A reader sizing Oblivion work concludes the game has no character runtime; a reader sizing M65 or crime sees no row for them.
- **Related**: CHAR-D3-05 and CHAR-D4-08 (same wiring event, different files), #7007 (tracking), #6769.
- **Suggested Fix**: Re-derive the table from the Coverage Matrix above: flip the four Oblivion cells, add rows for stealth (six games), crime (Oblivion, Skyrim), disposition, cast/bow/block/fall/armor-rating and skill-use leveling, rewrite the two prose paragraphs and the two "Doesn't Work Yet" rows (move the date), and add Oblivion to the ROADMAP bullet.

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
