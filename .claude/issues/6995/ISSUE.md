# #6995: Oblivion actors are not detectors: AIDT undecoded, no actor values (M65 follow-up)

## Summary
M65 Phase 5 (#6914) built Oblivion's detection formula, levels and sneak attacks, but live Oblivion actors never become detectors. Live in `AnvilTheCountsArms`, `stealth.status` lists none, although the row installs (`Detection rules: Oblivion (30 settings …)`).

## Cause
- **No detectors.** The detection tick, the crime witnesses and ambient hostility all enumerate actors through `systems::faction_hostility::living_actors`, which means actors carrying `CombatDisposition`. That component is stamped from the NPC's `AIDT` (`npc_spawn/stamps.rs`). `decode_ai_data` (`crates/plugin/src/esm/records/actor/mod.rs:68`) returns `None` for Oblivion, so no Oblivion actor carries it.
- **No mapping yet.** Oblivion's `AIDT` aggression and confidence are 0–100 numbers (an actor attacks anyone whose disposition toward it is below its aggression; CS *Aggression*), not the FO3 / Skyrim enums `ActorAiData` holds. Mapping them is a design decision, not a decode.
- **Skill terms read 0.** Oblivion's formula reads Sneak, Chameleon, Blindness and Night Eye, and `Oblivion.esm` has no `AVIF` records (ROADMAP: the legacy actor-value resolver comes first). So the skill and Chameleon terms read 0, and every sneak attack takes the Novice ladder.

## Acceptance
- Oblivion NPCs carry an `ActorAiData` from their `AIDT`, with the 0–100 model mapped by a sourced rule, so they become detectors (and crime witnesses).
- The CS formula's Sneak / Chameleon / Blindness inputs resolve on Oblivion once the legacy actor-value resolver lands.
- Live: `stealth.status` in an Oblivion interior lists detectors with values, and a smoke like `m65-stealth-detection.sh` passes on Oblivion.

