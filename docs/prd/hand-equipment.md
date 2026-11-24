# Hand Equipment — Product Requirements

**Status**: ACTIVE (2026-11-24). Revision 17 records the oil-pool VM admission and live hand-equipment check. See §12 for the revision history.

**Tracking**: #7397 (parent and follow-ups), #7398 (Phase 1), #7399 (Phase 2). Revision 3 records the initial Phase 0 work.

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

Skyrim players and mod authors rely on scripts that know which hand holds an item. Weapon racks
query the player's hands, and oil and gas traps read equipped items as part of their ignition
logic. Before this work the VM declined several of those readers. The Skyrim SE census at
`ba14baeeb` admits the weapon-rack activation script. Revisions 16 and 17 admit the gas and oil
trap scripts after adding the magic-effect keyword reader and placed destructible reset. Section 7.2
checks ignition through the original Papyrus handlers.

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

Pre-implementation baseline on 2026-11-23 (`66668ed4b`). See the
[current equipment state and readers](../engine/hand-equipment.md#3-state).

| # | Item | Class | At the baseline |
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
- G4. The baseline script families can call both hand readers. The census records admission and
  remaining blockers, and the live gate checks the trap's ignition behavior (§7.2).

### 4.2 Non-goals

- NG1. Per-hand spell casting, dual casting and concentration spells (a separate design, as the
  player-spellcasting PRD's Q1 decided).
- NG2. Dual-wield combat numbers (power attacks, damage multipliers; [UESP: Skyrim:Combat](https://en.uesp.net/wiki/Skyrim:Combat)) until Q1 says otherwise.
- NG3. Hand models for Oblivion and the Fallouts (§7.1).

---

## 5. Requirements by area

### 5.1 Slots (`HND-SLOT`)

- **HND-SLOT-001.** The engine MUST resolve each weapon's `ETYP` through the Skyrim load order's
  `EQUP` catalog, preserving `PNAM` parents and the `DATA` use-all-parents flag. See
  [UESP EQUP](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/EQUP).
- **HND-SLOT-002.** An item's occupancy MUST follow the resolved slot and the applicable equip
  request. Skyrim's authored parent rules are described in [CK Equip Slot](https://ck.uesp.net/wiki/Equip_Slot).

  | Resolved slot | Weapon occupancy | Spell occupancy |
  |---|---|---|
  | Both hands | Both | Both |
  | Either hand | Requested hand. ByroRedux uses the right hand for a default request. | Requested hand under ByroRedux's current rule. Original behavior is a verification item under HND-READ-007. |
  | `RightHand` | Right by default. A deliberate left equip is available for ordinary one-handed weapons (Q2). | Right, overriding the requested source |
  | Explicit `LeftHand` | Left | Left, overriding the requested source |
  | Explicit slot resolving to neither hand | Refuse the hand equip | Leave hand occupancy unchanged |
  | Missing `ETYP` | ByroRedux's one-hand fallback uses the requested hand or the default right. | ByroRedux uses the requested hand. |

  `authored_weapon_slots_control_player_equips_and_starting_inventory` checks explicit left-only
  and non-hand `ETYP` fixtures through menu actions, scripted equips and starting inventory.
  The NPC loadout fixture checks the same selection rule for starting weapons.
- **HND-SLOT-003.** Equipping an item MUST unequip what held the hands it takes ("Items marked with the
  same slot can't be equipped simultaneously").
- **HND-SLOT-004.** A shield MUST occupy the left hand through its ARMO `ETYP` resolving to the
  left hand. See the [authored slot census](../engine/hand-equipment.md#1-what-the-data-says).
- **HND-SLOT-005.** A carryable light (LIGH flag `0x0002` "Can be carried", [UESP: LIGH](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/LIGH)) MUST be equippable into the left hand as a
  torch. The [authored slot census](../engine/hand-equipment.md#1-what-the-data-says) records
  lights without `ETYP`. [UESP Skyrim:Block](https://en.uesp.net/wiki/Skyrim:Block) describes
  equipping a torch to the left hand for blocking.
- **HND-SLOT-006.** The engine MUST resolve each Skyrim spell's authored `ETYP` through the
  same folded `EQUP` catalog as weapons. The fixture checks a spell override that refers to a
  plugin-local equip slot whose FormID changes on load. See [UESP SPEL](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/SPEL).

### 5.2 State (`HND-STATE`)

- **HND-STATE-001.** Each actor MUST keep one authoritative hand state in `EquipmentSlots`.
  Equip and unequip apply the occupancy rules from HND-SLOT-002 and HND-SLOT-003.
- **HND-STATE-002.** Save and load MUST restore the held items and spells. A round-trip fixture
  changes a spell's plugin slot and checks that the hand still resolves to the same spell.
- **HND-STATE-003.** An NPC MUST spawn with the hands its loadout implies, within the documented limits:
  it dual wields only when its race's "Can Dual Wield" ([CK: Race](https://ck.uesp.net/wiki/Race)) and
  its combat style's "Allow Dual Wielding" ([CK: Combat Style](https://ck.uesp.net/wiki/Combat_Style))
  are both set. Use the [design's NPC loadout rule](../engine/hand-equipment.md#2-npc-hands-hnd-state-003).
  **Phase 0: verify** the original's choices if parity is required. Compare NPCs with the same
  inventory across the flag combinations and light levels, recording the held items and torch use.
- **HND-STATE-004.** The same NPC MUST regain its hand occupancy when its cell is evicted and
  returns. The fixture checks restored hand items against the restored inventory after respawn.

### 5.3 Readers (`HND-READ`)

- **HND-READ-001.** `GetEquippedItemType(aiHand)` MUST return the CK's codes for the hand asked (0
  left, 1 right): 0 nothing, 1–8 the weapon kinds, 9 a spell, 10 a shield, 11 a torch, 12 a crossbow
  ([CK: GetEquippedItemType](https://ck.uesp.net/wiki/GetEquippedItemType_-_Actor)).
- **HND-READ-002.** `GetEquippedWeapon(abLeftHand)` MUST return the named hand's weapon. See
  [CK GetEquippedWeapon](https://ck.uesp.net/wiki/GetEquippedWeapon_-_Actor).
- **HND-READ-003.** Skyrim's `EquipItemEx(item, equipSlot)` MUST apply HND-SLOT-002 to the SKSE slot
  argument. See the [SKSE Actor script](https://github.com/ianpatt/skse64/blob/master/scripts/modified/Actor.psc).

  | `equipSlot` | Meaning |
  |---|---|
  | 0 | Default. ByroRedux uses the resolved item's default. |
  | 1 | Request the right hand |
  | 2 | Request the left hand |

- **HND-READ-004.** Skyrim's hand state MUST classify the guard in priority order. A left-hand
  shield gives a shield guard. A left-hand torch or a weapon in either hand gives a weapon guard.
  All other hand states give a bare-hand guard. A live damage check depends on the Skyrim
  `BlockModel` in [CHARAL](../engine/charal.md).
- **HND-READ-005.** `GetEquippedSpell(aiSource)` MUST return the spell in left source 0 or right
  source 1. See [CK GetEquippedSpell](https://ck.uesp.net/wiki/GetEquippedSpell_-_Actor).
- **HND-READ-006.** Skyrim `GetEquippedShield()` MUST return the shield in the canonical left
  hand. Test a shield whose `ETYP` resolves left and whose biped mask omits the conventional
  shield bit. The VM regression `the_native_wave_reads_locks_doors_cells_states_shields_weapons_and_templates`
  checks this with another item in the conventional slot.
- **HND-READ-007.** `EquipSpell(akSpell, aiSource)` MUST use HND-SLOT-002 for hand sources. The
  [CK EquipSpell documentation](https://ck.uesp.net/wiki/EquipSpell_-_Actor) describes slot
  precedence. **Phase 0: verify** the original `EitherHand` behavior with the same spell and
  each requested hand in a local Skyrim install. Record occupancy after each call.

  | `aiSource` | Meaning |
  |---|---|
  | 0 | Request the left hand |
  | 1 | Request the right hand |
  | 2 | Voice, outside hand occupancy |

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Casting can use a spell equipped in a hand ([CK EquipSpell](https://ck.uesp.net/wiki/EquipSpell_-_Actor)) | The hand exposes the equipped spell to readers. Casting is assigned to the separate design in NG1. | None | A mod can read the hand's spell before per-hand casting is available. HND-READ-005. |
| 2 | Temporary carried lights have an authored duration ([CK Light](https://ck.uesp.net/wiki/Light)) | A point light follows the actor at a fixed offset and lasts while the torch is held. See [design §3](../engine/hand-equipment.md#3-state). | None | The light can drift from the hand pose or last longer than in the original. Q3 and the torch-light check in §7.2 cover the current behavior. |
| 3 | Original NPC hand choices and automatic torch use are Phase 0 verification items when parity is required | The [design's loadout rule](../engine/hand-equipment.md#2-npc-hands-hnd-state-003) supplies predictable hands. NPCs equip torches through scripts. | None | An NPC can choose different equipment or leave a dark area unlit. HND-STATE-003. |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | no | One weapon and a shield slot; its block rule already reads the shield bit. |
| Fallout 3 | no | One weapon, no shields. |
| Fallout: New Vegas | no | One weapon, no shields. |
| Skyrim (LE / SE / AE) | yes | All editions are in authored-data scope. Wave 1 uses Skyrim SE for the measured census and the planned runtime gates. LE is [parse-only in ROADMAP](../../ROADMAP.md#compatibility-matrix), and AE runtime acceptance still needs an edition-specific run. |
| Fallout 4 | no | Its authored slots serve a different model. See the [slot census](../engine/hand-equipment.md#1-what-the-data-says). |
| Fallout 76 | no | Parse-only support provides no actor runtime for this feature's hand-state checks. |
| Starfield | no | Its authored slots serve a different model. See the [slot census](../engine/hand-equipment.md#1-what-the-data-says). |

### 7.2 Acceptance

Use local Skyrim SE data for the real-data scenarios. A missing install is a skip. The parser
and VM fixtures use synthetic forms and can run without game data.

| Scenario | Runner and data | Pass condition |
|---|---|---|
| VM admission census | Run `refr_script_coverage` with `VM_REASONS=1` and `VM_REASON_FAMILIES` set to the reader or native under test, with local `Skyrim.esm` and `Skyrim - Misc.bsa`. See the [example invocation](../../crates/scripting/examples/refr_script_coverage.rs). | The weapon-rack activation script remains admitted. Declines at `HasMagicEffectWithKeyword` and `Reset` reach zero. The VM admits all 43 `TrapExplosiveGas` and 734 `TrapOilPool` attachments. |
| Papyrus hand readback | `cargo test -p byroredux-scripting --lib`. The VM tests cover weapon and spell readback and a shield whose biped mask differs from its hand slot. | Each hand reports the expected type code. The weapon, shield and spell readers return the corresponding form or `None`. |
| Equipment actions | Run `hands_follow_the_equip_slot_rules` in the `byroredux` binary tests. It runs the transition table below through both menu actions from Q2 and Papyrus `EquipSpell`. | Canonical occupancy after each transition matches the expected code pair and forms. |
| Authored slots | Run `skyrim_hand_slots_shields_and_two_handers_decode_from_the_official_load_order` in the `byroredux` binary tests with `--ignored` and the Skyrim SE masters. Run `authored_weapon_slots_control_player_equips_and_starting_inventory` and `skyrim_spell_override_resolves_its_authored_equip_slot` for the synthetic cases. | The official load order resolves the expected slots. The spell override and a weapon resolve the same remapped `EQUP` slot. HND-SLOT-006. |
| Save restoration | Run `skyrim_held_weapon_and_spell_survive_a_changed_plugin_slot` in the `byroredux` binary tests. The fixture uses the disk save/load commands and the live overlay on a rebuilt player with a changed plugin order. | Both hands recover the same records under their new FormIDs, and the left hand still indexes the original inventory stack. HND-STATE-002. |
| Cell eviction | Run `skyrim_npc_hands_survive_cell_eviction` in the `byroredux` binary tests. The synthetic cell uses the production eviction and placement-restore steps. | The same placed reference resolves to a fresh actor with the original right-hand spell and the left-hand weapon at its restored inventory index. HND-STATE-004. |
| Torch light | Run `a_held_torch_takes_the_left_hand_and_lights_it` in the `byroredux` binary tests. | A torch equips to the left, supplies one light while held, and removes that light when it leaves the hand. Q3. |
| NPC dual-wield flags | Run `skyrim_npc_dual_wield_requires_both_flags_and_a_free_left_hand`. Run the official-load-order fixture in the authored slots row to check the decoded race and combat style data. | Across all four flag combinations, the right-hand weapon stays consistent and the second weapon appears only with both flags set. A shield, two-handed primary, or missing second copy prevents a second weapon. HND-STATE-003. |
| Block guard | Run `skyrim_block_guard_uses_canonical_hands` in the `byroredux` binary tests. | A left shield takes priority over a right-hand weapon, a left torch or a weapon in either hand yields a weapon guard, and empty hands yield a bare-hand guard. Live damage reduction awaits the Skyrim `BlockModel`. HND-READ-004. |
| Live oil-pool ignition | Run [`skyrim-oil-pool.sh`](../smoke-tests/skyrim-oil-pool.sh). Each fresh Skyrim SE session walks through `BleakFallsBarrow01` reference `0009B0DE` with a torch or sword equipped and zero active magic effects. | The stock torch trial stays undamaged. With the equipment check enabled, the torch advances the pool through stages 1–3 and requests its authored explosion, and the sword leaves the pool undamaged. The torch trial verifies the authored reset branch restores stage −1. |

The base record `000862CC` disables both equipment and magic checks. The comparison uses temporary
Papyrus property overrides for `checkActorEquippedItems = True` and
`deleteSelfAfterIgnition = False` so the same placement exercises the authored hand check and reset
branch. The test observes the explosion request. Explosion rendering and blast effects are separate
from this gate.

A stock gas-trap check at `WarehouseTraps` reference `00082E19` exercised the equipment gate with
its authored properties. The torch reached stage 3, and the iron sword left the trap at stage −1.
A second crossing in the same enabled torch session ignited the oil pool again and restored it to
stage −1 a second time.

Use distinct synthetic forms for each one-handed sword, the shield, bow, torch and spell.
Assert the codes after each action in order, with the left code listed first.

| Action | Expected hand item type codes |
|---|---|
| Equip the left and right swords through the two menu actions | `(1, 1)` |
| Equip the shield | `(10, 1)` |
| Equip the two-handed bow | `(7, 7)` |
| Equip the torch, displacing the bow | `(11, 0)` |
| Equip an `EitherHand` spell to the right | `(11, 9)` |

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Baseline hand-reader cohort admitted | Script attachments currently admitted by the VM ÷ all script attachments in the fixed cohort that declined on a hand reader at `66668ed4b`. Count attachments by placed reference and attached script. Report other blockers separately. | Skyrim SE, by script family and total |

Publish the cohort count and remaining blockers in the Skyrim SE row of the
[ROADMAP compatibility matrix](../../ROADMAP.md#compatibility-matrix). The VM's aggregate
coverage measures a broader population and is a separate result.

---

## 9. Delivery sequence

**Phase 0 — Verify and design.** Revision 3 records the authored slot census and the hand-equipment
design. Exit: the design and the slot census are linked. Record any verification items reopened
in this review under #7397 before claiming the corresponding parity.

**Phase 1 — Weapon and shield state and readers (#7398).** HND-SLOT-001…004 for weapon and shield
occupancy, HND-STATE-001…002 for inventory items, and HND-READ-001…002. Exit: the official-load-order
slot check and the weapon, shield and bow readbacks pass. The v148 change is recorded in revision 5.

**Phase 2 — Torches, spells, equip actions and initial NPC loadouts (#7399).** HND-SLOT-005…006,
HND-READ-003, HND-READ-005 and HND-READ-007, the single-weapon and shield portion of HND-STATE-003,
and the spell portion of HND-STATE-002. Exit: the torch and spell readbacks, both inventory actions
(Q2), torch-light fixture (Q3) and hand-state save round trip pass. Revision 6 records the v149 build.

**Phase 3 — Remaining acceptance and integrations (#7397).** Complete authored left-only and
non-hand weapon cases (HND-SLOT-002), NPC dual wield (HND-STATE-003), cell eviction (HND-STATE-004),
canonical shield reads (HND-READ-006), and block guard classification (HND-READ-004). The oil-pool
gate covers the placed-destructible `ObjectReference.Reset` branch. Live block reduction depends on
CHARAL's Skyrim block model. Exit: the authored slot, four-way NPC flag, cell eviction,
shield read and guard fixtures pass, and the live oil-pool scenario passes. Track the live block
damage check with the CHARAL dependency.

Regression fixtures cover the canonical shield reader, authored weapon and spell slots, NPC
dual-wield spawn loadouts, hand-state restoration after cell eviction and save/load, guard
classification, and the equipment transitions through the menu and Papyrus (HND-READ-006,
HND-SLOT-002, HND-STATE-003, HND-STATE-004, HND-READ-004, HND-SLOT-006, and HND-STATE-002,
revisions 8–15).

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
| Papyrus hand readers and VM dependencies | [Scripting](../engine/scripting.md) |
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
| 7 | 2026-11-24 | Review round 1 folded in: Skyrim census and trap blocker, slot and shield semantics, block dependency, save and live gate criteria, per-game scope, phase ownership and metrics | codex: revise |
| 8 | 2026-11-24 | HND-READ-006 implemented. Skyrim's `GetEquippedShield` reads the canonical left hand, with a VM regression for a shield outside the conventional biped bit. | — |
| 9 | 2026-11-24 | HND-SLOT-002 implemented. Menu and scripted equips and starting loadouts honor explicit left-only and non-hand weapon slots. Fixtures cover the missing-slot fallback and repeated script requests. | — |
| 10 | 2026-11-24 | HND-STATE-003 implemented. NPC spawn resolves RACE `DATA` bit 30 through the Traits template and `NPC_.ZNAM` through the AI Data template. `CSTY.DATA` bit 2 supplies the style permission. Fixtures cover all four flag combinations, a worn shield, a two-handed primary, and inventory copy counts. | — |
| 11 | 2026-11-24 | HND-STATE-004 verified. A persistent Skyrim NPC recovers a runtime-equipped weapon and spell after cell eviction and respawn. The fixture checks both hands and the inventory index through the placed reference. | — |
| 12 | 2026-11-24 | HND-READ-004 verified. The block-guard fixture covers shield priority, a left torch, a weapon in either hand, and empty hands. Live block reduction depends on the Skyrim `BlockModel`. | — |
| 13 | 2026-11-24 | HND-SLOT-006 verified. A generated plugin overrides a right-hand spell to use a plugin-local slot with a left-hand parent. The fixture verifies that the load-order fold remaps the slot and resolves both the spell and a weapon through it. | — |
| 14 | 2026-11-24 | HND-STATE-002 verified. A disk save and live overlay recover a left-hand weapon and right-hand spell after their plugin moves to a different slot. The fixture checks both FormIDs and the weapon's original inventory index and count. | — |
| 15 | 2026-11-24 | The equipment-action fixture verifies the §7.2 transition sequence. Both menu actions equip distinct swords, the shield and bow displace earlier items, the torch displaces the two-handed bow, and Papyrus `EquipSpell` readies the spell on the right while the torch stays left. | — |
| 16 | 2026-11-24 | Added `Actor.HasMagicEffectWithKeyword` over applied magic effects and load-order-remapped `MGEF.KWDA` (#7403). The Skyrim SE census removes all 792 declines at that call and admits the 43 gas-trap attachments. The 734 oil-pool attachments now stop at `ObjectReference.Reset`. | — |
| 17 | 2026-11-24 | Implemented placed-destructible `ObjectReference.Reset` and admitted all 734 oil-pool attachments. Skyrim SE coverage reaches 17,089 of 19,412 scripted references (88.0%). The live gate crosses the model phantom and observes authored burn stages and reset with a torch. The sword control and stock torch run leave the pool undamaged. | — |
