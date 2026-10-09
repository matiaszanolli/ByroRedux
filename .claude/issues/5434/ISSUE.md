# #5434: GAME-D4-2026-10-08-01: The `KILL` story event is raised by one of the four live death producers and never carries its location

**Labels**: low,gameplay,combat,quests,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5434

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Combat & death (Dim 4)
- **Location**: `byroredux/src/combat.rs:365-386`; `byroredux/src/systems/water.rs:59-60`; `byroredux/src/systems/character.rs:1567`; `byroredux/src/extensions/commands.rs:537`
- **Status**: NEW
- **Description**: Only `combat_damage_system` stamps `KILL`. Deaths from NPC drowning, player drowning and the SDK AV batch insert `Dead` without raising it. On Skyrim (the SM title) a drowned NPC therefore never reaches KILL-rooted nodes. The KILL stamp also sets `location_1: None`, while the AHEL producer resolves the session LCTN (`resolve_current_lctn`). `WIKill06` authors a KILL→L1 alias fill (census). The location-alias runtime is Phase-3+ scope, so this half is latent today.
- **Suggested Fix**: Raise KILL from a shared helper at every live death producer (not the corpse restores), with `location_1: resolve_current_lctn(world)`.

## Completeness Checks
- [ ] **SIBLING**: every live Dead insert routes through the shared KILL helper; corpse restores do not
- [ ] **TESTS**: A regression test pins this specific fix
