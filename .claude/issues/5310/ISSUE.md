# #5310: ECS-2026-10-05-D7-01: `release_entities` despawns a gear root without detaching it from its surviving parent, so the player body's `Children` grows a dangling id per released item

**Labels**: low,ecs,inventory,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5310

**Source**: `docs/audits/AUDIT_ECS_2026-10-05.md` — `ECS-2026-10-05-D7-01` (HEAD `a2c24b16e`)

- **Severity**: LOW. Every consumer tolerates the dangling id, and entity ids are never recycled, so there is no aliasing. The `Children` vec grows without bound over a session.
- **Dimension**: 7 — Component Lifecycles
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:522-545` (`release_entities` → `despawn_batch`)
  - `byroredux/src/npc_spawn/loot_appearance.rs:845-848` (`step_releases`)
  - `byroredux/src/npc_spawn/resumable/mod.rs:388-390` (`parent_part`: `Parent` + `add_child`)
- **Status**: NEW (introduced by `f78e018ac`, #5028)
- **Description**:
  - `release_entities` was written for cell teardown, where the whole parent chain dies together. `World::despawn_batch` frees component rows only and never edits another entity's `Children`.
  - #5028 reuses it for a subtree whose parent survives. A player gear root is parented with `parent_part(player_gear_parent(..) /* PlayerBodyRootEntity */ or wearer, root)`, and both are process-lifetime entities.
  - So every equip → drop cycle (import, then release) leaves the despawned root's id in the body root's `Children`, and nothing prunes it. Spawn-time player gear released by the same path has the same effect. `Children` is excluded from save, so a reload of the save does not clean it either; only a new process does.
- **Evidence**: `step_releases` → `subtree_entities_under(world, root)` → `release_entities(world, ctx, &victims, "gear release")`. `despawn_batch` has no `Children` / `Parent` handling, and the doc at `world.rs:323` says the hierarchy refs are left as they are by design.
- **Impact**:
  - Propagation walks the body subtree whenever the player moves; each dangling child costs a lookup miss there.
  - The dangling ids inflate `child_reference_count` (the guard budget).
  - `set_player_view` restamps also walk them, but `mesh_entities_under` filters on `MeshHandle`, so there are no orphan `HiddenFirstPerson` rows.
- **Suggested Fix**: In `step_releases`, before releasing, remove the root from its `Parent`'s `Children`. Better, give `release_entities` a detach step for victims whose `Parent` is not itself a victim, so every future partial-subtree caller is covered.

Routed, not an ECS finding (to `/audit-starfield`): `asset_provider::tests::starfield_mat::mat_path_merges_cdb_authored_textures_when_indexed` is flaky in the full parallel `cargo test -p byroredux`. It failed once here, and passed 3 of 3 runs in isolation and 3 of 3 module runs. The test asserts `base_color` ends in `widget_color.dds`; the failure got `iris_iron_color.dds`. The cause is shared fixture state: `register_indexed` and `register_indexed_iron_color` (`tests/starfield_mat.rs:34-53`, the latter added by `978d25c19`) both write different CDB indexes into the process-wide `sf_cdb_index_cache` under the same key, `"test-archive|materials\\materialsbeta.cdb"`, so parallel tests race.

## Publisher note

Related, filed this run: **GAME-D1-2026-10-05-02** (#5268) reports that #5028's gear-release path currently has no production producer of `PendingGearRelease`. Whatever producer lands for it will hit this dangling-child leak, so the detach step in `release_entities` (for victims whose `Parent` is not itself a victim) should land first or together.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
