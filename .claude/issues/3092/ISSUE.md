# CHAR-2026-08-16-D1-01: the P2 melee slice is a CHARAL non-consumer

**Issue**: #3092
**Severity**: MEDIUM
**Labels**: `medium,gameplay,combat,bug`
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CHARACTER_2026-08-16.md` (Dimension 1 — CHARAL consumption).

**Location**: `byroredux/src/combat.rs`:30 (`UNARMED_DAMAGE`), :269-273 (`attack_damage`)

## Description

The P2 melee slice is a **CHARAL non-consumer**. `attack_damage` reads `EquippedWeapon.damage` or the flat `UNARMED_DAMAGE = 8.0` constant — it never routes through the character ruleset. And `DerivedOutput::Multiplier` still has no reader.

## Impact

CHARAL exists to translate per-game character *rules* into canonical ActorValues, Level and Perks. The first gameplay system to actually need derived combat numbers bypasses it entirely and uses flat constants.

Concretely: Strength contributes nothing to melee damage on any game, because the FO3/FNV and FO4 Melee Damage formulas CHARAL computes have no consumer. The layer is built and unwired at the exact point it was built for.

## Suggested Fix

Route `attack_damage` through the actor's `CharacterRuleset`-derived values, falling back to `UNARMED_DAMAGE` only when no derived value resolves.

**Blocked on prerequisites**: #2986 (FO3/FNV actors get no ActorValues at all), #3004 (no Health term in the derivation) and #3093 (the FO4 Melee Damage row keys on an AVIF vanilla does not author). Landing this without those leaves the derived path resolving nothing.

## Related

- #2986, #3004 (FO3/FNV actor values absent), #3093 (FO4 Melee Damage row)
- #2992 (FO4 weapon damage is zero — the other input to this calculation)
- #3032 (ECS-04 — `EquippedWeapon` has no runtime writer)

## Completeness Checks
- [ ] **PREREQS**: #2986 / #3004 / #3093 resolved first, or the derived path is verified to resolve
- [ ] **NO-GUESSING**: Formulas come from the per-game `charal-*-ruleset.md` docs, not invented
- [ ] **MULTIPLIER-READER**: `DerivedOutput::Multiplier` gains a consumer or is documented as deferred
- [ ] **SIBLING**: Other flat combat constants in `combat.rs` audited for the same bypass
- [ ] **TESTS**: A regression test asserts Strength changes melee damage

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3092 --json state` when live state is needed.*
