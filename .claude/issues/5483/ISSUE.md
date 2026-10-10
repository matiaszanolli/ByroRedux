# #5483: ECS-2026-10-09-D7-01: The persistent-CELL apply job's stamp range starts when the job is created, so it gives `CellRoot` ownership of the camera, the player capsule, the player body and the foreground exterior cell to the persistent CELL; the…

**Labels**: bug, ecs, gameplay, high

**Source**: `docs/audits/AUDIT_ECS_2026-10-09.md` — finding `ECS-2026-10-09-D7-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: HIGH. This is ECS ownership-state corruption on the default interactive exterior path (every launch without `--bench-frames`).
  - On a `--grid` boot, the first worldspace teardown despawns the camera, the player capsule (and its Rapier body) and the player's body. The session cannot recover; the workaround is `--bench-frames` or starting in an interior.
  - On any foreground-first exterior entry, the arrival cell's entities report the wrong cell and can leave ghost subtrees behind.
- **Dimension**: 7 — Cell load/unload symmetry (`CellRoot` / `CellRootIndex` ownership)
- **Location**:
  - `byroredux/src/cell_loader/exterior.rs:1170-1178`: `begin_worldspace_persistent_cell` seeds `stamp_cursor` with `next_entity_id()` at construction.
  - `exterior.rs:207-225`: the field doc, "Seeded at construction rather than at first `advance` so entities spawned between the two are still covered".
  - `exterior.rs:258-262` (`stamp_slice`) and the stamp calls at `:312`, `:325`, `:365`.
  - `exterior.rs:911-922`: the test `entities_spawned_before_the_first_slice_are_still_covered` pins the defect as intended behaviour.
  - Trigger chain:
    - `byroredux/src/scene/world_setup.rs:931-960`: in `ForegroundFirst` the job is created but not advanced.
    - `world_setup.rs:992-1009`: the foreground cell is applied synchronously.
    - `world_setup.rs:887-893`: `ForegroundFirst` is the default whenever `--bench-frames` is absent.
    - `byroredux/src/scene.rs:234` / `:243` / `:261`: `load_scene_content` runs before `spawn_initial_camera` (`world.spawn()` at `:928`) and before `spawn_player_body` (`body = world.spawn()` at `:1145`, `attach_player_body` at `:1275`).
  - Teardown: `byroredux/src/streaming_helpers.rs:579-584` (the persistent root joins the `unload_cells` batch); `byroredux/src/app_step.rs:1041-1046` (the exterior→interior door arm drains).
- **Status**: NEW.
  - #3379 (closed) replaced the fixed `first_entity` with a moving cursor but kept the construction-time seed, and added the test that pins the capture.
  - #5379 (closed) is the same "player owned by a cell → despawned at teardown" symptom through a different producer (cinematic re-adoption).
  - No open or closed issue and no audit report mentions the cursor capturing foreign entities.
- **Trigger Conditions**: An exterior entered with `ExteriorBootstrapMode::ForegroundFirst` whose worldspace has a persistent CELL (every vanilla worldspace). That covers:
  - every interactive `--esm … --grid x,y` launch (the README / CLAUDE.md invocation);
  - interior→exterior and cross-worldspace doors (`app_step.rs:1159`);
  - the exterior `dbgload` (`debug_load.rs:514`).

  The smoke matrix does not exercise this path: every `docs/smoke-tests/*.sh` passes `--bench-frames`, which selects `FullRadius`. That mode drives the job to completion inside `stream_initial_radius` before anything else spawns.
- **Description**: The other two resumable appliers do it correctly:
  - `ExteriorCellApplyJob` takes a fresh `next_entity_id()` at the top of each work unit (`exterior.rs:2229`, `:2277`).
  - `InteriorCellApplyJob` does the same per `advance` (`load.rs:994-1017`).

  So each stamps only what its own advance spawned. `PersistentCellApplyJob` instead stamps `[cursor, next_entity_id())` on every yield and on completion. The cursor starts at construction and spans every inter-frame gap, so any entity spawned by anyone between `begin_worldspace_persistent_cell` and a stamp is claimed.
  - `stamp_cell_root_range` is an overwrite-safe `insert_batch` of `CellRoot(persistent_root)` plus an append to `CellRootIndex[persistent_root]`.
  - In `ForegroundFirst` the job does nothing until the first steady-state `step_streaming` → `advance_streaming_apply` (`streaming_helpers.rs:763`). By then the bootstrap has already spawned:
    1. the foreground exterior cell or cells (`consume_streaming_payload`), including each cell's own root entity;
    2. on a `--grid` boot, the camera, the Character-mode capsule, and the player's body subtree assembled from `NPC_ 0x7`.
  - Between later yields, a boundary-crossing frame runs `reconcile_lod_rings` with an unbounded attempt count (`app_step.rs:301`; budget at `streaming_helpers.rs:52-60`). The next slice therefore claims the LOD blocks that frame spawned.
  - The author knew about the hazard for two spawners only. `app_events.rs:1090-1099` gates the loot-appearance and gear-import loaders off "Persistent-cell apply owns a cross-frame entity-range cursor; don't let it claim appearance entities belonging to another cell". The bootstrap window and the LOD rings are not gated.
- **Evidence**:
  ```rust
  // exterior.rs:1170-1178 — seed at construction
  let cell_root = world.spawn();
  register_cell_root(world, cell_root);
  world.insert(cell_root, CellFormId(cell.form_id));
  let first_entity = world.next_entity_id();
  Some(PersistentCellApplyJob { cell_root, …, stamp_cursor: first_entity, … })
  // exterior.rs:258-262 — every yield/completion
  let last = world.next_entity_id();
  stamp_cell_root_range(world, self.cell_root, self.stamp_cursor, last);
  // scene.rs setup_scene: :234 load_scene_content (→ begin persistent job, apply foreground)
  //                       :243 spawn_initial_camera (world.spawn())
  //                       :261 spawn_player_body (capsule world.spawn() + attach_player_body)
  ```
  The in-repo test `entities_spawned_before_the_first_slice_are_still_covered` proves the mechanism: an entity spawned between construction and the first `stamp_slice` ends up in the persistent root's list. The trace above shows that in production that entity is the camera, the player and the arrival cell.
- **Impact**:
  1. **Player loss after a `--grid` boot.**
     - The first `drain_streaming_state` (exterior→interior door, F9 or save-load teardown of that session, `dbgload`, or a crossing that does not preserve the persistent root) unloads `persistent_root`.
     - Its victim list now holds the camera, the player capsule and the body. `release_entities_timed` removes the capsule's Rapier body and drops the body's mesh, texture and skin-slot refs, and `despawn_batch` deletes all three entities.
     - `PlayerEntity`, `ActiveCamera` and `PlayerBodyRootEntity` then name dead ids, and the player cannot be recovered without a restart. This is #5379's symptom, now from the boot path.
  2. **Wrong cell ownership for the arrival cell, on every foreground-first entry.**
     - Each of that cell's entities, its root included, ends up with `CellRoot(persistent_root)`.
     - `GetInCell` (`crates/scripting/src/condition.rs:845-857`, which resolves `CellRoot → CellFormId`) reports the persistent CELL for every actor there.
     - Mid-life gear and corpse re-dress imports stamp onto `CellRoot(wearer)` (`npc_spawn/loot_appearance.rs:750`, `:951`, `:987`), so their gear lands in the persistent root's list. When the arrival cell later streams out, the actor is despawned through its real index, but the gear subtree survives. Its `Parent` points at the dead actor, so the gear is frozen ghost geometry that holds its GPU refs until the worldspace drains. A `Parent` naming a dead entity is also exactly what makes `validate_world` refuse every later save (SAVE-D4-2026-10-09-01's mechanism, from a different producer).
  3. **Double reclaim at the drain.** LOD blocks claimed on a crossing frame are released twice: once by `unload_cells(persistent_root)` through their `MeshHandle` / `TextureHandle`, then again by `unload_{,object_,placement_}lod_block`. The second release triggers `drop_mesh … already-released` warnings and an extra decrement on shared LOD textures.
- **Related**: #3379, #5379, #3376, #3377 (closed); SAVE-D4-2026-10-09-01 and CONC-D5-2026-10-09-01 (the same dangling-`Parent` and ghost-subtree class).
- **Suggested Fix**:
  - Give the persistent job the same discipline as its siblings: capture `let first = world.next_entity_id()` at the top of each `advance`, and stamp `[first, next_entity_id())` at each yield and at completion. The job spawns nothing of its own before its first advance, because the root is registered in `begin`, so the construction-time seed protects nothing.
  - Invert `entities_spawned_before_the_first_slice_are_still_covered` into a guard: an entity spawned between `begin` and the first `advance`, or between two yields, must have no `CellRoot`.
  - Then drop the loader gate's rationale in `app_events.rs`.
  - Defence in depth, as #5379 did for its producer: have `unload_cell_inner` refuse, and log, any victim that is the `PlayerEntity`, the `ActiveCamera` or a member of the player body's subtree.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
