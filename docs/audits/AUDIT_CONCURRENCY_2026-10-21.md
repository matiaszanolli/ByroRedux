**HEAD**: `4a3e89d56` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-12.md` (@ `5d31fd637`, 784 commits ago; the last full-codebase run. `AUDIT_CONCURRENCY_2026-10-17.md` was volumetrics-only and `AUDIT_CONCURRENCY_2026-10-19.md` streaming-deep only, so neither is a full baseline. Their findings were re-checked here) · **Audited**: Dim 2 (cloud-shadow bake, shadow-filter timer bracket, ground-cover #6031 / #6033 / #6032, volumetrics #5946), Dim 3 (all three lock-order lanes re-run, extension-host refactors #5592 / #5810, `collect_subtree` call sites), Dim 4 (the proof plus the cross-file hops of the parallel controller), Dim 5 (a PhysicsWorld-sink scan over every guard site), Dim 7 (new terrain-height worker pool #6195, `SharedParseCache` #6123, `extract_guarded` #6119, the `build_render_data` joins) · **Unchanged since baseline (skimmed)**: Dim 1 and Dim 6. No new `queue_submit` / `wait_for_fences` / `device_wait_idle` / `destroy_acceleration_structure` landed anywhere in `git diff 5d31fd637..HEAD`. The only new one-time submit is the shadow filter's init/recreate layout walk, already covered by #6063. Their guards were run.

# Concurrency & Synchronization Audit — 2026-10-21

**Command**: `/audit-concurrency` (all 7 dimensions, depth `deep`), inside `/audit-suite --preset comprehensive`. Run solo: no sub-agents, no source edits, no engine or GPU process. Severity scale: `.claude/commands/_audit-severity.md`.
**Dedup**: `/tmp/audit/issues.json` (119 open), `/tmp/audit/SUITE_FINDINGS.md` (read twice), and closed-issue searches for "interaction field ping-pong", "TerrainHeightService", "terrain height panic", and "projectile_flight_system PhysicsWorld" (#5955, closed).

## Guard runs at HEAD (repo `target/`, rustc default toolchain ≥ 1.96, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` | **RED: 3127 passed, 8 failed, 74 ignored.** Two cycles: `AnimationPlayer → AnimationClipRegistry` (1 test, `systems::cinematic`) and `ParticleEmitter → TextureHandle` (7 tests, `render::particles` + `render::water_wave_params_tests`). Both were already reported by the concurrent ECS audit as **ECS-2026-10-21-01 / -02**, and are not re-filed here (see "Lane state" below). |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib` (the multi-crate form that caught #6101) | core 829/0, physics 262/0, scripting 1205/0 (11 ignored) |
| `cargo test -p byroredux-renderer --lib` | 1458 passed, 0 failed, 1 ignored (`sky_cube::filter` needs a device) |
| Scheduler proof (`build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, all 9 `system_access_declaration_tests`, every `scheduler_access_tests::*` ordering and CI-lane pin) | all `ok`; no `#[ignore]` in `scheduler_access_tests.rs` or `boot/schedule/mod.rs`; `the_parallel_system_table_covers_every_parallel_registration` ok (9 parallel registrations) |
| GPU-sync source guards (`frames_in_flight_contract_names_every_dependent_resource`, `render_finished_is_sized_and_indexed_per_swapchain_image`, `queue_guard_released_before_one_time_fence_wait`, `wait_failure_arm_disposes_only_on_device_loss`, `every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask`, `taa_resolves_the_post_bloom_scene_tap`, `skip_clear_latches_are_promoted_after_submit_not_at_record_time`, `every_skipped_frame_drops_the_temporal_history_not_just_the_first`, `a_frame_without_a_recorded_dispatch_does_not_advance_the_field`, both `dynamic_rgba` tests) | all `ok` |
| Prior-report guards: CONC-SD-01 (`subtree_walk_tests::finds_a_skinned_descendant_and_survives_a_cycle`), CONC-SD-02 (four `parse_input_budget_tests`, c501edfc5), CONC-SD-03 (`tag_descendants_as_actor_terminates_on_a_cycle_and_tags_meshes_only`), `forcegreet_walk_with_real_physics_world`, `eat_walk_with_real_physics_world` | all `ok`: the three 10-19 findings are fixed |

