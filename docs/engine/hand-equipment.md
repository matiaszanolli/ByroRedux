# Hand Equipment — Design

**Status**: PHASE 2 BUILT (2026-11-24), for the [Hand Equipment PRD](../prd/hand-equipment.md) (APPROVED),
written by its Phase 0. Skyrim only (PRD §7.1).

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

1. The right hand takes the weapon the spawn path wields today.
2. The left hand takes a shield the NPC carries when the right weapon is one-handed (not
   `BothHands`).
3. Without a shield, the left hand takes a second one-handed weapon only when both the race's "Can
   Dual Wield" and the combat style's "Allow Dual Wielding" are set.
4. An NPC never takes out a torch on its own (declared gap). A script can still equip one.

## 3. State

The hands are the actor's `EquipmentSlots` (saved, and parked with the inventory it indexes), not a
component of their own:

- `weapon` is the right hand's weapon.
- `left_hand` is a one-handed weapon or a torch held in the left (v148).
- `left_spell` / `right_spell` are a spell readied in each hand, by FormID (v149); a two-handed spell
  is in both.
- A shield keeps its biped slot for its model, and a reader finds it among the occupants.

`byroredux_scripting::hands::hand_item(world, actor, hand)` derives what a hand holds, with
`HandCatalog` (each weapon's animation type and slot hands, the shields, the torches and the spells'
slots, built from the records):

- The right hand is its spell, else `weapon`.
- The left hand is the right weapon when its slot is `BothHands`. Otherwise it is its spell, else
  `left_hand` (a torch or a weapon), else a worn shield.

Nothing stores a second record of a hand, so two readers can never disagree.

One function writes the hands: `byroredux_scripting::hands::equip_to_hand(slots, item, hand, facts)`,
following CK *Equip Slot* ("Attempting to equip one will automatically unequip the other"):

- The item takes the hands its slot names (`hands_taken`): a `BothHands` weapon or spell both, a
  shield or torch the left, a `LeftHand` / `RightHand` spell its own, a one-handed weapon or an
  `EitherHand` spell the hand asked for, a Voice power none (refused).
- Whatever held those hands comes off first; a two-handed occupant comes off both.
- One item of a single stack moved to the other hand leaves the first.

Every equip goes through it:

- the inventory menu's equip, `EquipLeft` and the console's `inv.equip <form> [left]`
  (`toggle_equip_in`);
- a script's `EquipItem` / `EquipItemEx` of a weapon, torch or the player's shield, queued with its
  hand to that same path (`PendingNonArmorEquips`);
- a script's `EquipSpell` and another actor's shield, applied in the scripting crate.

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
| `GetEquippedShield` | the left hand when it holds a shield |
| `GetEquippedSpell(source)` | 0 left, 1 right: the hand's spell |
| `EquipItemEx(item, slot)` / `EquipSpell(spell, source)` | `equip_into_hand` with that hand; a spell's own slot overrides the source unless it is BothHands (CK *EquipSpell - Actor*) |
| `IsWeaponDrawn` | the drawn state `IsWeaponOut` reads |
| `IsTorchOut` (condition 102) | a torch in either hand; declined off Skyrim |
| Block | the left hand: a shield, else a torch or weapon in either hand, else bare hands (UESP *Skyrim:Block*). Skyrim has no block numbers yet (only Oblivion's `BlockModel`), so the guard is read but nothing is blocked |

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
- block's guard read from the hands.

Left for later: NPC dual wield (§2 rule 3) needs the RACE "Can Dual Wield" and CSTY "Allow Dual
Wielding" flags, which are not parsed yet (xEdit names the CSTY flag both as a record flag, bit 19, and
as `DATA` 0x04, so it needs a measurement of the masters first); the torch's burn time; the hand bone.
