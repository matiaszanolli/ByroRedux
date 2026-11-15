# #6958: TD2-2026-11-15-05: Six production reads of the player entity bypass `PlayerEntity::get`, and the #6362 guard cannot see them (regression of #6362)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6958
- **Labels**: low,gameplay,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: #6362 (CLOSED; this is a regression of the "one read" rule plus a guard blind spot)
- **Age**: 65adad604 (2026-05-21, character.rs), fcd1cce4a (2026-10-24, find_package), dbe562c1f (2026-10-12, combat.rs); the guard landed 67722b6e4 (2026-10-27)
- **Location**: `byroredux/src/combat.rs:708` (`note_player_damage`), `byroredux/src/systems/character.rs:228` and `:773` (two five-line `let Some(player_res) = try_resource::<PlayerEntity>() else {return}; let Some(e) = player_res.0 else {return}; drop(player_res)` ladders), `byroredux/src/systems/find_package.rs:111` (`is_actor`) and `:483` (`.is_some_and(|p| p.0 == Some(target))`), `byroredux/src/cell_loader/unload.rs:57` (`player_owned_entities`)
- **Description**: #6362 replaced 66 hand-spelled reads with `PlayerEntity::get` (`components/player.rs:28`) and added `no_source_hand_spells_the_player_entity_read` (`byroredux/src/workspace_hygiene_tests.rs:1191`). The guard's needle is the whitespace-stripped text `PlayerEntity>().and_then(`, so it matches only the `.and_then(|p| p.0)` spelling. The six sites above use the `let Some(res) = ...; res.0` ladder or `.is_some_and(|p| p.0 == Some(x))`, so they slipped through. They are the "two resources that differ in FlyCam are only safe to reason about if each has exactly one read" case the guard's doc names.
- **Suggested Fix**: `PlayerEntity::get(world)` at the four ladder sites, and a `PlayerEntity::is(world, entity) -> bool` (`Self::get(world) == Some(entity)`) for the two compare sites. Widen the guard needle to `try_resource::<` followed by a path ending in `PlayerEntity>()` outside `components/player.rs` (excluding the `Papyrus` prefix), and keep `PapyrusPlayerEntity` out of it.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope or scheduler `Access` changes, TypeId-sorted acquisition and the access declarations stay correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
