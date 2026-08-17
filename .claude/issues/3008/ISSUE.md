# RT-2026-08-16-09: p2-melee-core.sh asserts none of the fixture identity its spec claims, pins unarmed fallback as a literal

**Issue**: #3008
**Severity**: MEDIUM
**Dimension**: Playable-slice gate semantics
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (Dimension — Playable-slice gate semantics).

**Location**: `docs/smoke-tests/p2-melee-core.sh`:130, :156 · `docs/engine/p2-combat-fixture.md`

## Description

`docs/engine/p2-combat-fixture.md`'s closure gate states *"The smoke also asserts the frozen reference/base FormIDs and weapon family at preflight."*

The script asserts the placed reference `0x0380B4` and **nothing else** — not the base NPC `000E9895`, and neither concrete weapon leaf (`0001CB64` Draugr Battleaxe 18, `000236A5` Draugr Greatsword 17).

What it *does* assert about damage is `grep -Fq "damage=8.0"` on all seven swings — the `UNARMED_DAMAGE` constant, which `combat.rs`:269-273 returns **only when the aggressor has no `EquippedWeapon`**. The gate therefore encodes the *absence* of the weapon family as a pass condition.

## Evidence

The gate's own passing run asserts `damage=8.0` seven times and `health_after` `42.0 → -6.0` in 8.0 steps. `attack_damage` is `world.get::<EquippedWeapon>(aggressor).map_or(UNARMED_DAMAGE, …)`, so `damage=8.0` for every swing is **positive runtime proof that the player carries no weapon in this fixture**.

Re-verified 2026-08-17:
```
$ grep -rn "000E9895\|0001CB64\|000236A5" docs/smoke-tests/
(none)
```
Those IDs appear in `docs/engine/p2-combat-fixture.md` and in no smoke script.

## Impact

Two ways to fail:

1. The fixture doc **overstates** what the gate checks, so a content or FormID drift on the base NPC or the weapon leaves would pass.
2. The literal `damage=8.0` makes the gate **a lock on the current broken state**: any fix that gives the player an authored weapon — exactly what the sweep's player-loadout finding calls for — turns this gate RED, and the natural reading of that red will be *"the loadout fix broke combat"* rather than *"the gate asserted the fallback"*.

## Suggested Fix

Assert the base NPC and both weapon leaves at preflight as the fixture doc claims, and replace the literal `damage=8.0` with a check that damage matches the player's **resolved** `EquippedWeapon` damage or the documented unarmed rule — so the gate tracks the contract rather than the current value.

## Related

- #3000 (RT-2026-08-16-01 — the same gate's missing groundedness assertion)
- `AUDIT_ECS_2026-08-16` § ECS-2026-08-16-04 (the `EquippedWeapon` write-path gap)
- The sweep's `PLAYER_BASE_FORM_ID` finding (not separately filed)

## Completeness Checks
- [ ] **DOC-TRUTH**: `p2-combat-fixture.md`'s claim matches what the script asserts, in whichever direction is chosen
- [ ] **CONTRACT-NOT-VALUE**: The damage assertion tracks the rule, not the literal 8.0
- [ ] **NOT-A-LOCK**: Landing a player loadout does not turn this gate RED for the wrong reason
- [ ] **TESTS**: The gate fails on a base-NPC or weapon-leaf FormID drift

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3008 --json state` when live state is needed.*
