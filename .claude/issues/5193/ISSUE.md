# #5193 — REN-D5-2026-10-03-01: the loading-cover model stage has its own copy of the cell-teardown release path, and the copy misses Rapier bodies and skin/morph slots; its post-spawn failure arm frees no GPU handles at all

**Labels**: medium,renderer,memory,physics,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle
- **Location**: `retire_stage` and `LoadingScreen::spawn_model_stage` (`byroredux/src/loading_screen.rs`); the canonical path is `release_entities` / `release_entities_timed` (`byroredux/src/cell_loader/unload.rs`).
- **Status**: NEW. This absorbs Dim 1's REN-D1-2026-10-03-03, which independently found the `stage_camera == None` arm leaking the stage's mesh, BLAS and texture refs; that ID is retired into this one. The orchestrator confirmed `retire_stage` never calls `release_victim_rapier_bodies`. Cross-owner with `/audit-physics`, since the leaked objects include Rapier bodies.
- **Description**:
  - e60911864 (2026-10-02 16:35) added `retire_stage`, a hand-written subset of cell teardown: it collects victim GPU handles, drops BLAS for meshes whose last holder goes, then calls `drop_meshes` and `drop_textures`.
  - Four and a half hours later, f78e018ac (#5028) extracted that same teardown into `cell_loader::unload::release_entities`, so that "gear release and cell teardown share one despawn + GPU-handle path". `retire_stage` was never moved onto it.
  - Next to `release_entities_timed`, the copy omits:
    1. `release_victim_rapier_bodies`. The stage NIF is spawned through `load_nif_bytes` → `spawn_nif_nodes`, which inserts `CollisionShape` + `RigidBodyData` on any node with bhk collision. `physics_sync_system` registers newcomers before it steps, and the scheduler keeps running at `dt = 0.0` while the cover is up (`app_events.rs` `about_to_wait`: `if self.loading_screen.active() { 0.0 }`). The stage's fixed bodies are therefore created. `collect_newcomers` has no stage filter, and `crates/physics` has no orphan sweep, so the despawn leaves them in the `PhysicsWorld`.
    2. `queue_skin_unload_victims` and `pending_morph_unload_victims`. Skinned or morphed stage meshes are reclaimed only by the idle-eviction pass, not by the same frame's drain.
    3. `finish_unload_batch`: `shrink_storages`, plus a BLAS-scratch shrink once the stage's BLAS are gone.
  - Separately, `spawn_model_stage`'s `stage_camera(...) == None` arm runs *after* `load_nif_bytes` has registered meshes and textures and built their BLAS (`count > 0`). That arm calls `despawn_subtree`, whose own doc says it is for spawn failure paths "before any mesh was registered". Every mesh, BLAS and texture refcount the stage took is leaked.
  - That arm is reachable from data: `stage_pose` passes `trns.scale` / `initial_scale` through unvalidated, so a zero or non-finite authored scale, a non-finite translation, or point-sized geometry gives `radius <= EPSILON`. The `count == 0` arm has the same shape for any particle-emitter textures `spawn_nif_particle_emitters` acquired.
- **Evidence**:
  ```rust
  // loading_screen.rs retire_stage — the whole release:
  let (mesh_drops, texture_drops, _terrain_slots) =
      crate::cell_loader::collect_victim_gpu_handles(world, &victims, fallback_tex);
  world.despawn_batch(victims);
  /* … drop_blas for freed handles … */
  ctx.mesh_registry.drop_meshes(&mesh_drops);
  ctx.texture_registry.drop_textures(&ctx.device, &texture_drops);
  // spawn_model_stage, after load_nif_bytes returned count > 0:
  let Some(camera) = stage_camera(&posed, fov_y) else {
      despawn_subtree(world, root);   // no GPU release
  ```
- **Impact**:
  - Each Skyrim or FO4 door or save cover that shows a collision-bearing model leaves its fixed Rapier bodies behind. They are invisible but still collide, at the stage pose (menu-space translation converted to Y-up) in the destination world. They also grow the broad-phase without bound over a session.
  - This is a per-user-action leak, not per-frame.
  - The degenerate-pose arm leaks one model's worth of mesh, BLAS and texture refs per occurrence.
  - Not done: a vanilla census of which LSCR `NNAM` / `TNAM` models carry bhk collision. That needs an archive extract, and building is out of scope for this run.
- **Related**: #5028 (the extraction this copy predates); #1520 (the Rapier-on-despawn rule); #1003 / #3231 (skin/morph unload queueing).
- **Suggested Fix**:
  - Route `retire_stage` through `cell_loader::unload::release_entities`, adding a log label parameter in place of its hard-coded "gear release". Delete the copy.
  - Release the stage subtree through the same call in the `stage_camera == None` arm, or validate the pose (finite, non-zero scale) before `load_nif_bytes`.
  - Optionally exclude `LoadingModelStage` subtrees from physics registration altogether.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
