# #6910 M65 Phase 1: the sneak stance, IsSneaking and its save

Part of #6908. PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/sneak-and-detection.md#9-delivery-sequence). Depends on Phase 0's design (#6909). On every game.

## Acceptance Criteria
- [ ] SNK-STN-001: `InputAction::Sneak`, bound by default to Left Ctrl, toggles the player's stance (Q1)
- [ ] SNK-STN-002: the stance is one component on the player; `IsSneaking` (ObScript `0x111e`, CTDA 286) answers 1 while it is present and 0 otherwise
- [ ] SNK-STN-003: Papyrus `Actor.IsSneaking` returns the stance; `Actor.StartSneaking` enters it, or leaves it when already sneaking
- [ ] SNK-STN-004: the stance is not entered while the `Control::Sneaking` layer or `DisablePlayerControls`' sneaking flag disables it (V1 settles the stand-up case)
- [ ] SNK-STN-005: movement speed in the stance scaled by the load order's `fMoveSneakMult` (Oblivion, FO3 / FNV) or the race's `SNMV` movement type (Skyrim)
- [ ] SNK-STN-006: on FO3 / FNV, Sprint works in the stance and counts as running in the detection value (provisional until V3)
- [ ] SNK-STN-007: the first-person camera is lower in the stance (height per V4)
- [ ] SNK-SAVE-001: the stance survives a save and a load, with its `FORMAT_MAJOR` bump (Q4)
- [ ] SNK-DX-002: `stealth.sneak <on|off>` sets the stance through the same path as the key
- [ ] A1: `cargo test` cases show the action toggling the stance, `IsSneaking` 1 then 0, a disabled layer refusing it, and Sprint counting as running on the FO3 / FNV rows

**Exit:** §7.2 scenario 1 passes, and `IsSneaking` reads 1 in a held FNV session after `stealth.sneak on`.

