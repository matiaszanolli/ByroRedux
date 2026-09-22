# GAME-D6-2026-09-21-01: inventory.status reports a weapon damage that combat does not apply

**Issue**: #4711
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 6 — Player feedback (debug frontend)
**Location**: `byroredux/src/commands/gameplay.rs`

## Description
`inventory.status` prints `EquippedWeapon.damage` directly. Combat applies `attack_damage` = weapon + `melee_damage_charal_bonus` (FO3/FNV STR bonus, live for the player since #4458).

## Evidence
`format!(... damage={:.1} source=weapon", weapon.damage)` vs `combat.rs`'s `attack_damage`.

## Impact
Operators/smokes read a damage number combat never deals.

## Related
#3092; CHAR-D4-2026-09-21-01.

## Suggested Fix
Print `crate::combat::attack_damage(world, player)` instead of the raw field.
