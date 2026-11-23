# Concurrency Audit — 2026-11-23

**HEAD**: 876874834 (clean `origin/main` worktree) · **Baseline**: AUDIT_CONCURRENCY_2026-11-15.md (3334bbe48; ECS half only) and 2026-11-09 (the last run to cover the Vulkan dimensions) · **Audited**: Dims 1-7, delta-first (726 commits since the baseline; every dimension's paths changed) · **Previous**: AUDIT_CONCURRENCY_2026-11-15.md

Read-only run: nothing fixed, no engine launched, no GPU used. Four dimension agents (Dims 1+6, Dim 2, Dims 3+4+5, Dim 7); the one finding rated above LOW (CON-01) was re-verified in the code by the orchestrator, as was CON-03's undeclared read. **The Vulkan dimensions are source reads only**: barrier / stage-mask / layout questions are HYPOTHESES that need a `BYRO_VALIDATION=1` capture, per the skill's speculative-fix guardrail, and none is filed as a defect.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 6 |
| **Total** | **7** |

**No lock-order cycle, no cross-thread blocking edge, no live parallel-system under-declaration.** The dynamic lanes (`BYRO_LOCK_ORDER_CHECK=1`) are green on all four crates: `byroredux-core` 1027, `byroredux-physics` 280, `byroredux-scripting` 1492 and the engine binary **4128 passed, 0 failed** (131 ignored). The Dim 3 agent re-ran 55 targeted tests with the detector on (every `*_declares_everything_it_acquires` scan, `scheduler_access`, `schedule_order`, the door and hit ordering pins), all green. This session's own fixes (#7280, #7311, #7318-#7321, #7337, #7345, #7340/#7350) hold, and so do the 2026-11-15 audit's CONC-1 to CONC-6. `byroredux-renderer --lib`: 1688 passed (26 guard tests re-run by Dim 2), 0 failed. Every rayon `spawn` site is guarded by `catch_unwind` (`panic = "unwind"` is set, so an unguarded spawn would abort the process; none exists).

- **CON-01 (MEDIUM, GPU state, found independently by Dims 1+6 and Dim 2):** the terrain-patch BLAS set marks a patch as built and ready when its build is *recorded*, and the three discard sites in `draw_frame` never roll it back. After a frame that fails at `end_command_buffer`, `reset_fences` or `queue_submit`, the next frame plans an UPDATE refit of an acceleration structure that was never built. The sibling skinned BLAS has had this guard since #3991. The consequence at the Vulkan level is a hypothesis; the missing rollback is confirmed by reading.
- **LOW:** the terrain-tile dirty flag cleared before the copy is known to survive (CON-02); two XP exclusives that read `UiLabels` and write notifications without declaring either, and 41 of 57 exclusive rows with no completeness scan (CON-03); cross-file hops missing from the `PARALLEL_SYSTEMS` table for the fall and jump paths (CON-04); nothing pins the terrain-patch teardown (CON-05); a stale lock-ordering policy doc (CON-06); movie soundtrack WAVs orphaned after a hard process death (CON-07).

## Dimension coverage

| Dim | Subject | Method | Result |
|---|---|---|---|
| 1 | Vulkan queue and AS sync | source read, guard tests | CON-01, CON-02; queue Mutex scopes, all-slots wait and rider list, deferred destruction, post-submit latches verified |
| 2 | compute → AS → fragment chains | source read, chain tables, 26 guard tests | CON-01, CON-02; terrain patch, sky bake, env BRDF LUT and aperture planes, helper-lane skip, exposure meter, groundcover verified; 1 hypothesis row |
| 3 | ECS lock ordering | detector lanes + 70+ new multi-guard functions read | no cycle; CON-06 (doc) |
| 4 | scheduler proof soundness | declaration guards, 55 targeted tests | CON-03, CON-04 |
| 5 | Resource↔Storage and physics step | source read | `PhysicsWorld` stays a sink in all new code; no finding |
| 6 | GPU teardown ordering | source read | the three load-bearing orderings intact; CON-05 |
| 7 | worker threads | spawn-site inventory, guard tests | CON-07; the skill's checklist misses five thread / shared-state sites (not filed, below) |

## Findings

### CON-01: patch BLAS "built" state is committed at record time, so a discarded frame leaves a never-built AS marked as refit-able
- **Severity**: MEDIUM
- **Dimension**: Compute -> AS -> Fragment Chains
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs:508-523` (`entry.content = Some(...)` and `ready.insert`), planner at `:96-110` (`plan_patch_blas_ops`), call site `context/dispatch_skin_and_cluster.rs:665`, tail-Err sites `context/draw.rs:448`, `:506`, `:554`
- **Status**: NEW. No open or closed issue found (`gh issue list --state all` for "terrain patch BLAS rollback submit", "patch BLAS never built UPDATE", "terrain patch sync validation"; local `issues.json` titles; #7221 / #7245 / #7246 / #7263 do not mention it). `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` "Observations" verified the barriers and the build/refit flags but did not look at the discard path.
- **Description**: `record_patch_blas_frame` commits `entry.content = Some((request.content, refits))` for every recorded op right after it records `cmd_build_acceleration_structures` (`:508-520`). That field is the planner's only input for "refit or build": `plan_patch_blas_ops` returns `Refit` when `previous == now` and `refits < rebuild_after` (`:105`), and a refit records `mode(UPDATE)` with `src_acceleration_structure(entry.accel)` into the same AS (`:458-463`). `draw_frame` has three tail `Err` sites (`end_command_buffer`, `reset_fences`, `queue_submit`) that discard the whole command buffer; each calls `rollback_skin_frame_state(frame)` (`draw.rs:448, 506, 554`). That rollback covers the skinned-BLAS provisional set, the TLAS recording (`invalidate_tlas_recording`) and the bone-world promotion (#3991), but nothing in it, or in `promote_skin_frame_state` / `promote_skip_clear_latches`, names `terrain_patch_blas` (`rg terrain_patch_blas` finds hits only in `acceleration/{mod,tlas,blas_terrain_patch}.rs`, `dispatch_skin_and_cluster.rs`, `telemetry.rs`). The skinned path states the same hazard explicitly: `rollback_provisional_skinned_blas` exists so the next frame does not "UPDATE-refit acceleration structures that were never built (#3991)" (`skinned_blas_refit.rs:105-108`, `blas_skinned.rs:794-816`).
- **Evidence**:
  ```
  // blas_terrain_patch.rs:511-520
  for &(slot, op) in recorded.iter() { ...
      entry.content = Some(match op {
          PatchBlasOp::Refit => { ...; (content, refits + 1) }
          _ => { stats.built += 1; (request.content, 0) }
  ```
  ```
  // blas_terrain_patch.rs:105
  Some((built, refits)) if built == now && refits < rebuild_after => PatchBlasOp::Refit,
  ```
  The `ready` set (`:522`) is cleared at the start of each record, so only `content` survives a discarded frame.
- **Trigger Conditions**: a frame that records at least one first-time patch BUILD (a slot's first use, or a slot reassigned to another square or level) and then loses its command buffer at `end_command_buffer`, `reset_fences` or `queue_submit` (out-of-memory, device error short of loss). The next frame with the same slot content plans `Refit`: an UPDATE whose source AS was never built.
- **Impact**: ray queries traverse an uninitialised BVH for that patch until the slot is reassigned or the 600-refit rebuild fires; the visible result is wrong shadows / GI / reflections on the ground nearest the camera, with a device fault possible if the garbage is traversed. Spec-wise it is the UPDATE-from-unbuilt-source class #3991 closed for skinned BLAS. Not reachable in steady state; the engine already treats the three Err sites as recoverable (`draw.rs:237-243`, the #3837 note that calls the three sites reachable in practice).
- **Verification Path**: `cargo test` can cover it (no GPU): a source-scan guard in the style of `the_groundcover_field_flip_is_promoted_after_submit_not_at_record_time` / `skip_clear_latches_are_promoted_after_submit_not_at_record_time` (both in `context/draw.rs`), plus a pure test on the planner. No validation capture is needed to confirm the latch order; a fault-injected `queue_submit` failure on a patch-bearing exterior would show the UPDATE.
- **Related**: #3991, #5956, #6291 (same record-time-vs-submit-time class), #7221, rider 16 in `sync.rs`.
- **Suggested Fix**: follow the #3991 shape rather than inventing one: stage the content commit (`pending` list on `PatchBlasFrame`, applied by a `commit_` after `queue_submit` returns `Ok` next to `promote_skin_frame_state`), and on the rollback path set the staged entries' `content = None` so the next frame takes `Build`. Pin it with a post-submit source-scan test.
- **Dimension agent ID**: CON-D2-01
- **Also found by**: CON-D1-01 (Dims 1+6), independently.

### CON-02: the terrain-tile dirty flag is cleared before the copy is recorded, and again before it can survive a discarded frame
- **Severity**: LOW
- **Dimension**: Compute -> AS -> Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/resources.rs:24-39` (`fill_terrain_tiles` sets `*dirty = false`), `context/build_and_upload_instances.rs:1010-1018` (`upload_terrain_tiles_if_dirty`), `scene_buffer/upload.rs:1103-1190` (`upload_terrain_tiles`, `?` on `staging_pool.acquire`)
- **Status**: NEW as a finding (the flag predates the delta: #496 / #497 / #3664); it matters more now because `terrain_patch.comp` and the ground-cover compute passes read the tile SSBO. No open issue found (searched "terrain tile dirty upload": only closed perf issues #496 / #497 / #3664 / #4790).
- **Description**: `fill_terrain_tiles` returns `true` and clears `terrain_tiles_dirty` at record time. `upload_terrain_tiles` then records the staging copy into `cmd`. If that call fails (`acquire(byte_size)?` or `mapped_slice_mut()?`; the caller only logs `Failed to upload terrain tiles`) or if the frame's command buffer is discarded at one of the three tail `Err` sites, the SSBO keeps its old contents and nothing re-arms the flag: the next frame sees `dirty == false`. The tile table (layer texture indices, height-map index, cell origin) then stays stale until the next cell load or unload mutates a tile. `draw_frame` has no terrain-tile rollback beside `rollback_skin_frame_state`.
- **Evidence**: `if !*dirty { return false; } *dirty = false;` (`resources.rs:29-32`); the call site ignores the copy's result beyond a warn (`build_and_upload_instances.rs:1014-1016`).
- **Trigger Conditions**: a frame that has a pending tile change (cell transition) and fails the staging acquire or loses its command buffer.
- **Impact**: wrong / missing terrain layers on the affected cells, and wrong height-map lookups in the patch displacement and ground-cover placement, persisting until the next tile change. Not a GPU hazard.
- **Verification Path**: `cargo test` (a unit test on `fill_terrain_tiles` + the caller shape); no capture needed.
- **Related**: #3991 / #5956 / #6291 (same class), CON-D2-01.
- **Suggested Fix**: re-arm `terrain_tiles_dirty` on the failure arm of `upload_terrain_tiles_if_dirty`, and on the tail-Err rollback path; or commit the clear in `promote_skip_clear_latches`.
- **Dimension agent ID**: CON-D2-02
- **Also found by**: CON-D1-02 (Dims 1+6), independently.

### CON-03: about 40 exclusives carry an `Access` row with no `*_declares_everything_it_acquires` scan, and some rows have already drifted; the skill text names only two
- **Severity**: LOW
- **Dimension**: 4 (exclusive declarations, `sys.accesses` accuracy)
- **Location**: `byroredux/src/boot/schedule/update/combat/faction_hostility.rs:15-37`; `byroredux/src/boot/schedule/late/scripted_combat.rs:11-34` (`scripted_experience_system`, `scripted_skill_system`) and `:130-160` (`reconcile_pending_dead_actors_system`); scans live in `byroredux/src/boot/schedule/mod.rs:1151-1380`
- **Status**: NEW (closed #4574 / #3951 / #5069 were the same shape on other rows)
- **Description**: 16 exclusives have a completeness scan. Counting `add_exclusive_with_access` rows gives 57 rows, so 41 have a row and no scan: pool_regen_tick, bleedout, reconcile_pending_dead_actors, scripted_experience / scripted_skill / scripted_kill, evidence_deposit, jail_bed, guard_rush, encumbrance, the 13 `extension_*` sync / dispatch systems, player_body_facing, cinematic_animation_event, faction_hostility, water_interaction, npc_dialogue_selection, player_derived_stats, player_identity_sync, restoration, submersion, vats_mode, water_audio, water_damage, footstep, impact_audio, billboard, world_bound, particle, weather. The skill names only `bed_use_system` (a bare `add_exclusive`, no row at all) and `jail_bed_system`. Running an equivalent scan on both trees (`scan.py`, same-file calls followed, turbofish forms only) shows these rows under-declare same-file acquisitions:
  - `scripted_experience_system` and `scripted_skill_system`: `UiLabels` read, **new since the baseline** (f452ec0e4, #6825: `grant_experience` resolves `sStatsXP`, `combat.rs:1062`). `scripted_experience_system` also misses `ActorValueNames` (older).
  - `reconcile_pending_dead_actors_system`: `AiCombatState`, `AmbientPackageRuntime`, `EvaluatePackageRequest` removed by `reconcile_dead_actor` (`combat.rs:1605-1616`), `RagdollActive` read (older).
  - `make_faction_hostility_system`: `AmbushBehavior`, `LoadedCellIndex` (`ambush_gates`, `faction_hostility.rs:375-380`) and `IgnoreCrime` (`:538`) (older).
  The cross-file tails (`notifications::push`, `music::raise_sting`, `raise_level_story_event` behind `grant_experience`) are undeclared on the two XP rows as well; the scanner does not follow them.
- **Evidence**: `scripted_experience_system` row: `PendingPlayerXp`, `XpModifierRules`, `PlayerEntity`, `CharacterRuleset`, `CharacterLevel`, `LevelUpPoints`, `ActorValues` only; body reaches `UiLabels` through `grant_experience`.
- **Trigger Conditions**: none (exclusives never pair; the project rule is that an undeclared exclusive is by design). The rows are the operator view (`sys.accesses`) and the comparison basis for promoting a system to a parallel lane; a wrong row makes that promotion look safe.
- **Impact**: report accuracy and promotion safety only. No deadlock.
- **Verification Path**: re-run `scan.py` (or a `*_declares_everything_it_acquires` test over `grant_experience` for the two XP rows, which needs `COMBAT_SRC` entries `scripted_experience_system`, `scripted_skill_system`); today it fails on `UiLabels=read`.
- **Related**: #7319 (added six scans, left these), #7345.
- **Suggested Fix**: either add the scans for the rows that gained acquisitions this window (the two XP rows, `reconcile_pending_dead_actors_system`, `faction_hostility`), or drop the rows that cannot be kept honest; and correct the skill sentence ("`bed_use_system` and `jail_bed_system` stay bare") to the real count.
- **Dimension agent ID**: CON-D4-02

### CON-04: `player_controller_system`'s PARALLEL_SYSTEMS hop table does not list the cross-file hops the #7051 / #7056 fall and jump paths added
- **Severity**: LOW
- **Dimension**: 4 (scheduler proof soundness, guard blind spot)
- **Location**: `byroredux/src/boot/schedule/mod.rs:957-989` (the `player_controller_system` entry of `PARALLEL_SYSTEMS`); `byroredux/src/boot/schedule/early.rs:34-46` (the rows the hops need); callers at `byroredux/src/systems/character.rs:437` (`skill_use_emit::on_jump`) and `:1757` (`skill_use_emit::on_fall`, `detection::actor_value` at `:1751`)
- **Status**: NEW (same class as closed #4994, #5641, #6217, #6447, and the LimbEffects gap fixed in dfcbcafd5)
- **Description**: `player_controller_system` is one of two parallel systems in `Stage::Early` (the other is `timer_tick_system`). Since the baseline it reaches callees outside its listed source files:
  - `crate::skill_use_emit::on_jump` / `on_fall` -> `byroredux_scripting::oblivion_skill_use::queue_skill_use` (`crates/scripting/src/oblivion_skill_use.rs:479`): reads `CharacterRuleset`, writes `PendingSkillUse`.
  - `fall_landing` -> `crate::detection::actor_value` (`byroredux/src/detection/tick.rs:386`): reads `ActorValueNames`, then `ActorValues`.
  The row today declares `.writes_resource::<PendingSkillUse>()`, `.reads_resource::<ActorValueNames>()`, `.reads_resource::<CharacterRuleset>()` (and `ActorValues` as a write), so the declaration is correct. But `PARALLEL_SYSTEMS` lists no entry for `OBLIVION_SKILL_USE_SRC` / `queue_skill_use` or for `detection/tick.rs` `actor_value`, and the scan follows calls only inside listed files, so two of the new rows are held up by nothing: `PendingSkillUse` and `ActorValueNames` are acquired by no listed body (`CharacterRuleset` is read directly at `character.rs:186`, and `FallModel` / `FallTracker` in `character.rs`, so those three are guarded). A later change to `queue_skill_use` (a new gate resource, a notice push) would make `known_conflict_count() == 0` unsound for the Early pair with no test failing, which is the exact failure the table exists to prevent.
- **Evidence**:
  ```
  // character.rs:434-438
  if jump_fired && swim.is_none() {
      // #7051 ...
      crate::skill_use_emit::on_jump(world);
  }
  // character.rs:1749-1757
  let damage = model.damage(seconds, crate::detection::actor_value(world, player, "Acrobatics"));
  ... crate::skill_use_emit::on_fall(world);
  ```
  `PARALLEL_SYSTEMS["player_controller_system"]` hops: `queue_dead_actor_reconciliation`, `set_kinematic_translation`, `set_linear_velocity`, `weather_wave_adjustment`, `raise_kill_story_event`, `raise_story_event`, `note_death`, `locomotion_mult`, `crippled_legs`. No skill-use or detection entry.
- **Trigger Conditions**: none today (the declaration is complete, verified by hand against the two callee bodies). It becomes a live hole when either callee gains an acquisition.
- **Impact**: guard gap only. If it drifts, a data race is impossible (RwLock) but a cross-thread ABBA between `player_controller_system` and `timer_tick_system` becomes undetectable statically.
- **Verification Path**: add `(OBLIVION_SKILL_USE_SRC, "queue_skill_use")` and `(DETECTION_TICK_SRC, "actor_value")` to the entry, and extend `cross_file_hops_and_get_forms_reach_their_acquisitions` to pin `PendingSkillUse=Write` and `ActorValueNames=Read` on `player_controller_system`; that pin fails today without the hops (neither `PendingSkillUse` nor `ActorValueNames` appears in any listed body).
- **Related**: #4994, #6447, #5641, #6217 (all closed, same class).
- **Suggested Fix**: add the two hop entries and the pin above.
- **Dimension agent ID**: CON-D4-01

### CON-05: nothing pins the teardown of `TerrainPatchPass` or the patch BLAS set
- **Severity**: LOW (test-coverage gap; the code is correct today)
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/teardown.rs:146-149` (`patches.destroy(&self.device, alloc)`), `crates/renderer/src/vulkan/acceleration/mod.rs:538` (`self.destroy_terrain_patch_blas(device, allocator)`)
- **Status**: NEW. Closest closed issue: #6521 / SAFE-2026-10-27-01 (a `SceneBuffers` family missing from `destroy`). Searched `terrain patch teardown`, `destroy coverage`.
- **Description**: both calls exist and sit before `allocator.take()` and `Arc::try_unwrap`; none of the three load-bearing orderings is involved. No test names either. `SceneBuffers::destroy` is protected by `destroy_walks_every_per_frame_buffer_family`, but `TerrainPatchPass` and `TerrainPatchBlasSet` own handles with no `Drop` (`vk::Pipeline`, `vk::DescriptorPool`, `vk::AccelerationStructureKHR`). Their `GpuBuffer` fields self-free through `Drop` but hold an allocator `Arc` clone, so a dropped call leaks the AS and pipeline handles and trips the outstanding-references path at `teardown.rs:495-508`, the failure shape of SAFE-2026-10-27-01.
- **Evidence**: `rg "destroy_terrain_patch_blas|patches.destroy"` over `crates/renderer/src` returns the two production call sites only; `acceleration/tests/*.rs` has no destroy-coverage test.
- **Trigger Conditions**: a refactor that moves or deletes either call. Shutdown only.
- **Impact**: a shutdown leak plus a debug-build assertion; no per-frame effect.
- **Related**: #6521, `skin_slot_drain_sits_outside_the_skin_compute_guard` (the pattern to copy).
- **Verification Path**: cargo test (source-shape scan over `production_text(include_str!("teardown.rs"))` and `acceleration/mod.rs`).
- **Suggested Fix**: one source-scan test naming both call sites.
- **Dimension agent ID**: CON-D6-01

### CON-06: `docs/engine/ecs.md` § Lock-ordering policy does not record the directions the window's cycle fixes settled
- **Severity**: LOW
- **Dimension**: 3 (canonical order is the arbiter for new multi-lock code)
- **Location**: `docs/engine/ecs.md:571-750` (unchanged since the baseline: `git log 3334bbe48..HEAD -- docs/engine/ecs.md` is empty)
- **Status**: NEW
- **Description**: the policy is the stated arbiter for hand-ordered holds, but the orders fixed in this window live only in code comments and test names: `InventoryCatalog` before `Inventory` (CONC-4, `extensions/capture.rs:265`, and now held to the end of `capture_entity_projections`); `WalkSpeed` before `GlobalTransform` (CONC-1, `eat_sleep.rs:284`); `CreatureActor` before `Furniture` / `Seated` / `GlobalTransform` (ace99c164, `sandbox.rs:337-343`); `RapierHandles` -> `WaterContact` and `WaterContact` -> `ActorVitals`, so `ActorVitals` must not be held across `RapierHandles` (#7315, `trap_hits.rs:68-79`); `ActorValues` -> `ActorValueNames` (`read_caster`, `player_cast.rs:213`; the reverse is the closed #7063 cycle); `FormIdComponent` before `FormIdPool` appears only in the skill. The next author of a gather has nothing to consult but the skill's anecdotes.
- **Evidence**: the doc's clusters stop at `Transform … StringPool`, the cinematic read-onlys, the render-skip sinks, `CharacterRuleset -> ActorValues`, animation, scene/quest and physics shapes.
- **Trigger Conditions**: none; documentation.
- **Impact**: the same cycle shapes keep being reintroduced by new code (the skill lists 16 of them). Low, no runtime effect.
- **Verification Path**: `rg "InventoryCatalog|WalkSpeed|CreatureActor|WaterContact" docs/engine/ecs.md` returns nothing for the lock section.
- **Suggested Fix**: add a short "settled pairs" table to the policy with the six directions above and the test that pins each.
- **Dimension agent ID**: CON-D3-01

### CON-07: Hard process death orphans the movie soundtrack WAV (and the .fuz xWMA temp); the #7354 sweep covers only save staging
- **Severity**: LOW
- **Dimension**: Worker Threads and Thread-Safety Bounds (temp-file lifetime of the `movie-decode` / `.fuz` decode path)
- **Location**: `crates/video/src/lib.rs:387-460` (`decode_soundtrack`, `create_temp_file(soundtrack.stem, "wav")` at `:397`), `crates/core/src/atomic_file.rs:124-150` (`create_temp_file`) and `:199-229` (`TempPath`), `crates/audio/src/fuz.rs:295-302` (`write_temp_xwma`), `byroredux/src/video.rs:327` (the WAV is played by path)
- **Status**: NEW. Dedup: #7158 and #7172 (closed) cover the panic-unwind and early-return paths, #7143 (closed) the exclusive creation, #7354 (closed) reclaims only `.save_<slot>.ess.<pid>.<n>.tmp`. `gh issue list --state all --search "soundtrack WAV temp crash orphan"` and `"temp file leak crash reclaim"` returned no match for a hard-death sweep of these two stems.
- **Description**: `TempPath::drop` removes the file on a return or an unwinding panic, which is what #7158/#7172 pinned (`the_soundtrack_wav_is_removed_on_every_way_out`). It cannot run when the process dies without unwinding: SIGSEGV/SIGKILL, an OOM kill, a power cut, a Vulkan-driver crash, or the rayon abort-on-panic class this dimension polices. The movie WAV is the exposed one: it is created before the first frame, written whole by `decode_soundtrack`, and then stays on disk for the whole playback because the audio crate opens it by path (`byroredux/src/video.rs:327`, `play_audio`). The #7354 fix made the same observation for save staging ("a disk-space leak of one save per crash") and added `disk::reclaim_stale_staging_files` to `SaveState::new`; the two temp stems produced through the same helper got no equivalent.
- **Evidence**: `grep -rn "byroredux-movie\|byro-fuz\|SOUNDTRACK_STEM\|create_temp_file" crates byroredux tools` finds the creators and no reader of the stems other than the tests; nothing lists `std::env::temp_dir()` at boot. The names carry the pid (`{stem}-{pid}-{nanos}-{counter}.{ext}`), so a sweep can tell a dead owner from a live one. The soundtrack is capped at `MAX_SOUNDTRACK_SECS` = 3600 s of 48 kHz stereo s16 (about 1.4 GB), typical vanilla intro movies are a few MB to tens of MB.
- **Trigger Conditions**: the process dies without unwinding while a movie is open or playing (or inside a `.fuz` decode, a window of milliseconds to seconds). Every such death leaves one file per open movie.
- **Impact**: disk (or RAM, on a tmpfs `TMPDIR`) held until the next reboot or manual cleanup; no correctness effect. Same class and rationale as #7354 but for the temp directory.
- **Verification Path**: unit test with a fake temp dir is not possible (`create_temp_file` reads `std::env::temp_dir()`); the test is a sweep helper taking the directory, driven by a fixture of `byroredux-movie-<deadpid>-1-0.wav` and `byroredux-movie-<livepid>-...` files. No GPU needed.
- **Related**: #7354, #7158, #7172, #7143
- **Suggested Fix**: a boot-time sweep next to `reclaim_stale_staging_files` that removes `byroredux-movie-*.wav` / `byro-fuz-*.xwma` in `temp_dir()` whose embedded pid is not alive (or whose mtime is older than a day), best-effort and logged with a count. On Windows the same helper should also be checked for the open-handle case: `Movie::drop` removes the WAV while kira may still hold the file open after `stop_movie_sound()` (the removal error is swallowed by `TempPath::drop`); not verified on Windows.
- **Dimension agent ID**: CON-D7-01

## Hypotheses needing capture (not findings, not filed)

None of these is a defect on reading; each names the signal that would confirm or retire it. Run with `BYRO_VALIDATION=1` (or `gpuav`) in a release build on a real device.


- **H-1 (carried from rider 16 and AUDIT_INCREMENTAL_2026-11-22)**: the in-place rewrite of the patch vertex ranges, the in-place patch-BLAS UPDATE and the shared `scratch` order against the previous frame only through the transitive chain in table 2 ("derived by reading rather than by validation"). Needs a `BYRO_VALIDATION=1` sync-validation capture that exercises more than the steady-state refit the TSD-4.2 commit measured: walk across a patch boundary (slot reassignment, so `Build` and the two-builds-per-frame budget), flip a slot between fine and coarse (TSD-4.3 selector), and cross a cell border (tile copy + geometry upload in the same frame). Confirming signal: any `SYNC-HAZARD-WRITE-AFTER-READ` / `WRITE-AFTER-WRITE` naming the global vertex buffer (prior usage `ACCELERATION_STRUCTURE_BUILD` or `VERTEX_ATTRIBUTE_READ`), a patch BLAS buffer, or the patch scratch buffer. Absence across those three movements closes it.
- **H-2 = CON-D2-03** (far-sun UNDEFINED transition from `TOP_OF_PIPE`); confirming signal above.
- **H-3 (carried)**: CONC-2026-11-09-2.1 / #6835, local-grid brick buffer fill vs composite's earlier read; unchanged, open.


Per the speculative-fix guardrail none of these is a fix proposal. `docs/engine/terrain-displacement.md` §8.3 (line 744) and §8.4 (line 773) record `BYRO_VALIDATION=1` runs with patches on and the patch BLAS in the TLAS, at five positions, "no VUID or SYNC-HAZARD message". Those were stationary poses, so a cell-load frame (tile copy, first builds) was not exercised.

- **HYP-1: tile-buffer copy WAR against the previous frame's reads.** `upload_terrain_tiles` records `cmd_copy_buffer` into the shared `terrain_tile_buffer` (`scene_buffer/upload.rs:1177`) with only a trailing `TRANSFER -> FRAGMENT|COMPUTE` barrier and no leading one. The previous frame read the buffer from FRAGMENT and, since #7152, from COMPUTE (`terrain_patch.comp`). The ordering rests on a barrier chain through the previous frame's tail plus the all-slots host wait, and sync validation does not model the host wait. Confirming signal: `SYNC-HAZARD-WRITE-AFTER-READ` naming `terrain_tile_buffer` with `vkCmdCopyBuffer` as the later access, on a cell-load frame under `BYRO_VALIDATION=1`. Absence on such a capture closes the row.
- **HYP-2: patch compute overwrite against the previous frame's AS-build reads.** The entry barrier in `TerrainPatchPass::record` (`terrain_patch.rs:749-770`) has source stages `TRANSFER|VERTEX_INPUT|VERTEX_SHADER|FRAGMENT_SHADER|COMPUTE_SHADER` but not `ACCELERATION_STRUCTURE_BUILD_KHR`, while the previous frame's patch, static and skinned BLAS builds read the same pool at that stage (`SHADER_READ`). `sync.rs` rider 16 itself says the chain was "derived by reading rather than by validation". Confirming signal: `SYNC-HAZARD-WRITE-AFTER-READ` on the global vertex buffer, `vkCmdDispatch` later, `vkCmdBuildAccelerationStructuresKHR` earlier. The §8.3/§8.4 clean runs make a hit unlikely.
- **HYP-3: readback zero-fill ordering.** `cmd_fill_buffer(readback[frame])` (`terrain_patch.rs:744`) is the slot's first device access in the frame and sits before the entry barrier. Its predecessor is the compute write from two frames earlier. Same shape as CONC-2026-11-09-2.1 (brick-buffer fill). Confirming signal: `SYNC-HAZARD-WRITE-AFTER-WRITE` naming a patch readback `VkBuffer`, on a frame with no HUD upload or depth-history copy (those carry a `FRAGMENT -> TRANSFER` edge).

- **CON-D2-03 (far-sun occlusion image, HYPOTHESIS)**: `dispatch_far_sun` (`composite.rs`, new in ea979a6f7) does an `UNDEFINED → GENERAL` transition with `src = TOP_OF_PIPE` every frame, where the sibling per-frame discards (sky cube, cloud shadow, exposure meter) name the previous reader's stage. The confirming signal is a sync-validation WRITE_AFTER_READ layout-transition hazard on "far sun occlusion".

## Not filed

- **CON-D7-02 (skill checklist gap, none is a defect):** the Dim 7 checklist omits `fuz.rs`'s `run_bounded` reader threads, the `Archive::open_loose` registry (last write wins on a race), `ClaimCheckedDecoder` on kira's decode thread, the `bink2` `thread::scope` sites and the `byro-dbg` / `byro-launcher` tooling threads. Folded into the next `/audit-sync`, not an issue.
- The plugin-walk `scope.spawn` propagates a panic to the main thread (no abort) and is unguarded, the same exposure as before #3813: an `/audit-esm` matter.

## Invariants verified

### Invariants verified (quoted from HEAD)

Dimension 1
- **Queue Mutex scopes.** `draw.rs:519-531`: `let queue = self.graphics_queue.lock().expect(..)`, then `self.device.queue_submit(*queue, &[submit_info], self.frame_sync.in_flight[frame])`, `drop(queue)` before the recovery calls. The present guard `pq` spans `queue_present(*pq, ..)` (`draw.rs:649-656`). `texture.rs:1304-1307`: `let q = lock_recovering(queue); device.queue_submit(*q, &[submit_info], fence)` inside its own block, closed before the wait. These are the only production `queue_submit` / `queue_present` sites in `crates/` and `byroredux/src`; the egui path holds its guard across the crate's one-shot upload, unchanged.
- **All-slots wait.** `sync_and_acquire_frame.rs:83`: `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)`; the per-slot spelling is absent from that file. The deferred-destroy ticks (`mesh_registry`, `texture_registry`, `accel_manager`, `scene_buffers.tick_retired_instance_buffers`) follow the wait (`:244-262`).
- **Rider 16 is wired.** `sync.rs` names `TerrainPatchPass` and `TerrainPatchBlasSet` in the prose, in the const-assert message ("the in-place terrain-patch vertices/BLAS/scratch all rest on the same both-slots wait ... #7245") and in `frames_in_flight_contract_names_every_dependent_resource`'s table (both owner files contain the name). "A grown scratch is already deferred-destroyed" is true: `blas_terrain_patch.rs:402-406` pushes the old buffer to `pending_destroy_scratch` with `DEFAULT_COUNTDOWN`.
- **Patch BLAS barriers.** Compute exit barrier `SHADER_WRITE -> VERTEX_ATTRIBUTE_READ|SHADER_READ|HOST_READ` into `AS_BUILD|COMPUTE|FRAGMENT|VERTEX_*|HOST` (`terrain_patch.rs:790-810`; build input is `SHADER_READ` at the AS-build stage, as in 507945d8). Set entry barrier `AS_WRITE -> AS_READ|AS_WRITE` at `AS_BUILD` before one batched `cmd_build_acceleration_structures`; exit barrier `AS_WRITE -> AS_READ` into `AS_BUILD|FRAGMENT|COMPUTE` (`blas_terrain_patch.rs:478-500`). Scratch regions are disjoint and aligned (`align_size` per op, `align_scratch_address` base, `scratch_alignment_padding` slack). UPDATE preconditions hold: same `max_vertex` and counts (size mismatch drops the request), address change forces `Build`. A deferred mesh is absent from `ready`, so `build_tlas_instances` skips it (`tlas.rs:555-560`).
- **TLAS publish.** The frame's `AS_BUILD/AS_WRITE -> FRAGMENT|COMPUTE/AS_READ` barrier still runs on both arms (`dispatch_skin_and_cluster.rs:405-411`), after the patch refits in the same command buffer (`draw.rs:194-203`).
- **No blocking submit in the per-frame patch path.** `create_patch_blas_entry` and the scratch grow use `create_device_local_uninit`; `reserve_meshes` runs at init.
- **New per-FIF buffers.** `sky_aperture_buffers` is a `Vec<GpuBuffer>` of `MAX_FRAMES_IN_FLIGHT`, written at `build_and_upload_instances.rs:1589` (count, then planes, through `write_mapped_at` and the atom-aligned `flush_range`), descriptor written for every set `i` (`buffers.rs:1008-1031`), destroyed in `SceneBuffers::destroy` and covered by `destroy_walks_every_per_frame_buffer_family`. `SKY_APERTURE_BUFFER_BYTES = 16 + MAX_COMPOSITE_SKY_APERTURES * 48`; the shader clamps `min(skyApertureHeader.x, uint(MAX_COMPOSITE_SKY_APERTURES))` (`lighting.glsl:599`). `env_brdf_lut_buffer` is host-written once at creation, read-only afterwards, destroyed in `SceneBuffers::destroy`; no rider needed. The `GpuCamera` buffer is `size_of::<GpuCamera>()` per slot (`buffers.rs:582`). `ray_budget_buffer` keeps one 256-byte stride per slot (`size_of::<GpuRayBudget>() == 80` pinned).
- **Deferred queues.** `pending_destroy_blas` and `pending_destroy_scratch` tick after the fence wait and drain in `AccelerationManager::destroy` through `drain_pending_destroys`. No new immediate `destroy_acceleration_structure` at an eviction site: the only new destroys are in `destroy_terrain_patch_blas`.
- **Latches promoted post-submit.** `promote_skin_frame_state` and `promote_skip_clear_latches` (linear-capture copy, ground-cover field flip, caustic and volumetrics skip-clear) sit after the `submit_failed` arm (`draw.rs:586-587`); `discard_pending_skip_clear_latches()` runs at the top of the frame (`:100`).
- **Resize.** `recreate_swapchain` opens with `device_wait_idle` (`resize.rs:60`). The terrain patch pass owns no extent-dependent resource. The composite pass's far-sun targets are per-FIF `GpuImage`s: drained and recreated in `recreate_on_resize` (`composite.rs:1532, 1576`), descriptors rewritten (`:1648`), freed in `destroy` (`:1813`).
- **GPU timers.** `QUERIES_PER_FRAME` is 72 with `const _: () = assert!(Q_TERRAIN_PATCH_BLAS_END + 1 == QUERIES_PER_FRAME)`, active bits are `u64`, host `reset_query_pool(.., 0, QUERIES_PER_FRAME)` runs each frame, and both stamps of the patch-BLAS bracket surround the same call.

Dimension 6
- **The three load-bearing orderings are intact** (`teardown.rs`): `skin_slots` drained before `skin_compute.destroy` (`skin_slot_drain_sits_outside_the_skin_compute_guard`, `drain_pos < pipeline_destroy_pos`), `frame_upscaler::destroy_allocations` after `destroy_device_objects`, exposure meter before the exposure resource and the `Arc::try_unwrap` (`:193-205`). The terrain-patch destroy (`:146`) and the patch BLAS destroy inside `accel.destroy` (`:103-125`) are both before `allocator.take()`.
- **`TerrainPatchPass::destroy`** frees the per-FIF readbacks, then pool, pipeline, pipeline layout and set layout behind null checks; the constructor failure arm calls the same `destroy`. `destroy_terrain_patch_blas` drains every entry (AS then buffer) and the shared scratch; the reserved patch meshes go with `mesh_registry.destroy_all`.
- **Guard tests exist and none is `#[ignore]`d.** The only `#[ignore]` in `crates/renderer` is `vulkan/sky_cube/filter/tests.rs:7` (needs a device). Present: `the_all_slots_wait_argument_is_pinned` (`sync.rs:837`; needle composed at runtime and matching only `sync_and_acquire_frame.rs:83`), `frames_in_flight_contract_names_every_dependent_resource` (`:687`), `render_finished_is_sized_and_indexed_per_swapchain_image` (`:601`), `one_time_lock_scope_tests` (`texture.rs:1532`), `wait_failure_arm_disposes_only_on_device_loss` (`:1489`), `every_initialize_layouts_body_uses_the_shared_submit` (`:2014`), `no_initialize_layouts_caller_destroys_without_the_in_flight_check` (`:2111`), `a_failed_layout_walk_fails_closed_instead_of_warning_and_continuing` (`resize.rs:2353`), `rgba_updates_use_the_frame_command_buffer_and_keep_descriptors` and `only_the_submitted_recording_consumes_its_pixels` (`dynamic_rgba.rs:415, 506`), the three promote-after-submit tests (`draw.rs:1123-1197`), `every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask` (`skinned_blas_refit.rs:1148`; scans every `.comp` by `read_dir`; `groundcover_models.comp` is a second named consumer and the mask already carries COMPUTE; neither new `.comp` names `skinnedVertexAddress` nor includes `ray_hit.glsl`), `taa_resolves_the_post_bloom_scene_tap`, `hot_path_maps_are_fx_hashed`.
- **A FIF bump fails them.** The `MAX_FRAMES_IN_FLIGHT == 2` const-assert is intact (`sync.rs`, #870) and its message now names rider 16.
- **Weak spot in a guard, not a finding.** `every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask` matches the literal `skinnedVertexAddress` in the `.comp` file itself, so a compute shader that reaches the deref only through an include of `ray_hit.glsl` is not seen (the test's own comment concedes the include graph is not traced). No current `.comp` does; none is added by this delta.

#### Invariants verified (Dimension 2 checklist, with the quote that settles each)

- **Skin chain (M29) and #3582 second consumer**: `skin_publish_barrier_consumer_tests` (`context/skinned_blas_refit.rs:1146-1201`) still reads every `*.comp` under `shaders/` for `skinnedVertexAddress`, excluding only the producer, and requires the dst mask `ACCELERATION_STRUCTURE_BUILD_KHR|FRAGMENT_SHADER|COMPUTE_SHADER, SHADER_READ`. No new compute shader (`far_sun_shadow.comp`, `terrain_patch.comp`) names the address. Barrier text unchanged in the range.
- **Frame-tail order**: `record_post_passes` (`post_passes.rs:424-460`) runs SSAO -> far sun -> composite -> linear capture copy -> bloom -> exposure meter -> TAA -> upscale -> presentation, as `taa_resolves_the_post_bloom_scene_tap` pins. Bloom's closing barrier (`bloom.rs:870-880`) has dst `FRAGMENT|COMPUTE|TRANSFER`, so the compute meter's sampled read of the scene is covered.
- **Exposure meter**: `dispatch` records SHADER_READ_ONLY->GENERAL (src `COMPUTE|FRAGMENT`), `cmd_dispatch(1,1,1)`, then GENERAL->SHADER_READ_ONLY with dst `COMPUTE|FRAGMENT` (`exposure_meter.rs:250-280`); both consumers (FSR dispatch, `presentation.frag`) are in that mask; fixed mode takes the same path (`record_exposure_meter_pass` has no mode branch). Unchanged since the baseline (reflow only).
- **Cross-frame ping-pong**: SVGF / TAA / volumetrics / caustic barrier text and per-FIF indexing are unchanged in the range (whitespace-insensitive diff). New cross-frame resources are per-FIF: patch readback and descriptor sets, sky aperture buffers, cloud-shadow map + readback, far-sun image. The shared resources (global vertex buffer rewrite, patch BLAS + scratch, terrain tile SSBO) rest on the all-slots wait and are listed in `sync.rs` rider 3 and rider 16.
- **Volumetrics (#1105)**, bloom (#931), skip-clear latches, combustion local grid, brick-buffer hypothesis: no commit touches their barriers or latches since the 11-09 baseline; the carried PLAUSIBLE row CONC-2026-11-09-2.1 is still open as #6835 and still unconfirmed.
- **Ground-cover / sky-bake latches**: `the_groundcover_field_flip_is_promoted_after_submit_not_at_record_time`, `skip_clear_latches_are_promoted_after_submit_not_at_record_time`, `the_linear_capture_copy_is_promoted_after_submit_not_at_record_time` (all `context/draw.rs`) and `a_frame_without_a_recorded_dispatch_does_not_advance_the_field` (`groundcover/frame.rs`) still exist. The cc16a2a77 refactor swapped two private helpers for `descriptors::memory_barrier` with identical masks (compared in the diff), and the #4293 ordering scan was updated to the new name.
- **Ground-cover model tier (#4413)**: pre-barrier src `DRAW_INDIRECT|COMPUTE_SHADER|TRANSFER`, post-barrier on the EMIT phase dst `DRAW_INDIRECT|VERTEX_SHADER|FRAGMENT_SHADER|TRANSFER` (`groundcover_models.rs:803-860`), so COMPUTE_WRITE reaches both the instance read and the indirect args. A prior-frame vertex read is ordered through the reservoir `begin_frame` barrier (src FRAGMENT, whose first scope includes logically earlier stages) into the TRANSFER-source edge.
- **Shadow filter / cloud shadow readback**: `camera_transmittance` calls `invalidate_if_needed` before `mapped_slice_mut` (`cloud_shadow.rs:233-239`) and runs behind that slot's fence (`begin_frame_recording.rs:44-50`); the producing barrier has its own SHADER_WRITE -> HOST_READ (`cloud_shadow.rs` host_visible). The terrain-patch harvest does the same (`terrain_patch.rs:635-641`).
- **Latch pattern for the new stateful passes**: sky bake, cloud shadow, env LUT, aperture planes and far-sun are stateless per frame (no cross-frame commit), so nothing to latch. The terrain-patch compute pass keeps only host-side stats and `pending[frame]`, which `record` clears before any early return (`terrain_patch.rs:670`); a discarded frame leaves a harmless one-off stale diagnostic. The two stateful items that do latch at record time are CON-D2-01 and CON-D2-02.
- **GPU timers**: `QUERIES_PER_FRAME = 72` with `Q_TERRAIN_PATCH_BLAS_END + 1 == QUERIES_PER_FRAME` const-asserted (`gpu_timers.rs:702`); brackets are written only on the paths that emit both start and end (`dispatch_skin_and_cluster.rs:609-616, 656-667`), and read-back is masked per bracket (`BIT_TERRAIN_PATCH` 34, `BIT_TERRAIN_PATCH_BLAS` 35, `BIT_FAR_SUN` 31) rather than WAIT-reading an unwritten query.

#### Invariants verified

Dim 3
- Parallel batches are unchanged: nine `add_to_with_access` systems; the only multi-system parallel stages are Early (`player_controller_system`, `timer_tick_system`) and Late (`camera_follow_system`, `reverb_zone_system`, `log_stats_system`, `metrics_sample_system`); `Physics`, `Update` (animation) and `PostUpdate` (propagation) each run one parallel system. So no new cross-thread edge is reachable outside those two sets, and every new cycle candidate is main-thread (MEDIUM cap).
- The detector lane is green on all four crates at HEAD (numbers above).
- Regression spot-check of the 2026-11-15 fixes, all in place: CONC-1 `eat_sleep.rs:284` takes `WalkSpeed` before `GlobalTransform` (test `the_diner_gather_takes_walk_speed_before_global_transform`); CONC-2 `systems/water.rs` `drop(bed_q)` (test `submersion_with_a_bed_field_takes_its_locks_in_one_order`); CONC-3 `asset_provider/animation.rs:212-229` copies the requested forms out before the catalog; CONC-4 `capture.rs:265`; CONC-5 `unload.rs:849-897` own `AnimatedTextureFlip` pass; CONC-6 `LimbEffects` declared at `early.rs:32` and `limbs.rs` hops listed. Also in place: ace99c164 (`CreatureActor` first), #7315 (`live_actor_bodies` first loop), #5955 (`projectile_flight_system` resolves owners and `actor_bodies` before taking `PhysicsWorld`, `projectiles.rs:414-428`).
- CONC-4's hoist widens the `InventoryCatalog` read to the end of `capture_entity_projections`, adding `InventoryCatalog` -> {`GlobalFormIdResolver`, `FactionRanks`, `FactionReputation`, `Perks`, `ActorCinematicState`, `AmbientPackageRuntime`, `ScenePackagePlayback`}. Every other `InventoryCatalog` acquisition (barter, encumbrance, crafting, crime, combat `note_attack_noise` / `sneak_strike`, interaction, loot, inventory actions, `weapon_defense`) either consumes the guard in the same expression or holds only `Inventory` / `SceneAliasCandidate` after it; no site holds one of the seven and then takes the catalog. No reverse edge.
- Two-resource nesting: no new code holds two resources in code order against a sorted site. `land_target` holds `SpellCastModel` across `SpellCatalog` (`player_cast.rs:454`); every other site copies the model first (`preview`, `settle`), so there is no reverse. `CastState` / `SpellCastModel` in `commands/spell.rs` are plain reads.
- `StringPool` stays a sink in new code: `registered_sync_points` takes the registry first (fad27f4fe); `load_skyrim_character_clip` holds the pool only across `convert_hkx_clip` under `&mut World`; `find_by_name` drops the pool before `Name`.
- Edition-2024 migration (`let` chains, tail-expression drop order) in `animation.rs`, `camera.rs`, `cinematic.rs`, `knockdown.rs`, `head_look.rs`, `walk_anim.rs`, `water.rs`, `story_events.rs`, `world.rs`, `scheduler.rs`: read the diffs; each is a pure re-spelling, guard scopes are equal or narrower. `World::find_by_name` / `find_by_form_id` tail expressions now drop the iterator temporary before the guard, which is the intended 2024 behaviour.
- Extension host boundary: `extensions/systems.rs`, `mod.rs`, `commands.rs`, `capture.rs` diffs add no guard across guest entry (`ui_menu_snapshot` is a read snapshot).

Dim 4
- Fixes this session hold: #7280 `run_after(water_audio_system, make_water_interaction_system())` is registered (`late/audio.rs:58-70`); #7311 the `papyrus_provider_system_declares_everything_it_acquires` scan exists and passes; #7318 / #7319 / #7345 the 16 exclusive scans pass; #7320 and #7321 their order tests pass (`console_raised_hits_are_queued_after_the_frame_cleanup` is feature-gated, the door and activation-flush pins ran); #7337 the new `PlayerControlState` / `VatsState` reads are declared on `combat_input_system`, `player_cast_system`, `player_bow_system`, `interaction_system`, `player_controller_system` (scans pass).
- `player_controller_system`, `physics_sync_system`, `make_animation_system`, `camera_follow_system`, `metrics_sample_system`: declarations complete against their listed bodies (the 55-test run) and, for the new hops in CON-D4-01, against the callee bodies by hand.
- New exclusives since the baseline and their declaration state: `sneak_stance_system`, `stance_use_system`, `skill_use_system`, `oblivion_level_system`, `player_cast_system`, `player_bow_system`, `detection_tick_system` have rows and scans; `bed_use_system`, `door_motion_system`, `obscript_object_menu_mode_system`, `menu_open_close_event_system` are bare `add_exclusive` (undeclared by design). `jail_bed_system` has a row but no scan; its row matches a same-file scan today.
- Cross-stage one-frame-stale review of the resources added in the window (writer stage -> reader stage): `FallTracker` (Early parallel, same system); `PendingSkillUse` (queue; writers Early / Update / Late-before-drain, one drain in Late after `stance_use_system`); `StanceTimers`, `LevelNotices` (Late only); `RestSession` and `SleepCompletions` (Update writers, Late `oblivion_level_system`); `PlayerDetection`, `StealthPoints` (Update tick first, readers later in Update and Late, indicator / HUD in the app loop); `PlayerActionNoise` (written by `combat_input_system`, taken by the NEXT frame's tick, documented as "since the last tick", a queue not a stale read); `CastState` / `ActiveProjectiles` (cast in the combat segment, flight in the later script-runtime segment, so a launched shot flies the same frame); `PendingTrapHits` -> `HitEvent` (covered by `hit_event_consumers_run_after_every_producer`); `DefaultActivation` (door verdicts before `two_state_activator_system`, #7321 pin); `MenuRegistry` (republished once a frame by the app loop outside the scheduler, GUI-REG-007, so Update readers see the previous frame by design); `AmbientPackageRequestsSeen` (works in either order of the ambient and scene passes, so no pin needed); `PlayerControlState` (pre-existing writer in the script / dialogue stages; the new Update-stage `player_may_fight` / `player_may_activate` reads trail a script's `DisablePlayerControls` by at most one frame, the same lag the Early movement gate already has, and it is not new state).

Dim 5
- `PhysicsWorld` remains a sink in all new and changed code. `character_controller_system`: nothing is acquired between the `pw` read at `character.rs:505` and `drop(pw)` at `:644`; `fall_landing` runs after `drop(pw)`; the `FallTracker` write is a temporary consumed by `and_then` (`:659-661`). `projectile_flight_system`: `physics` is scoped to the `step` block (`projectiles.rs:438-462`), `Dead` / `PendingTrapHits` / `sneak_strike` are taken after. `actor_in_reach` and `sight_is_clear` collect `RapierHandles` before the physics read. `travel` / `guard` / `follow` / `wander` / `escort` pass 1b hold the physics read only across the pure `step_along_waypoints_detailed`; the new `GridWalk` / `NavGraphs` code added no acquisition there. `sync_contact_force_subscription` drops its read before taking the write.
- `physics_sync_system`: the #7278 latch (`world.resource_mut::<PhysicsWorld>().take_newcomers_since_buoyancy()`, `sync.rs:143`) is a temporary; `pw.note_newcomers_registered()` is called inside the existing `register_newcomers` guard. `waves_require_contact_rescan` (#7018 / #7279) takes `RigidBodyData` then `WaterContact` after the `PhysicsWorld` read is dropped (`water.rs:~990-1000`); `RigidBodyData` and `WaterContact` are declared on the row, and `Physics` is a single-parallel-system stage, so no other thread contends. No other site takes `WaterContact` then `RigidBodyData`.
- `cam.fly` -> `set_player_mode(&World)` writes `PlayerMode` from the debug drain, an exclusive that runs after the parallel Late batch (`DRAIN_SYSTEM_NAME` is pinned as a Late exclusive), so `camera_follow_system` only reading `PlayerMode` in parallel stays sound.
- `trap_hits::live_actor_bodies`: the first loop was fixed by #7315. The second loop still holds the `ActorColliderOwner` read across `ActorVitals` / `Dead` / `RapierHandles` gets. I searched every `ActorColliderOwner` acquisition (`ragdoll.rs:572,663`, `vats.rs:718`, `interaction.rs:1128,1195,1283`, `combat.rs:810`, `projectiles.rs:598`, `faction_hostility.rs:225`, `unload.rs:714`, `commands/view.rs:349`): none takes it while holding `ActorVitals`, `Dead` or `RapierHandles`, so no reverse edge exists and the lane never recorded one. Latent shape only; not filed.

#### Invariants verified

- Streaming worker shutdown: `shutdown` takes `worker`, stores `cancel` (Release), drops `request_tx`, then `join_with_timeout` (`streaming/mod.rs:840-868`); `Drop` calls `shutdown(1 s)` and short-circuits when `worker` is `None`. `live_load.rs` drains the old state (`drain_streaming_state`, `streaming_helpers.rs:737`, which `take()`s the slot) before it builds the new worker, so two `byro-cell-stream` workers never overlap.
- Worker to main flow: payloads by `mpsc`, no `&mut World` on a worker; `ReadAt` is positional; `open_shared` stamps `(len, mtime, dev, ino, ctime)` before the open.
- TextureStreamer: one `pool.spawn`, inside `spawn_read` under `catch_unwind`; `done_tx.send` always happens; a failed read backs off 600 frames; generation counters as above.
- Texture prefetch: panicking read finishes its slot; epoch guard; `take` waits only on `Running`.
- Terrain height: every job under `spawn_guarded`; results through `Arc<Mutex<Vec>>` drained on main; no `World` or registry access in a job.
- Movie decode, debug-server bounds, allocator sharing, rayon fan-outs: as in the delta table.
- `Send + Sync` bounds: no raw pointer crosses a thread; `UiManager`/Ruffle `Player` is not an ECS resource; kira work stays behind `AudioWorld` on the main thread.

## Guards and lane results

- `BYRO_LOCK_ORDER_CHECK=1 cargo test` over `byroredux-core --lib` (1027), `byroredux-physics` (280), `byroredux-scripting --lib` (1492) and `byroredux --bin byroredux` (4128): all green, run at HEAD in a fresh worktree.
- `byroredux-renderer --lib`: 1688 passed, 1 ignored (the device-only sky-cube test).
- Dimension-specific runs: see the per-dimension files' guard sections (Dim 2: 26 guard tests, Dim 7: the targeted worker-thread tests, Dim 3: 55 detector-on tests).

## Known-Open Register (cited, not re-filed)

- #5365 (narrowing the both-slots frame wait), whose rider list and pin tests are intact (`the_all_slots_wait_argument_is_pinned`).
- #6835 (carried unconfirmed by Dim 2, H-3).
- #7221 (TSD-4.2 per-pixel cost; the BLAS-in-TLAS chain is read-only audited here and rests on rider 16 and a capture, H-1).
