# Hand Equipment — Product Requirements

**Status**: ACTIVE (2026-11-24). Revision 6 records Phase 2's build (#7399). Revision 5 records Phase 1's build (#7398). Revision 4 files the tracking issues. Revision 3 settles Phase 0's verify items from the masters and the
CK / UESP pages, and links the design Phase 0 wrote. Revision 2: the owner approved the first draft
and took every §10 recommendation. Statements about how an original engine, format or tool behaves cite external
sources; a claim that could not be verified is marked **Phase 0: verify** instead of being stated as
fact.

**Tracking**: #7397 (parent), #7398 (Phase 1), #7399 (Phase 2). Phase 0 is done (revision 3).

**Scope**: what each of an actor's two hands holds in Skyrim — a weapon, a shield, a torch, a spell or
nothing — as the plugin data says it may, kept per actor, saved, and read by the scripts, conditions
and systems that ask about it. It covers the player and NPCs. Outside it: Skyrim's per-hand spell
*casting* (the [player-spellcasting PRD](player-spellcasting.md) Q1 left it out; a hand here only
holds the spell), dual-wield combat numbers, and every game without this model (§7.1).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism
promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| Equip slots, occupancy and the hand state | [Hand Equipment — design](../engine/hand-equipment.md) |
| Actor values, skills and block rules the hands feed | [CHARAL](../engine/charal.md) |
| The player's casting (a readied spell, Oblivion) | [Player Spellcasting PRD](player-spellcasting.md) |
| Papyrus natives reading the hands | [Scripting](../engine/scripting.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related
designs. Requirement IDs use the `HND-` prefix.

---

## 1. Product thesis

Skyrim content asks which hand holds what. The Papyrus VM's coverage census on vanilla `Skyrim.esm`
(2026-11-23, after the native waves) shows `GetEquippedItemType` declining 1,359 scripted references
on its own — the oil-pool and gas traps that ignite when the actor entering holds a torch or a fire
spell (792), and the weapon racks that read the player's left and right hands (567) — and the left
hand of `GetEquippedWeapon` declining with it. The engine keeps one wielded weapon and the armour
biped slots; it has no left hand. Answering "nothing in the left hand" would be a fabrication, so
those scripts stay declined until the hands exist.

The promise: an actor's two hands are real state, filled by the same rules the plugin data gives the
original (equip slots, "both hands", "either hand"), so every reader — a script, a condition, the
block rule, a trap — gets the answer the data implies.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Equip slot** | An `EQUP` record: a body position an item equips to, with its parent slots and whether it takes all of them ([UESP: EQUP](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/EQUP); [CK: Equip Slot](https://ck.uesp.net/wiki/Equip_Slot)). |
| **Hand** | The left or right hand: the two leaf slots `LeftHand` and `RightHand`. |
| **Hand item** | What a hand holds: a weapon, a shield, a torch, a spell, or nothing. |
| **Occupancy** | Which hand items fill which hands, per actor. |

---

## 3. Core model

The CK: "Equip Slots are used to define a position on a character's body to which an item can be
equipped"; "the Equip Slot BothHands has the parent slots LeftHand and RightHand, and Use All Parents
is checked … any item marked with the BothHands slot takes up both the left and right hands"; "the
Equip Slot EitherHand … will occupy either the left or the right hand, but not both"; "Items marked
with the same slot can't be equipped simultaneously. Attempting to equip one will automatically
unequip the other" ([CK: Equip Slot](https://ck.uesp.net/wiki/Equip_Slot)). Weapons and spells name
their slot in `ETYP` ([UESP: WEAP](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/WEAP): "Specifies
which equipment slot is used by the weapon (typically 'BothHands' or 'EitherHand')"; [UESP:
SPEL](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/SPEL) `ETYP`). A weapon's kind is separate
from its slot: its `DNAM` animation type (0 Other, 1 OneHandSword … 7 Bow, 8 Staff, 9 Crossbow) is not
how many hands it takes (UESP WEAP notes: "ETYP is purely how many hands the weapon needs").

The engine at HEAD on 2026-11-23 (`66668ed4b`):

| # | Item | Class | Today |
|---|---|---|---|
| 1 | `EQUP` records and `ETYP` on WEAP / SPEL | data | Not parsed (no `ETYP` / `EQUP` in `crates/plugin/src/esm/records`). |
| 2 | Weapon kind (WEAP `DNAM` animation type) | data | Parsed for Skyrim (`crates/plugin/src/esm/records/items.rs`, `b"DNAM" if Skyrim` → `anim_type`). |
| 3 | The wielded weapon | state | One `EquippedWeapon` per actor, the right hand by convention (`crates/core/src/ecs/components/inventory.rs`). |
| 4 | Shields | state | An armour in the shield biped slot (`EquipmentSlots`); the VM reads it as `GetEquippedShield` (CK *Biped Object*: "39 - shield"). |
| 5 | Torches, left-hand weapons, spells in a hand | state | None: no left hand, no equipped light, no hand for a readied spell. |
| 6 | Readers | consumer | `GetEquippedItemType`, `GetEquippedWeapon(true)`, `GetEquippedSpell` decline in the VM; the block rule reads a shield bit (Oblivion). |

---

## 4. Goals and non-goals

### 4.1 Goals

- G1. Each actor's two hands hold what the data allows, by the equip-slot rules of §3.
- G2. Every reader asks the hands: Papyrus, conditions, the block rule, traps.
- G3. The hands survive saves and cell reloads, as the inventory does.
- G4. Vanilla's declined hand readers run: the 1,359 references of §1 move to the VM.

### 4.2 Non-goals

- NG1. Per-hand spell casting, dual casting and concentration spells (a separate design, as the
  player-spellcasting PRD's Q1 decided).
- NG2. Dual-wield combat numbers (power attacks, damage multipliers; [UESP: Skyrim:Combat](https://en.uesp.net/wiki/Skyrim:Combat)) until Q1 says otherwise.
- NG3. Hand models for Oblivion and the Fallouts (§7.1).

---

## 5. Requirements by area

### 5.1 Slots (`HND-SLOT`)

- **HND-SLOT-001.** The engine MUST read every `EQUP` record (`PNAM` parents, `DATA` "use all
  parents") and the `ETYP` of WEAP and SPEL records, from every plugin, as the [UESP
  EQUP](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/EQUP) page lays them out.
- **HND-SLOT-002.** An item's hands MUST follow its slot: a slot that uses all its parents takes every
  parent hand; one that does not takes one of them ([CK: Equip Slot](https://ck.uesp.net/wiki/Equip_Slot)).
  An item whose slot fits one hand (`RightHand`, as 1,286 of Skyrim.esm's weapons are, or
  `EitherHand`) goes in the hand the equip call names, the right by default ([CK:
  EquipItemEx](https://ck.uesp.net/wiki/EquipItemEx_-_Actor): 0 default, 1 right, 2 left).
- **HND-SLOT-003.** Equipping an item MUST unequip what held the hands it takes ("Items marked with the
  same slot can't be equipped simultaneously").
- **HND-SLOT-004.** A shield MUST occupy the left hand, by its ARMO `ETYP`: all 237 of Skyrim.esm's
  shields name the `Shield` slot, whose one parent is `LeftHand` (design §1).
- **HND-SLOT-005.** A carryable light (LIGH flag `0x0002` "Can be carried", [UESP: LIGH](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/LIGH)) MUST be equippable into the left hand as a
  torch. No light carries an `ETYP` (Skyrim.esm: none of 435), so the hand is the documented rule:
  "To block with a shield or torch, equip one to your left hand" ([UESP:
  Skyrim:Block](https://en.uesp.net/wiki/Skyrim:Block)).

### 5.2 State (`HND-STATE`)

- **HND-STATE-001.** Each actor MUST keep what each hand holds as one component, written only by the
  equip and unequip paths.
- **HND-STATE-002.** The state MUST be saved, its FormIDs as stable identities, and parked across a
  cell's eviction with the inventory it indexes.
- **HND-STATE-003.** An NPC MUST spawn with the hands its loadout implies, within the documented limits:
  it dual wields only when its race's "Can Dual Wield" ([CK: Race](https://ck.uesp.net/wiki/Race)) and
  its combat style's "Allow Dual Wielding" ([CK: Combat Style](https://ck.uesp.net/wiki/Combat_Style))
  are both set. How the original picks an NPC's hand items, and whether it takes out a torch, is
  undocumented: a declared gap, filled by the engine rule in the design's §2.

### 5.3 Readers (`HND-READ`)

- **HND-READ-001.** `GetEquippedItemType(aiHand)` MUST return the CK's codes for the hand asked (0
  left, 1 right): 0 nothing, 1–8 the weapon kinds, 9 a spell, 10 a shield, 11 a torch, 12 a crossbow
  ([CK: GetEquippedItemType](https://ck.uesp.net/wiki/GetEquippedItemType_-_Actor)).
- **HND-READ-002.** `GetEquippedWeapon(abLeftHand)` MUST return the named hand's weapon ([CK:
  GetEquippedWeapon](https://ck.uesp.net/wiki/GetEquippedWeapon_-_Actor)); `GetEquippedSpell(aiSource)`
  the spell in the left or right hand ([CK: GetEquippedSpell](https://ck.uesp.net/wiki/GetEquippedSpell_-_Actor)).
- **HND-READ-003.** `EquipSpell(akSpell, aiSource)` and `EquipItemEx` MUST equip into the hand they
  name, the item's own slot overriding it as the CK says ("it will be overridden by the Spell's Equip
  Slot if it is set to anything other than BothHands").
- **HND-READ-004.** The block rule MUST read the left hand (a shield, a torch or a weapon), replacing
  the shield-bit read.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Hands follow the data's slots | The same (HND-SLOT-002) | None | None: a faithful model. |
| 2 | A spell in a hand is cast from it | Held, not cast (NG1) | None | A mod that reads a hand's spell sees it; casting from it waits for its design. |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | no | One weapon and a shield slot; its block rule already reads the shield bit. |
| Fallout 3 | no | One weapon, no shields. |
| Fallout: New Vegas | no | One weapon, no shields. |
| Skyrim (LE / SE / AE) | yes | The equip-slot model this PRD describes (Wave 1). |
| Fallout 4 | no | Its 38 `EQUP` slots model creature limbs, grenades and alternate attacks; no shields (design §1). |
| Fallout 76 | n/a | Out of every PRD. |
| Starfield | no | Its 25 `EQUP` slots model creature limbs and grenades, as Fallout 4's (design §1). |

### 7.2 Acceptance

1. The VM's coverage census on vanilla `Skyrim.esm` no longer declines `GetEquippedItemType` or the
   left hand of `GetEquippedWeapon`.
2. A Skyrim gate: the player equips a one-handed sword in each hand, then a shield (the left sword
   comes off), then a bow (both hands), then a torch; after each, the VM's hand readers return the
   CK's codes, and a save and a reload keep the hands.
3. An oil-pool trap ignites when the player walks in holding a torch, and not with a sword.

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Hand-reader coverage | scripted references declined by a hand reader ÷ the same at the baseline (1,359) | per game |

---

## 9. Delivery sequence

**Phase 0 — Verify and design (no engine code).** Settle every **Phase 0: verify** item from the data
and the CK / UESP pages; write `docs/engine/hand-equipment.md`. Exit: no unverified item left.

**Phase 1 — Slots, state and readers.** HND-SLOT-001..003, HND-STATE-001..002, HND-READ-001..002 for
weapons and shields, the save format bump. Exit: acceptance 1, and 2 without the torch.

**Phase 2 — Torches, spells in hands, NPC loadouts, block.** HND-SLOT-004..005, HND-STATE-003,
HND-READ-003..004. Exit: acceptance 2 and 3.

---

## 10. Decisions required

1. **Q1 — Dual wield.** Recommended: the hands hold two one-handed weapons (the state and the readers),
   but attacks keep their one-weapon numbers until a combat design covers dual wielding (NG2). The
   alternative takes on UESP's dual-wield combat rules in this PRD.
   **Decided** (2026-11-23): as recommended.
2. **Q2 — How the player chooses a hand.** Recommended: the inventory gains "equip left" and "equip
   right" actions on a one-handed item, as the original's two hand controls do, bound like the other
   inventory actions. The alternative equips to the right hand only and leaves the left to scripts.
   **Decided** (2026-11-23): as recommended.
3. **Q3 — A held torch's light.** Recommended: Phase 2 attaches the torch's light to the hand, as a
   carried light lights its surroundings. The alternative holds the torch as data only.
   **Decided** (2026-11-23): as recommended.

---

## 11. Document map

| Need | Go to |
|---|---|
| How the slots and the hand state are built | [Hand Equipment — design](../engine/hand-equipment.md) |
| Actor values and the block rule | [CHARAL](../engine/charal.md) |
| The player's casting | [Player Spellcasting PRD](player-spellcasting.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-23 | First draft | — |
| 2 | 2026-11-23 | Approved; Q1–Q3 decided as recommended | owner: approved |
| 3 | 2026-11-23 | Phase 0: the verify items settled from the masters (EQUP / ETYP scan of Skyrim, Fallout 4 and Starfield) and the CK / UESP pages; NPC hand choice a declared gap with an engine rule; the design written | — |
| 4 | 2026-11-23 | ACTIVE: tracking issues #7397 (parent), #7398 (Phase 1), #7399 (Phase 2) | — |
| 5 | 2026-11-23 | Phase 1 built (#7398): EQUP catalog, WEAP / ARMO `ETYP`, `EquipmentSlots.left_hand` (format v148) as the hand state instead of a new component, `equip_into_hand`, `GetEquippedItemType` / `GetEquippedWeapon(true)`; the shield's ARMO `ETYP` (HND-SLOT-004) landed with it. The design's §3 and §5 follow | — |
| 6 | 2026-11-24 | Phase 2 built (#7399): torches in the left hand and their light (Q3), spells in hands (SPEL `ETYP`, `EquipSpell` / `UnequipSpell` / `GetEquippedSpell`, format v149), `EquipItemEx`'s hand, `IsWeaponDrawn`, `IsTorchOut`, the menu's per-hand entries (Q2), block's guard from the hands, the NPC rule's parts 1–2. NPC dual wield (HND-STATE-003's flags) is left for a follow-up: the RACE / CSTY flags are not parsed yet | — |
