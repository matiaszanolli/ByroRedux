# CHAR-2026-11-07-L-03: combat.rs rustdoc tells callers to multiply by the weapon-condition ratio, which is not the sourced factor

**Issue**: #6762
**Labels**: documentation, low, game:oblivion, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-03, Dim 2). Game: Oblivion.

## Description
`oblivion_weapon_damage_multiplier` returns `0.5·(0.75+0.005A)·(0.2+0.015·MS)` (numbers correct); the leading 0.5 is `fDamageWeaponMult`. Its rustdoc tells the caller to multiply by "the weapon-condition ratio", but the condition term is a separate factor `0.5 + 0.5·ratio`. A caller following the doc multiplies by the raw ratio `r` and gets `0.5·r` where the source gives `(r+1)/4` (half the damage at r=0 instead of a quarter, a different slope throughout).

## Location
`crates/core/src/combat.rs:45-60`.

## Evidence
Source: `charal-oblivion-ruleset.md:357-358` ("`(0.5 + WeaponHealth/BaseWeaponHealth × 0.5)`") and `:278`; UESP *The Complete Damage Formula*, Modifiable Settings (`fDamageWeaponConditionBase` 0.5, `fDamageWeaponConditionMult` 0.5). `oblivion_weapon_damage_multiplier` has zero callers today.

## Impact
None today; a future Oblivion melee consumer that follows the doc under-damages worn weapons.

## Suggested Fix
Reword to "multiply by `fDamageWeaponConditionBase + ratio · fDamageWeaponConditionMult` (0.5 / 0.5)", or take the factor as a parameter and pin the `(r+1)/4` identity.

## Completeness Checks
- [ ] **SIBLING**: `oblivion_hand_to_hand_damage` and the module header carry no similar condition claim
- [ ] **TESTS**: a pin for the `(r+1)/4` identity if the factor becomes a parameter
