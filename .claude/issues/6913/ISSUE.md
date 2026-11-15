# #6913: M65 Phase 4: the detection indicator

Part of #6908. PRD [§9 Phase 4](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/sneak-and-detection.md#9-delivery-sequence). Depends on Phase 2 (#6911) for levels and on Phase 0's V20 / V21 for the tables. Scaleform meters stay with the [Game UI Integration PRD](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md).

## Acceptance Criteria
- [ ] SNK-HUD-001: the native HUD shows the detection indicator while the player is in the stance, in the game's vocabulary, and hides it otherwise
- [ ] SNK-HUD-002: each (worst level, detector hostile to the player) pair maps to the game's state by a table in the ruleset row (tables per V20)
- [ ] SNK-HUD-003: the FO3 / FNV and Oblivion MenuXml HUDs drive their vanilla sneak element (element and trait per V21)
- [ ] SNK-HUD-004: on Skyrim and Fallout 4 the vanilla Scaleform meter may stay empty; the native indicator stands in (PRD §6 row 9)
- [ ] SNK-HUD-005: on Starfield the indicator shows only while the player holds rank 1 of the Stealth skill, a gate in Starfield's ruleset row
- [ ] A9: a `cargo test` builds the native HUD's `PanelSnapshot` for each game's ruleset row and asserts its indicator field for every (level, hostility) pair and its absence outside the stance; no image compared

**Exit:** §7.2 scenario 9 passes for FO3, FNV, Oblivion and Skyrim.

