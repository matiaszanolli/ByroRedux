# TD2-2026-10-15-12: 62 production sites read the player entity as `try_resource::<PapyrusPlayerEntity>().map(|p| p.0)` with no accessor

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-12`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src` (21 files: `relationships.rs:60`, `keywords.rs:96`, `package.rs:667`, `dialogue.rs:607`, `bleedout.rs:197`, `combat.rs:58`, `trigger.rs:164`, `fragment/{hits,distance,updates,package,perk,terminal,systems}.rs`, `fragment/effects/{resolve,scene}.rs`, ...); `byroredux/src/extensions/systems.rs`, `systems/{follow,cinematic,npc_dialogue,trap_hits,scripted_sound,archetype_idle}.rs`
- **Status**: NEW
- **Age**: spread across M30..M47.2; `crates/scripting/src/papyrus_demo/mod.rs:72` defines the type
- **Effort**: small
- **Description**: the 4-line `world.try_resource::<..>().map(|player| player.0)` (with the long `crate::papyrus_demo::` path) is re-spelled everywhere; a second player resource (`byroredux::systems::character::PlayerEntity(pub Option<EntityId>)`) is kept in sync by `extension_player_entity_sync_system`, so call sites cannot tell which to read.
- **Evidence**: `grep -rn 'PapyrusPlayerEntity>()' crates byroredux --include='*.rs' | grep -v test` = 62 `try_resource` reads.
- **Impact**: moving the player resource (the planned P5 save/load work already special-cases it) is a 62-site edit.
- **Related**: #4694 (PlayerRef 0x14 resolution), TD2-06 on the 10-11 report.
- **Suggested Fix**: `impl PapyrusPlayerEntity { pub fn get(world: &World) -> Option<EntityId> }` and migrate the production sites; fold into TD2-2026-10-15-06's `flush_advances`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
