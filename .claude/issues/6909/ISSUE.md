# #6909 M65 Phase 0: sneak-detection design, settings census and the verify items (no engine code)

Part of #6908. PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/sneak-and-detection.md#9-delivery-sequence). No engine code. Depends on HEAD only.

Decisions Q1–Q7 are taken (2026-11-14, each the recommendation); the design applies them. Every **Phase 0: verify** item of the PRD is a criterion here (the §9 Phase 0 verify table, V1–V23).

## Acceptance Criteria
- [ ] `docs/engine/sneak-detection.md` written and registered in `docs/engine/index.md`: stance component, detection tick and its schedule placement, level and alert state, the actor-to-actor sight test Q7 keeps, the CPU light level Q3 decided, consumer migration, FO3 / FNV sneak-attack damage (the Related rows with no owner at HEAD)
- [ ] A3 baseline: the settings census run once on `Fallout3.esm`, `FalloutNV.esm`, `Oblivion.esm` and `Skyrim.esm`, each setting the ruleset rows name reported with its value or documented default; table checked in (input to SNK-DX-003)
- [ ] The vanilla references scenarios A5 (an FNV interior with cover) and A7 (an FNV cell with a robot) run on, named
- [ ] V1 (SNK-STN-004): whether disabling sneaking stands an already sneaking player up, by an in-game test per game
- [ ] V2 (SNK-STN-005): the `MOVT` values vanilla Skyrim races name in `SNMV`, by xEdit read of `Skyrim.esm`
- [ ] V3 (SNK-STN-006): whether FO3 and FNV let the player run in the stance, by an in-game test (the requirement changes if the game forbids it)
- [ ] V4 (SNK-STN-007): first-person camera height in the stance, from a setting the census finds or an in-game measurement
- [ ] V5 (SNK-STN-008): sneak clip names (KF games) and Skyrim's behaviour-graph variable, by an archive scan
- [ ] V6 (SNK-DET-004): values of `fSneakAlertMod`, `fSneakCombatMod`, `fSneakLightMoveMult`, `fSneakLightRunMult`, `fSneakActionMult`, `fSneakDistanceAttenuationExponent` on both Fallout masters, from the census
- [ ] V7 (SNK-DET-006): the FO3 / FNV detection cadence, from a timer setting the census finds or the executable's default table
- [ ] V8 (SNK-DET-007): Skyrim's `fSneakBaseValue` (UESP −15; no CK settings row), from `Skyrim.esm` or the executable's default table
- [ ] V9 (SNK-DET-008): whether Oblivion and FO3 / FNV have a view cone, from a setting the census finds or an in-game test
- [ ] V10 (SNK-DET-012): the rule that moves a detector to level Lost on each game
- [ ] V11 (SNK-DET-015): Skyrim's stealth-point regeneration beyond `fCombatStealthPointRegenAlertWaitTime`
- [ ] V12 (SNK-ATK-001): whether level Unseen counts as unaware for a sneak attack, per game
- [ ] V13 (SNK-ATK-002): Oblivion's Apprentice marksman multiplier and Novice bonus (CS *Sneak* vs the GMST pages; the loading screen)
- [ ] V14 (SNK-ATK-003): the FO3 / FNV sneak-attack damage multiplier (`fCombatDamageBonusSneakingMult`, `fCombatDamageBonusMeleeSneakingMult`, `fDamageSneakAttackMult`)
- [ ] V15 (SNK-ATK-004): where Skyrim's base sneak-attack multipliers live (`fCombatSneakAttackBonusMult` is 100 with no description)
- [ ] V16 (SNK-ATK-006): what `fVATSStealthMult` scales (GECK *VATS Settings*: default 0.5, empty description)
- [ ] V17 (SNK-PERK-002): what `StartCannibal` and `StartMisterSandman` change, and whether "eaten" survives a save
- [ ] V18 (SNK-PERK-004): which terms Skyrim 48 and 57 scale (CK leaves both blank)
- [ ] V19 (SNK-PERK-006): what FNV 69, 70 and 71 do (GECK description cells empty)
- [ ] V20 (SNK-HUD-002, PRD §3.2, §6 row 9): each game's indicator table by (level, hostility); on FO3 / FNV which pair renders [DETECTED] versus flashing [DANGER] (GECK *GetDetectionLevel* and Fallout wiki *Sneak (Fallout 3)* disagree)
- [ ] V21 (SNK-HUD-003): the sneak element and trait of the vanilla `hud_main_menu.xml` on each MenuXml game
- [ ] V22 (§6 row 2): whether each detector's timer runs on its own phase
- [ ] V23 (§6 row 4): whether detection state survives a save

**Exit:** the design is merged, the verify table has no unanswered row without an issue, and the census table is checked in.

