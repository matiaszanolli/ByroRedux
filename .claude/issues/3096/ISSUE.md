# SUBSYS-2026-08-16-01: weapon reach and speed have no canonical landing site

**Issue**: #3096
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,gameplay,bug`
**Source report**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md` (subsystem-gap sweep).

**Location**: `crates/plugin/src/esm/records/items.rs`:183-184 (Oblivion `speed`/`reach` read into `_`-prefixed bindings and dropped), :215-216 (Skyrim's 100-byte `DNAM` "not decoded yet"), :98-119 (`ItemKind::Weapon` — ten fields, none of them `reach` or `speed`) · `crates/core/src/ecs/components/inventory.rs`:142-146 (`EquippedWeapon` carries `damage` only) · `byroredux/src/combat.rs`:23, 26

## Description

Authored weapon **reach and speed have no canonical landing site**. Every melee weapon in every game therefore has identical reach and swing cadence.

The data is parsed and discarded: Oblivion's `DATA` reads `speed`/`reach` into `_`-prefixed bindings; Skyrim's 100-byte `DNAM` is explicitly not decoded; `ItemKind::Weapon` has ten fields and neither of these; `EquippedWeapon` carries `damage` alone.

`byroredux/src/combat.rs` compensates with two global constants — `MELEE_REACH_BU = 180.0` and `MELEE_COOLDOWN_SECONDS = 0.45`.

## Impact

A dagger and a warhammer have the same reach and the same swing rate. This is a **canonical-model gap**, not a parser bug: the values exist on disk for at least Oblivion, and #2992's validated FO4 `DNAM` layout puts reach at offset 12 and speed at offset 4 — so the FO4 half is already unblocked.

Melee feel is the P2 slice's core loop, and it is currently weapon-independent.

## Suggested Fix

Add `reach` and `speed` to `ItemKind::Weapon` and to `EquippedWeapon`, populate them per game (Oblivion's `DATA` already reads them; FO4's offsets are known from #2992), and have `combat.rs` prefer the equipped weapon's values over the global constants.

Keep the constants as the unarmed/fallback path.

## Related

- **#2992 (FO4-D6-01 — its validated 132-byte `DNAM` layout supplies FO4's reach/speed offsets)**
- #3032 (ECS-04 — `EquippedWeapon` has no runtime writer, needed for this to be player-visible)
- #2976 (the `Block` action, same combat slice)

## Completeness Checks
- [ ] **CANONICAL-SINK**: `reach`/`speed` land on the canonical item model, not on a per-game struct
- [ ] **SIBLING**: All games' parsers populate them, or explicitly record which do not and why
- [ ] **FALLBACK**: The global constants remain the unarmed path rather than being deleted
- [ ] **TESTS**: A regression test asserts two weapons with different authored reach behave differently

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3096 --json state` when live state is needed.*
