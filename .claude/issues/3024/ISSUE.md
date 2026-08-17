# SAVE-D4-02: EquippedWeapon.inventory_index has no validation

**Issue**: #3024
**Severity**: MEDIUM
**Dimension**: 4 — pre-write validation gates
**Labels**: `medium,ecs,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 4 — pre-write validation gates).

**Location**: `crates/core/src/ecs/components/inventory.rs`:142-146 (`EquippedWeapon`), registered `byroredux/src/save_io.rs`:250 and overlaid :101 · `crates/save/src/validate.rs`:184-218 (`validate_equipment`, which checks only `EquipmentSlots`)

## Description

`EquippedWeapon.inventory_index` is a **new save-participating intra-entity reference with no validation**, while its structural twin `EquipmentSlots.occupants` has a dedicated check in the same function.

## Evidence

```
$ grep -n "EquippedWeapon" crates/save/src/validate.rs
(absent)
```

`validate_equipment` (`validate.rs`:184-218) validates `EquipmentSlots` occupancy and nothing else. `EquippedWeapon` is registered for save at `save_io.rs`:250 and participates in the mutable delta overlay at :101. Re-verified 2026-08-17.

## Impact

An `inventory_index` pointing past the end of the entity's `Inventory` — or at a different item after a reload reorders it — is written to disk unchecked. The consumer (`byroredux/src/combat.rs`:269, `attack_damage`) reads `EquippedWeapon.damage`, so a stale index silently arms the actor with the wrong item or none.

The asymmetry is the tell: the sibling reference in the same subsystem *is* validated, so the omission reads as an oversight rather than a decision.

## Suggested Fix

Extend `validate_equipment` to bound-check `inventory_index` against the entity's `Inventory` length and, if item identity matters, verify the referenced item as well.

## Related

- #3023 (SAVE-D4-2026-08-16-01 — three more unvalidated carriers, same dimension; fix together)
- #2992 (FO4 weapon stats decode to zero — the same `EquippedWeapon` consumer)

## Completeness Checks
- [ ] **SIBLING**: Fixed alongside #3023 as one validation pass
- [ ] **SYMMETRY**: `EquippedWeapon` gets parity with `EquipmentSlots.occupants`
- [ ] **BOUND-CHECK**: The index is validated against the actual `Inventory` length
- [ ] **TESTS**: A regression test writes an out-of-range index and asserts rejection

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3024 --json state` when live state is needed.*
