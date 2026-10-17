# CHAR-2026-10-16-M-02: Fallout `LevelingModel` hardcodes the level cap and XP curve; the authored `iMaxCharacterLevel` / `iXPBumpBase` GMSTs are ignored (FO3 + Broken Steel capped at 20)

Labels: medium,character,bug,game:fo3,game:fnv
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/leveling.rs:92-107` (`with_gmst` handles only `SkillXp`; `other => other`), `:118-142` (`FO3`/`FNV` consts), consumer `byroredux/src/combat.rs:640`
- **Status**: NEW (the `XpCurve` doc records the DLC-cap deferral, "not wired yet", with no issue, and the deferral predates `grant_experience` making the cap live)
- **Source**: `charal.md:144` ("XP / level curve … AUTHORED — GMST — pending"); `charal-fnv-fo3-ruleset.md:459-460` (FO3 cap 20, "30 with *Broken Steel*"; FNV 30, "50 with the four add-ons"); GECK *Experience Settings* (local `/mnt/data/src/reference/geck-uesp-wiki/(main)/Experience Settings.wiki`): needed XP for level n = `(n−1)·((n−2)/2·iXPBumpBase + iXPBase)`, defaults 200 / 150; differencing gives `150·L + 50`, i.e. `xp_a = iXPBumpBase`, `xp_b = iXPBase − iXPBumpBase`.
- **Description**: Only Skyrim's two curve GMSTs are overlaid. Measured from the masters: `FalloutNV.esm` authors `iMaxCharacterLevel` 30, `iXPBumpBase` 150; `Fallout3.esm` authors `iXPBumpBase` 150 and no cap (exe default 20); **`BrokenSteel.esm` authors `iMaxCharacterLevel` 30**; `Fallout4.esm` authors `iXPBase` 200 (its `fXPMod*` pair is already consumed). `build_character_ruleset` already passes the merged-index `game_setting_float` closure into `build_ruleset`, so the seam is one match arm away.
- **Evidence**: `leveling.rs` `with_gmst`: `Self::SkillXp {..} => …, other => other`; `grant_experience`: `capped = leveling.level_cap() > 0 && progress.level >= leveling.level_cap()`.
- **Impact**: A FO3 load order with `BrokenSteel.esm` stops at level 20 (authored: 30). Any mod retuning `iXPBumpBase`/`iMaxCharacterLevel` is ignored. Not a crash.
- **Related**: M-05 (a retuned curve is exactly what makes the loop hazard reachable for Fallout).
- **Suggested Fix**: Extend `with_gmst`'s `XpCurve` arm (`xp_a = iXPBumpBase`, `xp_b = iXPBase.unwrap_or(200) − xp_a`, `level_cap = iMaxCharacterLevel` when authored, FO3 20 fallback); add a request-list pin like `skyrim_gmst_overlay_reads_only_authored_curve_settings` and a capture line citing the GECK page.
- **Note**: Related closed #2943 (level_cap docstring / DLC bump deferral), which predates `grant_experience` making the cap live.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
