# Hand Equipment — Design

**Status**: Phase 2 implementation landed (2026-11-24). The active
[Hand Equipment PRD](../prd/hand-equipment.md) tracks acceptance and remaining Skyrim work.

## 1. What the data says

Measured with a raw scan of the masters (2026-11-23):

**Skyrim.esm** has seven equip slots (`EQUP`):

| FormID | Slot | Parents | Use all parents |
|---|---|---|---|
| `00013F42` | RightHand | — | — |
| `00013F43` | LeftHand | — | — |
| `00013F44` | EitherHand | LeftHand, RightHand | no |
| `00013F45` | BothHands | LeftHand, RightHand | yes |
| `000141E8` | Shield | LeftHand | yes |
| `00025BEE` | Voice | — | — |
| `00035698` | Potion | — | — |

The `ETYP` each record type carries:

| Record | ETYP |
|---|---|
| WEAP | RightHand 1,286; BothHands 1,141; EitherHand 52; none 5 |
| SPEL | EitherHand 632; LeftHand 83; Voice 50; BothHands 40; RightHand 22 |
| SCRL | EitherHand 60; BothHands 14 |
| ARMO | Shield 237; none 2,525 |
| LIGH | none 435 (243 carry the "can be carried" flag) |

So:

- **A shield's hand is its ARMO `ETYP`** (`Shield`, whose one parent is `LeftHand`), not its biped slot.
  This settles HND-SLOT-004.
