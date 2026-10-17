# CHAR-2026-10-16-L-09: `docs/feature-matrix.md` and the `character/mod.rs` docstring still say there is no runtime leveling

Labels: low,character,documentation,doc-rot
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW (doc rot)
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `docs/feature-matrix.md:271` (row "Runtime leveling (XP grant / level-up)": `✗` for all seven games) and `:309-310` ("Leveling … uniformly `✗` … `CharacterLevel.xp` is stamped `0` at spawn and nothing increments it"); `crates/core/src/character/mod.rs:38-40` (the `components` bullet omits `LevelUpPoints`, added to the re-exports at `:95`)
- **Status**: NEW (the M47.2 runtime landed 2026-10-12/13)
- **Description**: `grant_experience` (`byroredux/src/combat.rs:619`) increments `CharacterLevel.xp`, rolls levels to the cap and banks `LevelUpPoints`, reachable from kill XP, `RewardPlayerXP`, `levelup xp` and (Skyrim) `Game.AdvanceSkill`. The matrix row should be ✓ for FO3/FNV, ~ for FO4 and Skyrim (see the Coverage Matrix above) and ✗ for Oblivion/FO76/Starfield. The regen/affliction sentences in the same paragraph remain true.
- **Impact**: The status floor that other audits and readers trust says the opposite of the code.
- **Suggested Fix**: Update the row and the paragraph; add `LevelUpPoints` to the docstring bullet.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).
