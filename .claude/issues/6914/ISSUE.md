# #6914: M65 Phase 5: detection, sneak attacks and perks, Wave 2 (Oblivion, Skyrim)

Part of #6908. PRD [§9 Phase 5](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/sneak-and-detection.md#9-delivery-sequence). Wave 2 (Oblivion, Skyrim). Depends on Phases 2 and 3 (#6911, #6912).

## Acceptance Criteria
- [ ] SNK-DET-005: Oblivion runs the CS formula over `Oblivion.esm`'s settings, the CS defaults as fallback
- [ ] SNK-DET-007: Skyrim runs the CK formula over its settings (`fSneakBaseValue` per V8)
- [ ] SNK-DET-015: Skyrim combat search follows stealth points and the `iCombatStealthPoint*` settings (regeneration per V11)
- [ ] SNK-ATK-002: Oblivion sneak attacks multiply by `fPerkSneakAttackMelee/Marksman<Mastery>Mult`, not for two-handed weapons or spells, not stacked with a power attack (V13)
- [ ] SNK-ATK-004: Skyrim sneak attacks take the weapon class's multiplier composed through *Mod Sneak Attack Mult* (18) (V15)
- [ ] SNK-PERK-004: Skyrim *Mod Detection Light* (47) scales the light term before Sneak and Invisibility apply; 48 and 57 per V18
- [ ] SNK-PERK-005: Skyrim 18 consumed at SNK-ATK-004's call site; its catalog row Consumed
- [ ] SNK-PERK-007: Skyrim *Apply Sneaking Spell* (69, PERK-HOOK-009) reads the stance this PRD supplies
- [ ] SNK-STN-008: the player's locomotion in the stance plays the game's sneak animations (clips and graph variable per V5)
- [ ] A2: a `cargo test` per ruleset row (Oblivion, Skyrim) matches its sourced formula's transcription test
- [ ] A3: the census reports Oblivion's and Skyrim's settings with values or fallbacks
- [ ] A8 (Oblivion, Skyrim): the sneak-attack `cargo test` per game

**Exit:** §7.2 scenarios 2, 3 and 8 pass on Oblivion and Skyrim, and formula coverage is published for both.