### Lane state (cross-reference, not re-filed)
The `lock-order-check` CI job has been red on every `main` run since 2026-10-20 (the ECS audit's CI scan). I grouped every cycle in the CI log for run 39633944988 (`1be33af66`):

| Count | Cycle | Root |
|---|---|---|
| 17 | `GlobalTransform → TextureHandle → ParticleEmitter → GlobalTransform` | ECS-2026-10-21-02 |
| 7 | `TextureHandle → ParticleEmitter → TextureHandle` | ECS-2026-10-21-02 |
| 3 | `WaterVolume → WaterFlow → MeshHandle → TextureHandle → ParticleEmitter → WaterVolume` | ECS-2026-10-21-02 |
| 1 | `AnimationClipRegistry → AnimationPlayer → AnimationClipRegistry` | ECS-2026-10-21-01 |

All 27 particle-family cycles close through the single edge #6109 (5f7e01fbe) added: `collect_victim_gpu_handles` (`byroredux/src/cell_loader/unload.rs:808-816`) takes `ParticleEmitter` last, under seven other guards. Fixing ECS-2026-10-21-02 (collect the emitter LUT handles in their own scope, or take `ParticleEmitter` first) clears all three particle families. There is no independent GlobalTransform or water cycle. The site's own comment ("pure reads have no such risk") is the #4982 misreading: the detector cannot tell `&mut World` from `&World`.

## Findings summary

| ID | Severity | Dimension | Status | Title |
|---|---|---|---|---|
| CONC-2026-10-21-01 | MEDIUM | Worker Threads | NEW | Terrain-height jobs run on `ThreadPool::spawn` with no panic handler and no `catch_unwind`: any panic in archive extraction, DDS decode or generation aborts the engine |
| CONC-2026-10-21-02 | LOW | Worker Threads | NEW | Two concurrent same-cache-key height jobs share one temp file (`<key>.dds.tmp<pid>`): the loser's rename fails and its layer is marked `Failed` for the session, and the main thread can read a truncated `.dds` |
| CONC-2026-10-21-03 | LOW | Compute → AS → Fragment Chains | NEW (residual of #6031, closed) | The interaction-field ping-pong is committed at record time, so a tail `Err` that discards the command buffer still marks the unwritten half as holding a field |

Counts: **0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW** (3 NEW, 0 regressions). Cross-referenced, not counted: ECS-2026-10-21-01, ECS-2026-10-21-02 (lock-order lane red), SCR-2026-10-21-03 (parallel `player_controller_system` undeclared `KillRecords` / FormID / alias access via drowning).

## Findings

### CONC-2026-10-21-01: Terrain-height jobs abort the process on any panic
- **Severity**: MEDIUM ("missing error handling on a recoverable path"; the consequence is a process abort, but no concrete panic input is known)
- **Dimension**: Worker Threads & Thread-Safety Bounds (Dim 7)
- **Location**: `byroredux/src/terrain_height.rs:53-57` (pool built with no `panic_handler`), `:91-97` (`self.pool.spawn(move || { let result = run_job(&job); ... })`); `byroredux/src/asset_provider/terrain_height.rs:113-205` (`run_job` → `extract_first` → `Rgba8::decode_dds` → `th::from_*` → `write_atomic`)
- **Status**: NEW. The pool is from #6195 (5017a64c2, 2026-10-20). Related: #6119 (closed, the same class on the pre-parse extractor) and #5665 (open, the `sf-cdb-index` worker has no `catch_unwind`).
- **Description**: `rayon::ThreadPool::spawn` gives a panicking job to the pool's panic handler. Without one, rayon aborts the process (`rayon-core-1.13.0/src/registry.rs:377`: "If there is no handler, or if that handler itself panics, then we abort"). The job does exactly the work the other workers wrap in `catch_unwind`:
  - archive extraction over untrusted BSA/BA2 bytes. #6119 exists because a reader panic is a real class, so `pre_parse.rs` now guards `extract_guarded` (`:393`) and `parse_one_nif` (`:276`).
  - the texture prefetch (`texture_prefetch.rs:227`) wraps `provider.extract` for the same reason.
  - the job also decodes a DDS and runs the FFT integrator on whatever dimensions the decoder returns (capped at 8192², non-zero).

  The workspace deliberately keeps `panic = "unwind"` in release (`Cargo.toml:329-346`, FFI-03 / #1383), so this one worker is the exception that turns a recoverable panic into an abort.
- **Evidence**: `grep -n 'catch_unwind\|panic_handler' byroredux/src/terrain_height.rs byroredux/src/asset_provider/terrain_height.rs` returns nothing. `grep -rn panic_handler byroredux/src crates/*/src` returns nothing.
- **Trigger Conditions**: an exterior cell whose LTEX layer (authored height, normal, or diffuse texture) comes from an archive or loose file that makes the reader, decoder or generator panic. A mod-supplied texture is enough. The request fires at terrain spawn (`acquire_spawn_heights` / `request_heights`), so the abort happens while a cell streams in.
- **Impact**: the whole engine aborts with no unwinding, so no `Drop`, no log flush, and no save ring flush. A guarded path would log one `Failed` layer that renders without height.
- **Related**: #6119, #5665, #6195 (feature issue, open).
- **Suggested Fix**: wrap the job body in `std::panic::catch_unwind(AssertUnwindSafe(|| run_job(&job)))` and push `Err("height job panicked")`, so the slot completes and the state becomes `Failed`. Optionally also set a logging `panic_handler` on the builder. Add a test that spawns a panicking job and asserts `drain` reports it (pure CPU, mirrors `a_permit_moved_into_a_panicking_task_is_released`).

### CONC-2026-10-21-02: Concurrent same-key height jobs share one temp file
- **Severity**: LOW (cosmetic: one layer loses its height map for the session; the on-disk cache ends up whole)
- **Dimension**: Worker Threads (Dim 7), the on-disk cache write the skill names as "the shared state to check"
- **Location**: `byroredux/src/asset_provider/terrain_height.rs:201-205` (`write_atomic`: `let tmp = path.with_extension(format!("tmp{}", std::process::id()))`), `:155-197` (`cached_or_generate`); dedup in `byroredux/src/terrain_height.rs:72-98`
- **Status**: NEW
- **Description**: jobs are deduplicated per `LayerHeightSources` (authored / normal / diffuse path triple), but the cache file is keyed by `(GENERATOR_VERSION, tier, green, source bytes)`. Two distinct triples can resolve to the same key. Examples: two LTEX records that share a normal map but differ in diffuse, or byte-identical textures under different archive paths, which vanilla data ships. The pool has two workers (`WORKER_THREADS = 2`), so both jobs can miss the cache and run `write_atomic(&dds_path, …)` at once. The temp name carries only the pid, so both threads write the same temp path:
  1. A writes `tmp` and renames it onto `<key>.dds`.
  2. B had already opened `tmp` (`File::create` truncates the same inode) and is still writing.
  3. B's `rename(tmp, path)` fails with `ENOENT`. B's sources become `HeightState::Failed` for the rest of the session ("no height map for …"), so that layer never gets its map.
  4. Meanwhile A has published `<key>.json` and returned `Ready`. `apply_ready_heights` can `std::fs::read` the `.dds` while B is mid-write after its truncate, so it reads a short file. `enqueue_dds…` then fails and the lane stays 0 for that tile.
- **Evidence**: by reading. The three steps above need only two in-flight jobs with equal keys. No test makes two keys collide.
- **Impact**: a terrain layer intermittently renders flat (no height blend) for the session, depending on thread timing. The cache on disk is whole afterwards (both writers write identical bytes), so the next session is fine.
- **Suggested Fix**: make the temp name unique per write (pid plus a process-wide `AtomicU64` counter, or `tempfile::NamedTempFile::new_in(cache_dir)` then `persist`). Optionally deduplicate in-flight work by cache key as well as by source triple.

### CONC-2026-10-21-03: Interaction-field ping-pong committed at record time
- **Severity**: LOW (same impact analysis as #6031: a stale trail for one frame mid-session; uninitialised VRAM only if the very first recorded frame is discarded)
- **Dimension**: Compute → AS → Fragment Chains (Dim 2), cross-frame ping-pong
- **Location**: `crates/renderer/src/vulkan/groundcover/frame.rs:317-323` (`if self.record_interaction(...) { commit_field_state(...) }`), `:776-783` (`commit_field_state`); tail `Err` sites in `crates/renderer/src/vulkan/context/draw.rs:512-624` (`end_command_buffer_failed`, `reset_fences_failed`, `submit_failed`); field buffer `crates/renderer/src/vulkan/groundcover/construct.rs:117` (`create_device_local_uninit`, never filled)
- **Status**: NEW, residual of #6031 (closed by 21b9743e0 today). #6031 moved the flip from `prepare` into `record_scatter`, which fixes the no-TLAS and upload-failure skips. But record time is still before the three tail `Err` sites that discard the command buffer.
- **Description**: `commit_field_state` flips `field_write_half` and sets `field_previous = Some(origin)` as soon as the dispatch is recorded. If `end_command_buffer`, `reset_fences` or `queue_submit` then fails, the recording is discarded but the host state says the half was written. The next frame then reprojects from a half the GPU never wrote this time: the field from two frames back, or uninitialised memory on the first frame. `previous.w = 1.0` marks it valid. The renderer already has the right shape for this class: the skin chain (#3991, `promote_skin_frame_state` / `rollback_skin_frame_state`), the volumetrics and caustic skip-clear latches (#5956, `promote_skip_clear_latches`), and SVGF / TAA / volumetrics `mark_frame_completed` (#917) are all promoted only after a successful submit. The ground-cover field is the one cross-frame ping-pong outside that set. As #6031 itself notes, `groundcover_interaction.comp` does not sanitise a NaN it reads, so an uninitialised half can persist until the camera leaves the field extent.
- **Trigger Conditions**: a tail `Err` (device lost excluded: OOM in `end_command_buffer`, a failed `reset_fences`, or a failed `queue_submit`) on a frame where ground cover recorded its interaction dispatch. It is worst on the first such frame of a session.
- **Verification Path**: `cargo test` (a source-shape pin like `skip_clear_latches_are_promoted_after_submit_not_at_record_time`); no GPU capture needed.
- **Impact**: low-probability visual corruption of grass displacement. It is not a Vulkan hazard: no barrier or layout is involved.
- **Related**: #6031 (closed), #5956 (closed), #3991 (closed).
- **Suggested Fix**: stage the committed state in a `pending_field_state: Option<…>` at record time and promote it next to `promote_skip_clear_latches(frame)` after the submit succeeds. Discard it in `discard_pending_skip_clear_latches`'s position at the top of `draw_frame`. Alternatively, zero both halves once at creation with a fenced one-time `cmd_fill_buffer` (the buffer already has `TRANSFER_DST`), which removes the uninitialised case outright.

## Verified clean (per dimension)

### Dim 1 — Vulkan queue and AS sync (skimmed)
No new submit, fence, wait-idle or AS destroy in the 784-commit diff. #6035 (`9762d9fa2`) only changes the BLAS reservation arithmetic in `acceleration/predicates.rs`. #5766 (`cb0cf5798`) is a CPU-side camera-parked test in `assemble_camera_and_lights.rs`. The queue-mutex, FIF and one-time-submit guards all pass.

### Dim 2 — compute chains
- **SKYAL cloud-shadow bake** (`sky_cube/cloud_shadow.rs`, c231f5b5f): the per-FIF map goes `UNDEFINED → GENERAL` with src `FRAGMENT_SHADER`. Every consumer of set 1 binding 24 is a fragment shader (`triangle.frag`, `water.frag`, `groundcover_blade.frag` through `exteriorCloudSunTransmittance`); `volumetrics_inject.comp` only mentions `lighting.glsl` in a comment. The post-bake transition is `COMPUTE → FRAGMENT`. The camera-column readback has its own `SHADER_WRITE → HOST_READ` buffer barrier, and is read in `begin_frame_recording` after that slot's fence, before the command buffer reset, with `invalidate_if_needed`. Clean. The camera value lags two frames, which is by design.
- **Shadow-filter timer bracket** (fc83608d3): `BIT_SHADOW_FILTER` is set at the end write, the pool is reset host-side over all 58 queries, and results are read without `WAIT`, so a skipped bracket cannot block. Clean.
- **#5946 / #6032 / #6033 / #6034**: construction order, compile-time bounds, push-block guard and comments only. No barrier change.
- The #6031 interaction field is CONC-2026-10-21-03 above.

### Dim 3 — ECS lock ordering
- The bin lane is red only on ECS-2026-10-21-01 and -02 (see "Lane state"). The multi-crate lane that caught #6101 is green.
- **Extension host** (#5592 `deliver`, #5810 `with_extension_host`): `apply_pending_world_commands` takes ECS guards while holding the host `Mutex`. That Mutex is locked only on the main thread (the Late exclusive dispatch systems and the `persist.rs` capture / preflight / restore / shutdown paths). Nothing on a pool or debug thread takes it, so the untracked `Mutex → ECS` edge cannot close a cross-thread cycle. Guest entry runs under the host lock with no ECS guard held (snapshots are taken first). Clean.
- **`collect_subtree` sites**: 25 production call sites were reviewed. The only guard held across one is `retry_cinematic_readoption` (`systems/cinematic.rs:885-940`: `Transform`, `CellRoot`, `Parent` → `Children`). That function takes `&mut World`, `Transform`/`Parent` → `Children` is the canonical direction, and no `Children → CellRoot` edge exists elsewhere. Not a finding.
- `AudioWorld → GlobalTransform` in the #5678 orphaned-voice prune (`crates/audio/src/lib.rs:1534-1560`) repeats the existing `sync_emitter_positions` edge. It is a Late exclusive, and the lane is green on it.

### Dim 4 — scheduler proof
Live and non-vacuous: 9 parallel registrations equal the `PARALLEL_SYSTEMS` table. `player_controller_system`'s cross-file reach was re-walked. `refresh_action_state` and `min_walkable_normal_y` acquire nothing new, and the drowning tail is SCR-2026-10-21-03 (already filed). `animation_system_inner` calls only same-file or core helpers. The new Pending* / request queues (`PendingMagicHits`, `HitShaderKicks`, `PendingTextureSwaps`, `PendingScriptedLines`, `PendingMenus`, `PendingSleep`, `PendingCellResets`) persist until drained, so an earlier-stage consumer only defers by a frame. Their semantics are the scripting audit's (SCR-2026-10-21-10 covers marker cleanup).

### Dim 5 — RwLock patterns and physics
A scan of every `let … = world.(try_)resource(_mut)::<PhysicsWorld>()` binding in `byroredux/src` and `crates/scripting/src` (`/tmp/audit/concurrency/pw_scan.py`) found no storage acquisition under a live PhysicsWorld guard. `flee.rs`, `trap_hits.rs`, `knockdown.rs`, `script_queries.rs`, `interaction.rs` and the #6070 stand-in removal in `ragdoll.rs:557-585` each scope the guard to a block or take it last. C-2 / #5955 (projectile flight) is closed and holds. `ragdoll_centre` holds a `Ragdoll` read and then takes PhysicsWorld, which is the sink-last direction.

### Dim 6 — resource lifecycle (skimmed)
The cloud-shadow maps, readbacks, pool, pipeline and sampler are destroyed in `SkyCube::destroy` (`sky_cube.rs:616`) before the allocator. They are fixed-size, so there is nothing to rebuild on resize. Terrain height maps are ordinary registry textures owned by the tile record and released with it. #6063's leak-on-in-flight shape is pinned.

### Dim 7 — worker threads
- `SharedParseCache` (#6123): the slot lock is dropped across the parse, and two racing callers both parse with last write winning, which is benign. Nothing takes an ECS guard under it. Clean.
- `build_render_data` joins: the only write, the `SceneEffectSoftCache` memo, runs serially at `render/mod.rs:1064` before the joins. The branches are read-only. Clean.
- `apply_ready_heights` drains the service under its write guard, drops it, and only then reads `TerrainTileSlot` / `TerrainLayerHeights`. Clean. Its `flush_pending_loose_textures` (a blocking transfer-fence submit) fires only on frames where a map lands. This is the existing loose-texture pattern, and the bindless set is `UPDATE_AFTER_BIND`. Noted for perf (PERF-D3-01 covers the decode cost), not a sync defect.
- The terrain-height pool itself is CONC-2026-10-21-01 / -02.

## Out of scope / handed off
- PERF-D3-01 (terrain height full decode on a cache hit) belongs to performance; CONC-2026-10-21-01/02 cover only the thread-safety half of the same subsystem.

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-10-21.md` (labels: `concurrency` for -01 / -02, `sync` + `renderer` for -03).
