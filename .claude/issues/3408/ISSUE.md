# SKY-2026-08-27b-D3-01: a race default skin with `BOD2 == 0` loses its body mesh to the #2094 occupancy filter — 351/351 vanilla creature-race NPCs, all 322 Draugr included

- **Severity**: HIGH
- **Dimension**: 3 (NPC equip + FaceGen)
- **Location**: `byroredux/src/npc_spawn.rs:977` (the retain), fed from `byroredux/src/npc_spawn.rs:795-796` (the race-skin equip + `race_skin_slots` record)
- **Confidence**: CONFIRMED — reproduced through the live `build_npc_equip_state` against real `Skyrim.esm`, not inferred.

## Description

`build_npc_equip_state` equips the race default skin first as the lowest-priority layer (#2093), then at the end runs the #2094 occupancy filter:

```rust
// byroredux/src/npc_spawn.rs:977
armor_to_spawn.retain(|armor| equipment_slots.occupants.contains(&Some(armor.inv_idx)));
```

Its premise — "a queued mesh is only kept when its inventory index still holds a biped bit" — silently fails for any ARMO whose authored mask is zero, because `EquipmentSlots::equip` iterates the set bits of the mask and a zero mask sets none:

```rust
// crates/core/src/ecs/components/inventory.rs:226-233
pub fn equip(&mut self, slot_mask: u32, idx: InventoryIndex) -> Vec<InventoryIndex> {
    let mut displaced = Vec::new();
    for bit in 0..MAX_BIPED_SLOTS {
        if slot_mask & (1u32 << bit) == 0 {
            continue;
        }
```

Such a skin can therefore never satisfy the retain, and the mesh it resolved — the whole creature body — is discarded. `race_skin_slots` records `(inv_idx, 0)`, so the `displaced_mask` fold above it also collapses to `0` and contributes nothing.

## Evidence

Measured on real `Skyrim.esm`.

ARMO census — 10 of 2,762 armour records author `BOD2 == 0`, and every one of them carries ARMAs (i.e. they *do* name meshes):

```
ARMO biped_flags==0: 10 (of which 10 have ARMAs); nonzero=2752
  SkinDraugrHair01 (0003BC83) armatures=1
  SkinDraugrHair02 (0003BC84) armatures=1
  SkinDraugrBeard01 (0003BC81) armatures=1
  SkinSlaughterfish (0004124A) armatures=3
  SkinSabrecat (00016EE6) armatures=2
  SkinDraugr (00016EE3) armatures=1
  SkinFrostbiteSpider (0003636F) armatures=2
  SkinFrostbiteSpiderCold (00048C0E) armatures=2
```

RACE census — 7 of 99 races point `WNAM` at one of them:

```
races=99  default-skin BOD2==0: 7 | BOD2!=0: 90 | no skin: 2
  race DraugrRace            (00000D53) skin SkinDraugr
  race FrostbiteSpiderRace   (000131F8) skin SkinFrostbiteSpider
  race SabreCatRace          (00013200) skin SkinSabrecat
  race SkeeverRace           (00013201) skin SkinSkeever
  race SlaughterfishRace     (00013203) skin SkinSlaughterfish
  race FrostbiteSpiderRaceLarge (00053477) skin SkinFrostbiteSpider
  race DraugrMagicRace       (000F71DC) skin SkinDraugr
NPC_ records on those races: 351 of 5118 total
  {"DraugrMagicRace": 8, "DraugrRace": 314, "FrostbiteSpiderRace": 10,
   "FrostbiteSpiderRaceLarge": 6, "SabreCatRace": 4, "SkeeverRace": 8,
   "SlaughterfishRace": 1}
```

Driving the real `build_npc_equip_state` (a temporary `#[ignore]` probe test in `byroredux/src/npc_spawn/tests.rs`, run against real `Skyrim.esm`, then reverted):

```
PROBE 00022401 EncDraugr02MissileHeadM01     race=DraugrRace skin=SkinDraugr bod2=0x0 meshes=0 slots_occupied=0
PROBE 000EA50E EncDraugr03AmbushMelee2HHeadM07 race=DraugrRace skin=SkinDraugr bod2=0x0 meshes=0 slots_occupied=0
PROBE 00073989 dunFellglow_WarlockPet        race=FrostbiteSpiderRace skin=SkinFrostbiteSpider bod2=0x0 meshes=1 slots_occupied=1
PROBE creature-skin NPCs: skin-mesh-DROPPED=351 skin-mesh-kept=0
```

A separate pass of the same probe counted **170 of the 351** ending with `armor_to_spawn.len() == 0` — no mesh source of any kind, only the shared skeleton.

## Impact

Every Draugr, sabrecat, skeever, frostbite spider and slaughterfish placement in vanilla Skyrim + Dawnguard + Dragonborn spawns without its body. Where the actor also carries an outfit (most Draugr do), the armour renders on a bodyless skeleton; where it does not (170 records), nothing renders. This is the single most common enemy family in the game. Nothing catches it: the equip guards added by #3361 walk only the six Bannered Mare humans, all of whom sit on `NordRace` (`SkinNaked`, `BOD2 != 0`).

## Suggested Fix

Exempt the race-skin entry from the occupancy filter when its authored mask is zero — a skin that claims no biped region cannot be *displaced* out of one, so the filter has no opinion about it. Concretely: retain when `armor.inv_idx == skin_inv_idx && skin_biped_flags == 0`, in addition to the existing occupancy test. Add a real-data guard alongside #3361's that asserts a `DraugrRace` NPC resolves at least one mesh.

## Related

#2094 (the filter), #2093 (the race-skin layer), #3357 (the previous race-skin resolver defect, CLOSED).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27b.md` (`/audit-skyrim`).*
