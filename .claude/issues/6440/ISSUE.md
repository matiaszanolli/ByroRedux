# CHAR-2026-10-27-M-01: VATS same-target fold applies FNV Damage Threshold once to the summed shots, not once per shot

Labels: medium, character, combat, game:fnv, bug
Source: docs/audits/AUDIT_CHARACTER_2026-10-27.md

**Source**: `docs/audits/AUDIT_CHARACTER_2026-10-27.md` finding `CHAR-2026-10-27-M-01` (MEDIUM)
**Location (re-mapped to current tree)**: `byroredux/src/systems/vats.rs` (`vats_confirm` fold: `landed` -> `hits` -> one `HitEvent { damage: shot * shots }`, ~:607-656); consumer `byroredux/src/combat.rs` (`combat_damage_system` -> `apply_damage_threshold`, ~:359 and :834-846); test `landed_shots_at_one_target_sum_into_one_hit_event` (`vats.rs`, ~:966)

**Game**: FNV. FO3 has no `DamageThresholdRules`; its percentage DR is linear, so the fold is harmless there.
**Introduced by** `1fb7e6f56` (Refs #6422), the fix for #6422's "same-target shots overwrite each other". #6422 is still open (Concentrated Fire half); this is a distinct defect in that fix, filed separately so it can be closed on its own.

## Source
DT is a per-shot defence.
- GECK *fMinDamMultiplier*: "the proportion of damage an actor takes **from a shot** that would be negated by their Damage Threshold".
- GECK *Ammo*, "# Proj per Shot": "the weapon's damage figure is divided amongst all of the projectiles ... higher projectile counts may not be able to bypass Damage Threshold".
(`/mnt/data/src/reference/geck-uesp-wiki/(main)/`)

## Description
`HitEvent` holds one component per entity. Rather than letting later shots overwrite earlier ones, `vats_confirm` counts landed shots per target and writes a single event whose `damage` is `per_shot x shots` (comment: "The target's own defenses then apply once to that total in the combat funnel"). For FNV, `combat_damage_system` then computes `max(N*d - DT, N*d*m)` (`apply_damage_threshold`: `(damage - threshold).max(damage * rules.min_damage_mult)`) where vanilla computes `N * max(d - DT, d*m)`. DT is subtracted once instead of N times, so the per-shot `fMinDamMultiplier` floor never engages where it should.

## Evidence
With `fMinDamMultiplier` 0.2:
- Three landed 10-damage shots at a DT-15 target: vanilla 3 x max(-5, 2) = **6**; folded max(30 - 15, 6) = **15** (2.5x).
- Three 20-damage shots at DT 15: vanilla 3 x 5 = **15**; folded max(60 - 15, 12) = **45** (3x).
The error grows with shot count and is largest where DT matters: weak weapons vs armoured targets. The existing test's fixture installs no `DamageThresholdRules`, so it cannot see this.

## Impact
Multi-shot VATS against an armoured FNV target overstates damage two- to three-fold, changing kill outcomes (which persist through `Dead`, kill XP, crime hooks). One `HitEvent` also means one ObScript `OnHit` run instead of one per landed shot. Reachable today via `vats.queue` xN + `vats.confirm` and the smoke driver; player-reachable once a VATS menu is wired. (The previous behaviour under-delivered; this over-delivers.)

## Related
#6422 (open), #6350, #6403; ESM-2026-10-27-03 (the `vats.damage_mult` feeding `weapon_mult` is read from a mis-decoded sub-record). Funnel change belongs to `/audit-gameplay`.

## Suggested Fix
Carry per-shot damage plus a `shots: u32` count (default 1) on `HitEvent`; `combat_damage_system` applies `apply_damage_threshold` per shot and multiplies. Alternative: sequence same-target shots across frames. Add a DT-installed leg to the fold test asserting 6, not 15, for the first example.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

