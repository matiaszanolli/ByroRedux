# Hand Equipment — Design

**Status**: DESIGN (2026-11-23), for the [Hand Equipment PRD](../prd/hand-equipment.md) (APPROVED),
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

One component per actor, `Hands { left: HandItem, right: HandItem }`, where `HandItem` is `Empty`,
`Weapon { index, base }`, `Shield { index, base }`, `Torch { index, base }` or `Spell { form }`. A
`BothHands` item puts the same item in both. `Hands` is the one record of what a hand holds, and one
function writes it: `equip_to_hand(world, actor, item, hand)`.

- `equip_to_hand` resolves the item's hands: a `BothHands` slot takes both; `Shield` and a torch take
  the left; `LeftHand` and `RightHand` spells take theirs. Anything else takes the hand asked for
  (the right by default). It unequips what held those hands (CK *Equip Slot*: "Attempting to equip
  one will automatically unequip the other").
- `EquippedWeapon` stays as the right hand's weapon, with its damage, reach and speed cache. Its 51
  combat readers keep working unchanged, and `equip_to_hand` keeps the two in step. A shield keeps
  its biped slot for its model; `Hands.left` is what readers ask.
- Saved (stable FormIDs), parked across cell eviction beside the inventory it indexes, with a format
  bump.

## 4. Readers

| Reader | Answer |
|---|---|
| `GetEquippedItemType(hand)` | 0 empty; a weapon by its `DNAM` animation type (1 OneHandSword … 8 Staff; 9 Crossbow is code 12); 9 spell; 10 shield; 11 torch (CK *GetEquippedItemType - Actor*) |
| `GetEquippedWeapon(abLeftHand)` | the hand's weapon or None |
| `GetEquippedShield` | `Hands.left` when it is a shield |
| `GetEquippedSpell(source)` | 0 left, 1 right: the hand's spell |
| `EquipItemEx(item, slot)` / `EquipSpell(spell, source)` | `equip_to_hand` with that hand; a spell's own slot overrides the source unless it is BothHands (CK *EquipSpell - Actor*) |
| `IsTorchOut` (condition) | a torch in either hand |
| Block | the left hand: a shield, a torch, or a weapon (UESP *Skyrim:Block*) |

## 5. Phases

As the PRD §9. Phase 1 builds the slot catalog, `ETYP`, `Hands`, `equip_to_hand`, the readers for
weapons and shields, and the format bump. Phase 2 adds torches (a carried light lit at the hand, the
owner's decision Q3), spells in hands, the NPC rule of §2, the inventory's equip-left and equip-right
actions (Q2), and block.
