# ECS-2026-08-16-04: native inventory can never equip a weapon; EquippedWeapon has no runtime writer

**Issue**: #3032
**Severity**: MEDIUM
**Dimension**: 7 — Component Lifecycles
**Labels**: `medium,ecs,gameplay,bug`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 7 — Component Lifecycles, P2 gameplay slice).

**Location**: `byroredux/src/inventory.rs`:106, :319-370 · `byroredux/src/combat.rs`:269-273

## Description

Two independent defects that compound:

1. `describe_kind` returns `equip_slot_mask: None` for `ItemKind::Weapon` (only `ItemKind::Armor` ever produces `Some`). `apply_action`'s `ToggleEquip` bails with `MutationResult::Unavailable` whenever the mask is `None`, so **a weapon row in the menu is inert**.
2. `EquippedWeapon` — the component `combat::attack_damage` reads to decide melee damage — is written at exactly **two spawn-time sites** and never mutated afterwards.

`EquipmentSlots` and `EquippedWeapon` are therefore two disjoint views of "what is equipped", and only the former is player-mutable.

## Evidence

```rust
// inventory.rs:106
ItemKind::Weapon { damage, .. } => ("Weapon", format!("Damage {damage}"), None),
```
```rust
// combat.rs:269-273
fn attack_damage(world: &World, aggressor: EntityId) -> f32 {
    world.get::<EquippedWeapon>(aggressor)
        .map_or(UNARMED_DAMAGE, |weapon| weapon.damage.max(0.0))
}
```

Re-verified 2026-08-17: a workspace grep for `EquippedWeapon` insert/remove outside tests returns **nothing** in `byroredux/src` beyond the two spawn-time writers (`inventory::attach_to_player`, `npc_spawn.rs`:783).

## Impact

The P2 slice's only damage lever is decided **once**, by `prefer_weapon`'s "highest base damage, lowest FormID breaks ties" heuristic at player spawn.

- Selecting a different weapon in the menu cannot change melee damage
- Unequipping a weapon leaves `EquippedWeapon` intact, so the player still swings for its damage
- `docs/smoke-tests/p2-melee-core.sh` cannot see any of this — it asserts `damage=8.0`, the *unarmed* path (see #3008)

Compounds with #2992: on FO4 every weapon's damage is zero, so `prefer_weapon` degenerates to lowest-FormID *and* the result is unchangeable.

## Suggested Fix

Give `ItemKind::Weapon` a weapon-slot mask in `describe_kind`, and have `apply_action` reconcile `EquippedWeapon` with the weapon slot on **both** the equip and unequip branch — so `EquipmentSlots` stays the single source of truth and `EquippedWeapon` becomes its derived cache.

## Related

- #3008 (RT-2026-08-16-09 — the gate that asserts the unarmed fallback and so cannot observe this)
- #2992 (FO4-D6-2026-08-16-01 — zero weapon damage, which compounds it)
- #3024 (SAVE-D4-2026-08-16-02 — `EquippedWeapon.inventory_index` validation)
- `docs/engine/playable-vertical-slice.md` (corpse loot will need this same write path)

## Completeness Checks
- [ ] **SINGLE-SOURCE**: `EquipmentSlots` is authoritative; `EquippedWeapon` is derived, not a parallel truth
- [ ] **BOTH-BRANCHES**: Equip *and* unequip reconcile the component
- [ ] **SIBLING**: Armor's existing mask path checked for the same reconcile gap
- [ ] **GATE**: #3008's gate updated so it can actually observe an equipped weapon
- [ ] **TESTS**: A regression test equips, unequips, and asserts damage tracks both

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3032 --json state` when live state is needed.*
