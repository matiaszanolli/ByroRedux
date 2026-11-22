# #7219 CHAR-2026-11-21-L-22: `CharacterRulesProfile` accessor and field docs lag the rows they describe (stealth, crime, creature stats, essential time)

**Labels**: low,character,documentation,doc-rot

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D5-05
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: all (fo4/starfield for stealth, skyrim for crime, oblivion for creature stats)
- **Location**: `crates/core/src/character/profile.rs`: the `stealth` field doc (~:349-353: "Fallout 4 and Starfield wait on a source (Q2), so they answer from the sight test"); the `crime()` accessor doc (~:771: "`None` where no crime model is sourced and wired (every game but Oblivion)"); the `creature_stat_model()` doc (~:729: "[`NpcStatModel::None`] ... on Oblivion, whose `CREA` `DATA` is a different struct this has not been sourced for"); the `essential_death_time_secs()` accessor doc (~:758: "`None` where the game has no bleedout recovery")
- **Status**: NEW
- **Description**: Each is contradicted by a row in the same file:
  - `FALLOUT4.stealth` is `StealthKind::Fallout4` and `STARFIELD.stealth` is `StealthKind::Starfield` (M65 #6996 / #7046). Only `FALLOUT76` and `NONE` carry `StealthKind::SightOnly`.
  - `SKYRIM.crime` is `Some(CrimeModel::SKYRIM)`; the Oblivion capture was already corrected to "Oblivion and Skyrim" (#6766), but the accessor doc was not.
  - `OBLIVION.creature_stats` is `NpcStatModel::OblivionCreature`, wired by #7010.
  - The `essential_death_time_secs` field doc says `None` is not "the game has no bleedout recovery" (#6259: Starfield authors recovery data instead), and the accessor doc one screen below states the opposite.
- **Evidence**: `FALLOUT4` / `STARFIELD` / `SKYRIM` / `OBLIVION` const literals in `profile.rs`.
- **Impact**: the accessor docs are what a consumer author reads; they will wrongly conclude FO4/Starfield have no detection formula or that Oblivion creatures are unpopulated.
- **Related**: CHAR-D5-07 (the matching prose in `sneak-detection.md`).
- **Suggested Fix**: Reword the four docs to match the rows (state the rows by value, not by game list), or phrase them as "see the row".

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
