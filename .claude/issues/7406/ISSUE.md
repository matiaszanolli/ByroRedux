# #7406 SK-D3-02: The SDK inventory projection reports a left-hand weapon as unequipped

**Labels**: low,tech-debt,bug,game:skyrim,legacy-compat

**Source**: `docs/audits/AUDIT_SKYRIM_2026-11-23.md` — finding `SK-D3-02` (HEAD `e01ca71f0`, re-validated against `origin/main` `4e68ab7b7` at publish).

- **Severity**: LOW
- **Dimension**: NPC Equip (host contract; owner /audit-tooling)
- **Location**: `byroredux/src/extensions/capture.rs:308` (`weapon_equipped = slots.weapon == Some(index)`), `crates/sdk/src/inventory.rs:125-131` (`is_equipped = biped_slots != 0 || weapon_equipped`)
- **Status**: NEW (same family as open #6938, which is the BO64 variant of "new equipped state not folded into the SDK projection")
- **Description**: #7398 added a second weapon location, `EquipmentSlots.left_hand`, and updated the in-engine consumers that go through `equipped_indices()` (conditions, weight, DT, loot, save validation). The mod-host capture hand-rolls the equipped test from `occupants` and `weapon`, so a weapon held only in the left hand has `biped_slots == 0` and `weapon_equipped == false`, and `InventoryEntry::is_equipped()` is false. A dagger in each hand shows one entry as equipped (right) and, if they are separate rows, the other as not equipped.
- **Evidence**: `capture.rs:296-308` (mask from `occupants`, boolean from `slots.weapon`); `left_hand` appears nowhere under `byroredux/src/extensions/` or `crates/sdk/` (grep).
- **Impact**: SDK/Studio and mod-host readers of a Skyrim player's or NPC's inventory under-report equipment. No gameplay effect in the engine itself.
- **Related**: #6938, #7397.
- **Suggested Fix**: fold `left_hand == Some(index)` into the `weapon_equipped` boolean (or add a `left_equipped` field when the WIT contract is next bumped); extend the SDK capture test.

## Completeness Checks
- [ ] **SIBLING**: the other hand state (`left_spell` / `right_spell`, Phase 2) is considered for the projection too
- [ ] **TESTS**: the SDK capture test covers a left-hand-only weapon
