# CHAR-2026-09-29-D5-01: `feature-matrix.md` says the FO76/Starfield player actor-value seed is "partial"; #4453 made it empty the next day

**Labels**: low,documentation,doc-rot,character,game:fo76,game:starfield

**Source report**: `docs/audits/AUDIT_CHARACTER_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: FO76, Starfield
- **Source**: not numeric. The code is the authority: `actor_value_derive.rs` maps `NpcStatModel::None => Vec::new()`, and `CharacterRulesProfile::FALLOUT76` / `STARFIELD` both carry `npc_stats: NpcStatModel::None` (#4453).
- **Location**: `docs/feature-matrix.md:341` ("CHARAL: player actor-value seed … FO76/Starfield partial")
- **Status**: NEW. The row was written by `d55041d5e` (#4676, 2026-09-22) and went stale with `c4f30cbde` (#4453, 2026-09-23).
- **Description**: This is a wiring event rotting prose. With an empty derivation, `build_player_character_template` returns only `factions`, `spells` and `spell_modifiers`: no `ActorValues`, `ActorVitals`, `CharacterLevel` or `Background`. The FO76/Starfield player seed is absent, not partial.
- **Evidence**: See Location and Source. `inventory.rs:360-366` is the `pairs.is_empty()` early return.
- **Impact**: The feature matrix overstates coverage for two games.
- **Related**: #4676, #4453
- **Suggested Fix**: Change the row to "FO76/Starfield no (NPC stat wire layout uncaptured, #4453)".

**Validated at HEAD 9fcfdc3fc**: `docs/feature-matrix.md` "CHARAL: player actor-value seed" row still says "FO76/Starfield partial"; FO76/Starfield profiles carry `NpcStatModel::None`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
