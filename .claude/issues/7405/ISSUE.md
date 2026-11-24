# #7405 SK-D3-01: A partial removal from a dual-wielded stack leaves one item in both hands

**Labels**: low,gameplay,inventory,bug,game:skyrim,legacy-compat

**Source**: `docs/audits/AUDIT_SKYRIM_2026-11-23.md` — finding `SK-D3-01` (HEAD `e01ca71f0`, re-validated against `origin/main` `4e68ab7b7` at publish).

- **Severity**: LOW
- **Dimension**: NPC Equip (Hand Equipment Phase 1)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:78` (partial branch of `remove_items`), `byroredux/src/barter.rs:452` (sell), `byroredux/src/crafting.rs:403` (improve-one-of-a-stack split); invariant set by `crates/scripting/src/hands.rs` (`equip_to_hand`, `facts.stack_count < 2`; Phase 2 `d3460fa73` moved it there from the inventory's `equip_into_hand` and kept the rule)
- **Status**: NEW
- **Description**: `equip_into_hand` lets one inventory row sit in both `EquipmentSlots.weapon` and `EquipmentSlots.left_hand` only while the row's `count >= 2` ("One item of a stack cannot be in both hands"). Nothing re-checks that invariant when the count later drops. `remove_items` releases equipment only for a row it empties (`emptied`); the partial branch (`stack.count -= take`) leaves both hands pointing at the row. Barter's sell and the smithing split reach it.
- **Evidence**: player holds `ItemStack{DAGGER, 2}`; `ToggleEquip` then `EquipLeft` on the row gives `weapon == left_hand == InventoryIndex(n)` (exercised by `hands_follow_the_equip_slot_rules`: "the stack's second dagger goes right"). `barter.rs:452` `remove_items(world, player, form_id, 1, Some(session.stock))` takes one: row count 1, `emptied` empty, no `release`. `hand_item` then returns the dagger for both `GetEquippedItemType(0)` and `(1)` and `GetEquippedWeapon(true)`/`()`. The state persists in the save (both fields are saved).
- **Impact**: after selling or scripting away one of two identical one-handers wielded in both hands, scripts and the inventory status see one dagger in two hands; `is_equipped` also blocks `drop_item` on the remaining row. Needs two identical weapons dual-wielded, so rare, and no crash; wrong reader answers until the player re-equips.
- **Related**: PRD HND-SLOT-003, #7398 (closed), #5863 (the emptied-row release this mirrors).
- **Suggested Fix**: after any count decrement of a row that is in both `weapon` and `left_hand` (central place: `remove_items` partial branch and the crafting split), if the new count is below 2 release `left_hand` (keep the right). Add a case to `hands_follow_the_equip_slot_rules`.

## Completeness Checks
- [ ] **SIBLING**: every count decrement of an equipped row (`remove_items` partial branch, barter sell, the crafting split) re-checks the both-hands invariant
- [ ] **TESTS**: a case in `hands_follow_the_equip_slot_rules` (or beside it) removes one of two dual-wielded items
