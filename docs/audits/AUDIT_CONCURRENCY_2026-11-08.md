**HEAD**: `0419437ab` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-31.md` (@ `670f81917`, 480 commits ago) · **Audited**: all 7 dimensions, depth `deep`, one agent per dimension (read-only on source; no engine, GPU or `BYRO_VALIDATION` run, so every Vulkan-sync call is a source-read conclusion)

# Concurrency & Synchronization Audit, 2026-11-08

**Command**: `/audit-concurrency` (all 7 dimensions, `deep`). Build: `CARGO_INCREMENTAL=0`, rustc 1.96.0. **Dedup**: `/tmp/audit/concurrency/issues.json` (open issues) plus targeted `--state all` searches; the 2026-10-31 report.

**Counts: 0 CRITICAL / 1 HIGH / 2 MEDIUM / 5 LOW new.** No production lock inversion, no missing barrier, no queue-submit race. Both lock-order reds are test fixtures; the MEDIUM in Dim 7 is a missing panic guard.

## Baseline carry-over

| Baseline item | State |
|---|---|
| CONC-2026-10-31-01 `improve_plan` Inventory then ItemInstancePool | **Fixed**, #6561 closed; `crafting.rs` copies rows and drops the guard first |
| CONC-2026-10-31-02 `mine_system` missing from HitEvent pin | **Fixed**, #6580 closed |
| #6506 `moved_surface_buffers` not destroyed | **Fixed** via #6521 (dd4c50340); `SceneBuffers::destroy` pins every `Vec<GpuBuffer>` family |
| Baseline claim "core alone stays green" | **No longer true** (see CONC-2026-11-08-3.1) |

## Guard runs at HEAD

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` | green, 3616 passed, 0 failed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting` | green, 1340 passed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib --no-fail-fast` | **RED on core** (`resource_2_mut_both_writable`, 12/12 runs; plus a ~1% flake, 3.2); physics 275/0, scripting 1340/0 |
| CI form `cargo test --workspace --no-fail-fast --exclude byroredux-ui` | **RED**, 11999 passed, 2 failed (the core test above; `byroredux_debug_server roster_tests::every_inspectable_component_is_registered`, not concurrency) |
| `cargo test -p byroredux-renderer --lib` | 1626 passed, 0 failed, 1 ignored (`sky_cube::filter::tests`, needs a Vulkan device). Every named GPU-sync guard ran and passed; none is `#[ignore]`d |
| Scheduler proof (44 + 55 tests: `scheduler_access`, `system_access_declaration`, `schedule_order`, `per_frame_driver`, CI-lane pins) | all green, none ignored. Non-vacuity floors hold with **zero headroom** (9 parallel systems vs floor 9; 7 same-stage pairs vs floor 7) |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics` | 275 passed (`sync`: 34); named bin guards 1 passed each |
| streaming / texture_streamer / terrain_height / prefetch / shared-parse bin tests | 78 passed, 1 ignored |

## Findings (ranked)

| ID | Sev | Dim | Title |
|---|---|---|---|
| CONC-2026-11-08-3.1 | HIGH | 3 | `resource_2_mut_both_writable` closes a lock-order cycle with itself; core lib lane and CI `lock-order-check` red |
| CONC-2026-11-08-3.2 | MEDIUM | 3 | `Health`/`Position` fixtures held in plain order by two tests; intermittent red of `every_query_kind_hands_off_and_untracks_cleanly` |
| CONC-2026-11-08-7.1 | MEDIUM | 7 | `TextureStreamer` pool job has no `catch_unwind`/`panic_handler`; a panicking archive read aborts the process (sibling of #6233) |
| CONC-2026-11-08-2.1 | LOW | 2 (+1.1) | Linear capture arms `pending_readback`/`frames_recorded` at record time, not after `queue_submit` (screenshot and depth twins identical) |
| CONC-2026-11-08-4.1 | LOW | 4 | `VatsState` written by an Early exclusive that runs after its Early parallel reader; one-frame lag unpinned |
| CONC-2026-11-08-4.2 | LOW | 4 | `vats_mode_system` is a `HitEvent` producer missing from `hit_event_consumers_run_after_every_producer` |
| CONC-2026-11-08-5.1 | LOW | 5 | `probe_spawn_ground` takes `ContactConfig` under a `PhysicsWorld` read guard (sink-rule edge; no cycle today) |
| CONC-2026-11-08-6.1 | LOW | 6 | `both_staging_frees_delegate_to_the_shared_helper` and the helper doc cover only two of the three capture staging frees |

All eight are **NEW** (no open or closed issue matched). CONC-2026-11-08-1.1 (INFO) is the same defect as 2.1 and is folded into it.

### Notes on the ratings
- **3.1 is HIGH only under the red-lane rule.** It is a test body on one thread; no shipped thread can reach it. It matters because it reddens CI `lock-order-check` and masks any real cycle behind it. The TypeId order flipped between baseline and HEAD (likely `Cargo.lock`/`Cargo.toml` churn; cause not pinned), the same deterministic-per-build family as #6101/#6234. The spot check of `world_tests.rs:905-919` confirms the shape: `resource_2_mut::<ResA, ResB>()` (TypeId-sorted) followed by plain `resource::<ResA>()` then `resource::<ResB>()` in one test. Fix: read the values in separate statements or use `try_resource_2`; better, give plain-order-hold tests private fixture types so they cannot collide with sorted-pair tests in the process-wide graph (also fixes 3.2).
- **7.1 is MEDIUM on precedent, not a demonstrated crash.** `byroredux/src/texture_streamer.rs` (landed 2026-11-03, after the baseline) has no `catch_unwind`/`panic_handler` (verified by grep); rayon aborts on an unhandled spawned-job panic and the workspace keeps `panic = "unwind"`. No reachable panic was found in the BA2 DX10 re-read path (returns `io::Error`). Fix: guard the read, map a panic to `bytes: None`, always send the `FinishedRead`, and add a test beside `a_panicking_height_job_is_reported_failed`.
- **Verification path for 2.1:** `cargo test` source-order scan only (shape of `skip_clear_latches_are_promoted_after_submit_not_at_record_time`); no validation layer needed. Consider fixing the three capture twins together. Diagnostic-only output; reachable only on a device-loss-class tail `Err`.

## Incidental (outside concurrency; candidates for `/audit-tooling`)
- `byroredux-debug-server` `roster_tests::every_inspectable_component_is_registered` is red at HEAD: `FactionReputation` (CHARAL component, 79371213d) derives `inspect` but is not registered in `crates/debug-server/src/registration.rs`. This also reddens the CI-form run. Not covered by an open issue (#4755, #5694, #6759 are closed and about other gaps).
- `fuz_to_wav_bytes` (`ffmpeg` subprocess, up to 10 s) runs on the frame thread inside `VoiceSoundCache`'s write guard (`dialogue_voice.rs:265-282`); a stall, not a deadlock; bounded by #6307.

## Skill-text drift to fix in `.claude/commands/audit-concurrency/SKILL.md`
- Dim 3 "core alone stays green" is false until 3.1/3.2 are fixed.
- Dim 7: name `texture_streamer.rs` with the four-pool guard rule; add Paths for `asset_provider/material/cdb.rs` (`sf-cdb-index` thread), `crates/audio/src/fuz.rs` (`run_bounded` reader threads) and `asset_provider/archive.rs::open_loose` (all reviewed, not findings); note the capture-ordering test `finish_readback_runs_after_the_in_flight_fence_wait` pins screenshot and depth only, not linear capture.
- Dim 2/6: the capture-twin list is now three (screenshot, depth, linear) at both the record-time latch and the shared staging free.
- Dim 4: the non-vacuity floors are exactly met; any consolidation that drops a parallel pair fails them.

## What was not confirmed
No `BYRO_VALIDATION=1` / syncval capture was possible (one-engine-instance rule), and the `vulkan-validation` CI lane was not run. Source-read conclusions that a capture would settle: the M39 residency barrier pair in `texture.rs` and the AS-build input access flag (`skinned_blas_refit.rs:802-810`). Confirming signals: zero `SYNC-HAZARD-WRITE-AFTER-READ`, `VUID-vkUpdateDescriptorSets-None-03047` and `VUID-vkDestroyImage-image-01000` with `render.texture_streaming` on across a grid-cross. The combustion local grid, penumbra shadow filter and TAA surface-search changes were read clean (the local-grid WAR-against-composite hypothesis was examined and dropped: Stage D's `FRAGMENT|COMPUTE|TRANSFER` source scope and the depth-copy barrier give an execution chain), but rest on reading plus the commit bodies' own clean syncval notes.

Next step: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-11-08.md` (domain label *concurrency* for 3.1, 3.2, 4.1, 4.2, 5.1, 7.1; *sync* for 2.1, 6.1).

---

# Appendix: per-dimension reports (verbatim from the dimension agents)



## Appendix Dim 1

# Concurrency audit 2026-11-08 — Dimension 1: Vulkan Queue & Acceleration-Structure Sync

Auditor scope: `crates/renderer/src/vulkan/{sync.rs,texture.rs}`, `vulkan/context/*`, `vulkan/acceleration/`, plus the texture-streaming promotion path in `texture_registry/`. HEAD 0419437ab. Read-only; no engine/GPU launched (no BYRO_VALIDATION run, so nothing below is validation-confirmed).

## 1. Delta scope

