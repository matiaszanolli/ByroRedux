# #6996: M65 follow-up: decode Fallout 4's detection formula (and Starfield's)

Part of #6908 (M65). Phase 6 (#6915) recorded Fallout 4 and Starfield detection as the actor-to-actor sight test (formula coverage 0, PRD §10 Q2). It also found that a source now exists for Fallout 4. This issue tracks decoding and building both games' rows.

## What the executables show (2026-11-15)
- **`Fallout4.exe` is readable.** Its `.text` is plain beside the `.bind` section, and the settings are static `{value, name}` objects, so the Phase 5 tooling (RIP-relative xref) works on it.
  - The detection model is its own system, not Skyrim's:
    - sound and visual stealth-point drains are split (`fCombatStealthPointDrainSoundMult` / `…VisualMult`, `iCombatStealthPoint{,Sneak}Detection{Sound,Visual}Threshold`);
    - cover (`fDetectionCover*`, `fSneakVisualCoverMod`);
    - object visibility (`fDetectionObjectVisibilityMult*`);
    - vertical and angular visual distance (`fDetectionVisualDistanceAngle*`, `…UpMult` / `DownMult`);
    - sound and visual base values and ratings (`fSneakSoundBaseValue`, `fSneakVisualBaseValue`, `fSneakSoundRatingMult`, `fSneakVisualRatingMult`);
    - an Agility skill term (`fSneakAgilitySkillMin` / `Max`).
  - About 130 detection settings.
  - Formula routines start at `0x1406c8580` and `0x1406c89f4` (both read `fSneakSkillMult`).
  - Sneak attacks add `fCombatSneakGunMult`, `fCombatSneakGrenadeMult` and `fCombatSneakMineMult` to Skyrim's class table.
- **`Starfield.exe`** carries the setting names (`fSneakAgilitySkillMin`, `fSneakAmbientSoundMult`, …), but not as `{value, name}` static objects. Reading its defaults and code references needs new tooling first.

## Acceptance
- A `StealthKind::Fallout4` row:
  - the decoded formula, its stealth points and its levels;
  - a transcription test (PRD A2) and the census over `Fallout4.esm` (A3);
  - sneak attacks with the gun, grenade and mine classes.
- The FO4 perk hooks 15, 18, 47, 48 and 57 leave Pending (SNK-PERK-008).
- The same for Starfield once its setting layout is read, or a recorded reason why not.
- ROADMAP's formula-coverage number updated.

