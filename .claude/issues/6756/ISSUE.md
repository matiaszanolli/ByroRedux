# CHAR-2026-11-07-M-01: The FO3/FNV/FO4 derived-stat rows hardcode coefficients the masters author as GMSTs, so a retuned load order is ignored

**Issue**: #6756
**Labels**: bug, medium, game:fnv, game:fo3, game:fo4, character
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding M-01, Dim 2). Game: FO3, FNV, FO4.

## Description
Since #5915, #5238, #6424, #6427 and #6567 a number a load order can author resolves load-order-merged GMST, then a compiled-in default (`LevelingModel`, `NpcHealthCurve`, `VatsModel`, `CrimeModel`, `ArmorModel` all have `with_gmst`). The derived-stat table is the one CHARAL family left out: each row is `DerivedStatFormula::affine/bilinear` with the vanilla number baked in, and the builders take only the EditorID resolver. Every baked value equals its vanilla master value, so nothing is wrong on a vanilla load order.

## Location
`crates/core/src/character/fallout.rs:61-101` (`add_fnv_fo3_shared`: Carry Weight, Melee, Unarmed), `:142-169` (`fallout4_ruleset`), `:174-243` (`fallout3_ruleset`, `falloutnv_ruleset`: Health, AP). Build site that already holds a GMST closure: `byroredux/src/npc_spawn/stamps.rs:252-258` (`build_ruleset` passes `gmst`, used for leveling only).

## Evidence
`fn fallout3_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F)` has no `gmst` parameter; `fallout.rs:65` `affine(av(s), 10.0, 150.0)`, `:187` `affine(av(a), 2.0, 65.0)`, `:181` `bilinear(av(e), 20.0, LEVEL, 10.0, 0.0, 90.0)`; no `game_setting_float` use in `fallout.rs`.

Source: `charal-fo4-ruleset.md:149-150` ("AUTHORED GMSTs ... read not hardcoded") and `:262-268`; `charal.md:143` and `:665-670` (names only `fAVDSkill*` as the remaining seam). Byte scan, FalloutNV/Fallout3: `fAVDActionPointsBase` 65, `fAVDActionPointsMult` 3/2, `fAVDCarryWeightsBase` 150 (sic), `fAVDCarryWeightMult` 10, `fAVDHealthEnduranceMult` 20, `fAVDHealthLevelMult` 5/10, `fAVDMeleeDamageStrengthMult` 0.5, `fAVDUnarmedDamageBase` 0.5, `fAVDUnarmedDamageMult` 0.05; Fallout4: `fAVDCarryWeightBase` 200, `Mult` 10, `fAVDActionPointsBase` 60, `Mult` 10, `fAVDMeleeDamageMult` 0.1.

## Impact
A carry-weight, AP or health-curve rebalance mod (a common category) changes nothing for the player's Health/AP (re-stamped every frame, #5039), Carry Weight (any actor via `GetActorValue`) or Melee/Unarmed bonuses. No crash, log line or test. FO3/FNV Health folds `100 - levelMult` into the bias (90/95) and no master authors the 100, so the overlay must keep the engine-supplied 100.

## Related
#6567, #5915, #5238, #6424, #6427, #3170. Earlier unfiled mentions: `AUDIT_CHARACTER_2026-10-05.md:200`, `-10-16.md:346`.

## Suggested Fix
Give the builders a `gmst` closure and overlay the named settings with today's numbers as fallbacks (a `DerivedCoefficients` row on the profile, or `with_gmst` on the per-game constants). Read the FO3/FNV Carry Weight base as `fAVDCarryWeightsBase` (the master's spelling). Fix `charal-fnv-fo3-ruleset.md:131` and `charal.md:665-670` in the same commit and name the five GMSTs the capture omits.

## Completeness Checks
- [ ] **SIBLING**: every derived row (Health, AP, Carry Weight, Melee, Unarmed, Crit) across FO3/FNV/FO4 considered, not only Carry Weight
- [ ] **TESTS**: a retuned-GMST test pins the overlay, like `a_retuned_scaling_factor_reaches_the_damage_pipeline`
