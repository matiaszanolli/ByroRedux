# #3762 — CHAR-2026-08-30-D5-01: every CREA actor's authored DATA.Damage is dropped at the population boundary, so 692 FNV / 186 FO3 creatures attack for the flat 8.0 unarmed baseline

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: medium, character, combat, gameplay, game:fnv, game:fo3, bug

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-30.md` (Dimension 5 — Population Boundary), HEAD `64f64480`
**Finding ID**: `CHAR-2026-08-30-D5-01`

- **Severity**: MEDIUM
- **Status**: NEW

## Location

- `crates/plugin/src/esm/records/actor_value_derive.rs:222-249` — `derive_creature_actor_values`
- `crates/plugin/src/esm/records/actor/mod.rs:281` — `CreatureStats::damage`
- `byroredux/src/combat.rs:318-330` — `attack_damage`

## Description

#3390 gave `CREA` records a stat model that emits their 7 SPECIAL and Health. It deliberately does **not** emit `DATA.Damage` — correctly, because FO3/FNV publish no `AVIF` it maps onto and inventing one would be a guess — and parks it on `CreatureStats::damage` "for a future combat consumer".

That consumer already exists and shipped: `byroredux/src/combat.rs`'s `combat_damage_system` (live since 2026-08-15/16, registered in `boot.rs`). `attack_damage` reads `EquippedWeapon.damage + melee_damage_charal_bonus(...)` when a weapon is equipped and the flat `UNARMED_DAMAGE = 8.0` otherwise.

Nothing anywhere reads `CreatureStats::damage` — `grep -rn 'creature_stats' --include='*.rs'` returns only the parser, `derive_creature_actor_values`, and tests.

## Evidence

Measured over both vanilla masters with a purpose-written probe (a temporary `crates/plugin/examples/_tmp_char_crea_dmg.rs`, run and deleted; tree unchanged):

| | FNV | FO3 |
|---|---|---|
| `CREA` records | 1,578 | 533 |
| authoring a 17-byte `DATA` | 1,578 (100 %) | 533 (100 %) |
| with non-zero `DATA.Damage` | 1,030 | 331 |
| mean non-zero damage | 30.7 | 22.3 |
| max | 500 | 1,000 |
| **non-zero damage AND no inventory `WEAP`** | **692** | **186** |

Samples: `QJDeathclawWanderer02` = 125, `FFEU02DeathClaw` = 100, `FFER01Radscorpion` = 60, `FFER15YaoGuai` = 75. Each of those, having no weapon to equip, resolves through `attack_damage`'s `None => UNARMED_DAMAGE` arm to **8.0** — a Deathclaw hits for 8 instead of 125, a 15.6x shortfall.

The CHARAL `MeleeDamage` bonus does not compensate: `melee_damage_charal_bonus` is inside the `Some(weapon)` arm, so an unarmed creature does not even receive `STR x 0.5`.

Re-verified at HEAD: `byroredux/src/combat.rs:33` `pub(crate) const UNARMED_DAMAGE: f32 = 8.0;` and `:329` `None => UNARMED_DAMAGE`.

## Source

`docs/engine/charal-fnv-fo3-ruleset.md` § Derived statistics — the Fallout family's damage model is `MeleeDamage = STR x 0.5` **additive on a weapon's own damage**, and `UnarmedDamage = ceil((10 + Unarmed)/20)`; neither is a creature's attack damage. `CreatureStats`' own docstring (sourced to xEdit `wbDefinitionsFNV.pas`) states `DATA.Damage` is "the creature's attack damage. Authored here rather than on a weapon (creatures fight unarmed)".

## Impact

Creatures are targetable and hostile — `derive_creature_actor_values` gives them Health, and `ActorVitals` follows — so they participate fully in the shipped melee slice as aggressors, all dealing the same 8 damage regardless of species. Combat balance on FO3/FNV content is uniformly wrong for the 878 measured actors, with no crash, no log line and no failing test.

**The gap widened rather than narrowed with #3390**: before it, creatures had no `ActorValues` at all and so were not melee-eligible; #3390 made them combat participants while leaving the one number that defines their attack unread.

## Related

- #3390 (the `CREA` stat model that created the exposure)
- #3092 (the `MeleeDamageConfig` route this would parallel)
- #2962 (the unresolved "should the shipped combat consumer dispatch per-game into CHARAL math" question the `crates/core/src/combat.rs` module docs raise)

## Suggested Fix

Do **not** invent an AVIF for it. Follow the `MeleeDamageConfig` precedent (#3092): carry `CreatureStats::damage` onto the spawned entity as a small dedicated component (a creature-attack analogue of `EquippedWeapon`), and give `attack_damage` a third arm that prefers it over `UNARMED_DAMAGE` for an actor that carries it.

If that routing is judged to belong to combat rather than CHARAL, the minimum is to file it explicitly — the current state is an unfiled, unbounded deferral inside a live system.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `NPC_` unarmed path, and the FO4/Skyrim creature analogues)
- [ ] **LOCK_ORDER**: If a RwLock scope changes in `combat.rs`, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — a creature with authored `DATA.Damage` and no weapon deals that damage, not `UNARMED_DAMAGE`
