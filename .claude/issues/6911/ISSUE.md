# #6911 M65 Phase 2: detection tick and consumers, Wave 1 (FO3, FNV)

Part of #6908. PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/sneak-and-detection.md#9-delivery-sequence). Wave 1 (Fallout 3, Fallout: New Vegas). Depends on Phase 0 (#6909) and Phase 1 (#6910), and on the CHARAL FNV ruleset's formula capture. Q5 (witness at Seen) and Q7 (levels toward the player only; one actor-to-actor sight test for every other case) apply.

## Acceptance Criteria
- [ ] SNK-DET-001: one detection tick computes, for every detector within the game's maximum distance of the player, a detection value and a level toward the player, and is the only producer of levels
- [ ] SNK-DET-002: the formula is a property of the character ruleset row the load order selects; no game named in the tick
- [ ] SNK-DET-003: FO3 runs the GECK formula over FO3's game settings, without level terms
- [ ] SNK-DET-004: FNV runs the formula `stealth.rs` transcribes, its coefficients read from the load order (values per V6)
- [ ] SNK-DET-006: each detector re-evaluated at the game's cadence (FO3 / FNV per V7)
- [ ] SNK-DET-008: a detector sees the player only through an unobstructed ray, inside its view cone where the game authors one (V9)
- [ ] SNK-DET-009: the light term reads a CPU-computed light level from the lights the renderer is given (Q3)
- [ ] SNK-DET-010: the sound term includes movement sound from equipped weight
- [ ] SNK-DET-011: a `CreateDetectionEvent` enters the tick as an action sound at its position (`PendingDetectionEvents` read)
- [ ] SNK-DET-012: the level follows from the value by the game's thresholds (FO3 / FNV `fSneakNoticedMin`; level Lost per V10)
- [ ] SNK-DET-013: each detector carries an alert state (Normal, Alert, Combat, Lost) and the formula's detector-state factor reads it
- [ ] SNK-DET-014: an actor flagged by `SetNotShowOnStealthMeter` does not count toward the indicator while not hostile to the player
- [ ] SNK-DET-016: the sound term includes the action sound from the weapon's `VNAM` sound level
- [ ] SNK-DET-017: on FO3 / FNV sound without line of sight is attenuated by `fSneakSoundLosMult` (Oblivion's no-walls rule is a ruleset field)
- [ ] SNK-USE-001: a crime is witnessed only by an actor whose level toward the player is Seen (Q5)
- [ ] SNK-USE-002: `GetDetected` / `GetDetectionLevel` toward the player answer from the level; other targets from Q7's sight test
- [ ] SNK-USE-003: `IsActorDetected` answers 0 on the player and from Q7's sight test otherwise
- [ ] SNK-USE-004: Papyrus `Actor.IsDetectedBy` is a native (level toward the player; Q7 otherwise)
- [ ] SNK-USE-005: ambient hostility starts combat with the player only from a detector at Seen; other pairs by Q7
- [ ] SNK-USE-006: guard rush reads the guard's level toward the player
- [ ] SNK-USE-007: a source-scan test holds calls of `PhysicsWorld::line_of_sight_blocked` and `DetectionConfig::range` outside the detection module at zero; `GetLOS` and attraction call the module's exported line test
- [ ] SNK-USE-008: dead-body discovery stays an actor-toward-body test through Q7's sight test; a discovering actor enters alert state Alert
- [ ] SNK-USE-009: flee counts the player as a threat only at level Seen; other threats by Q7
- [ ] SNK-USE-010: FO3 and FNV fix catalogs carry an implemented engine-behaviour entry, with its test, for `GetDetectionLevel` answering the real level when the player is the target
- [ ] SNK-SAVE-002: levels and alert states are not saved; the first tick after a load re-derives them (Q4)
- [ ] SNK-DX-001: `stealth.status` lists, per counted detector, distance, line of sight, each formula term, value, level and alert state
- [ ] SNK-DX-003: the install log names the ruleset row and every setting read, with the ones that fell back to a documented default
- [ ] A2: a `cargo test` per ruleset row (FO3, FNV) matches the transcription test of its sourced formula
- [ ] A3: the ignored real-data census test reports every setting's value or fallback and fails only on a name with neither
- [ ] A4: the SNK-USE-007 source-scan test
- [ ] A5: a `docs/smoke-tests/` script on the Phase 0 FNV interior reads, through `stealth.status`, a detector at level Lost behind cover and at Seen in its sight
- [ ] A6: a `cargo test` in the shape of `a_witnessed_theft_adds_the_bounty_and_marks_the_faction`: no bounty with every detector below Seen, the authored bounty with one at Seen

**Exit:** §7.2 scenarios 2 to 6 pass on FO3 and FNV, and the consumers-on-the-level metric is 100 % in ROADMAP's M65 row.