- Commits since 2026-10-31 in the repo: 480. On this dimension's paths (sync.rs, texture.rs, acceleration/, context/, texture_registry/): 58.
- Net diff vs baseline 670f81917 on those paths: 35 files, +7250/-3315.
- Mechanical diff of synchronization-relevant call sites, baseline vs HEAD (non-test code, `crates/renderer/src`):
  - `queue_submit` / `queue_present` / `wait_for_fences` / `device_wait_idle` / `queue_wait_idle` / `with_one_time_commands*` / `destroy_acceleration_structure` / `thread::spawn` / `rayon`: **zero new sites**, zero removed (one line re-indented in blas_static.rs).
  - `cmd_pipeline_barrier` / `memory_barrier(` / `image_barrier*` in `context/`, `acceleration/`, `texture_registry/`: **only new sites are the 3 in `context/linear_capture.rs`** (new this window). `vulkan/texture.rs` gained the residency-change barrier pair (see 3.f).
  - `acceleration/{tlas,blas_skinned,mod,predicates}.rs`, `sync.rs` logic, `draw.rs` submit/present, `resize.rs` idle, `sync_and_acquire_frame.rs` wait order: unchanged apart from field-path renames (`self.skin.*`, `self.post_state.*`, `self.diag.*`, `self.exterior.*` from #6697-#6700) and one added readback call.
- Notable commits:
  - Refactors with barrier-set parity checked: #6708 (`record_skinned_blas_refit` -> `SkinChain` phase methods) — barrier/stage/access multiset identical to baseline (diffed); #6709 (`build_blas_batched` -> `prepare_static_blas_batch` / `ensure_batch_scratch` / `record_batch_builds` / `alloc_compact` / `commit_compacted_batch`) — scratch-serialise barrier before first build (#4177), `i>0` barrier, AS_WRITE->AS_READ before size query, forget-on-in-flight unwind all preserved; #6710 (`record_geometry_pass` phases); #6697-#6700 (VulkanContext field grouping).
  - M39 texture streaming: 494d077b2, 073e3af53, 2e30cdf10, 3d67d963e (streamer pool, host-side reads only), 15ee603de, 9179059a4, dd2cd84e9, 6a4c661a8 (#6738), b8d89bdcb (#6737), 23f4aeee3 (replay deferred descriptor writes against live view), b34b897df (#6624 cube fallback).
  - New diagnostic: `context/linear_capture.rs` (M51 PTR-OUT-001 / RLT-REF-007), added to the sync.rs rider list.
  - Volumetric local fire grid (626197ae5..3500d2737): outside this dimension's Paths; per-FIF `state/optical/irradiance/stats_buffers[frame]`, history barriers, all-slots wait documented in `begin_frame` — no single-slot host-visible resource added.

## 2. Guard runs

| Command | Result |
|---|---|
| `CARGO_INCREMENTAL=0 TMPDIR=/mnt/data/tmp cargo test -p byroredux-renderer --lib` | **1626 passed, 0 failed, 1 ignored**, exit 0 (log: /tmp/audit/concurrency/dim1_test.log) |
| Ignored test | only `vulkan::sky_cube::filter::tests` (`#[ignore = "requires a Vulkan device"]`) — not a named guard |
| `vulkan::sync::tests::the_all_slots_wait_argument_is_pinned` | ok (not ignored) |
| `vulkan::sync::tests::frames_in_flight_contract_names_every_dependent_resource` | ok (not ignored; now also scans `linear_capture_staging`) |
| `vulkan::sync::tests::render_finished_is_sized_and_indexed_per_swapchain_image` | ok |
| const-assert `MAX_FRAMES_IN_FLIGHT == 2` (`sync.rs:145`, #870) | present, compile-time |
| `vulkan::texture::one_time_lock_scope_tests::queue_guard_released_before_one_time_fence_wait` | ok |
| `...::one_time_commands_free_cmd_buffer_on_every_error_path` | ok |
| `vulkan::texture::one_time_failure_class_tests::wait_failure_arm_disposes_only_on_device_loss` | ok |
| `texture_registry::dynamic_rgba::tests::rgba_updates_use_the_frame_command_buffer_and_keep_descriptors` | ok |
| `texture_registry::dynamic_rgba::tests::only_the_submitted_recording_consumes_its_pixels` | ok |
| `context::teardown::...skin_slot_drain_sits_outside_the_skin_compute_guard` | ok |

`grep '#\[ignore'` over `crates/renderer/src` returns the single sky_cube hit; no named guard is ignored. A MAX_FRAMES_IN_FLIGHT bump fails the const-assert at build time, and `the_all_slots_wait_argument_is_pinned` / `frames_in_flight_contract_names_every_dependent_resource` fail on source scan.

## 3. Checklist walk

a. **Queue submission single-Mutex** — HOLDS. `draw.rs:~582` binds the `MutexGuard` (`let queue = self.graphics_queue.lock()`), `queue_submit(*queue, ..)`, `drop(queue)` before any recovery; present at `draw.rs:~722` binds `pq` across `queue_present`. `present_queue` is an `Arc::clone` when families match (`init.rs:152-155`). `texture.rs` one-time helper: `let q = lock_recovering(queue); queue_submit(*q, ..)` inside a block that closes before `wait_for_fences` (`texture.rs:1284-1318`); reusable-fence guard (`expect`, #5209) held across the wait by design; non-device-lost wait failure leaks cmd/fence and poisons the fence mutex via caught panic (#5270). Guards green.

b. **Frame-in-flight both-slots wait** — HOLDS. `sync_and_acquire_frame.rs:83` still `wait_for_fences(&in_flight, true, u64::MAX)`; the only delta in that file is the added `linear_capture_finish_readback()` call after the wait (`:159`). New single-slot resource `linear_capture_staging` is covered by the wait and was added to the sync.rs rider list (item 4) and to `frames_in_flight_contract_names_every_dependent_resource`. Its grow path (`ensure_linear_capture_staging` destroys + recreates immediately) is only reached at record time after the all-slots wait and an extent change fails the capture first, so no in-flight reader exists. #5365 untouched (do not re-file).

c. **AS build -> read barriers** — HOLDS. No change to `tlas.rs`/`blas_skinned.rs`. Static batch: `record_batch_builds` emits scratch-serialise before first build (#4177), between builds, and AS_WRITE->AS_READ before the compacted-size query; compaction copy and builds are separate fenced one-time submits. Skinned refit barrier multiset identical to baseline (publish/scratch barriers intact, dst WRITE|READ per #1790).

d. **AS build INPUT access flag (507945d8)** — HOLDS on read. `skinned_blas_refit.rs:802-810`: COMPUTE_SHADER/SHADER_WRITE -> ACCELERATION_STRUCTURE_BUILD|FRAGMENT|COMPUTE with `SHADER_READ` (not AS_READ). Confirmation by BYRO_VALIDATION still owed (not run here).

e. **Deferred destruction vs in-flight reads** — HOLDS. Tick sits after `texture_registry.begin_frame` and the both-slots wait (`sync_and_acquire_frame.rs:~245-255`): mesh, texture, accel, retired-instance-buffer ticks. `destroy_acceleration_structure` sites are unchanged in kind: `blas_static.rs` (unwind / eviction-to-pending / failed-build paths on never-published or fenced objects), `blas_skinned.rs:277` (tick), `mod.rs` (shutdown `destroy` after idle), `tlas.rs` (resize behind idle). No new immediate destroy at an eviction site.

f. **Swapchain recreate / idle** — HOLDS. `resize.rs:60` `device_wait_idle` before destroying swapchain-dependent resources; `:1576` idle before upscaler switch; `tlas.rs:1118` belt-and-suspenders idle; `mesh/geometry_ssbo.rs:849` idle on rebuild failure. Resize delta is only the new local-fire view plumbing (bindings 12..=17) and field renames.

g. **Blocking one-time submits / texture promotions** — HOLDS, one submit per frame.
   - `VulkanContext::update_texture_streaming` (`telemetry.rs:111`, called once from `byroredux/src/app_frame.rs:71` before `draw_frame`) -> `flush_pending_promotions(.., &self.graphics_queue, transfer_pool, &self.transfer_fence, MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME=32 MiB)` -> exactly one `flush_upload_batch` (`take` capped by `MAX_RESIDENCY_CHANGES_PER_FRAME=64`) -> `with_one_time_commands_reuse_fence`: queue Mutex only around `queue_submit`, persistent `transfer_fence`, no other lock held across the wait. No second submit site per frame: all `flush_pending_uploads` callers are load/spawn-time (`cell_loader/references`, `npc_spawn/loot_appearance`, `scene.rs`, `streaming_helpers::flush_pending_lod_textures`), as at baseline. The streamer pool (3d67d963e) only reads archive bytes; `texture_streamer.rs` has no vk/ash/queue references.
   - Residency-change cmd (`texture.rs:record_dds_residency_change`): first barrier `ALL_COMMANDS -> TRANSFER` on the old image (SHADER_READ_ONLY -> TRANSFER_SRC) so its source scope covers earlier frames still sampling it (same queue, submission order); second barrier `TRANSFER -> ALL_COMMANDS` restores old to SHADER_READ_ONLY and publishes the new image. Old image is retired through `pending_destroy` with `current_frame_id`, destroyed `>= MAX_FRAMES_IN_FLIGHT` begun frames later, after the descriptor replay in the same `begin_frame`.
   - Descriptor visibility: `update_texture_streaming` runs outside the begin_frame..submit window, so `fence_confirmed_idle_slot` is `None` (cleared by `note_frame_submitted`) and `apply_descriptor_write` queues for ALL slots; `flush_pending_set_writes` runs at each slot's `begin_frame` (post-wait) and, since 23f4aeee3, `resolve_pending_set_writes` replays against the live view (dedup per (handle,binding), fallback if released), so no write can name a retired view. Walked the promote-then-demote-next-frame case: slot A gets N1 at its begin, N1 retires at F+1 and is destroyed at F+3 after slot A's replay to N2 in the same `begin_frame`. No hazard found.
   - #6738/#6737 hardening: stale/second change per handle in a batch refused; refused install retires the new image via `pending_destroy` (never referenced by a descriptor); truncated payload refused before slicing. Failure arm keeps `may_be_in_flight` leak policy (#4891).
   - Overlay uploads (`dynamic_rgba.rs`): file unchanged this window; both guards green; `update_rgba` extent-change path still defers old image via `pending_destroy`.

h. **New linear capture (`linear_capture.rs`)** — barriers correct on read. Pre-bloom tap: `SHADER_READ_ONLY -> TRANSFER_SRC` (src COLOR_ATTACHMENT_OUTPUT|TRANSFER / COLOR_ATTACHMENT_WRITE), copy, restore with dst COMPUTE|FRAGMENT|TRANSFER; recorded between composite and bloom (`post_passes.rs:389`). Post-TAA tap: COMPUTE/SHADER_WRITE -> TRANSFER/TRANSFER_READ in GENERAL, read-only copy, no restore needed. Readback is at top of next frame after the all-slots wait, with non-coherent invalidate (#2740 pattern). No queue calls, no new waits.

## 4. Findings

No CRITICAL, HIGH or MEDIUM findings. No NEW findings requiring an issue.

### CONC-2026-11-08-1.1 — Linear-capture readback is armed at record time, not on submit success (INFO, not filed)
- **Dimension**: 1 (Vulkan queue sync)
- **Location**: `crates/renderer/src/vulkan/context/linear_capture.rs` `linear_capture_copy_recorded` (arms `pending_readback`, bumps `frames_recorded`) vs `context/draw.rs` submit-failure arms.
- **Status**: NEW observation, same shape as the screenshot and depth-capture twins (`screenshot_pending_readback`, `depth_capture_pending_readback` are also armed at record time). Existing pattern; not a regression.
- **Description**: If `reset_fences`/`queue_submit` fails after recording, the next frame still consumes `pending_readback` and averages whatever the staging buffer held into the EXR (stale or uninitialised bytes). The `dynamic_rgba` path got the opposite treatment (`note_frame_submitted` -> `submitted(slot)`).
- **Trigger Conditions**: `--linear-capture` run AND a `queue_submit`/`reset_fences` error on a capture frame (practically only device loss, which ends the run anyway).
- **Impact**: Diagnostic-only output corruption; no GPU hazard (readback happens after the all-slots wait; the buffer is never read while in flight). Non-finite samples are already counted in the report.
- **Suggested Fix**: Optional — clear `overlay.linear_capture.pending_readback` and decrement `frames_recorded` in the submit-failure rollback (`rollback_skin_frame_state` neighbour), or accept as is. Severity INFO; given the no-speculative-fix guardrail and the twins' identical behaviour, left unfiled.

## 5. Residual / what this audit did NOT confirm
- No BYRO_VALIDATION / syncval run was possible (one-engine-instance rule). Items 3.c, 3.d and 3.g (ALL_COMMANDS barrier covers earlier-frame sampling of the promoted image) are source-read conclusions; confirming signal if desired: `BYRO_VALIDATION=1` run with `render.texture_streaming` on in a streaming exterior (grid-cross) producing zero `SYNC-HAZARD-WRITE-AFTER-READ` / `VUID-vkUpdateDescriptorSets-None-03047` on bindless sets and zero `VUID-vkDestroyImage-image-01000`.
- Volumetric local-grid compute/barrier correctness is outside this dimension's Paths (renderer audit lane).


## Appendix Dim 2

# Concurrency audit 2026-11-08 — Dimension 2: Compute -> AS -> Fragment chains

HEAD 0419437ab. Baseline `docs/audits/AUDIT_CONCURRENCY_2026-10-31.md` (670f81917). Read-only, no engine or GPU process; every Vulkan-sync statement below is code reading plus the guard tests, and the commit bodies' own sync-validation notes where cited.

Result: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 1 LOW (CONC-2026-11-08-2.1, diagnostic-only). Every other new chain traces clean.

## 1. Delta scope

`git log --since=2026-10-31 -- crates/renderer/src/vulkan crates/renderer/shaders` is **100 commits**. Dim-2 Paths since the baseline:

| Path | Commits since baseline |
|---|---|
| `volumetrics.rs` + `volumetrics/` (new `local_grid.rs`) | 22 |
| `context/post_passes.rs` | 12 |
| `shadow_filter.rs` | 2 (a2180dd18 RLT-SHD-003, 270b56cd7 comment) |
| `taa.rs` | 4 (6926fddab silhouette search, 695417b9a gamma param, destroy refactor, doc) |
| `svgf.rs` | 4 (bypass param, destroy refactor) |
| `restir.rs` | 2 (doc/shader-header moves, 76ec72dcb) |
| `skin_compute.rs`, `context/skinned_blas_refit.rs`, `context/dispatch_skin_and_cluster.rs` | refactors only (#6697 SkinningState, #6708 SkinChain phases, #6719 comments) |
| `context/linear_capture.rs` | new (75b4024df, 76ec72dcb) |
| `caustic.rs`, `composite.rs`, `material.rs`, `groundcover.rs` | destroy refactor / bindings 12..17 / flag / cell-colour fields |
| `sky_cube*`, `groundcover/`, `groundcover_models.rs`, `bloom.rs`, `exposure_meter.rs`, `water_caustic.rs`, `gbuffer.rs` | **0 commits** |

Notable commits:
- d3e3acc7b, 626197ae5, 1adf9bdc1, a1a243bc2, 9dadde306: the combustion local grid (new `volumetrics_local_sim.comp`, new `combustion_phase_bake.comp`, atlas, irradiance field, brick buffer).
- a2180dd18: penumbra-aware shadow filter.
- 6926fddab: TAA 3x3 neighbourhood surface search.
- 684bc9161: the froxel sun trace now samples the bindless texture array from compute (alpha coverage).
- 75b4024df, 76ec72dcb: scene-linear EXR capture, pre-bloom and post-TAA taps.
- 494d077b2: M39 texture residency.
- 7c72057be: #6608 ground-cover stale scatter, after an NVRM Xid 109 device loss.

The only new GPU work is `volumetrics_local_sim.comp` (per frame), `combustion_phase_bake.comp` (once, at init), the two capture copies and the M39 promotion copies. Everything else is shader or CPU change on existing passes.

## 2. Guard runs

`CARGO_INCREMENTAL=0 TMPDIR=/mnt/data/tmp cargo test -p byroredux-renderer --lib` -> **1626 passed, 0 failed, 1 ignored** (the ignored one is `sky_cube::filter::tests::gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`, which needs a Vulkan device). Log: `/tmp/audit/concurrency/dim2_test.log`. The shared build finished with `BUILD_EXIT=0`.

Named guards, all seen passing in the log:
- `context::skinned_blas_refit::skin_publish_barrier_consumer_tests::every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask`
- `context::post_passes::tests::taa_resolves_the_post_bloom_scene_tap`
- `context::post_passes::tests::dispatch_ran_never_clears_and_resets_latch`
- `context::post_passes::tests::record_volumetrics_pass_routes_skip_clears_through_the_shared_latch`
- `context::post_passes::tests::record_bloom_pass_skips_raw_correctness_views_before_dispatch`
- `context::draw::skin_dispatch_ran_ordering_tests::skip_clear_latches_are_promoted_after_submit_not_at_record_time`
- `context::draw::skin_dispatch_ran_ordering_tests::the_groundcover_field_flip_is_promoted_after_submit_not_at_record_time`
- `groundcover::frame::tests::a_frame_without_a_recorded_dispatch_does_not_advance_the_field`
- `volumetrics::unit_tests::every_skipped_frame_drops_the_temporal_history_not_just_the_first`
- `volumetrics::transport_occupancy_tests::the_dispatch_empty_branch_resets_the_occupancy_mask`
- `caustic::skip_clear_mask_pin_tests::volumetrics_dispatch_names_transfer_in_its_source_scope`
- `composite::composite_frag_doc_tests::the_shadow_filter_dispatch_and_the_composite_flag_share_one_predicate`
- `scene_buffer::shader_contract_tests::bloom_dispatches_after_composite_and_applies_itself_downstream`
- `reflect::tests::local_grid_params_block_matches_the_host_struct_in_both_consumers`
- the `volumetrics::local_grid::*` placement and memory-ledger tests
- `shadow_filter::tests::*` (push block, width lane, guides)
- `gpu_timers::tests::shadow_filter_bracket_reports_measured_duration_inside_svgf`
- `linear_capture::tests::*` (3)

The `skin_publish_barrier_consumer_tests` guard still `read_dir`s every top-level `shaders/*.comp` except `skin_vertices.comp`. It matches the token `skinnedVertexAddress`. Today that hits `caustic_splat.comp` and `groundcover_models.comp`, and the dst mask still carries `COMPUTE_SHADER`. It cannot see a `.comp` that reaches the deref only through an `#include` of `ray_hit.glsl`; no `.comp` does today (only `skin_vertices.comp`, the producer, includes it).

## 3. Checklist walk

- **Skin chain (M29): clean.** `record_builds_and_refits` at `skinned_blas_refit.rs:802` publishes `COMPUTE/SHADER_WRITE -> AS_BUILD|FRAGMENT|COMPUTE / SHADER_READ`. The refit window closes with `AS_BUILD_WRITE -> AS_BUILD_READ`. Dispatch bracket, publish, first-sight builds and refits run in the pinned order (`skin_chain_phase_order_tests`). The #6697, #6708 and #6719 refactors changed no stage or access mask (diff grep). The `has_populated_output` latch is still staged in `skin_pending_populated` and promoted after submit.
- **Frame-tail order: clean.** `post_passes.rs:351-415` runs SVGF (+ shadow filter), caustic, volumetrics, SSAO, composite, linear-capture pre-bloom copy, bloom, exposure meter, TAA (+ post-TAA copy), upscale, presentation. The pre-bloom copy restores `SHADER_READ_ONLY_OPTIMAL` with dst `COMPUTE|FRAGMENT|TRANSFER`, which is what bloom's first sample needs. The TAA output stays `GENERAL`: the post-TAA tap is read-only (`SHADER_WRITE -> TRANSFER_READ`, no restore needed), and the native-blit after-barrier restores `GENERAL` for a `GENERAL` source (`frame_upscaler.rs:735-745,823`).
- **Cross-frame ping-pong: clean.** Local-grid atlas (`state`/`dynamics`/`optical`), irradiance field, bricks, params and stats are all `[frame]`-indexed, with `previous = (frame + N - 1) % N` for history, in both `record_sim` and `record_frame`. Composite's bindings 12..16 are built per slot `i` from `fire_views()` (`composite.rs:965-1000,1360-1398`). SVGF, TAA and caustic are unchanged in indexing.
- **Volumetrics (#1105): clean.** The three deferred-write latches (`tlas_written`, `lights_written`, `boundary_geometry_written`) are all set by their writers in the same `if let` block that calls `dispatch`. `dispatch` asserts and resets them before any early `?`. Inject -> integrate is `COMPUTE -> COMPUTE` with `inj_to_int` + `pre_int_write` (src includes `FRAGMENT|TRANSFER`). Stage F publishes `integrated_volumes` plus the three local-grid images `COMPUTE -> FRAGMENT`. `record_publish_bricks` publishes the brick buffer `COMPUTE -> FRAGMENT`. The `HOST_READ` barrier publishes binding 18 and the stats buffer.
- **Bloom (#931): unchanged.** The file has 0 commits; the contract test passes.
- **Ground-cover / sky-bake: unchanged and clean.** `groundcover/` and `groundcover_models.rs` have 0 commits and `sky_cube*` has 0 commits. The #6031 and #6291 two-step field latch is still guarded and green. #6608 was fixed in the binary: `render_one_frame` now calls `prepare` on loading-stage frames via the off path, pinned by `a_loading_stage_frame_takes_the_groundcover_off_path`. `GpuGroundCoverCell` grew 64 -> 208 B (da6d65dc1) and its buffer is sized from `size_of` (`construct.rs:272`). There is one `draw_frame` caller and it is always preceded by `collect_and_prepare_groundcover`, so the stale-chunk window is closed.
- **Passes new since 10-15 (shadow filter / cloud shadow): clean.**
  - The shadow filter's first barrier is a buffer barrier on the reservoir buffer, `FRAGMENT/SHADER_WRITE -> COMPUTE/SHADER_READ` (`shadow_filter.rs:389-412`).
  - Iteration 0 reads the raw ratio and writes B, then A and B alternate, then the last iteration writes `delta`.
  - Every iteration ends in a write->read image barrier, `COMPUTE -> COMPUTE`, or `COMPUTE -> FRAGMENT` on the last. The B/A WAR is covered by the execution dependency.
  - `shadow_filter.comp` writes `outDelta` or the dest ratio on every invocation path, including the guide-invalid and pass-through returns, so no pixel keeps stale correction.
  - The filter is dispatched inside the SVGF bracket or on the SVGF-absent/latched fallback, and composite's flag reads the same predicate (test).
  - RLT-SHD-003's new penumbra-width lane lives in the same reservoir buffer, so the existing barrier covers it.
  - Cloud-shadow bake: `cloud_shadow.rs` still carries `FRAGMENT -> COMPUTE`, `COMPUTE -> FRAGMENT` and `SHADER_WRITE -> HOST_READ` (`sky_cube/cloud_shadow.rs:197-243`); the file is unchanged.
- **Ground-cover model tier: unchanged** (0 commits).
- **Exposure meter: unchanged and clean.** The pre-barrier is `COMPUTE|FRAGMENT -> COMPUTE`, and the post-barrier publishes to `COMPUTE|FRAGMENT` for FSR and presentation. It runs in fixed mode too, and is skipped only by the raw-debug and failure latches.
- **Combustion local grid (#6533): clean on reading, plus the commit bodies' own sync-validation notes.** Order is Stage B (froxel-field history barriers) -> B2 -> inject.
  - B2 `record_frame` is an irradiance read->write / write->read barrier, unconditional. Then `record_sim`, only if `has_work()`: `cmd_fill_buffer` on `bricks[frame]`, then a `TRANSFER -> COMPUTE` barrier, then the atlas history barriers, the dispatch, and a publish of `state`/`dynamics`/`optical` `COMPUTE -> COMPUTE` before inject samples them.
  - Inject writes irradiance for every froxel and reads `previousLocalIrradiance` only when `prev_camera_pos.w > 0.5` (`volumetrics_inject.comp:1445`), so a skipped or reset frame is gated by `history_valid`.
  - Stage F `composite_barriers` publishes state, optical and irradiance to composite.
  - Skipped frames: `record_skipped_frame` calls `grid.invalidate_history`, and `clear_local_grid_params` zeroes and disables slot `frame`'s table, so composite cannot march stale images. `GpuLocalGridParams::new(.., has_work(), ..)` agrees with whether `record_sim` ran.
  - Promotion: `grid.dispatched_this_frame` -> `mark_frame_completed` -> `commit_history`, only inside `VolumetricsPipeline::mark_frame_completed`, which is only called after a successful `queue_submit` (`draw.rs:667`).
  - The commit bodies of 1a, 1b, 1c and the follow-ups record "sync validation clean on s3a and s5" and a clean validation run with the bench.
  - Tests: `the_local_grid_follows_the_skip_and_completion_wiring`, `local_grid_params_block_matches_the_host_struct_in_both_consumers`.
- **MaterialBuffer SSBO: clean.** Host upload before draw recording, unchanged. `material.rs` only gained a flag.
- **Volumetrics skip path (#4780 / #5956): clean.** The pending latches are reset at the top of `draw_frame` and promoted after submit (`draw.rs:123,659`). `record_skipped_frame` runs on every not-ran frame, including the dispatch `Err` arm, and the local-grid reset rides it. The all-slots-fence rider for the combustion occupancy mask is unchanged.
- **#4989 back-to-back palette dispatches:** not re-raised (needs a hazard capture).

Other new consumers traced:
- Inject's new set 1 is the TextureRegistry bindless array, indexed with `nonuniformEXT` (`volumetrics_inject.comp:102`), bound from `texture_registry.descriptor_set(frame)` per frame. No descriptor is updated mid-frame; M39 promotion writes are deferred to the slot's `begin_frame`.
- M39 residency copy: `record_dds_residency_change` uses `ALL_COMMANDS -> TRANSFER` and back, restores `SHADER_READ_ONLY` on both images, and retires the old image through `pending_destroy` after `MAX_FRAMES_IN_FLIGHT` frames. Lifetime review belongs to Dim 6; the barriers are fine.
- TAA 6926fddab adds only extra `texelFetch`es on the existing prev mesh-ID and prev-normal inputs, with no new binding or barrier. Its params UBO is still uploaded before the bulk barrier and still latches `taa_failed`.
- BLAS compaction copy in `build_blas_batched` is a refactor move (#6709), not new.

## 4. Findings

### CONC-2026-11-08-2.1: linear capture arms its readback and frame count at record time, not after `queue_submit`
- **Severity**: LOW (diagnostic output only; no GPU hazard, no engine state corrupted)
- **Dimension**: Compute -> AS -> Fragment chains (the latch promote-after-submit class)
- **Location**: `crates/renderer/src/vulkan/context/linear_capture.rs:388-394` (`linear_capture_copy_recorded`), called from `linear_capture_record_copy` (:293) and `linear_capture_record_copy_post_taa` (:357). The readback is `linear_capture_finish_readback` (:125), at the top of the next `draw_frame`.
- **Status**: NEW. Same record-time arming exists in the `screenshot` and `depth_capture` twins (`screenshot.rs:268`, `depth_capture.rs:264`); none of the three is rolled back on a discarded command buffer. I found no open issue for it (`issues.json` is 102 open; searched capture/screenshot/readback).
- **Description**: `linear_capture_copy_recorded` bumps `job.frames_recorded` and sets `pending_readback = Some(extent)` while the copy is still only recorded. `draw_frame`'s three tail `Err` sites (`end_command_buffer`, `reset_fences`, `queue_submit`) discard the command buffer. They roll back skin state (`rollback_skin_frame_state`) and the staged skip-clear latches (`discard_pending_skip_clear_latches`), but nothing touches the capture state.
- **Trigger Conditions**: `--linear-capture` active, plus a tail `Err` on a frame that recorded a copy. After that:
  1. The next frame's `linear_capture_finish_readback` sees `pending_readback = Some`.
  2. It accumulates the staging buffer's contents, which is the previous frame's pixels or the allocation's unspecified initial bytes if it was the first frame.
  3. It counts that read in `frames_read`.
  4. The EXR is written with one wrong frame in the average and no error.
- **Evidence**: `linear_capture.rs:388-394`; `draw.rs:123` and `:659` (the only staged/promoted latch pair, which omits the capture); `rollback_skin_frame_state` is the only other tail-Err hook.
- **Impact**: One stale or uninitialised frame in a reference EXR, silently. The capture tool feeds the M-LIGHT oracle gates. Reachable only when `end_command_buffer`, `reset_fences` or `queue_submit` fails, which in practice means device loss, so it is rare.
- **Suggested Fix**: Stage `frames_recorded` and `pending_readback` in a `pending_*` slot and promote them next to `promote_skip_clear_latches`; discard them next to `discard_pending_skip_clear_latches`. Pin it with a source-scan test shaped like `skip_clear_latches_are_promoted_after_submit_not_at_record_time`.
- **Verification Path**: `cargo test` only (a source-order scan of `draw.rs` / `linear_capture.rs`). It needs no validation layer or RenderDoc, because the defect is host-side ordering. Consider fixing the three capture twins together.

## 5. Hypotheses considered and dropped (no finding)

- **Local-grid WAR on `bricks[frame]` / atlas / `delta` against composite's FRAGMENT read two frames earlier.** The new barriers name only `COMPUTE` (or `TRANSFER` for the fill) in their src stage. I dropped this because the execution chain exists: Stage D's `pre_int_write` has src `FRAGMENT|COMPUTE|TRANSFER`. The `copy_depth_to_history` barrier has src `FRAGMENT_SHADER` and dst `TRANSFER`, so it covers the `cmd_fill_buffer`. Every shadow-filter last-iteration barrier is `COMPUTE -> FRAGMENT`. Sync-validation runs in the 1b/1c commit bodies were clean. If a syncval hazard ever appears on these resources, the confirming signal would be a WAR report on `brick_buffers[]` / `state[]` from `vkCmdFillBuffer` or the sim dispatch against the composite sample. I do not claim one.
- **`TAA` -> native-blit after-barrier restoring `GENERAL` with dst stage `FRAGMENT` only.** Unchanged since the baseline, already reviewed, and it is the fallback path. Not re-raised without a hazard report.
- **Stale `grid.dispatched_this_frame` after a discarded submit followed by a no-box frame.** `commit_history` would run with zero live boxes, so it is a no-op. Not a finding.

## 6. Not covered

No BYRO_VALIDATION or syncval capture was run (engine launch forbidden). The local-grid, shadow-filter and TAA changes are clean by reading plus the commit bodies' own validation notes, not by a fresh hazard count. Cross-queue concerns and swapchain/present semaphores belong to Dim 1. Texture-residency object lifetime and descriptor-slot retirement belong to Dim 6.


## Appendix Dim 3

# Concurrency audit, Dimension 3: ECS lock ordering and deadlock (system level)

HEAD 0419437ab, 2026-11-08. Baseline AUDIT_CONCURRENCY_2026-10-31.md (HEAD 670f81917). Read-only on source. All runs `CARGO_INCREMENTAL=0`, `TMPDIR=/mnt/data/tmp`, `BYRO_LOCK_ORDER_CHECK=1`, `RUST_BACKTRACE=1`. Working tree was clean at start.

**Counts: 0 CRITICAL / 1 HIGH / 1 MEDIUM / 0 LOW new.** Both findings are test fixtures in `crates/core/src/ecs/world_tests.rs`, not production inversions. The core lib lane is red for the first time since #6101. Both baseline open items are fixed.

## 1. Delta scope

- 62 commits touch `byroredux/src/systems`, `byroredux/src/extensions` or `crates/scripting` since 2026-10-31. Most are record decode, VATS, mines, CTDA getter and module-move work.
- `byroredux/src/extensions/`: 0 commits in the window. The last is 67722b6e4 (10-27). The `ExtensionHost` boundary is unchanged since the baseline.
- `crates/core` changed in 33 files, +2218/-199 since baseline (mostly `source_scan.rs`). `lock_tracker.rs` and `world.rs` are untouched by the findings.
- `Cargo.lock` gained 85 lines and `Cargo.toml` changed (renderer EXR capture 75b4024df, `syn` dev-dep for the acquisition scan c98a4552f). This is the most likely cause of the TypeId reshuffle in finding 3.1.

## 2. Guard runs

| Command | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux` (detector on) | **GREEN: 3616 passed, 0 failed, 93 ignored.** Baseline was 3422 passed with 2 failed. |
| `cargo test -p byroredux-scripting` (detector on) | **GREEN: lib 1340 passed, 0 failed, 11 ignored.** Integration tests are all real-data `#[ignore]`s. |
| `cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib` | **RED.** core 923 passed, 1 failed (`resource_2_mut_both_writable`), deterministic: 7 of 7 whole-lane runs and 12 of 12 `-p byroredux-core --lib` runs. A second core test failed in 2 of about 190 parallel runs (finding 3.2). With `--no-fail-fast`: physics 275/0, scripting 1340/0. Plain fail-fast stops after core, so physics and scripting do not run. |
| `cargo test -p byroredux-core --lib` (default features) and `--features inspect` | RED, same test (924 and 927 tests). The SKILL line "core alone stays green" is stale. |
| CI form `cargo test --workspace --no-fail-fast --exclude byroredux-ui` (detector on) | **RED:** 11999 passed, 2 failed. One is core's `resource_2_mut_both_writable`, the only "lock-order cycle" line in the log. The other, `byroredux_debug_server registration::roster_tests::every_inspectable_component_is_registered`, is not concurrency: `FactionReputation` has the inspect derive but no debug registration. It belongs to `/audit-tooling`. |
| Static scheduler proof, all `ok` in the bin lane | `build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, `every_parallel_system_declares_everything_it_acquires`, `the_parallel_system_table_covers_every_parallel_registration`, `cross_file_hops_and_get_forms_reach_their_acquisitions`, the 8 exclusive-system declaration scans, `hit_event_consumers_run_after_every_producer`, `lock_order_job_propagates_cargo_status_through_tee`, `vulkan_validation_job_enables_the_lock_order_detector`. |
| Fixed-inversion regressions, `ok` | `reference_state::tests::unload_captures_do_not_invert_production_lock_orders`, `crafting::tests::improving_raises_a_held_item_in_place_and_never_hands_out_a_copy`. |

## 3. Baseline open items

- **CONC-2026-10-31-01** (`improve_plan`, Inventory then ItemInstancePool): **FIXED**, #6561 CLOSED, commit 3c9dca757. `byroredux/src/crafting.rs:218-232` copies the matching rows out, drops the `Inventory` guard, then calls `stack_quality`. The test helper `quality_of` (`:765`) copies the stack in one statement. The `improving_raises…` test is green under the detector.
- **CONC-2026-10-31-02** (`mine_system` missing from the HitEvent ordering pin): **FIXED**, #6580 CLOSED. `mine_system` is now in `hit_event_consumers_run_after_every_producer`'s producer list (`byroredux/src/boot/schedule/mod.rs:259-260`). The test passes.

## 4. Checklist walk

- **Static proof.** Holds. Both declaration-conflict tests and the non-vacuity tests are green and none is `#[ignore]`d. Scheduler soundness is Dim 4.
- **Dynamic supplement.** The bin and scripting lanes are green, so no cycle is reachable from hand-built worlds. The core lane is red for a fixture reason (3.1, 3.2). I did not run the `vulkan-validation` lane because GPU launches are forbidden.
- **`&mut World` vs `&World`.** No new multi-read function appears in the lanes.
- **New system bodies.** I read every added acquisition in the diff of `systems/`, `crafting.rs` and the scripting crate. All are clean:
  - `mines::land_mine_hits`: the `PendingMineHits` guard is a `let`-statement temporary, dropped before the `HitEvent` write. It holds the `HitEvent` write across `has::<Dead>` and `has::<GlobalTransform>`, a new edge. I found no reverse holder: `combat.rs:322`, `combat_ai.rs:266`, `trap_hit_requests.rs:49` and the `combat_damage_system` gather (`combat.rs:378`) take `HitEvent` with no `Dead` or `GlobalTransform` guard live.
  - `vats_confirm`: the `HitEvent` guard is explicitly dropped before the `HitShots` write ("one at a time"). `vats_queue_part`: every `VatsState` read is a statement temporary, and `has_concentrated_fire` runs with none held. `vats_scan`: the `VatsModel` and `LoadedCellIndex` reads are copied or cloned out, and the #6444 GlobalTransform gather-then-filter shape is intact. The #6484 `target_bodies_by_owner` collects owners and takes one `RapierHandles` get at a time.
  - `faction_hostility::living_actors`: the `CombatDisposition` guard dies in the closure before the `Dead` reads. `dead_body` and `mines` now share it.
  - `random_conversation`: `sweep_orphaned_partners` gathers under a read guard, drops it, then takes `query_mut`. The `held_by_us` check is a transient `get`.
  - `perk_entry_points::lend_base_form` and `tempering::with_item_enchanted`: guards are not held across `f()`. `recipes_at` holds the `CraftingBook` resource read across them. That is main-thread menu code with no reverse path, and the bin lane is green.
  - Water sampler order is consistent at all three sites: `WaterPlane`, `WaterVolume`, `WaterFlow`, `WaterSurfaceMesh`, `WaterBedField` (`physics/water.rs:656-665`, `systems/water.rs:220`, `systems/character.rs:1286`). The one test inversion on this chain was fixed in-window by f131c5275.
  - `script_requests::FOLDER_MUSIC_NOTED` is a leaf `Mutex`, with the `log::info!` after the unlock. `MenuCodes` is copied out before use.
- **FormIdPool leaf mutex.** `crates/scripting/src/condition/eval.rs` changed in 3 commits. I re-read the `FormIdComponent` then `FormIdPool` order and found no new caller breaking it. The `vats_scan` per-actor `FormIdPool` read holds nothing else.
- **TypeId-sorted resource pairs.** No new nested `try_resource` pair in the window. The only unsorted-pair problem is the test fixture in 3.1, where the same shape (#6101) re-opened.
- **Fixtures vs boot registrations.** New fixtures (`pair_world` calls `byroredux_scripting::register`; the VATS fixtures register `HitShots`; the mines fixture registers the storages it touches) do not short-circuit the new paths.
- **Canonical order and StringPool sink.** No new multi-lock hold touches `StringPool`.
- **Guard ↔ sandbox boundary.** `extensions/` is byte-identical in the window. `with_extension_host` (`extensions/systems.rs:249-280`) still drops the `ExtensionHostSlot` guard (`:254-259`) before `lock()`, then runs dispatch, commits commands, drops the host, and emits diagnostics. The commit step takes ECS locks under the host mutex only after the guest has returned, which is the documented design. No finding.

## 5. Findings

### CONC-2026-11-08-3.1: `resource_2_mut_both_writable` closes a lock-order cycle with itself, and the core lib lane is red

- **Severity**: HIGH under the red-lane rule used for the 10-27 cycles. It is a test body, with no production inversion, and the cycle is not reachable by any shipped thread.
- **Dimension**: ECS Lock Ordering (system level), core test lane
- **Status**: NEW (no matching open or closed issue). This is a recurrence of the #6101 class (TypeId-order-dependent test shape, deterministic per build).
- **Location**: `crates/core/src/ecs/world_tests.rs:905-919`
  ```rust
  let (mut a, mut b) = world.resource_2_mut::<ResA, ResB>();   // records the TypeId-sorted edge
  ...
  let a = world.resource::<ResA>();                            // holds ResA ...
  let b = world.resource::<ResB>();                            // ... then takes ResB (:917)
  ```
- **Evidence**:
  - Fails in isolation, single-threaded: `target/debug/deps/byroredux_core-d73b7e2321c2158c --exact ecs::world::tests::resource_2_mut_both_writable` panics at `lock_tracker.rs:516` with "attempted acquisition of `ResB` while holding `ResA` … `ResB` → `ResA` → `ResB`". The backtrace's first test frame is `world_tests.rs:917`. The same test passes with `BYRO_LOCK_ORDER_CHECK` unset.
  - `resource_2_mut::<ResA, ResB>` takes the pair in TypeId order. The cycle needs `TypeId(ResB) < TypeId(ResA)`, so the sorted edge is B→A and the later A→B read pair closes it inside one test.
  - It passed at the baseline (core 888/0) and is red in four forms: the multi-crate lane (7 of 7), `-p byroredux-core --lib` default features (12 of 12), `--features inspect`, and the CI workspace form. So the TypeId order flipped between baseline and HEAD. Cause not pinned. `Cargo.lock` and `Cargo.toml` changed in the window and are the likely reason.
  - The CI `lock-order-check` job reddens on this test (`status=${PIPESTATUS[0]}`, #4993).
- **Trigger Conditions**: any build whose TypeId order for the `ResA`/`ResB` fixtures is B < A. There is no cross-thread scenario; the test runs on one thread.
- **Related**: #6101, #6234. The SKILL.md Dim 3 text "core alone stays green" is now false.
- **Suggested Fix**: do not hold the two plain reads together. Read the values in separate statements, for example `let a = world.resource::<ResA>().0;` then `let b = world.resource::<ResB>().0;`, or use `try_resource_2::<ResA, ResB>()`, which sorts. Better, give every test that deliberately holds an unsorted pair its own private fixture types, so a plain-order hold cannot collide with a sorted-pair test in the process-wide graph. Pin the flip-proofing in `try_resource_2_does_not_deadlock_against_resource_2_mut`'s neighbourhood.

### CONC-2026-11-08-3.2: `Health`/`Position` fixture pair is held in plain order by two tests, which intermittently reddens `every_query_kind_hands_off_and_untracks_cleanly`

- **Severity**: MEDIUM. Same class and same test-only scope as 3.1, but it fails in about 1 of 100 parallel runs. A flaky red lane is worse to triage than a deterministic one.
- **Dimension**: ECS Lock Ordering, core test lane
- **Status**: NEW (same family as 3.1; file together or separately).
- **Location**:
  - Plain-order holders: `world_tests.rs:345-346` (`multiple_read_queries_coexist`: `query::<Health>` then `query::<Position>`) and `:1237-1238` (`query_read_then_write_different_types_ok`: `query::<Health>` then `query_mut::<Position>`).
  - Sorted-pair victim: `:1651` (`every_query_kind_hands_off_and_untracks_cleanly`, `query_2_mut::<Health, Position>`). The poison test at `:1466` also calls `query_2_mut::<Position, Health>`.
- **Evidence**:
  - Captured failure log: `/tmp/audit/concurrency/d3_flake_hit.log`. Panic is "attempted acquisition of `Health` while holding `Position` … `Health` → `Position` → `Health`", with the first test frame at `world_tests.rs:1651`. The sorted branch of `query_2_mut` takes `Position` first, so `TypeId(Position) < TypeId(Health)` in this build.
  - Attribution by pair-run: `<bin> --test-threads=1 --exact every_query_kind_hands_off_and_untracks_cleanly <X>` fails for exactly two X: `multiple_read_queries_coexist` and `query_read_then_write_different_types_ok`. The other 86 `ecs::world::tests` tests, run in the same pairing, do not trigger it.
  - It passes under whole-suite `--test-threads=1` (all 924 green). It hit 1 of 7 early whole-lane runs and 1 of 150 later runs of the lane-3 core binary (`d73b7e2321c2158c`). A further 80 runs of the default-feature binary (`fc9b5d2a76bb6796`) had none. The rate depends on which of the conflicting tests inserts its edge first; the victim is whichever runs second.
- **Trigger Conditions**: parallel `cargo test` of core with the detector on, in a build where `TypeId(Position) < TypeId(Health)`.
- **Suggested Fix**: same as 3.1. Give these two tests private fixture types, or make them take the pair through `query_2_mut` or a sorted order. Add a one-line doc on the shared `Health`/`Position` fixtures that nothing may hold them in plain order.

## 6. Not findings

- No production inversion was found. Every new system body is a snapshot-then-use shape. The `ExtensionHost` boundary is untouched and clean. The scripting and bin lanes' ~5000 tests run green under the detector.
- The debug-server `every_inspectable_component_is_registered` failure (`FactionReputation`) surfaced in the CI-form run. It is a tooling-registration gap for `/audit-tooling` and is not a concurrency item.
- Full-log artifacts are in `/tmp/audit/concurrency/`: `d3_lane1.log`, `d3_lane2.log`, `d3_lane3.log`, `d3_ci_form.log`, `d3_flake_hit.log`.


## Appendix Dim 4

# Concurrency audit 2026-11-08 — Dimension 4: Scheduler Proof Soundness

HEAD 0419437ab. Baseline: docs/audits/AUDIT_CONCURRENCY_2026-10-31.md (670f81917). Read-only audit; no source edits, no engine launch.

## 1. Delta scope

`git log --since=2026-10-31 -- byroredux/src/boot byroredux/src/scheduler_access_tests.rs crates/core/src/ecs byroredux/src/systems` returns 79 commits. Most are not about the scheduler (VATS, perks, texture streaming, god-rays, groundcover, ...). The ones that matter for this dimension:

| Commit | What |
|---|---|
| 2a02c5446 #6684 | ordering guards read the BUILT schedule (`schedule_order`) |
| c98a4552f #6685 | access scan became a `syn` syntax-tree walk |
| c22a9d410 #6701 | `Access::with(fn(Access)->Access)` lets a subsystem export its own declarations |
| 065aac9c4 #6702 | `Scheduler::run_before` / `run_after` / `resolve_order` (+518 lines in `scheduler.rs`) |
| 22e0f71fc #6703, 49aa54e36 #6704 | Late / Update / PostUpdate registration split into one file per subsystem |
| d7320311b #6720 | `per_frame_driver_functions_stay_within_their_size_pins` counts code lines |
| 5129240c4 #6546 | Access rows of interaction / container-loot / combat-damage updated |
| cc80f60b6 #6580 | `mine_system` added to the `HitEvent` producer pin (baseline CONC-2026-10-31-02) |
| 6390bfb90 #6488 | player river current follows the bed (new `WaterBedField` read in `character.rs`) |
| 00d53e659 #6579 | shared `living_actors` roster and `PassClock` |
| 6186ab77e #6625 | VATS exits on cell transition / player death |

Structural diff of the schedule, baseline vs HEAD (both were extracted from the registration files):

- The set of registered system names is identical. Only two helper fns were added (`register_combat_damage_system`, `register_interaction_system`) and one module (`extension_dispatch`).
- Parallel registrations: 9 in both (`add_to_with_access`). Exclusive registrations: 115 undeclared (`add_exclusive`) + 48 declared (`add_exclusive_with_access`) in both. The same 101 exclusive names appear at both commits.
- `Access::with` is used only by three exclusive rows (`npc_dialogue.rs:226,237,238`); no parallel row delegates.
- `scheduler.rs`: the non-test diff only adds code (constraints, `resolve_order`, a fail-fast in `run`). Nothing in the parallel dispatch path was removed.

## 2. Guard runs

All runs used `CARGO_INCREMENTAL=0`, `TMPDIR=/mnt/data/tmp`, against the shared build (BUILD_EXIT=0).

| Command | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- scheduler_access system_access_declaration schedule_order per_frame_driver` | 44 passed, 0 failed, 0 ignored |
| `... -- boot::schedule scheduler_access_tests` (superset: also picks up `hit_event_consumers_run_after_every_producer`, `story_dispatch_...`, `transient_cleanup_is_the_last_late_exclusive`, `fragment_activation_order_tests`, ...) | 55 passed, 0 failed, 0 ignored |
| `cargo test -p byroredux-core --features inspect --lib -- scheduler access` (new `resolve_order` / `Access::with` unit tests) | 52 passed, 0 failed, 0 ignored |
| `grep '#\[ignore' byroredux/src/scheduler_access_tests.rs byroredux/src/boot` | no matches |

Tests confirmed present and green in the run:

- Proof: `build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, `every_parallel_system_declares_everything_it_acquires`, `the_parallel_system_table_covers_every_parallel_registration`, `cross_file_hops_and_get_forms_reach_their_acquisitions`, `condition_src_covers_every_condition_file`, and the 7 exclusive `*_declares_everything_it_acquires` scans.
- Orderings: `player_wind_read_is_declared_and_weather_writer_is_exclusive`, `billboard_runs_after_camera_follow_in_late`, `footstep_runs_after_camera_follow_in_late`, `submersion_runs_after_camera_follow_and_before_water_audio`, `player_body_facing_runs_in_update_before_propagation`, `story_dispatch_and_forcegreet_update_ordering_is_pinned`, `hit_event_consumers_run_after_every_producer`.
- CI-lane pins: `lock_order_job_propagates_cargo_status_through_tee`, `vulkan_validation_job_enables_the_lock_order_detector`, `..._fails_on_a_panic`, `..._resolves_lavapipe_and_fails_on_init_failure`, `..._requires_a_selected_device`, `..._requires_live_rt`.
- Size pins: `per_frame_driver_functions_stay_within_their_size_pins`.

Non-vacuity floors (`scheduler_access_tests.rs:297-312`: `parallel_system_count() >= 9`, `analyzed_pair_count() >= 7`) hold, and they hold with zero headroom. Counted from the registration files:

- 9 parallel systems: `player_controller`, `timer_tick`, `physics_sync`, `camera_follow`, `log_stats`, `metrics_sample`, `reverb_zone`, `animation`, `transform_propagation`.
- 7 same-stage pairs: Early 1 (player_controller, timer_tick) + Late 6 (C(4,2) over camera_follow, log_stats, metrics_sample, reverb_zone).
- Update, PostUpdate and Physics each hold one parallel system.

So any further demotion trips the floor, which is the intended behaviour. `PARALLEL_SYSTEMS.len()` (9) equals `parallel_registration_count` (9).

## 3. Checklist walk

- **Proof is live.** The two named tests are not ignored and pass. Floors hold (exactly at the floor). The table/registration count match is enforced by `the_parallel_system_table_covers_every_parallel_registration` (green). `resolve_order` runs inside `build_scheduler` (`mod.rs:49`) and panics on an unknown name, a forbidden cross-stage edge or a cycle.
- **Moved systems keep their declared access.** The system-name set and the parallel / exclusive classification are unchanged against the baseline (section 1), and `build_scheduler_reports_zero_access_conflicts` is green.
- **Blind spots / (a) parallel system changed with an uncovered acquisition.** CLEAN. No parallel system was added. The parallel bodies that changed:
  - `player_controller_system` (`character.rs`): the new `world.query::<WaterBedField>()` at `character.rs:1286` is declared at `early.rs:60`, and the scan sees it.
  - `PlayerEntity` moved to `components::player`. The scan matches `PlayerEntity::get` by name, and the test is still green.
  - `physics_sync_system` (`sync.rs` / `water.rs`): the new `WaterBedField` read is declared at `physics.rs:~48`. Env-flag caching and `local_current` / `authored_wave_height_over_bed` are pure.
  - `metrics_sample_system` / `log_stats_system` only gained fields of the already-declared coverage resource.
  - `timer_tick_system` gained only a save-registration fn.
  - `transform_propagation` / `animation`: comment-only changes.
  I also checked the cross-file calls out of `character.rs` that the scan's hop table does not list: `scene::min_walkable_normal_y` is pure. `ground_character_body_at`, which reaches `register_newcomers_and_refresh_queries` and `probe_walkable_floor_near`, is a separate pub fn called from transition orchestration outside the scheduler (#3267). It is not reached from `player_controller_system`. Neither `query_2_mut` nor `resource_2_mut` appears in any parallel body (the only `query_2_mut` in `debug.rs` is `spinning_system`).
- **Cross-stage sequencing / (b).** No unpinned problem found; one benign note, finding 4.1. Resources added in the delta: `PendingMineHits`, `WeaponSubject`, `WeaponSkillTable`, `ArmorModel`, `MenuCodes`, `EvidenceChestDeposit`, `TextureResidencyTelemetry`, `LoadedCellIndex` / `CurrentCellContext` / `CurrentExteriorContext` (moved), `PlayerEntity` (moved). None is a new single-writer / multi-reader resource written in a later stage than a reader. Most are installed at boot or by the cell loader outside the scheduler. `PendingMineHits` is written and drained by `mine_system` itself. `WindField` still has a single writer (`weather`, Early exclusive); the local-gust commit 69a3b0b30 touched only the reader and the shaders, and its player/physics/billboard orderings are pinned and green. `PendingEvidenceDeposits` deliberately straddles the frame boundary (documented in `registry_completeness_tests.rs:766`, pre-existing, not a regression).
- **`resolve_order` soundness (new in #6702).** Read in full. Edges are only applied within a (stage, exclusive) phase. A cross-phase edge in the forbidden direction returns `ContradictsStageOrder`. A same-name self-edge becomes a `Cycle` error rather than being silently ignored. The plan is computed for every phase before any is applied, so an error leaves the schedule untouched. All 9 in-tree `run_after` uses are parallel→exclusive or exclusive→exclusive and are valid. `transient_cleanup_is_the_last_late_exclusive` is green, so the constraint-based reordering did not displace `event_cleanup_system`.
- **(c) HitEvent / story-event producers.** The baseline gap CONC-2026-10-31-02 is FIXED: #6580 (cc80f60b6) added `"mine_system"` to the producer list at `schedule/mod.rs:260`, and the test is green against the built schedule. Production `HitEvent` producers at HEAD:
  - Pinned: `combat_input_system` (`combat.rs:328`), `make_npc_combat_ai_system` (`combat_ai.rs:281`), `projectile_flight_system` (`projectiles.rs:592`), `mine_system` (`mines.rs:157`), `pending_trap_hit_flush_system` (`trap_hit_requests.rs:60`), and the two script-event systems that feed the trap flush.
  - Not pinned: `vats_mode_system` (`vats.rs:851`, Early exclusive). It runs before every consumer, which are Update/Late, so order is correct by stage. The later producers also hold on an existing event (NPC AI #4324, trap flush, mines #6540, projectile queue #5860). `combat_input_system` is gated off during VATS (`player_can_act`, `character.rs:91-101`), so there is no same-frame overwrite. See finding 4.2.
  - Test-only inserts: `interaction.rs`, `combat.rs` (the 1515 / 1565 / 1652 sites), `combat_anim.rs`, `obscript_objects.rs`.
  - Story-event queue: `player_controller_system` still declares `writes_resource::<PendingScriptStoryEvents>()` (`early.rs:85`). No new parallel story-event producer appeared.
- **(d) Exclusives vs parallel.** No parallel system lost its declaration (`undeclared_parallel_count() == 0`, `known_conflict_count() == 0` and `unknown_pair_count() == 0` asserted green). No new exclusive was added, and none was promoted or demoted: the 9 / 115 / 48 counts and the name set are identical to the baseline. `mine_system` remains an undeclared `add_exclusive` (`quest_scene/story_manager.rs:52`), which the spec allows.

## 4. Findings

0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW (both pin-coverage / documentation gaps with no observed defect).

### CONC-2026-11-08-4.1: `VatsState` is written by an Early exclusive that runs after its Early parallel reader, with no pin test or note
- **Severity**: LOW (test-coverage gap; behaviour is benign)
- **Dimension**: Scheduler Proof Soundness (cross-stage sequencing)
- **Location**: `byroredux/src/boot/schedule/early.rs:121-125` (writer `vats_mode_system`, exclusive); `byroredux/src/boot/schedule/early.rs:29` + `byroredux/src/systems/character.rs:99,148` (reader `player_controller_system`, parallel); `byroredux/src/app_events.rs:1566-1570` (reader, dt scale)
- **Status**: NEW
- **Description**: The VATS mode machine is the single writer of `VatsState`, and it is deliberately an Early exclusive so it runs after this frame's `ActionState` edges (comment at `early.rs:107-120`). The exclusive phase follows the parallel batch, so `player_controller_system`'s movement gate (`player_accepts_movement_input`) sees the previous frame's mode. The app loop reads `VatsState` before the scheduler runs to scale `dt`, so the world freeze also trails the mode edge by one frame: the enter frame runs at full `dt`, and the frame after exit runs at `dt=0`. This is the phase-ordered version of the staleness the spec describes (reader earlier than the single writer). All other readers (`player_can_act` in `combat_input`, interaction, etc.) run in Update or later and are fresh. Unlike `WindField` (`player_wind_read_is_declared_and_weather_writer_is_exclusive`), nothing pins or documents this lag.
- **Evidence**: `vats_mode_system` is `add_exclusive_with_access` at `early.rs:120`; `player_controller_system` is `add_to_with_access` at `early.rs:21` and declares `.reads_resource::<VatsState>()`; `app_events.rs:1566` reads `VatsState` before the tick. The `early.rs` comment records only the ActionState-edge rationale, not the lag.
- **Trigger Conditions**: pressing or leaving the VATS key. The effect is at most one frame of movement or world time (the first-tick `dt` clamp is 1/30 s) at the enter / exit edge. No lock or data hazard.
- **Suggested Fix**: add a one-line note beside the `vats_mode_system` registration, and a small `scheduler_access_tests` pin asserting the writer is exclusive Early and `player_can_act` consumers (`combat_input_system`, `interaction_system`) are in Update or later. Do NOT move the writer. This is intended behaviour; the fix is to make it deliberate.
- **Related**: #6350 (VATS), #6625.

### CONC-2026-11-08-4.2: `vats_mode_system` is a `HitEvent` producer outside the `hit_event_consumers_run_after_every_producer` pin
- **Severity**: LOW (latent; today's order is correct)
- **Dimension**: Scheduler Proof Soundness (registration-order pin completeness)
- **Location**: `byroredux/src/systems/vats.rs:851`, `byroredux/src/boot/schedule/mod.rs:239-271`
- **Status**: NEW (same class as baseline CONC-2026-10-31-02, now fixed for `mine_system`)
- **Description**: `vats_confirm` (reached from `vats_mode_system`) inserts `HitEvent` for each landed VATS target (#6422 folds shots per target). The producer list in the pin does not name it. It is currently safe because the producer is in `Stage::Early` and all four consumers (`combat_damage_system`, `hit_event_system`, `reference_script_hit_system`, `obscript_hit_system`) are in Update. The only way it breaks is a stage move of `vats_mode_system`, which the pin would not notice. A same-frame overwrite is also not possible today: `combat_input_system` is gated off during VATS by `player_can_act`, and the other producers hold on an existing event.
- **Evidence**: producer list at `mod.rs:247-261` (seven names, no `vats_mode_system`); `vats.rs:848-861` uses `query_mut::<HitEvent>().insert`.
- **Trigger Conditions**: someone moves `vats_mode_system` to Update, or later than a consumer, to fix the finding-4.1 lag.
- **Suggested Fix**: add `"vats_mode_system"` to the producer array (one line, the same shape as #6580). The test reads the built schedule and already tolerates exclusive and stage positions.
- **Related**: #6580, #6540, #4324.

## Verdict

The scheduler proof is live and sound at HEAD. All nine parallel systems still declare what they acquire (including the new `WaterBedField` reads). The system set is unchanged from the baseline, and the new `run_before` / `run_after` machinery is correct and fail-fast. Baseline CONC-2026-10-31-02 is fixed. Two LOW pin-coverage notes remain, both around VATS (4.1, 4.2). No tests were failing or ignored.


## Appendix Dim 5

# Concurrency audit 2026-11-08 — Dimension 5: RwLock Patterns (Resource<->Storage & Physics Step)

HEAD 0419437ab. Baseline: AUDIT_CONCURRENCY_2026-10-31 (670f81917). Read-only audit; nothing run on GPU.

## 1. Delta scope

- `git log --since=2026-10-31` on the Dim-5 Paths (+ `crates/audio`, `byroredux/src/systems`): 43 commits; strictly on the 7 Dim-5 path files: 10 commits.
- `git diff 670f81917..HEAD --stat` on the Dim-5 Paths: 8 files, +443/-82. Lock-relevant content:
  - `crates/physics/src/water.rs` (+163): #6488 `local_current` (pure fn, no World), #6489 contact-rescan gate (`waves_require_contact_rescan`, one `WaterContact` read guard, nothing else held), plus tests.
  - `crates/physics/src/world/mod.rs` (+165): #6487 `pending_link_impulses` / `deliver_link_impulses` / `release_link_impulse_forces`. Pure Rapier-side state; `PhysicsWorld` still imports only `byroredux_core::math` (no `&World`, no ECS guard) so the sink property holds.
  - `byroredux/src/systems/character.rs` (+113): `player_water_state` now calls `local_current` with `bed_q` (read guard already held, same acquisition order as `collect_water_surfaces`: WaterPlane, WaterVolume, WaterFlow, WaterSurfaceMesh, WaterBedField). f131c5275 fixed a *test-only* reversed order (WaterBedField held while taking WaterPlane) that the detector reported; production was already consistent.
  - `crates/physics/src/sync.rs` (+58/-?): only `env_flag_once` for BYRO_PROFILE flags (#6689). No lock changes. `unload.rs`, `eat_sleep.rs`: type-path renames only (`systems::PlayerEntity` -> `components::PlayerEntity`, `LoadedCellIndex`, `ctx.skin.*`). `forcegreet.rs`, `ragdoll.rs`: untouched.
- New callers of `set_linear_velocity` / `set_kinematic_translation` / `cast_ray` / `PhysicsWorld` / `RapierHandles` / `AudioWorld` / `ContactConfig` in added lines since 2026-10-31 (excluding tests and `reads/writes_resource` declaration moves from the schedule split): exactly one — `systems/vats.rs` `target_bodies_by_owner` (#6484, `world.get::<RapierHandles>` per owned collider, each guard dropped before the next; the `ActorColliderOwner` read is collected to a Vec first). `visible_fraction` holds a `PhysicsWorld` read guard only across pure `cast_ray` calls, after the bodies map is built. Clean. No new `resource_mut::<PhysicsWorld>` sites. Schedule access declaration for `physics_sync_system` (`boot/schedule/physics.rs`) covers WaterBedField / WaterFlow / WaterCurrentVolume / WindField / Ragdoll / WaterContact write — complete for the new reads.

## 2. Guard runs

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 CARGO_INCREMENTAL=0 cargo test -p byroredux-physics` | 275 passed, 0 failed (lib); doctests 0 |
| `cargo test -p byroredux-physics sync` | 34 passed, 0 failed (241 filtered) |
| `sync::tick_documentation_tests::register_newcomers_parallel_section_holds_no_world_guard` | ok (inside the 34) |
| `sync::phase_sync_tests::pull_dynamic_does_not_close_transform_global_transform_lock_cycle` | ok (inside the 34) |
| bin (`BYRO_LOCK_ORDER_CHECK=1`) `forcegreet_walk_with_real_physics_world` | 1 passed |
| bin `eat_walk_with_real_physics_world` | 1 passed |
| bin `unload_captures_do_not_invert_production_lock_orders` | 1 passed |
| bin `picked_up_restore_walks_meshes_before_taking_the_marker` | 1 passed |
| bin `view_restamp_does_not_close_the_render_skip_lock_cycle` | 1 passed |
| bin extra, detector on: `rapier_release_tests` 9, `systems::character` 52, `systems::forcegreet` 6, `systems::eat_sleep` 18, `ragdoll::` 21 (9 ignored), `systems::knockdown` 3, `systems::vats` 21, `systems::audio` 25 | all passed, 0 failed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-audio` | 58 passed, 9 ignored, 0 failed |

## 3. Checklist walk

- **Resource<->Storage unordered pair / no PhysicsWorld guard spans a storage iteration**: CLEAN. I scripted a scan of every non-test `let ... = world.[try_]resource[_mut]::<PhysicsWorld>()` binding (~70 sites repo-wide) for `world.*` calls / `(world, ...)` helper calls inside the guard's scope, then hand-read every hit. `physics_sync_system` phases: Phase 1 (`collect_newcomers` read guards -> Vec, dropped; `register_newcomers` takes `ContactConfig` snapshot by copy, then `pw` held 1064..1219 with no `world.*` calls); Phase 2 `push_kinematic` (pw 1284 after storage guards); 2.5 `apply_buoyancy` (pw write scope 1081..1572 has no `world.*`; `WaterContact` write applied after it drops; `WaterContactScratch` is `mem::take`n, guard not held across the scan); 3 step (scoped); 4 `pull_dynamic` (RapierHandles/RigidBodyData dropped -> pw read scoped -> Parent+GlobalTransform scoped -> Transform read -> Transform write). `ragdoll.rs:739` takes `LocalBound`/`WorldBound` *before* `pw` (comment #3655), nothing under it (the `world.next_entity_id()` calls at 836/892 are a plain field read, not a lock). `knockdown.rs:108`, `audio.rs:287/384/413`, `cinematic.rs:448/502`, `loot_appearance.rs:258`, `trap_hits.rs:237`, the AI `physics = try_resource` readers (combat_ai, escort, flee, flee_package, follow, guard, navmesh_path, projectiles, travel, wander, script_queries): nothing acquired while held. One exception: `scene/character_spawn.rs:279` — see finding 5.1.
- **Fix shape = snapshot then acquire; guards live**: `pull_dynamic_does_not_close_transform_global_transform_lock_cycle`, `unload_captures_do_not_invert_production_lock_orders`, `picked_up_restore_walks_meshes_before_taking_the_marker`, `view_restamp_does_not_close_the_render_skip_lock_cycle` all green under the detector. `get_actor_value_does_not_hold_actor_values_across_ruleset` lives in `crates/scripting` (Dim 3's run; not re-run here).
- **Helper order (`set_linear_velocity` / `set_kinematic_translation`)**: CLEAN. Both read `RapierHandles` through `query().and_then(|q| q.get(e).copied())` — the guard is moved into the closure and dropped on return, so nothing lives into the `resource_mut::<PhysicsWorld>()`. All callers (character.rs:682/980, cinematic.rs:299/678/1317/1469, graph_locomotion.rs:299, camera.rs:121) hold no `PhysicsWorld` guard at the call (character.rs:418 explicit `drop(pw)` at 558; the 583 diagnostic `pw` is scoped to its `if`). #5371: `forcegreet.rs` / `eat_sleep.rs` hold no `PhysicsWorld` guard at all now; both walk tests green. #5556: `footstep_system` clones `FootstepConfig` (audio.rs:~308) before `AudioWorld`; `select_footstep_sound` is pure — confirmed. Other `AudioWorld` write holders (`asset_provider/audio.rs`, `combat_anim.rs:434`, `dialogue_voice.rs:472`, `script_requests.rs`, water audio, `impact_audio_system`) take nothing under it. `audio_system` (`crates/audio/src/system.rs:43`) deliberately holds `AudioWorld` across `GlobalTransform`/emitter reads; no site in the repo takes those storages and then `AudioWorld` (all callers above acquire `AudioWorld` with no storage guard live), so the edge is one-directional; audio tests (58) green under the detector.
- **ContactConfig snapshot once per batch**: CLEAN at `sync.rs:1022` (`.map(|r| *r)` copy; not re-locked per newcomer; pinned by `tick_documentation_tests` string at sync.rs:2698). `ragdoll.rs:523` and `character.rs:415` also copy before the `PhysicsWorld` acquire. `ContactConfig` has no writer guard anywhere (insert only), so it cannot be one side of a cycle today. Exception on the *ordering* rule only: finding 5.1.
- **Cell-unload teardown (#1520)**: CLEAN. `release_victim_rapier_bodies` (unload.rs:965): `RapierHandles` and `Ragdoll` reads collected into Vecs inside a scoped block (each guard dropped at its `if let` end), early return, then `try_resource_mut::<PhysicsWorld>` -> `remove_body` / `remove_ragdoll` with no storage touched under it; despawn happens after in the caller. `rapier_release_tests` 9/9.
- **Placement**: `physics_sync_system` is registered once in `Stage::Physics` (`boot/schedule/physics.rs`) with a full access declaration (PhysicsWorld r/w, WaterContactScratch, ContactConfig, RapierHandles, Transform, Water* components incl. the new `WaterBedField` / `WaterFlow` read, `WindField`, `TotalTime`). Declaration completeness for the #6488 water reads confirmed; conflict proof is Dim 4's.

## 4. Findings

### CONC-2026-11-08-5.1: `probe_spawn_ground` takes `ContactConfig` while holding a `PhysicsWorld` read guard
- **Severity**: LOW (lock-hygiene / sink-rule violation; no cycle possible today)
- **Dimension**: RwLock Patterns (Resource<->Storage, Physics)
- **Location**: `byroredux/src/scene/character_spawn.rs:279` (`let pw = world.resource::<PhysicsWorld>()`) held through the `match pw.cast_ray_down(..)` whose `Grounded` arm calls `character_spawn_center_y(world, ..)` (line 293 -> 153-163), which does `world.try_resource::<ContactConfig>()`.
- **Status**: NEW (no matching open/closed issue found; code dates from e9aeb2706, 2026-09-21 file move, not covered by the 2026-10-31 report).
- **Description**: This creates a `PhysicsWorld -> ContactConfig` edge. `PhysicsWorld` is documented as a sink (nothing acquired beneath it) and every other consumer (`register_newcomers` sync.rs:1022, `ragdoll.rs:523`, `character.rs:415`) snapshots `ContactConfig` *before* taking `PhysicsWorld`, i.e. the edge elsewhere, if any, is `ContactConfig -> PhysicsWorld` in spirit. It is not a cycle today: `ContactConfig` is only ever read (`try_resource`) and no site holds a `ContactConfig` guard across a `PhysicsWorld` acquire, and both guards are read guards on a main-thread spawn path (`probe_spawn_ground` is only called from the boot-time spawn planner, scene/character_spawn.rs:608).
- **Evidence**: character_spawn.rs:279-293 and 153-163; `grep ContactConfig>` shows no write/`resource_mut` use anywhere.
- **Trigger Conditions**: Would become a real ABBA only if a future writer of `ContactConfig` (e.g., a console tuning command using `resource_mut::<ContactConfig>`) ran concurrently with this path on a different thread, or if some site later held `ContactConfig` across `PhysicsWorld`. Neither exists now; a detector run of the spawn path would record the edge but not a cycle.
- **Impact**: None at present; erodes the sink invariant and adds detector-graph surface.
- **Suggested Fix**: Snapshot-then-acquire: read the kcc offset (or call `character_spawn_center_y`) before `let pw`, or copy the `cast_ray_down` / `static_colliders_aabb` results into locals, `drop(pw)`, then build `GroundProbe::Grounded`.
- **Related**: #5371, #5556 (same class, already fixed elsewhere).

### Candidates examined and dropped
- `set_kinematic_translation` / `set_linear_velocity` appearing to keep the `RapierHandles` guard across `resource_mut` (match-scrutinee temporary): disproved — the guard is moved into the `and_then` closure and dropped there.
- `std::env::set_var` in the new `an_env_flag_is_read_once_and_then_cached` test (sync.rs:2658) racing other test threads' `getenv`: pre-existing repo pattern (13 uses), unique variable name, edition 2021; not reported.
- `audio_system` holding `AudioWorld` across storage reads: by design, one-directional (see checklist); not a finding.
- Dim 3's two `world_tests.rs` fixture cycles: not re-reported.

Counts: CRITICAL 0, HIGH 0, MEDIUM 0, LOW 1.


## Appendix Dim 6

# Concurrency audit 2026-11-08 — Dimension 6: Resource Lifecycle (GPU teardown ordering)

HEAD 0419437ab. Baseline AUDIT_CONCURRENCY_2026-10-31 (670f81917) skimmed Dim 6 as "untouched"; that no longer holds.
Read-only; no engine/GPU run. Vulkan-sync suspicions would be HYPOTHESIS rows (none raised).

## 1. Delta scope

- `git log --since=2026-10-31` on the Dim 6 Paths (context/, buffer, image, egui_pass, material, acceleration/, scene_buffer/): 51 commits. Whole `crates/renderer/src`: 105.
- Resource-bearing commits read in full:
  - World-space combustion local grid (1adf9bdc1 / 626197ae5 / d3e3acc7b, then #6533 / #6728 / #6751): `volumetrics/local_grid.rs` (+1726 lines), `volumetrics/init.rs`, `volumetrics.rs`.
  - `context/linear_capture.rs` (75b4024df, 76ec72dcb): new EXR capture staging buffer.
  - Shadow filter: a2180dd18 (RLT-SHD-003) and 270b56cd7 (#6747) change push-constant and comment text only; no lifecycle change.
  - M39 texture residency (494d077b2, 073e3af53, 9179059a4, 6a4c661a8 #6738, b8d89bdcb #6737, 23f4aeee3, b34b897df #6624): `vulkan/texture.rs`, `texture_registry/{upload,residency,release,mod}.rs`.
  - dd4c50340 (#6521): `SceneBuffers::destroy` now tears down `moved_surface_buffers` and adds `destroy_coverage_tests.rs`.
  - b7fe116f1 (#6371): `descriptors::destroy_pass_objects` is now used by SVGF, TAA, SSAO and caustic.
  - TAA 6926fddab, 695417b9a: shader and parameter changes only, no new GPU objects.
  - Refactors: #6709 (`build_blas_batched` phases), #6708 and #6710 (skinned refit and geometry pass), #6697 to #6700 (`VulkanContext` field grouping).
  - gpu_timers: `QUERIES_PER_FRAME` 60 to 62 for the local-sim bracket.
- Grep of the whole renderer diff since 670f81917 for `create_buffer / create_image / create_sampler / create_descriptor_pool / allocate_descriptor_sets / allocate_command_buffers / create_query_pool / GpuBuffer::create / GpuImage::create / StagingGuard::create / .build(` found no GPU object created outside the sites reviewed above. In the `build_blas_batched` hunk, the query pool and `compact_buffer` creation moved verbatim into methods.

## 2. Guard runs

| Command | Result |
|---|---|
| `CARGO_INCREMENTAL=0 cargo test -p byroredux-renderer --lib` (TMPDIR=/mnt/data/tmp) | **1626 passed, 0 failed, 1 ignored**, exit 0 (log `/tmp/audit/concurrency/dim6_test.log`) |
| `texture::initial_layout_submit_tests::every_initialize_layouts_body_uses_the_shared_submit` and `…no_initialize_layouts_caller_destroys_without_the_in_flight_check` | ok |
| `context::resize::tests::a_failed_layout_walk_fails_closed_instead_of_warning_and_continuing` | ok |
| `context::teardown::ordering_count_doc_sites_agree_tests::both_ordering_count_sites_name_the_same_count` | ok |
| `context::teardown::skin_slot_drain_is_not_nested_under_skin_compute_tests::skin_slot_drain_sits_outside_the_skin_compute_guard` | ok |
| `scene_buffer::destroy_coverage_tests::destroy_walks_every_per_frame_buffer_family` (new, #6521) | ok |
| `descriptors::pass_teardown_tests::migrated_passes_destroy_through_the_shared_helper` | ok |
| `sync::tests::the_all_slots_wait_argument_is_pinned` | ok |
| `volumetrics::unit_tests::the_local_grid_follows_the_skip_and_completion_wiring` | ok |

The tests are source-shape only. No fault-injection hook exists, so the VUID before/after for failed layout walks stays unproven (as before).

## 3. Checklist walk

**Destruction order (three load-bearing orders)** — all hold, read at `context/teardown.rs`:
- `skin_slots` drain at L77-83 is unconditional, then `skin.compute.destroy` at L148-150, so slots come before the pipeline pool. Pinned by `skin_slot_drain_sits_outside_the_skin_compute_guard`.
- `FrameUpscaler::destroy_device_objects` runs at L338-340 in the allocator-independent block, after `presentation.destroy`. `destroy_allocations` runs at L203-205 inside the allocator guard. A guard test in `frame_upscaler.rs` pins that `destroy_allocations` is absent from `Drop` (L1240-1290).
- `post.exposure_meter.destroy` and `post.exposure.destroy()` (L184-196) both precede `allocator.take()` / `Arc::try_unwrap`. Everything allocator-backed is destroyed inside `destroy_allocator_owned_resources`, which runs before the `take()`. Resources not covered by that function also precede it: depth images, `mesh_registry`, and the three capture staging buffers. The doc header still says "three" and the count-agreement test passes.
- New allocator-backed resources do not break the order. The local grid is inside `volumetrics.destroy` (L229-232). Its buffers and images are drained and `.clear()`ed, so no `Arc<Allocator>` clone survives to the `try_unwrap`. `destroy_linear_capture_staging` (L344) runs while `self.allocator` is still `Some`.

**Combustion local grid (`LocalGrid`, per-FIF)** — clean:
- `new()` is one-shot and builds into a partial struct. The error arm calls `destroy`, which null-guards every handle, with a SAFETY note that nothing was submitted.
- Stats and params buffers are pushed before their zero-fill, so a failed write still gets cleaned up.
- `destroy` covers `state / dynamics / optical / irradiance / phase_table` (`GpuImage`), `stats / params / brick` (`GpuBuffer`), pool, sets via pool, 2 pipelines, pipeline layout, set layout and sampler.
- `VolumetricsPipeline::destroy` takes and destroys `local_grid` first. It also covers `combustion_light_moment_buffers` and `combustion_occupancy_buffers` (cleared) and the 3 samplers.
- Resize: volumetrics is rebuilt whole (`recreate_bloom_and_volumetrics`), so the grid is rebuilt for every in-flight slot. Composite's bindings 12..=17 are rewritten from the new `local_fire_views()` (`resize.rs` L1116-1130, init.rs L778). A missing grid is a hard error on both the init and resize paths, the same policy as binding 6.
- The layout walk and clear for the grid images ride the existing `VolumetricsPipeline::initialize_layouts`, which uses `with_one_time_commands` plus `release_density_noise_staging(&upload_result)`. The resize arm (`may_be_in_flight` then `mem::forget`, else `destroy`) and the init arm (`init_stages.rs` L628-655) both branch on the failure class. No new `initialize_layouts` caller bypasses the shared pattern.

**Penumbra shadow filter** — no lifecycle delta. `destroy` is idempotent and null-guarded (frames, pool, pipeline, layouts, point sampler). The `new` error arms split NotSubmitted (destroy) from `may_be_in_flight` (`forget`) (#6063). The resize arm (`resize.rs` L717-761) keeps the two-stage recreate then `initialize_layouts`, and `forget`s or destroys by class. It destroys before `reservoir_buffers` (teardown L242-247).

**Linear capture (`linear_capture.rs`)** — the staging buffer goes through the shared `create_staging_buffer` / `destroy_staging_buffer`. It grows only (free and re-create during recording, relying on the all-slots `wait_for_fences`), and is freed at teardown while the allocator is live. It is already listed in the `sync.rs` #870 rider list and pinned by `the_all_slots_wait_argument_is_pinned`. The readback runs after the all-slots wait (`sync_and_acquire_frame.rs` L161). The copy restores `SHADER_READ_ONLY_OPTIMAL`. A job survives a resize but fails on the extent change. The buffer is not freed there, but the next copy reuses or regrows it, so there is no leak. Only a minor guard-coverage gap, see 6.1.

**M39 texture residency / retired images** — clean:
- Retired live images push to `entry.pending_destroy` aged by `current_frame_id`. `tick_deferred_destroy` runs after the all-slots wait; `destroy()` calls `drain_pending_destroys` first.
- A change refused at install (`land_change`, #6738) retires the new image through `pending_destroy`, not by drop.
- A failed `record_dds_residency_change` returns before or without owning an image. The `StagingGuard` has a `Drop` safety net, and the `Texture` drop is a logged safety net.
- A failed batch submit uses the #4891 rule: `forget(staged)` when `may_be_in_flight`, else destroy texture and staging.
- 23f4aeee3 replays deferred descriptor writes against the live view, so a queued write cannot name a destroyed retired view.
- `TextureRegistry::destroy` drains `dynamic_rgba`, `pending_destroy` and textures, then takes and destroys `staging_pool` before the samplers, pool and layout. The #6624 cube fallbacks are ordinary registry textures.
- No release-between-record-and-install race: the flush is synchronous and skips `ref_count == 0` handles up front.

**Swapchain recreate (`resize.rs`)** — no new per-extent object has been added outside volumetrics (rebuilt wholesale). G-buffer, SVGF, ReSTIR reservoirs, shadow filter, caustic, water-caustic, bloom, composite, TAA and presentation arms are unchanged in shape. The fail-closed arms (#6343) are intact and the source-shape tests pass. TAA 6926fddab and 695417b9a add no GPU objects.

**`destroy_pass_objects` refactor (#6371)** — a pure helper (pipeline, layout, pool, set layout, null-guarded). SVGF still destroys its separate à-trous pipeline, layout, pool and set layout after it. TAA, SSAO and caustic samplers and slots are still destroyed. SSAO's unguarded `destroy_sampler` is legal on null handles. Nothing was lost versus the hand-written bodies.

**AS cleanup** — the `blas_static.rs` refactor (#6709) keeps the full rollback (`unwind_prepared`, `compact_accels` owned by the caller, query pool destroyed, and the #5201 `forget` on `may_be_in_flight`). `AccelerationManager::destroy` is unchanged.

**scene_buffer / MaterialBuffer / EguiPass / GpuImage** — **#6506 / #6521 (`moved_surface_buffers`) is FIXED and CLOSED**: `descriptors.rs` L425-428 destroys and clears it, and `destroy_walks_every_per_frame_buffer_family` plus `the_bundle_struct_carries_every_family_too` pin every `Vec<GpuBuffer>` family. The non-Vec fields (`bind_inverses_persistent`, `bind_inverse_upload_staging`, `terrain_tile_buffer`, `ray_budget_buffer`, staging pool, retired-instance queue) are all destroyed. The scan does not cover non-Vec buffer fields (they are currently complete). `egui_pass.rs`, `image.rs`, `buffer.rs` and the `material.rs` lifecycle are unchanged.

**Per-frame leaks** — no new per-frame descriptor, command-buffer, staging or query allocation. The only per-frame-adjacent creator is the linear-capture grow-only staging buffer. `QUERIES_PER_FRAME` grew to 62 as a constant, and the pool is sized from it.

## 4. Findings

No CRITICAL / HIGH / MEDIUM. One LOW.

### CONC-2026-11-08-6.1: shared staging-free guard and doc still describe only two capture twins; the third (linear capture) is unpinned
- **Severity**: LOW (guard/doc coverage gap, no runtime defect)
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`both_staging_frees_delegate_to_the_shared_helper`, ~L1385-1432; `destroy_staging_buffer` `# Safety` doc, ~L80-104); `crates/renderer/src/vulkan/context/linear_capture.rs:395-418`
- **Status**: NEW (no open or closed issue found: searched "linear_capture staging", "destroy_staging_buffer")
- **Description**: #4039 pinned that the screenshot and depth-capture staging frees delegate to `helpers::destroy_staging_buffer`, so the "both-slots fence wait" contract lives in one place. `linear_capture.rs` (75b4024df) added a third twin and does delegate (L416-418). The test iterates only the two original owners, and the helper doc names only `screenshot_record_copy` / `depth_capture_record_copy` as callers. `sync.rs` already lists `linear_capture_staging` (#870 rider 4 plus a source-scan entry), so the free is sound today.
- **Evidence**: `linear_capture.rs:416` calls `helpers::destroy_staging_buffer`. The `[(label, src, needle)]` array in `helpers.rs` has two entries (screenshot, depth_capture).
- **Impact**: Re-inlining a raw `destroy_buffer` and `free_allocation` in `destroy_linear_capture_staging` would not fail any guard. The #4039 divergence failure mode could repeat on the newest twin.
- **Trigger Conditions**: A refactor of `linear_capture.rs` that inlines the free, plus a future change that narrows the all-slots wait (#5365).
- **Suggested Fix**: Add the `linear_capture` entry (`production_text` of `linear_capture.rs`, needle `pub(super) fn destroy_linear_capture_staging(&mut self) {`) to the test array, and add `linear_capture_record_copy` and `linear_capture_record_copy_post_taa` to the helper doc's caller list.

## 5. Summary

- Counts: CRITICAL 0, HIGH 0, MEDIUM 0, LOW 1, INFO 0.
- #6506 is closed and fixed (via #6521, dd4c50340). Do not re-file.
- Baseline's "Dim 6 untouched" no longer holds. The delta added about 1.7k lines of new GPU resource code (combustion local grid) and the M39 residency image-replace machinery. Both follow the project's create, partial-destroy, per-FIF rebuild and shared-submit patterns without a break.
- No `BYRO_VALIDATION` run (not permitted). The failed-layout-walk VUID before/after remains source-shape proven only.


## Appendix Dim 7

# Concurrency audit 2026-11-08, Dimension 7: Worker Threads & Thread-Safety Bounds

HEAD 0419437ab, baseline 670f81917 (AUDIT_CONCURRENCY_2026-10-31, which found Dim 7 unchanged). No source edits, no engine/GPU process.

Counts: 0 CRITICAL / 0 HIGH / 1 MEDIUM / 0 LOW new. One incidental non-concurrency red test (see Incidental).

## 1. Delta scope

`git log --since=2026-10-31` on the dimension's Paths: 22 commits (479 commits repo-wide since the baseline). Diff stat on the Paths: 24 files, +1610/-946, of which `byroredux/src/texture_streamer.rs` is +457 (new file).

Commits that matter for threads/locks:
- 2e30cdf10 / 3d67d963e / 15ee603de / 9179059a4 / dd2cd84e9 (M39 texture streaming): **new worker**, `TextureStreamer` with its own 2-thread rayon pool (`texture-stream-N`) and an `mpsc` return channel. Baseline report said "no new thread": this one postdates it.
- 494d077b2 / 073e3af53 (renderer M39 residency): `extract_for_registry` / floor reads (`floors` flag threaded through `prefetch_textures`), `queue_promotion_bytes`. No new lock.
- f9f3eb831 (#6591, `SharedParseCache`): `exterior` arg removed; `apply_exterior_processing` now runs on every fresh parse.
- 0419437ab / 510e7dc4d / cc39d7792 / d548def7d (#6638/#6640/#6594, LOD descriptor and legacy tree ring): new `WorldStreamingState` fields, all main-thread; worker only gained `replace_model_root: bool` (pure) and `prepare_placed_model_scene` (pure `&mut NifScene`).
- 7cff6e979 (#6480, `.fuz`): `run_bounded` reader threads were already present (#6307, 78be43036, 10-25); only header arithmetic changed.
- 75b4024df / 76ec72dcb (linear capture, EXR): single-threaded, see checklist.
- Debug server (`listener.rs`, `system.rs`), `read_at.rs`, `crates/ui/src/player.rs`, `render/mod.rs` joins: no commit touches the concurrency shape. `render/mod.rs` only gained `exterior_haze_floor_now` (read-only).

First-step grep (`thread::spawn|Builder|scope`, `rayon::`, `mpsc::`, `ThreadPoolBuilder`, `spawn_fifo`, `par_iter`, non-test, crates + byroredux + tools), diffed against the Paths list:
- Inside Paths: streaming worker (`streaming/mod.rs:571`) and its stream pool (`pre_parse.rs:97`, `spawn_fifo` `:557`); `texture_streamer.rs:133`; `terrain_height.rs:56`; `texture_prefetch.rs` (`pool.spawn`); `load_order.rs:815` `in_place_scope`; `render/mod.rs:1147/1185/1201/1208` joins; `debug-server/listener.rs:184,267`; `audio/fuz.rs:365,370`.
- **Outside Paths (coverage gaps, all reviewed, none a finding):**
  - `asset_provider/material/cdb.rs:272` `sf-cdb-index` std thread (#5291; panic memoised under `catch_unwind` at `:189`, #5665 closed). Pre-dates the window. Single-flight via `OnceLock` slot plus `CDB_BUILD_GATE`.
  - `crates/physics/src/sync.rs:1047` `into_par_iter` collision-shape conversion on the global pool. Pure per-newcomer function, insertion serial and in newcomer order; Dim 5 territory. Unchanged since #6689 scan classification. A panic there would propagate to the caller (par_iter, not `spawn`), not abort.
  - `asset_provider/archive.rs:371` `open_loose` process-lifetime registry (#6200, 10-20): lock released between check and insert, so two racers both index and the last write wins (benign); never re-stamped, same documented behaviour as the CDB caches. Not in the spec text.
  - `core/scheduler.rs:787` (`par_iter_mut`, Dim 4), `bsa/ba2.rs:1432` and `csg.rs:637` (`thread::scope`, inside `#[cfg(test)]`), `ui/player.rs:896`, `plugin/.../weather_settings.rs:512`, `papyrus/parser/script.rs:1446`, `scripting/animation_graph.rs:2311`, `byroredux/.../terrain_height.rs:222`, `texture_prefetch.rs:261`, `launcher/engine.rs:162` (test) are all test-only. `tools/byro-dbg/src/tui.rs:350` (client I/O thread) and `tools/byro-launcher/src/engine.rs` (stderr reader, #6279) are tooling and unchanged in the window.
- New `static Mutex/OnceLock/LazyLock` in the window: only `XWMA_ONCE: Once` (fuz) and two `OnceLock<HashMap>` empty-defaults in `asset_provider/audio.rs`. No new `static Mutex`. No thread, pool or channel for the combustion local grid (`local_grid.rs:448` is an env-cache `OnceLock<u32>`), linear capture, `--bench-capture-every`, or screenshot readback.

## 2. Guard runs (`CARGO_INCREMENTAL=0`, `TMPDIR=/mnt/data/tmp`, rustc 1.96.0)

| Command | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- streaming:: texture_streamer terrain_height texture_prefetch shared_parse_tests prefetch pre_parse` | **78 passed, 0 failed, 1 ignored**. Includes `cancel_flag_stops_a_worker_with_a_queued_backlog`, `join_with_timeout_*` (3), `worker_panic_safe_recovers_panic_with_empty_payload`, `stream_parse_pool_runs_tasks_on_its_own_dedicated_threads`, `stream_parse_pool_leaves_headroom_for_the_frame_pool`, `a_panicking_extraction_reads_as_missing`, `a_permit_moved_into_a_panicking_task_is_released`, `a_panicking_height_job_is_reported_failed`, `concurrent_writers_of_one_cache_key_all_succeed_and_readers_see_whole_files`, `a_running_read_is_waited_for_and_its_bytes_taken_once`, `shared_parse_tests::*` (5), cdb prefetch slot tests (3). |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- render::` | **241 passed, 0 failed, 2 ignored** (lock-order detector on). |
| `cargo test -p byroredux-renderer --lib -- allocator capture_ordering egui` | **27 passed, 0 failed** (allocator lock-site allowlist test, `finish_readback_runs_after_the_in_flight_fence_wait`, egui dependency chain). |
| `cargo test -p byroredux-bsa` | lib **120 passed, 0 failed, 11 ignored** (needs-game-data); other targets 1+1+2 passed, rest ignored. Threaded per-archive `read_at` tests are in this run. |
| `cargo test -p byroredux-debug-server` | **25 passed, 1 FAILED**: `registration::roster_tests::every_inspectable_component_is_registered` (`FactionReputation` carries the inspect derive but has no registration). Not a concurrency defect, see Incidental. |
| `cargo test -p byroredux-audio --lib` | **58 passed, 0 failed, 9 ignored** (includes the `run_bounded` timeout/overflow/stderr-flood tests). |
| `cargo test -p byroredux-ui --lib` | **118 passed, 0 failed, 3 ignored**. |

No test covers `TextureStreamer`'s pool job panicking (the module has two tests: settle clock, bench line).

## 3. Checklist walk

- **Streaming worker shutdown**: clean. `streaming/mod.rs:840-868` takes `worker`, stores `cancel` (Release), drops `request_tx`, then `join_with_timeout`; `Drop` calls `shutdown(1 s)` and short-circuits on `worker == None`. `cancel_flag_stops_a_worker_with_a_queued_backlog` and the three `join_with_timeout` tests pass. Per-cell `catch_unwind` (`pre_parse.rs:242`), per-NIF (`:276`) and `extract_guarded` (`:391`) present and tested. Delta (`replace_model_root`, `prepare_placed_model_scene`) is pure.
- **Worker <-> main flow**: clean. Payloads cross by `mpsc`; no `&mut World` on the worker; stream pool is a dedicated pool with headroom (test passes). `ReadAt` is `pread` on Unix (`read_at.rs`), no shared cursor; bsa threaded tests green. `open_shared` still stamps `(len, mtime, dev, ino, ctime)`. `open_loose` and the CDB caches are documented process-lifetime (see coverage gaps).
- **M39 `TextureStreamer`**: pool/channel shape sound. `in_flight` and `done_rx` are main-thread-only; `MAX_READS_IN_FLIGHT = 4` bounds staged bytes; `Arc<TextureProvider>` is `Send + Sync` by compile; texture slots are never recycled (`release.rs:7`), so a late `FinishedRead` cannot land on a re-used handle, and `queue_residency_change` re-plans against live residency and validates the payload's first level. **The pool job has no `catch_unwind` and the pool no `panic_handler`: CONC-2026-11-08-7.1.**
- **Texture prefetch** (`texture_prefetch.rs`): clean. Panicking read still finishes its slot (`catch_unwind` at `:230`, now around `extract_for_registry`); epoch guard drops stale results; withdrawn (`Queued`) keys are not waited on; `take` only waits on `Running`; `clear()` bumps the epoch and wakes waiters; `ready_bytes` accounting cannot underflow (`Ready` slots only exist under the current epoch). `floors` is decided on the main thread at queue time and `extract_texture_floor_or_warn` reads cubemaps whole, so a staged floor cannot reach a cubemap resolve.
- **Terrain height** (`terrain_height.rs`): clean. Every job under `spawn_guarded` (`:109`), `a_panicking_height_job_is_reported_failed` green, results returned through `Arc<Mutex<Vec>>` drained on main; cache writes go through unique-temp `write_atomic`, and the concurrent-writers test passes. Only change is the `LayerHeightSources` move to `components` (#6711).
- **Debug server**: clean, unchanged. Per-client threads only touch the bounded `CommandQueue` (`MAX_QUEUED_COMMANDS` 64, check+push under one lock) and `active_streams` (cap 8, check+push+spawn under one lock); drain system is Late exclusive. `--bench-capture-every` (`app_events.rs:650`) is all main-thread, reuses `ScreenshotBridge` `try_claim`/`take_result_for`/`cancel`, no new thread. Linear capture: `linear_capture_finish_readback` runs at `sync_and_acquire_frame.rs:161`, after the all-slots `wait_for_fences` at `:83`, and `LinearCaptureJob` is plain `&mut self` state. The ordering test pins only the screenshot and depth readbacks, not linear capture (see Notes).
- **Allocator sharing**: clean. Allocator guards are scoped blocks (`helpers.rs:41`, `image.rs:231`, `buffer.rs:446`); the lock-site allowlist test passes after the staging-allocate sites moved into `create_staging_buffer` (`lock_recovering`). Queue `Mutex` is locked only around `queue_submit` (`texture.rs:1284`) and `egui_pass.rs:310,418`; the fence wait is outside the queue lock. `GpuMemoryBudget::update_live` is a main-thread `resource_mut` after `sample_live_memory_budget`.
- **`Send + Sync` bounds**: clean. `UiManager`/Ruffle `Player` is not an ECS resource (`ui/src/lib.rs:6,56`); no raw pointer crossing threads added; kira work stays on the main thread via `AudioWorld`, and `audio/system.rs` drops its query guards before `build_spatial_track` (`:399-425` then `:437`). No audio-thread change in the window (`positional` flag only).
- **Other rayon fan-outs**: clean. `build_render_data` joins (`render/mod.rs:1147-1215`): bodies are `build_skinned_palettes` (caller-owned scratch), `assemble_camera`, `collect_static_mesh_draws`, `collect_lights`, `collect_fog_volumes`, `fog_height_reference`. A scan of `render/*.rs` for `query_mut|resource_mut|try_resource_mut|.write()` finds production hits only in `water.rs`, `particles.rs`, `sky.rs`, `update_morph_weights` and the soft-material cache at `:1075`, all outside or before the joins; test modules aside, every branch is read-only. `SharedParseCache` (#6591): guard is dropped before `parse` (inner block), re-taken only for the store, `apply_exterior_processing` runs on the unshared value before the `Arc`, nothing takes an ECS guard under it; five `shared_parse_tests` green. Load-order fold untouched in the window.

## 4. Findings

### CONC-2026-11-08-7.1: `TextureStreamer`'s pool job has no `catch_unwind`, so a panicking archive read aborts the process (sibling of #6233)

- **Severity**: MEDIUM
- **Dimension**: Worker Threads
- **Location**: `byroredux/src/texture_streamer.rs:133-138` (pool built with no `panic_handler`), `:222-235` (`self.pool.spawn(move || { provider.extract_texture_from(...); done.send(...) })`)
- **Status**: NEW. Same class as #6233 (closed, terrain-height pool, MEDIUM), #6119 and #5665. The streamer postdates both fixes (2e30cdf10 / 3d67d963e, after the 10-31 baseline).
- **Description**: `rayon::ThreadPool::spawn` hands a panicking job to the pool's panic handler, and with none installed rayon-core 1.13.0 aborts the process. The workspace keeps `panic = "unwind"` (`Cargo.toml:350`), so every other archive-reading worker wraps its read: stream parse (`extract_guarded`, `pre_parse.rs:391`), prefetch (`texture_prefetch.rs:230`), terrain height (`spawn_guarded`, `terrain_height.rs:109`). This one calls `TextureProvider::extract_texture_from`, which reaches `Archive::extract_texture_from_or_warn` (`Ba2Archive::extract_dx10_mips`, zlib/LZ4 inflate of untrusted chunks) and, on a miss, the plain `extract` fallback for BSA/loose entries. The same code is guarded on the other two pools because reader panics are an established class.
- **Evidence**: `grep -n 'catch_unwind\|panic_handler' byroredux/src/texture_streamer.rs` returns nothing; `grep -rn panic_handler byroredux/src crates/*/src` returns nothing. `cargo test` has no panicking-read test for the module. I did not find a concrete reachable panic: the `extract_dx10_mips` / `finish_dx10_payload` / `read_chunk_payload` path returns `io::Error` and the slicing is bounds-safe (`chunks[keep_from..]` with `keep_from <= len`). So reachability is unproven and the rating rests on #6233's precedent, not on a demonstrated crash.
- **Trigger Conditions**: `render.texture_streaming` on; a streamed texture wanted at a finer level than resident; a BA2/BSA/loose entry whose re-read panics in a decoder. A modded or corrupt archive is enough. Fires on a pool thread, mid-frame.
- **Impact**: process abort with no unwind, no `Drop`, no save-ring flush. Secondary: without a guard the job never reaches `done.send`, so `in_flight` would keep the handle forever if a panic handler were ever added later.
- **Suggested Fix**: wrap the read in `std::panic::catch_unwind(AssertUnwindSafe(..))`, map a panic to `bytes: None`, and always send the `FinishedRead` (so `step` clears `in_flight` and logs the existing "found no archive entry" warning); add a test next to `a_panicking_height_job_is_reported_failed`. Alternatively give the pool a `panic_handler` that sends the `None` read, but the per-job guard matches the other three workers.
- **Verification Path**: `cargo test` (a job closure that panics, asserting the `FinishedRead` arrives with `bytes: None`). No GPU needed.

## 5. Notes (not findings)

- Spec text drift to fix in SKILL.md Dim 7: (a) add `texture_streamer.rs` as a worker with the guard rule now stated for four pools; the spec already says "no `catch_unwind`", which is the gap in 7.1; (b) Paths omit `asset_provider/material/cdb.rs` (`sf-cdb-index` thread), `crates/audio/src/fuz.rs` (`run_bounded` reader threads) and `asset_provider/archive.rs::open_loose`; (c) the capture-ordering test (`finish_readback_runs_after_the_in_flight_fence_wait`) pins screenshot and depth only, so linear capture's ordering (`sync_and_acquire_frame.rs:161`) rests on a read, not a test. Low value to pin, but it is the one capture readback without a guard.
- Perf-adjacent, not concurrency: `fuz_to_wav_bytes` (an `ffmpeg` subprocess, up to `FFMPEG_TIMEOUT` = 10 s) runs on the frame thread inside `VoiceSoundCache`'s write guard (`dialogue_voice.rs:265-282`). Bounded by #6307, and the scheduler's declared access keeps conflicting systems out, so it is a stall, not a deadlock. `run_bounded` joins its stderr reader after `kill`/`wait`; a grandchild holding the pipe would extend that join, which `ffmpeg` spawned directly does not do.
- `TextureStreamer::step` logs a `warn!` per landing with `bytes == None` and `apply_wanted` rebuilds requests every frame with no negative memo, so a persistently unreadable source would re-read and re-warn each frame. Needs a real unreadable entry to matter; left unfiled.
- `build_texture_provider` runs on the main thread the first frame streaming is enabled (archive opens go through `open_shared`), a one-time stall.

## 6. Incidental (outside this dimension)

- `byroredux-debug-server` roster guard is red at HEAD: `FactionReputation` (CHARAL component, 79371213d) derives `inspect` but is not registered in `crates/debug-server/src/registration.rs` (test at `:641`, #4755 rule). Dedup: #4755 and #5694 are closed and about older gaps; no open issue found. Suggest `/audit-tooling` pick it up (NEW, tooling, LOW-MEDIUM).