- **A torch has no slot in the data.** It is a light with the "This light can be carried" flag (CK
  *Light*; CK *IsTorchOut*: "a light object equipped with the 'This light can be carried' box
  checked"), and it goes in the left hand (UESP *Skyrim:Block*: "To block with a shield or torch,
  equip one to your left hand"). Its hand is therefore an engine rule from that page, not data. This
  settles HND-SLOT-005. A carried light's `Time` "determines how long temporary lights, such as
  carried torches, last" (CK *Light*).
- **A one-handed weapon's slot is `RightHand`, yet it can go in the left hand.** The left hand comes
  from the equip call: Papyrus `EquipItemEx(item, equipSlot)` takes 0 default, 1 right, 2 left (CK
  *EquipItemEx - Actor*), and the player's left-hand control does the same. `RightHand` names the
  default hand of an item that fits in one hand; `BothHands` takes both, and `EitherHand` takes either.

**Fallout4.esm** (38 slots) and **Starfield.esm** (25) model creature limbs (`HandyRMOptional`,
`CreatureHead`, `CreatureRangedMouth1`), grenade throws, alternate attacks and the power-armor
battery. Fallout 4 has three ARMO in a `BlockSlot` and no shields. Neither is a player's two-hand
model, so both stay out of scope (PRD §7.1).

## 2. NPC hands (HND-STATE-003)

The CK documents only constraints:

- *Combat Style*: "Allow Dual Wielding — Allows an NPC to dual wield. Note, this will only work on
  NPCs who have Dual Wielding animations — currently that's only 'humanoid' NPCs"; "Equipment Score
  Mults" weight the actor's choice between weapons.
- *Race*: "Can Dual Wield: Actor is allowed to use a weapon in both hands."

Nothing documents how an NPC picks what goes in which hand, or whether it takes out a torch in the
dark. By the owner's rule (no decoding the original executables for behaviour), that is a declared
gap. The engine rule, stated as ours:

1. The selected starting weapon takes its authored hands. The default hand for a flexible weapon
   is right. A weapon with a non-hand slot stays in inventory.
2. A worn shield occupies the left hand until a starting weapon takes that hand.
3. A right-hand one-handed weapon can be paired with a second one-handed weapon when the left hand
   is free and both the race's "Can Dual Wield" and the combat style's "Allow Dual Wielding" are set.
   Choose the eligible inventory stack with the highest base damage, breaking ties by lowest FormID.
   Using the same stack in both hands requires at least two copies.
4. An NPC never takes out a torch on its own (declared gap). A script can still equip one.

Read the race through the NPC's Traits template and combat style (`NPC_.ZNAM`) through its AI Data
template. The flags are `RACE.DATA` bit 30 and `CSTY.DATA` bit 2. An absent race or combat style
leaves the second weapon unequipped. In the five Skyrim SE official masters, 15 styles set the
`DATA` flag with CSTY header bit 19 clear, and `csWEMageShieldStaff` has the opposite combination.
The spawn rule uses the `DATA` field. [xEdit Skyrim CSTY definitions](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas#L4418-L4474)

## 3. State

The hands are the actor's `EquipmentSlots` (saved, and parked with the inventory it indexes), not a
component of their own:

- `weapon` is the right hand's weapon.
- `left_hand` is a one-handed weapon or a torch held in the left (v148).
- `left_spell` / `right_spell` are a spell readied in each hand, by FormID (v149); a two-handed spell
  is in both.
- A shield keeps its biped slot for its model, and a reader finds it among the occupants.

Saves store held spell and inventory item FormIDs by stable plugin identity. The live load overlay
resolves those forms through the current plugin order and restores the inventory and its hand
indices together (HND-STATE-002).

Cell eviction parks `EquipmentSlots` with its inventory under the placed reference's identity.
When the cell returns, the NPC receives its authored loadout first. The placement stamp then
restores the parked slots and inventory together, preserving the held item's index (HND-STATE-004).

`byroredux_scripting::hands::hand_item(world, actor, hand)` derives what a hand holds, with
`HandCatalog` (each weapon's animation type and slot hands, the shields, the torches and the spells'
slots, built from the records):

- The right hand is its spell, else `weapon`.
- The left hand is the right weapon when its slot is `BothHands`. Otherwise it is its spell, else
  `left_hand` (a torch or a weapon), else a worn shield.

`EquipmentSlots` is authoritative for all Skyrim hand readers in §4.

One function writes the hands: `byroredux_scripting::hands::equip_to_hand(slots, item, hand, facts)`,
following CK *Equip Slot* ("Attempting to equip one will automatically unequip the other"):

- `hands_taken` applies the [PRD slot rules](../prd/hand-equipment.md#51-slots-hnd-slot). An explicit
  left-only weapon takes the left hand and a non-hand slot has no hand destination. A weapon with
  no `ETYP` uses the same rule as a flexible one-handed weapon.
- Whatever held those hands comes off first; a two-handed occupant comes off both.
- One item of a single stack moved to the other hand leaves the first.

Every equip goes through it:

- the inventory menu's equip, `EquipLeft` and the console's `inv.equip <form> [left]`
  (`equip_in`);
- a script's `EquipItem` / `EquipItemEx` of a weapon, torch or the player's shield, queued with its
  hand to that same path (`PendingNonArmorEquips`);
- a script's `EquipSpell` and another actor's shield, applied in the scripting crate;
- starting weapon selection for the player and NPCs (`equip_starting_weapon`).

`EquippedWeapon` stays the right hand's cache (damage, reach, speed; 51 combat readers). It is
re-derived whenever the menu path changes the right weapon, and dropped every frame when it names a
weapon the slots no longer wield (a script's spell readied over it).

A held torch lights its surroundings (Q3): `held_torch_light_system` keeps one point light, the LIGH's
own radius, colour and falloff, at the holder's left side at hand height. It despawns the light when
the torch leaves the hand. The hand bone is not followed, and the torch's burn time is not modelled.

## 4. Readers

| Reader | Answer |
|---|---|
| `GetEquippedItemType(hand)` | 0 empty; a weapon by its `DNAM` animation type (1 OneHandSword … 8 Staff; 9 Crossbow is code 12); 9 spell; 10 shield; 11 torch (CK *GetEquippedItemType - Actor*) |
| `GetEquippedWeapon(abLeftHand)` | the hand's weapon or None |
| `GetEquippedShield` | Skyrim returns the shield in the canonical left hand (HND-READ-006). Oblivion reads its block model's biped slot. |
| `GetEquippedSpell(source)` | 0 left, 1 right: the hand's spell |
| `EquipItemEx(item, slot)` / `EquipSpell(spell, source)` | Route through `equip_to_hand`. Fixed spell slots determine occupancy and `EitherHand` uses the requested hand under the current rule. PRD HND-READ-003 and HND-READ-007 cover slot arguments and source verification. |
| `IsWeaponDrawn` | the drawn state `IsWeaponOut` reads |
| `IsTorchOut` (condition 102) | a torch in either hand; declined off Skyrim |
| Block | `block_guard` classifies a left shield, otherwise a left torch or a weapon in either hand, otherwise bare hands. Skyrim's `BlockModel` uses that choice for a covered physical hit. [Skyrim block rules](charal-skyrim-ruleset.md#block-damage-reduction). |

## 5. Phases

As the PRD §9.

**Phase 1** builds:
- the slot catalog (`EQUP`) and the WEAP and ARMO `ETYP`;
- `EquipmentSlots.left_hand` and the hand rules (`equip_to_hand` since Phase 2);
- the `GetEquippedItemType` / `GetEquippedWeapon(true)` readers for weapons and shields;
- the format bump;
- the engine's `EquipLeft` action, reached today by `inv.equip <form> [left]`.

**Phase 2** (built) adds:
- torches: equipped into the left hand, lit at the hand (Q3);
- spells in hands, with the SPEL `ETYP`, `EquipSpell` / `UnequipSpell` / `GetEquippedSpell`;
- the NPC rule's first two parts (§2): the right hand holds the spawn's weapon, and a two-handed
  weapon takes a worn shield off;
- the inventory menu's per-hand entries (Q2);
- `EquipItemEx`'s hand argument, `IsWeaponDrawn`, the `IsTorchOut` condition;
- the helper to classify block guards from the hands.

Phase 3 implements the dual-wield spawn rule in §2. The right-hand weapon still supplies the existing
combat stats under Q1. The [PRD's delivery sequence](../prd/hand-equipment.md#9-delivery-sequence)
tracks remaining acceptance. Torch burn time and the hand bone remain follow-ups.
