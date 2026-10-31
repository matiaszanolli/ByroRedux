# CHAR-2026-10-31-M-01: Skyrim armor mitigation hardcodes five authored GMSTs and gates on `GameKind::Skyrim` in the consumer

Labels: medium,character,bug,game:skyrim

Source: `docs/audits/AUDIT_CHARACTER_2026-10-31.md` (CHAR-2026-10-31-M-01). Severity MEDIUM (mod-compatibility: vanilla values right, a retuned load order is silently ignored). Dimension: Ruleset Seam & Doctrine. Game: Skyrim. Introduced by `31aca901d`.

**Location**: `byroredux/src/armor.rs`: consts at lines 28-37 and the `index.game == GameKind::Skyrim` test in `install_armor_book` (line 66). Consumer: `byroredux/src/combat.rs:404` (`crate::armor::mitigate`).

## Source
`Skyrim.esm` authors these (byte-scanned): `fArmorScalingFactor` 0.12, `fArmorBaseFactor` 0.03, `fMaxArmorRating` 80, `fArmorRatingPCMax` 1.4, `fArmorRatingMax` 2.5, `fArmorRatingBase` 1.0. CK *Settings* page (`/mnt/data/src/reference/ck-uesp-wiki/Category/Settings.wiki` lines 317-327, 2791): base factor "each piece of armor worn provides a hidden 3% damage reduction"; PC/NPC Max "multiplied by this number ... at 100 skill".

## Description
The per-game seam is data in `CharacterRulesProfile`, and every number a load order can author resolves load-order-merged GMST then a compiled-in default (`CrimeModel::with_gmst`, `VatsModel::with_gmst`, `BarterModel`; the #6424/#6427 class). `armor.rs` instead holds `REDUCTION_PER_RATING`, `HIDDEN_PER_PIECE`, `MAX_REDUCTION`, `SKILL_COEFF_PLAYER`, `SKILL_COEFF_NPC` as bare consts, none read from `EsmIndex::game_setting_float` (a grep for `fArmor|fMaxArmor|ArmorScaling` finds nothing). Whether the model applies is decided by `index.game == GameKind::Skyrim` inside the consumer - the doctrine regression #5921 (`3162b21b8`) retired for the encumbrance, barter, experience, bleedout and perk-entry tables.

## Impact
Armor-rebalance mods and Creation Club armor changes retune these settings; with them ignored the reduction is wrong on those load orders. A future game sharing the armor model needs another consumer branch instead of a profile row.

## Suggested Fix
Add an `ArmorModel` row to `CharacterRulesProfile` (`Some(...)` on `SKYRIM` only) carrying scaling factor, base factor, cap and PC/NPC skill ranges with a `with_gmst` overlay. Install it as a resource like `build_vats_model` / `install_crime_model`, and make `ArmorBook` populate when the row exists. Pin it in `crime_and_radiation_gauge_rows_are_stated_on_every_profile`'s successor and add a `with_gmst` test.

Related: CHAR-2026-10-31-L-01 (armor constants duplicated); related #5921.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
