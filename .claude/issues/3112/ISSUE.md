# Issue #3112: WEAPON_EQUIP_SLOT = 31 is not a free bit — it is Skyrim/FO4 BOD2 body-part 61 (FX01), so an armor in that slot silently unequips the player's weapon

- **Finding ID**: `ECS-2026-08-20-03`
- **Severity**: MEDIUM
- **Labels**: `medium,ecs,gameplay,bug`
- **Source report**: `docs/audits/AUDIT_ECS_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3112

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3112 --json state`.

---

**Severity**: MEDIUM
**Dimension**: 7 — Component Lifecycles (P2 gameplay slice)
**Source**: `docs/audits/AUDIT_ECS_2026-08-20.md` (`ECS-2026-08-20-03`)

> **Ownership note**: this finding is in the **P2 gameplay slice**
> (`byroredux/src/{combat,inventory,settings_io}.rs` + the action half of
> `byroredux/src/interaction.rs`), which **has no owning audit skill**. `/audit-ecs` covers it by
> explicit scope extension only. Labeled `ecs` + `gameplay` for that reason.

**Location**: `byroredux/src/inventory.rs:20-24`, `:146-150`, `:422-424`

## Description

#3032's fix gave weapons an equip slot by carving out bit 31 of the `EquipmentSlots` occupancy mask, on
the stated premise that it is outside the record contract:

```rust
/// Dedicated equipment bit for the currently wielded weapon. Biped slot
/// masks occupy the lower 32-bit contract in legacy records; keeping this
/// bit separate prevents a weapon toggle from displacing body armor.
const WEAPON_EQUIP_SLOT: usize = 31;
const WEAPON_EQUIP_SLOT_MASK: u32 = 1 << WEAPON_EQUIP_SLOT;
```

The premise is self-contradictory — bit 31 *is* in "the lower 32-bit contract".
`EquipmentSlots::occupants` is `[Option<InventoryIndex>; MAX_BIPED_SLOTS]` with
`MAX_BIPED_SLOTS = 32` (`crates/core/src/ecs/components/inventory.rs:171,184`), and the armor mask that
indexes it is the raw parsed `biped_flags` u32, **not** a truncated low half:

```rust
// byroredux/src/inventory.rs:146-150 — describe_kind, Armor arm
(*biped_flags != 0).then_some(*biped_flags),
```

On Skyrim+ that u32 is BOD2's first word. `crates/plugin/src/esm/records/items.rs:370-377` parses BOD2
straight into `biped_flags` (`slot_mask` is the separately-derived low u16, so nothing truncates
`biped_flags` itself), and `crates/plugin/src/equip.rs:24-36` documents the 30-based mapping: bit 0 =
slot 30, so bit 31 = slot **61 / FX01**. That is a real, authorable `BSDismemberBodyPartType`.

## Evidence

`byroredux/src/inventory.rs:422-424` — the reconcile step reads occupant 31 unconditionally after every
toggle, regardless of what kind of item caused the toggle:

```rust
let equipped_weapon = equipment.occupants[WEAPON_EQUIP_SLOT];
drop(equipment_query);
reconcile_equipped_weapon(world, player, equipped_weapon);
```

`reconcile_equipped_weapon` (`:428-459`) then looks the occupant's form id up in `InventoryCatalog` and,
finding `weapon_damage: None` for an armor, takes the `else` arm at `:457`:

```rust
} else {
    let _ = world.remove::<EquippedWeapon>(player);
}
```

## Impact

On a Skyrim/FO4 load order, equipping any ARMO whose BOD2 sets bit 31 writes the armor's
`InventoryIndex` into occupant 31 and immediately **removes** the player's `EquippedWeapon`, dropping
melee damage back to `UNARMED_DAMAGE` with no player-visible cause. The reverse direction also holds:
equipping a weapon writes occupant 31 and displaces the armor from that slot.

`combat::attack_damage` (`byroredux/src/combat.rs:274-287`) is the direct consumer, so this is a live
gameplay defect, not a bookkeeping one.

## Related

#3032 (CLOSED — the equip path it added is otherwise intact and verified this run: `describe_kind` gives
weapons a mask, `apply_action` reconciles `EquippedWeapon` on both branches, and `EquippedWeapon` is no
longer spawn-frozen). This is a defect in that fix's **choice of bit**, not a regression of it.

## Suggested Fix

Move the weapon occupancy out of the biped-slot array entirely — either a separate
`Option<InventoryIndex>` field on `EquipmentSlots`, or widen `occupants` and put the weapon at index 32+
so no authored mask can ever address it.

Whichever shape, the invariant to pin with a test is: **"no value producible by `describe_kind` for an
`ItemKind::Armor` can collide with the weapon slot."**

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any other place that indexes `occupants` by a
      hard-coded bit, and the `.at(WEAPON_EQUIP_SLOT as u8)` / `equip_slot_mask` test fixtures at
      `inventory.rs:516` and `:596`
- [ ] **TESTS**: A regression test pins this specific fix — equip an ARMO with `biped_flags` bit 31 set
      and assert `EquippedWeapon` survives
