# #6912 M65 Phase 3: sneak attacks and sneak-gated perks, Wave 1

Part of #6908. PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/sneak-and-detection.md#9-delivery-sequence). Wave 1. Depends on Phase 2 (#6911) and the Perk Entry Points PRD's activate runtime (`683d9a92b`, #6657).

## Acceptance Criteria
- [ ] SNK-ATK-001: `HitEvent.sneak_attack` is true for a strike the player makes in the stance on a detector whose level toward the player is below Noticed, false otherwise (Unseen per V12)
- [ ] SNK-ATK-003: on FO3 / FNV a sneak attack is a critical hit when the weapon allows criticals; damage multiplier per V14
- [ ] SNK-ATK-005: Papyrus `OnHit`'s `abSneakAttack` and the SDK hit event carry SNK-ATK-001's value
- [ ] SNK-ATK-006: `VatsModel::stealth_mult` stays unconsumed until V16 says what it scales
- [ ] SNK-PERK-001: Robotics Expert is offered when its tabs pass with the player in the stance (its target tab's `GetDetected` reads SNK-USE-002)
- [ ] SNK-PERK-002: Cannibal, Ghastly Scavenger, Dine and Dash and Mister Sandman stay unoffered until V17 settles their commands
- [ ] SNK-PERK-003: *Ignore Running During Detection* (FO3 / FNV 31, Skyrim 15) removes the running term when it composes non-zero (Silent Running)
- [ ] SNK-PERK-006: FNV 69, 70, 71 stay Pending in the hook catalog until V19
- [ ] SNK-PERK-008: Fallout 4's 15, 18, 47, 48, 57 stay Pending (Q2); FO4's *Apply Sneaking Spell* (69) gets the stance
- [ ] A7: a `docs/smoke-tests/` script holds FNV on the Phase 0 robot cell, adds Robotics Expert with `obscript.cmd`, places the player with `cam.pos`, `stealth.sneak on`, `input.press activate`, answers "Deactivate" with `ui.answer`, and reads `GetUnconscious` 1 on the robot (the Perk Entry Points PRD's A8 live leg)
- [ ] A8 (FO3 / FNV): a `cargo test` per game shows a strike on an unaware detector setting `sneak_attack` and taking the game's multiplier, and on an aware one doing neither

**Exit:** §7.2 scenarios 7 and 8 pass on FNV, and the sneak-gated-perks metric is published for FO3 and FNV.

