# Concurrency & Synchronization Audit — 2026-07-25

**Scope**: All 7 dimensions (comprehensive sweep).
- **Dimension 1** — Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)
- **Dimension 2** — Compute → AS → Fragment Chains
- **Dimension 3** — ECS Lock Ordering & Deadlock
- **Dimension 4** — Scheduler Access Declarations (regression guard)
- **Dimension 5** — RwLock Patterns (Resource↔Storage, Physics Step)
- **Dimension 6** — Resource Lifecycle (GPU teardown ordering)
- **Dimension 7** — Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds

**Depth**: deep (traced concurrent paths + timing windows).
**Repo**: `/mnt/data/src/gamebyro-redux` @ `ca7a4e0e` (main, clean).

**Method**: Each dimension ran as an independent agent sweep against current
`main`, re-reading rather than trusting the prior clean bill from
`AUDIT_CONCURRENCY_2026-07-16.md`. This sweep lands after a substantial churn
window: the FSR 3.1 upscaler integration + presentation-pass split (renderer,
Dimensions 1/2/6), the CHARAL character-ruleset arc + quest alias/object-
targeting effects (ECS, Dimension 3), the `b5e38c22` ABBA-detector CI fix
(Dimension 4), and a fresh trace of every physics/AI-locomotion RwLock pairing
(Dimension 5). Per the standing speculative-fix guardrail, no Vulkan barrier/
stage/layout change is proposed on reasoning alone anywhere below — findings
whose only evidence is "this looks wrong" are explicitly marked HYPOTHESIS and
carry a concrete validation-layer/RenderDoc confirmation path instead of a
prescribed fix.

## Executive Summary

This sweep is **not clean** — it is the first sweep in this audit lineage to
surface HIGH-severity findings. Two genuine lock-order inversions were found
in the ECS/physics RwLock surface (Dimension 5): four AI-locomotion systems
(`follow.rs`, `escort.rs`, `travel.rs`, `guard.rs`) acquire `PhysicsWorld`
before `GlobalTransform`, inverting the order `physics_sync_system` and
`ragdoll.rs` established after `b5e38c22`; and `character_controller_system`
acquires `Transform` before `RapierHandles` while `pull_dynamic` acquires the
reverse. Both are currently masked by stage-barrier scheduling accident, not
by an acquisition-order invariant, and both are **not just theoretical** — a
debug build with `BYRO_LOCK_ORDER_CHECK=1` against live content with
follow/escort/travel/guard NPCs will trip the cross-thread ABBA detector
itself. `b5e38c22`'s "long tail" fix closed the sites unit tests could reach;
these three sites (plus a fourth MEDIUM diagnostic-only inversion) are
precisely the ones no test currently drives with a live `PhysicsWorld`.

Beyond that, the renderer surface (Dimensions 1/2/6) is sync-correct on every
traced invariant but carries seven MEDIUM/LOW findings that are almost
entirely in the "confirmed-safe today, fragile under a plausible future
change" or "failure-path only" category — the FSR SDK boundary needs
validation-layer confirmation before any barrier change is even discussed, and
two renderer resize-failure paths (SSAO, water-caustic) leave a descriptor set
bound to a destroyed image view if the resize allocation itself fails under
VRAM pressure. Dimension 4 found the ABBA detector's CI wiring has a gap of
its own: the one CI job that boots the real engine and dispatches the actual
parallel scheduler does not enable `BYRO_LOCK_ORDER_CHECK`, and even the boot-
time invariant `debug_assert`s in that job are masked by a `|| true` +
substring-match harness that would not fail on a tripped assert. Dimension 3
found two LOW "safety currently rests on unstated exclusive-scheduling" notes
in the new CHARAL/save code, the same finding class as the already-closed
#2126. Dimension 7 is clean — no findings.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 2 |
| MEDIUM   | 7 |
| LOW      | 12 |
| **Total** | **21** |

All findings below are **NEW** unless otherwise marked (verified per-dimension
against `/tmp/audit/concurrency/issues.json`, 29 open issues, no keyword
overlap found except where explicitly noted as Existing/Regression).

---

## Findings — CRITICAL

None.

---

## Findings — HIGH

### CONC-D5-01: `PhysicsWorld → GlobalTransform` order in four AI locomotion systems inverts the order `b5e38c22` established
- **Severity**: HIGH
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/systems/follow.rs:106,116,134`; `byroredux/src/systems/escort.rs:143,158,186,200`; `byroredux/src/systems/travel.rs:140,152,90`; `byroredux/src/systems/guard.rs:121,128,73` — versus `crates/physics/src/sync.rs:539-543` and `byroredux/src/ragdoll.rs:385-390`
- **Status**: NEW
- **Description**: `b5e38c22` normalized `GlobalTransform`-before-`PhysicsWorld` acquisition in `ragdoll.rs` to match `physics_sync_system`'s `push_kinematic`, but four AI locomotion systems acquire the pair in the opposite order and were not touched by that fix. Each holds `world.try_resource::<PhysicsWorld>()` live across a Pass-1 loop while calling `world.get::<GlobalTransform>(target)` (a tracked read lock) inside that scope, then passes the still-held physics guard on to `step_toward`.
- **Evidence**: `follow.rs:106` binds `physics`; `follow.rs:134` calls `world.get::<GlobalTransform>(target_entity)` with `physics` provably still alive (used again at `:156`). Same shape in `escort.rs:143→186`, `travel.rs:140→152→90` (via `resolve_destination`), `guard.rs:121→128→73` (via `resolve_anchor`). The reverse edge is recorded twice: `sync.rs:539-543` (`GlobalTransform` read, then `PhysicsWorld` write, with the transform guard still live) and `ragdoll.rs:385-390` (`GlobalTransform` write, then `PhysicsWorld` read). All four systems also call `resolve_entity_by_global_form_id` under the same physics guard, adding `PhysicsWorld → FormIdComponent/FormIdPool` edges — currently consistent with `dump_awake_fallers`, so latent-only.
- **Impact**: (1) Latent cross-thread ABBA: `ragdoll_writeback_system` holds a `GlobalTransform` **write** guard while waiting on `PhysicsWorld`; any of these four systems holds `PhysicsWorld` read while waiting on `GlobalTransform` read. Add a thread queued for a `PhysicsWorld` write (`register_newcomers`, `set_linear_velocity`, `apply_buoyancy`) and a real 3-way cycle closes — a hard hang. (2) Immediate and non-hypothetical: a debug build with `BYRO_LOCK_ORDER_CHECK=1` against any cell with a `FollowBehavior`/`EscortBehavior`/`TravelBehavior`/`GuardBehavior` NPC that resolves a live target will abort with "ECS cross-thread deadlock risk (ABBA)" on the first tick — the exact tool `b5e38c22` was fixing CI for trips on this live content path.
- **Trigger Conditions**: A live `PhysicsWorld` resource (any cell load) plus at least one actor with a resolvable follow/escort/travel/guard target/anchor. Full deadlock additionally needs one of these systems promoted out of `add_exclusive`, a second `Stage::Physics` system, or an off-scheduler caller touching the same pair. All four unit tests for these systems deliberately run **without** a `PhysicsWorld` resource (`try_resource` returns `None`, no lock taken), which is why `BYRO_LOCK_ORDER_CHECK=1` in CI has never seen this pair.
- **Related**: `b5e38c22`, #313, `crates/core/src/ecs/lock_tracker.rs::global_order::record_and_check`, CONC-D5-02 (same class, different pair), CONC-D4-NEW-01/-03 (CI coverage gaps that explain why this was never caught).
- **Suggested Fix**: In all four systems, resolve targets/anchors and snapshot the needed `GlobalTransform.translation` values into scratch structs **before** `let physics = world.try_resource::<PhysicsWorld>()`, so no `world.get::<GlobalTransform>`/`resolve_entity_by_global_form_id` call happens under the physics guard. Add a regression test per system that installs a real `PhysicsWorld` resource before calling `*_system_inner` under `BYRO_LOCK_ORDER_CHECK=1`, so the detector actually covers the pair going forward.

### CONC-D5-02: `character_controller_system` acquires `Transform → RapierHandles`; `pull_dynamic` acquires the reverse
- **Severity**: HIGH
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/systems/character.rs:171-183` versus `crates/physics/src/sync.rs:588-624`
- **Status**: NEW
- **Description**: A storage↔storage inversion in the same physics territory, untouched by `b5e38c22`. `character_controller_system`'s snapshot block acquires `CharacterController` → `Transform` → `RapierHandles`, with the `Transform` read guard still live when `RapierHandles` is acquired. `pull_dynamic` does the exact reverse: `handles_q`/`body_q` are held from function entry and are still live when `query_mut::<Transform>()` (a **write** lock) is taken at the end.
- **Evidence**: `character.rs:171-182` — `tq` bound, dereferenced, then `world.query::<RapierHandles>()` while `tq` is still in scope → edge `Transform → RapierHandles`. `sync.rs:589-624` — `handles_q`/`body_q` never dropped before `query_mut::<Transform>()` at `:622` → edges `RapierHandles → Transform`, `RigidBodyData → Transform`. No unit test drives `character_controller_system` at all; `rapier_release_tests.rs` short-circuits before the `Transform` lock in `pull_dynamic` on empty updates — so neither edge reliably reaches the CI graph.
- **Impact**: The more dangerous of the two HIGH findings because `pull_dynamic`'s `Transform` acquisition is a **write** lock: a thread holding `RapierHandles` read and blocking on `Transform` write, against a thread holding `Transform` read and blocking on `RapierHandles` read, deadlocks directly once a `RapierHandles` writer (`register_newcomers`, or `activate_ragdoll`) is queued. `character_controller_system` runs in `Stage::Early` and `physics_sync_system` in `Stage::Physics`, so today the stage barrier serializes them — protection by scheduling accident, not by an acquisition-order invariant.
- **Trigger Conditions**: Character mode (`PlayerMode::Character`) with a physics-backed player capsule, plus any of: a second `Stage::Physics` system, `character_controller_system` moved into a parallel batch sharing a stage with physics, or a debug-server console command thread reading both `Transform` and `RapierHandles`. Detector-panic trigger: a debug build with `BYRO_LOCK_ORDER_CHECK=1` in character mode with a dynamic body present.
- **Related**: `b5e38c22` (same class, missed), CONC-D5-01, #313.
- **Suggested Fix**: Preferred — in `pull_dynamic`, `drop(handles_q); drop(body_q);` immediately after the collection block and before `query_mut::<Transform>()` (the collected `updates` Vec already carries everything needed; this also shortens two hot read-guard holds and matches `sync.rs`'s own two-phase discipline elsewhere). Alternative — in `character.rs`, read `RapierHandles` before `Transform`.

---

## Findings — MEDIUM

### CONC-D5-03: `dump_awake_fallers` holds `PhysicsWorld` while acquiring `RapierHandles`, inverting every other site in the crate
- **Severity**: MEDIUM
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `crates/physics/src/sync.rs:237-266`
- **Status**: NEW
- **Description**: The #1698 awake-faller diagnostic takes `world.resource::<PhysicsWorld>()` at line 240 and holds it to the end of the function (it reads `pw.bodies` at 271-276), then acquires `RapierHandles`, `RenderLayer`, `FormIdComponent`, `PhysicsSourceForm`, `FormIdPool` underneath it — the opposite order from `push_kinematic`/`pull_dynamic`/`apply_buoyancy`, all of which acquire `RapierHandles` before `PhysicsWorld`.
- **Evidence**: `sync.rs:240` — no scope, no `drop`, on `PhysicsWorld`; `sync.rs:251` acquires `RapierHandles` underneath it → edge `PhysicsWorld → RapierHandles`. Reverse edge from `sync.rs:533,543`. Both occur inside the same `physics_sync_system` call on one thread, so the same-thread tracker won't fire, but `global_order::record_and_check` — the cross-thread graph — will, since `push_kinematic` runs earlier in the same frame and already recorded the forward edge.
- **Impact**: Doubly gated (`BYRO_PROFILE_FALLERS` env var + one-shot `AtomicBool` + ≥16-awake-body floor), so lower likelihood than the two HIGH findings, but the diagnostic an operator reaches for during a settle-storm investigation will, on a debug build with the order checker on, panic the frame instead of dumping — and the panic poisons `PhysicsWorld`'s `RwLock`, taking the rest of the session down. It also permanently seeds a `PhysicsWorld → RapierHandles` edge in the process-wide graph, which then makes unrelated later acquisitions panic too.
- **Trigger Conditions**: `BYRO_PROFILE_FALLERS` set, ≥16 awake dynamic bodies, first occurrence in the process. Deadlock (rather than panic) additionally needs a second `Stage::Physics` system.
- **Related**: #1698, `b5e38c22`, `byroredux/src/boot.rs:894-902` (the Access declaration already acknowledges this hidden read surface).
- **Suggested Fix**: Collect the awake-body snapshot (handle → translation.y, linvel().y pairs) into a `Vec` under the `PhysicsWorld` guard, `drop(pw)`, then open `RenderLayer`/`FormIdComponent`/`PhysicsSourceForm`/`FormIdPool`. Restores `RapierHandles → PhysicsWorld` ordering and shortens the hold on the hottest resource in the engine.

### CONC-D4-NEW-01: The only CI job that boots the real engine does not enable the ABBA detector
- **Severity**: MEDIUM
- **Dimension**: Scheduler Access Declarations
- **Location**: `.github/workflows/ci.yml:131-173` (`vulkan-validation`) vs `:75-87` (`lock-order-check`)
- **Status**: NEW
- **Description**: `BYRO_LOCK_ORDER_CHECK=1` is set only on the `lock-order-check` job (`cargo test --workspace`, single-threaded hand-built `World`s). The `vulkan-validation` job — the only CI job that boots the actual engine (`cargo run -p byroredux -- --bench-frames 5` under lavapipe, debug profile so `global_order` is compiled in) — does not set the env var, so the detector is compiled in but inert (`global_order::ENABLED` false, `record_and_check` returns immediately).
- **Evidence**: `ci.yml` `vulkan-validation` step exports only `VK_ICD_FILENAMES`, `VK_INSTANCE_LAYERS`, `RUST_LOG` — no `BYRO_LOCK_ORDER_CHECK`. `ENABLED` is a `LazyLock<AtomicBool>` seeded from `std::env::var_os` at first touch (`lock_tracker.rs:216-217`).
- **Impact**: The live 5-frame run is the only place in CI where rayon actually dispatches the real parallel batch across worker threads against a real loaded world — precisely the workload the cross-thread graph was built for — and it is the one place the detector is switched off. This is the structural reason CONC-D5-01/-02/-03 above were never caught: they require a live `PhysicsWorld`/real cell, which only this job provides, and this job runs with the detector disabled.
- **Related**: #313, #1410, `b5e38c22`, CONC-D5-01, CONC-D5-02, CONC-D5-03, CONC-D4-NEW-03.
- **Suggested Fix**: Add `BYRO_LOCK_ORDER_CHECK: 1` to the `vulkan-validation` job's `env:` block (or add a second `--bench-frames` invocation with it set). Cost is negligible for a 5-frame run.

### CONC-D4-NEW-02: `vulkan-validation` swallows the boot-time access-invariant `debug_assert`s
- **Severity**: MEDIUM
- **Dimension**: Scheduler Access Declarations
- **Location**: `.github/workflows/ci.yml:163-172`; guards at `byroredux/src/boot.rs:1002-1030`
- **Status**: NEW
- **Description**: The three #1394/#1602 guards (`undeclared_parallel_count`/`known_conflict_count`/`unknown_pair_count`, all `debug_assert_eq!(..., 0)`) live in `install_runtime_registries`, called from `App::new` — before the event loop, so they do execute in the `vulkan-validation` job. But the step runs `OUTPUT=$(... cargo run ... 2>&1 || true)` and fails **only** if the output contains the literal substring `[Vulkan]`. A `debug_assert` panic's text contains no such substring, so the job goes green on a tripped guard.
- **Evidence**: `ci.yml:164` — `|| true`; `ci.yml:168` — `if echo "$OUTPUT" | grep -qF '[Vulkan]'` is the sole failure predicate. Panic text from `boot.rs:1011/1023/1029` contains no `[Vulkan]` marker.
- **Impact**: These guards are the primary regression pin for the whole scheduler-access-declaration dimension, and they are currently enforced by nothing in CI: `cargo test` never calls `build_scheduler` (it's `pub(crate)`, sole caller `App::new`), and the one job that does call it discards the exit code. A future `add_to()` or a new conflicting pair (the exact #1601 shape) would reach `main` with a green CI. Today's state is fine (verified statically by Dimension 4), so this is a guard-integrity gap, not a live defect.
- **Related**: #1394, #1601, #1602, `byroredux/src/scheduler_access_tests.rs`, CONC-D4-NEW-01 (same job, adjacent gap).
- **Suggested Fix**: Cheapest — also fail the step on a `panicked at` substring, or capture the real exit code (`set -o pipefail`, keep `|| true` only for the known "no suitable device" bail, matched explicitly). Sturdier — since `scheduler_access_tests.rs` is already compiled into the bin's test binary, add a real `cargo test` asserting the three counts are 0, replacing the `include_str!`-grep proxies.

### CHAIN-D2-02: FSR SDK output-image layout contract is asserted by the engine but never verified — HYPOTHESIS
- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs:592-663` (`record_fsr_barriers_before`), `:700-741` (`record_fsr_barriers_after`)
- **Status**: NEW — **HYPOTHESIS, needs validation-layer confirmation**
- **Description**: The engine hand-declares the layout the vendored FFX Vulkan backend will leave every SDK-touched image in (output → `GENERAL` before dispatch, asserted `old_layout = GENERAL`/`SHADER_WRITE` after). Nothing in the repo pins the FFX backend's internal resource-state tracking to those assumptions; if the SDK leaves the output in a different layout, the after-barrier's `old_layout` is a lie and the transition is UB (VUID-VkImageMemoryBarrier-oldLayout-01197).
- **Evidence**: `frame_upscaler.rs:640-646` declares `old_layout(SHADER_READ_ONLY_OPTIMAL) → new_layout(GENERAL)`; `:720-726` declares the exact inverse. The only cross-check is a `SAFETY` comment asserting the conclusion, not code that verifies it.
- **Impact**: If wrong — corrupted/black upscaled output or a hard validation error every frame. If right — zero cost, this row closes. Per the standing speculative-Vulkan-fix guardrail, this is reported as a hypothesis, not a bug.
- **Trigger Conditions**: Every FSR frame (`--upscaler fsr3`, the default per `5c56e311`/`5c7acfe2`).
- **Verification Path**: Run `BYRO_VALIDATION=1` (sync validation) for ~200 frames in FSR mode; grep for `VUID-VkImageMemoryBarrier-oldLayout-01197` / `SYNC-HAZARD-WRITE-AFTER-WRITE` / `SYNC-HAZARD-READ-AFTER-WRITE` naming the `upscale_output_*` image. A clean 200-frame run across both FIF slots is meaningful evidence this closes as a non-issue; a RenderDoc capture of the output image's layout timeline is the definitive artifact.
- **Related**: CHAIN-D2-03 (same boundary, failure-path variant), commit `33d6a18e`, `5c7acfe2`.
- **Suggested Fix**: None proposed on reasoning alone. If validation is clean, land a comment on `record_fsr_barriers_after` recording the validated SDK contract + version, so a future SDK bump re-triggers the check.

### CHAIN-D2-03: FSR dispatch-failure recovery assumes the SDK recorded nothing into the command buffer before erroring — HYPOTHESIS
- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs:441-468`, `:667-698` (`record_fsr_depth_restore`)
- **Status**: NEW — **HYPOTHESIS**
- **Description**: When `context.dispatch` returns `Err`, the recovery path latches `dispatch_failure` and records depth-restore + native-blit barriers whose declared `old_layout` values are correct only if the SDK recorded zero image transitions into `cmd` before failing. `ExecuteGpuJobsVK` in the vendored SDK (`third_party/fidelityfx-sdk-v1.1.4/sdk/src/backends/vk/ffx_vk.cpp:4187-4240`) iterates every queued GPU job and records each into the command buffer, checking `errorCode` only **after** the loop, with the code overwritten each iteration — so a mid-sequence failure can leave partially-recorded transitions while reporting `FFX_OK`, or an error can arrive after real work was already recorded.
- **Evidence**: `frame_upscaler.rs:453-457` SAFETY comment: "`record_fsr_barriers_before` established the exact layouts these two transition out of" — true only under the zero-partial-recording assumption. `blit_output_src_access` (`:812-818`) encodes the same assumption in code.
- **Impact**: On a real SDK dispatch rejection, this could produce a device loss or corrupted frame instead of the intended graceful degradation to the native blit — a crash-on-crash in the exact path meant to handle "something already went wrong."
- **Trigger Conditions**: Any `ffxFsr3UpscalerContextDispatch` failure — SDK OOM, invalid descriptor, device-lost mid-frame. Rare, never exercised on the happy path.
- **Verification Path**: Add a debug-only env gate (e.g. `BYRO_FSR_FORCE_DISPATCH_FAIL=1`) making the FFI shim's `dispatch` return `Err` without calling into the SDK, to isolate "recovery is sound when nothing was recorded." Separately, run `BYRO_VALIDATION=1` with a genuinely invalid dispatch description to see whether the SDK records before validating. Confirming signal: validation reporting an `oldLayout` mismatch on the depth or output image only on the forced-failure frame.
- **Related**: CHAIN-D2-02, commit `f9a42e07` ("survive an FSR dispatch failure instead of dropping the frame").
- **Suggested Fix**: Not on reasoning alone. If the SDK is confirmed to record before it can fail, the robust shape is recording the FSR boundary barriers + dispatch into a secondary command buffer that is simply not executed on failure — a real restructure, not to be attempted without the repro above.

### RL-D6-01: SSAO recreate failure on resize leaves scene descriptor binding 7 pointing at a destroyed AO image view
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:392-453`
- **Status**: NEW
- **Description**: `recreate_texture_ssao_bindings` destroys the old `SsaoPipeline` (and its per-FIF AO images/views) before attempting to build the replacement. If `SsaoPipeline::new` fails, the `Err` arm only logs a warning and leaves `self.ssao = None`; `scene_buffers.write_ao_texture` is never called, so scene descriptor set 1 / binding 7 (`aoTexture`) still holds the destroyed `vk::ImageView` + `vk::Sampler`. This failure does not propagate, so `recreate_screen_passes` completes and rebuilds framebuffers — the `#1211` `framebuffers.is_empty()` bail-out does not catch it, and the next frame binds the stale set.
- **Evidence**: `resize.rs:401` destroys `ao_image_views`/`ao_sampler`; `:439-446` — `write_ao_texture` only in the `Ok` arm; `:449-452` — `Err` arm logs and returns with no rebind, no propagate. `scene_buffer/descriptors.rs:16-35` — `write_ao_texture` is the sole writer of binding 7; `triangle.frag` samples `aoTexture` unconditionally.
- **Impact**: On a resize where SSAO re-creation fails (realistic trigger: VRAM pressure during a drag-resize with a large cell loaded), every subsequent frame binds a descriptor referencing freed GPU memory. Validation layers report an invalid/destroyed imageView; on release drivers this reads freed memory → garbage AO, corruption, or device loss. Failure-path-only, hence MEDIUM.
- **Related**: Success-path twin already fixed as `#33 / LIFE-H2` (`AUDIT_RENDERER_2026-04-10b.md:57-60`); the failure arm was never covered. Sibling of RL-D6-02.
- **Suggested Fix**: Keep a 1×1 white "AO = 1.0" placeholder image + sampler owned by `VulkanContext` and rebind binding 7 to it for all frame-in-flight slots in the `Err` arm (also needed for the init-time `self.ssao = None` case at `mod.rs:2149-2152`, which leaves binding 7 entirely unwritten today).

### RL-D6-02: Water-caustic accumulator resize failure leaves `WaterPipeline` set 2 bound to a destroyed storage image view
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:614-657`, `crates/renderer/src/vulkan/water.rs:455-466`
- **Status**: NEW
- **Description**: On the two failure arms of the water-caustic resize block, the accumulator is destroyed and `self.water_caustic_accum` set to `None`; the rebind is guarded by an `if let (Some(w), Some(accum))` and is therefore skipped, leaving `WaterPipeline::water_caustic_descriptor_sets[frame]` binding 0 holding the destroyed per-FIF storage view. `record_draw` binds set 2 **unconditionally**, and the geometry pass gates the water draw only on `self.water.is_some()` — never on the accumulator. This is strictly worse than RL-D6-01 because the access is a shader **write** (`imageAtomicAdd`).
- **Evidence**: `resize.rs:633-634` and `:644-645` destroy + null the accumulator on both failure arms; `:652-657` rebind is skipped when `accum` is `None`; `water.rs:459-466` binds set 2 with no `Option` gate; `context/geometry_pass.rs:512-542` gates the water draw only on `self.water`. The init-path twin at `context/mod.rs:2105-2113` carries a stale safety comment claiming the shader-side gate (`sunDirection.w > 0`) protects an unwritten set 2 during a "scaffold-only window" — but Phase D and Phase E (#1255/#1257) have both shipped, so that window is closed.
- **Impact**: Post-failure, every exterior/water frame binds a descriptor set whose storage image was freed and issues an atomic write against it. Failure-path-only → MEDIUM.
- **Related**: #1255/#1210 Phase C; sibling of RL-D6-01; also refreshes the stale comment at `context/mod.rs:2105-2113`.
- **Suggested Fix**: Either gate the set-2 bind + water draw on accumulator presence, or keep a 1×1 R32_UINT dummy storage image owned by `WaterPipeline` and rebind set 2 to it whenever the accumulator drops out (covers both the resize-failure and init-failure arms). Update the stale `mod.rs:2105-2113` comment either way.

---

## Findings — LOW

### CONC-D1-2026-07-25-01: Presentation render pass suppresses its implicit outgoing dependency with `dstStageMask = NONE`
- **Severity**: LOW
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/presentation.rs:173-178`
- **Status**: NEW
- **Description**: The presentation render pass declares an explicit `srcSubpass = 0 → dstSubpass = VK_SUBPASS_EXTERNAL` dependency with `dstStageMask = NONE` and no `dstAccessMask` — replacing Vulkan's implicit end-of-pass dependency with one whose second sync scope is empty, leaving the pass's `COLOR_ATTACHMENT_WRITE` unordered against any later command. Every sibling pass (composite, egui) declares a real dst scope; presentation is the only outlier.
- **Evidence**: `.dst_stage_mask(vk::PipelineStageFlags::NONE)` at `presentation.rs:173-178`, versus `composite.rs:547-555` (`COMPUTE_SHADER|TRANSFER`) and `egui_pass.rs:322-328` (`BOTTOM_OF_PIPE`, with an explicit "don't rely on the implicit edge" comment).
- **Impact**: No live hazard today — the two current downstream consumers (egui overlay, screenshot copy) each carry their own incoming barrier with a matching src scope, and the present itself is covered by the `render_finished` semaphore. The exposure is forward-looking: a future pass added between `presentation.dispatch` and `end_command_buffer` without its own barrier would race the swapchain image with nothing to catch it in `cargo test`.
- **Trigger Conditions**: Not reproducible today; requires a future code change adding an unbarriered swapchain-image consumer.
- **Verification Path**: `BYRO_VALIDATION=sync` on a screenshot-capture frame and an egui-overlay frame; absence of `SYNC-HAZARD-READ_AFTER_WRITE` on the swapchain image is the evidence the two self-synchronizing consumers are sufficient today.
- **Related**: `composite.rs:547-555` (the pattern to mirror), commit `33d6a18e`.
- **Suggested Fix**: Give the outgoing dependency a real dst scope mirroring the actual consumers (`COLOR_ATTACHMENT_OUTPUT | TRANSFER` / `COLOR_ATTACHMENT_READ|WRITE | TRANSFER_READ`), or delete the explicit dependency and let Vulkan synthesize the implicit one. Confirm with `BYRO_VALIDATION=sync` before/after.

### CONC-D1-2026-07-25-02: HYPOTHESIS — swapchain layout transition may not be covered by the acquire semaphore's wait stage
- **Severity**: LOW
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/presentation.rs:143-172` + `crates/renderer/src/vulkan/context/draw.rs:2249-2250`
- **Status**: NEW — **HYPOTHESIS, not a confirmed bug**
- **Description**: The submit waits on `image_available[frame]` with `wait_dst_stage_mask = [COLOR_ATTACHMENT_OUTPUT]`; the presentation pass's swapchain attachment has `initial_layout = UNDEFINED` and an incoming dependency whose dst scope includes `FRAGMENT_SHADER`, which the acquire wait does not block. In principle the `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL` transition (and the implicit discard) could execute before the presentation engine finished reading the image for its previous present.
- **Evidence**: `presentation.rs:143-144,166-169` (layout declarations); `draw.rs:2249-2250` (`wait_stages = [COLOR_ATTACHMENT_OUTPUT]`).
- **Impact**: If real — intermittent tearing/partial-frame corruption of the previous frame under MAILBOX or rapid resize, on drivers where the from-UNDEFINED transition isn't a no-op. Strong prior this is a false positive: the identical shape existed when `composite` owned the swapchain write, and sync-val was run against exactly this construct without flagging an acquire-ordering hazard (it did flag an unrelated WAW, already fixed).
- **Trigger Conditions**: Requires a driver that returns the acquired index optimistically and signals the semaphore later. Not reproducible on demand.
- **Verification Path**: `BYRO_VALIDATION=sync` on a release build, looking for `SYNC-HAZARD-WRITE_AFTER_PRESENT`/`WRITE-AFTER-READ` naming the swapchain image at the presentation render-pass begin. Absent that message, close as false positive, not "fixed."
- **Related**: `composite.rs:512-524` (comment recording the prior sync-val run), project rule against speculative Vulkan sync fixes.
- **Suggested Fix**: Do not change anything on this reasoning alone. Only if sync-val confirms, add `FRAGMENT_SHADER` to `wait_stages` at `draw.rs:2250` (cheap, no render-pass surgery).

### CONC-D1-2026-07-25-03: FSR dispatch-failure recovery depends on undocumented FFX partial-recording behaviour
- **Severity**: LOW
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs:441-473`, `:479-521`
- **Status**: NEW
- **Description**: `FrameUpscaler::record` treats an `ffxDispatch` error as "nothing was recorded except my own boundary barriers," but the vendored SDK's `ExecuteGpuJobsVK` records every queued job into the command buffer and only checks the error code after the loop — a mid-sequence failure can already have recorded barriers/dispatches.
- **Evidence**: `ffx_vk.cpp:4198-4236` records all jobs before checking `errorCode`; `frame_upscaler.rs:441-467` recovery assumes only its own barriers ran.
- **Impact**: The recovery path happens to be correct today — verified independently: FFX transitions land on states the pre-barriers already established (no-op), and the blit's src stage/access mask includes `COMPUTE_SHADER`, ordering any partial FFX storage writes before the recovery blit's transfer write. The correctness is incidental rather than designed; a future narrowing of the "over-broad" blit masks (an attractive-looking cleanup) would silently reintroduce a same-command-buffer WAW with no test coverage.
- **Trigger Conditions**: Requires an actual `ffxDispatch` failure (SDK OOM, internal overflow, device-lost mid-frame). Not reproducible on demand; one-shot latch per swapchain generation.
- **Verification Path**: Not reachable by `cargo test`. Validation-layer confirmation needs a fault-injected dispatch failure; practical mitigation is documentation plus keeping the currently over-broad masks.
- **Related**: commit `f9a42e07`, `frame_upscaler.rs:808-818` (`blit_output_src_access`, unit-tested).
- **Suggested Fix**: Add a comment at `frame_upscaler.rs:441` recording that FFX `ExecuteGpuJobsVK` records all jobs before checking its error code, so the wide src mask on the recovery blit is documented as load-bearing, not defensive padding. No code change required.

### CHAIN-D2-01: FSR's new error propagation in `record_post_passes` bypasses the #917 "no advance on unsubmitted dispatch" invariant
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:568-590`; `frame_upscaler.rs:358-359`; `crates/renderer/src/vulkan/svgf.rs:1287`; `crates/renderer/src/vulkan/taa.rs:770`
- **Status**: NEW
- **Description**: FSR introduced the first `?`-propagating error path inside `record_post_passes`. It sits after `svgf.dispatch`/`taa.dispatch` have already set `dispatched_this_frame`, and aborts `draw_frame` before `queue_submit`, so `mark_frame_completed()` never runs for that frame for SVGF/TAA — the latch stays `true` and a later frame's `mark_frame_completed` bumps `frames_since_creation[frame]` for a dispatch that never reached the GPU. This is precisely the failure mode #917/REN-D10-NEW-03 was written to prevent.
- **Evidence**: `frame_upscaler.rs:358` is the sole `Err` return in `FrameUpscaler::record`; `svgf.rs:1287` sets `dispatched_this_frame[frame] = true` with a comment that no longer covers errors introduced *after* that point.
- **Impact**: `frames_since_creation[frame]` over-advances by one, so `should_force_history_reset` can close one frame early — a one-frame smear/ghost on SVGF and TAA history.
- **Trigger Conditions**: Requires `FrameUpscaler::record` to observe `is_fsr_dispatch_active() == true` while `fsr_frame` is `None`. Both are derived from the same predicate within one `draw_frame` today, so this path is **unreachable as written** — it becomes reachable the moment a second `Err` return is added to `record`, or the jitter gate and record gate stop reading the same predicate.
- **Verification Path**: Not a Vulkan-sync claim — a host-side state-machine claim, verifiable by a unit test that calls `svgf.dispatch` then skips `mark_frame_completed` and asserts `frames_since_creation` did not advance on the next frame. No RenderDoc needed.
- **Related**: #917/REN-D10-NEW-03, #1932/TAA-D13-01, #479.
- **Suggested Fix**: Either make `record_post_passes` infallible by latching the upscaler failure the same way SVGF/TAA/caustic do (`log::error!` + `dispatch_failure`, return `Ok`), or clear `svgf.dispatched_this_frame`/`taa.dispatched_this_frame` on the `draw_frame` error-return path alongside `recreate_image_available_for_frame`. The former is the smaller change and matches convention.

### CHAIN-D2-04: Single shared depth image is now also layout-transitioned by the FSR pass late in the frame
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1168` (`depth_image`, single not per-FIF); `crates/renderer/src/vulkan/frame_upscaler.rs:633-646`
- **Status**: NEW (widening of a pre-existing structural condition)
- **Description**: `depth_image` is a single image shared by all frame-in-flight framebuffers, unlike every color attachment (explicitly per-FIF to remove cross-frame hazards). Historically the only late-frame readers were SSAO/SVGF (same-layout `SHADER_READ`); FSR now additionally performs two **layout transitions** on it per frame. With `MAX_FRAMES_IN_FLIGHT = 2`, the frame-entry fence wait is on `in_flight[frame]` (frame N-1), not frame N, so frame N+1's render pass could begin writing depth while frame N's FSR transition is still executing.
- **Evidence**: `draw.rs:735-738` documents the per-FIF color design explicitly; depth is the one attachment that doesn't follow it.
- **Impact**: A cross-frame WAW/WAR on depth would surface as flickering depth-dependent effects (SSAO shimmer, FSR disocclusion artefacts), not a crash — likely benign given in-order queue execution on current drivers, but unconfirmed.
- **Trigger Conditions**: Frame overlap — any frame where the GPU hasn't finished frame N by the time frame N+1's render pass starts. Normal at high frame rates.
- **Verification Path**: `BYRO_VALIDATION=1` with sync validation, FSR mode, 300+ frames of camera motion. Confirming signal: `SYNC-HAZARD-WRITE-AFTER-READ`/`-WRITE` naming the depth image at render-pass begin. A clean 300-frame run is meaningful evidence of non-issue (sync-val tracks cross-submission hazards via queue-batch tracking).
- **Related**: #1583, commit `d822a783`.
- **Suggested Fix**: If validation fires, make depth per-FIF like every other attachment (`Vec<vk::Image>` indexed by frame). Do not add speculative barriers first.

### CHAIN-D2-05: ReSTIR reservoir ping-pong reads never-initialised device memory on first-use frames
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/restir.rs:52-60,102-131`; `crates/renderer/shaders/triangle.frag:2485-2530`
- **Status**: NEW
- **Description**: `ReservoirBuffers` are allocated with `create_device_local_uninit` and never cleared on creation or resize. The temporal ping-pong therefore reads undefined device memory on each slot's first use and again after every `recreate_on_resize`, relying entirely on shader-side validation (`sameSurface && ... && rp.M > 0.0 && rp.W > 0.0 && !isnan && !isinf`) instead of an explicit clear.
- **Evidence**: The shader gate is genuinely strong, but the surface tag is a masked field (`packReservoirLightAndSurface`) with well under 32 bits of effective comparison width — garbage that happens to match the masked surface ID plus a finite positive W/M and an in-range light index will be accepted.
- **Impact**: At worst a small number of single-frame bright specks on the first frames after launch or a resize — visually indistinguishable from the temporal-discontinuity recovery window already scheduled for those exact frames. Not a correctness cliff. SVGF/TAA (the analogous consumers) do clear their history on init; ReSTIR is the outlier.
- **Trigger Conditions**: Frames 0-1 of a session; frames 0-1 after any resize or runtime upscaler switch.
- **Verification Path**: Add a `vkCmdFillBuffer(0)` on both slots inside `ReservoirBuffers::new`/`recreate_on_resize` behind a debug env var and compare the first two frames' output. If identical, the shader validation is sufficient and this closes as documentation-only.
- **Related**: #1814/PERF-D5-NEW-04, commit `e5d02f83`, `svgf.rs:183-185` (`should_force_history_reset`).
- **Suggested Fix**: A one-time `vkCmdFillBuffer(0)` in `ReservoirBuffers::new` and `recreate_on_resize` (near-free, once per swapchain generation — requires adding `TRANSFER_DST` to buffer usage), or an explicit per-slot `frames_since_creation` gate mirroring SVGF/TAA.

### CHARAL-D3-01: `pool_regen_tick_system` holds a 3-deep nested lock stack whose safety rests on undocumented exclusive scheduling
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `crates/core/src/character/regen.rs:120-150`
- **Status**: NEW
- **Description**: The only new CHARAL system touching `World` builds a hold-stack of three distinct locks by sequential acquisition rather than a TypeId-sorted paired accessor: `PoolRegenConfig` (read) is held through `try_resource_mut::<PoolRegenAccumulator>()`, then through `try_resource::<CharacterRuleset>()`, then through `query_mut::<ActorValues>()` and the per-actor loop. Correct today only because the system is registered `add_exclusive(Stage::Update, ...)` — a dependency living in a different crate and unstated in `regen.rs`. Same finding class as the already-closed #2126 (`SCR-D6-NEW3-03`), whose fix was a documented "nested-lock safety depends on exclusive scheduling" comment that this new code didn't inherit.
- **Evidence**: Held set at the `query_mut::<ActorValues>()` call: `{PoolRegenConfig(R), CharacterRuleset(R), ActorValues(W)}`. Nothing else in the tree records the reverse edge today.
- **Impact**: No live deadlock. The risk is a future maintainer moving this system to the parallel lane or adding a system that acquires `ActorValues` before `CharacterRuleset` — either creates a genuine ABBA only caught under `BYRO_LOCK_ORDER_CHECK=1` or as a production hang.
- **Trigger Conditions**: Only reachable once `PoolRegenConfig` is actually inserted (currently `build_character_ruleset` returns `None` for it, so the system short-circuits). Deadlock additionally requires the scheduler change described above.
- **Related**: #2126 (CLOSED, same finding class), #313, #1410.
- **Suggested Fix**: Preferred — drop `config` early (copy the AVIF ids into locals before the `CharacterRuleset` acquire), reducing the hold-stack from 3 to 2. Alternative — port the #2126 doc block verbatim onto `pool_regen_tick_system`.

### SAVE-D3-02: `SaveCommand::execute` holds `SaveRegistry`+`SaveState` guards across the entire ~30-storage snapshot walk
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/save_io.rs:451-520`
- **Status**: NEW
- **Description**: `execute` acquires `ResourceRead<SaveRegistry>` and `ResourceWrite<SaveState>` and holds both across `validate_world`, `validate_form_ids`, and `save_world` — which between them acquire read locks on ~26 component storages and ~7 resources. This is the widest single-hold edge fan-out in the process, safe today only because `DebugDrainSystem` (the sole executor of console commands) is `add_exclusive` and listener threads never touch `World`. As with CHARAL-D3-01, that invariant is not restated at the call site.
- **Evidence**: `save_io.rs:452,455` — neither guard dropped before `:479`/`:504`. Neither `SaveState` nor `SaveRegistry` is itself a registered save column, so the always-on same-thread tracker won't fire spuriously.
- **Impact**: No live deadlock. Documentation/robustness only — moving command dispatch off the exclusive lane, or adding a parallel system touching `SaveState`, would create a wide cycle surface with no compile-time or test-time guard.
- **Trigger Conditions**: Requires a scheduler change; unreachable today.
- **Related**: #2126, #2017 (ring cursor), #2019 (remap logging).
- **Suggested Fix**: Drop `state` before `save_world` (only `state.dir` and the already-computed slot are needed after validation, both cheaply copied), and add the #2126-style exclusive-scheduling note for the `registry` guard that genuinely must stay alive.

### CONC-D4-NEW-03: ABBA detector coverage is bounded by test reachability — the "long tail" cannot be declared closed
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `crates/core/src/ecs/lock_tracker.rs:194-300`
- **Status**: NEW
- **Description**: The cross-thread ABBA graph only records an edge when a lock is acquired while another is already held on a thread a test actually drives. Code paths with no test coverage contribute zero edges — neither cleared nor flagged. A static workspace-wide scan found 849 distinct ordered lock-acquisition pairs (140 appearing in both directions somewhere in the tree) — mostly false positives since the scan doesn't model guard lifetimes, but it shows the acquisition surface is far wider than what the green test run proves.
- **Evidence**: Concrete uncovered-or-thinly-covered regions with multi-lock functions: `byroredux/src/cell_loader/` (needs real ESM/BSA data), `byroredux/src/render/` collection passes, `byroredux/src/save_io.rs`, `byroredux/src/npc_spawn.rs`, `byroredux/src/scene.rs::setup_scene`, and most of `byroredux/src/commands/` (only 2 of ~10 modules were touched by `b5e38c22`).
- **Impact**: Low today — those paths are predominantly single-threaded (cell/scene load on main thread outside the scheduler; console commands inside the exclusive `DebugDrainSystem`), so an inconsistent order there is latent rather than live. Becomes real the moment any is promoted into the parallel batch or moved to a loader thread. This is the same structural gap that let CONC-D5-01/-02/-03 go undetected.
- **Related**: #313, #1410, `b5e38c22`, CONC-D4-NEW-01, CONC-D5-01/-02/-03.
- **Suggested Fix**: Fixing CONC-D4-NEW-01 (detector on during the live 5-frame bench) is the highest-yield next step, since it covers loader/render/scene paths unit tests cannot reach. Optionally record, in a comment near `global_order`, that clearance is coverage-bounded so a future audit doesn't read a green job as proof of absence.

### RL-D6-03: `set_upscaler_mode` failure is non-fatal at the call site and soft-locks the renderer into permanent frame-skip
- **Severity**: LOW
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:981-1039`, `byroredux/src/app_step.rs:338-341`
- **Status**: NEW
- **Description**: The two `recreate_swapchain` call sites in `main.rs` treat a resize failure as fatal (`log::error!` + `event_loop.exit()`); the new runtime-upscaler-switch call site does not — it logs and returns while the frame loop continues, after `set_upscaler_mode` has already destroyed TAA, rebound composite, mutated `renderer_config.upscaler`, and entered `recreate_swapchain` (which destroys framebuffers up front and only rebuilds them much later). Any `?` in between — including the new `upscaler.recreate(...)?` and `PresentationPipeline::new(...)?`, both before the framebuffer rebuild — leaves `framebuffers.len() == 0`, `self.presentation == None`, and a drained `FrameUpscaler`.
- **Evidence**: `app_step.rs:338-341` — no exit, no rollback, versus `main.rs:721-724`/`:969-972` which do exit. `frame_upscaler.rs:769-780` — on `Err`, the reassignment never runs, so `self` keeps the emptied vectors.
- **Impact**: Not memory-unsafe — the existing `#1211` `framebuffers.is_empty()` guard converts it into a permanent "skip every frame" state rather than a panic. But the window never recovers on its own; only a later `WindowEvent::Resized` re-enters `recreate_swapchain`, and the user sees a frozen window with one log line.
- **Trigger Conditions**: Needs an allocation/SDK failure mid-switch.
- **Related**: `#1211` (the guard that downgrades this from a panic), `#1671`.
- **Suggested Fix**: Either mirror the `main.rs` policy (treat a failed `set_upscaler_mode` as fatal), or have it roll back `renderer_config.upscaler` to `previous` and retry `recreate_swapchain` once so the engine lands in a renderable state instead of a permanent frame-skip.

### RL-D6-04: One-time command buffer still leaked on two `?` paths in `with_one_time_commands_inner`; blast radius widened by new FSR callers
- **Severity**: LOW
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/texture.rs:662-696`
- **Status**: Existing: #1861 (narrowed, not closed)
- **Description**: #1861's fix covered the post-submit failure paths (`reset_fences`, `create_fence`, `queue_submit`, `wait_for_fences` — all now free the command buffer + destroy the fence). Two `?` sites still leak the allocated command buffer: `begin_command_buffer(...)?` and `end_command_buffer(...)?`. Neither frees `cmd`.
- **Evidence**: `texture.rs:666-668` and `:693-695` are the only remaining early returns between allocation and the #1861-annotated cleanup block; the recording-closure failure path is correctly handled.
- **Impact**: Unchanged magnitude from #1861 — bounded by how many one-time submits fail, both under device-loss/OOM where the process is already doomed. What changed: the FSR work added `FrameUpscaler::initialize_outputs` and `ExposureResource::initialize` as new callers, both re-entered on **every swapchain recreate** (not load-time-only anymore), which is why this stays open rather than closing as "one-shot init only."
- **Related**: #1861 (OPEN, LOW) — not a regression, strictly improved from 3 sites to 2, but its "load-time one-shot" framing is now stale.
- **Suggested Fix**: Free `cmd` on both `?` paths (same two-line shape already used at `:683-684`), and amend #1861's description to note the per-resize FSR/exposure callers.

### RL-D6-05: `FrameUpscaler` teardown (including the allocator-independent FSR SDK context) sits entirely inside the `Some(allocator)` guard
- **Severity**: LOW
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:3295-3299`, `crates/renderer/src/vulkan/frame_upscaler.rs:788-805`
- **Status**: NEW
- **Description**: `FrameUpscaler` is a mixed subsystem: its per-FIF output images need the gpu-allocator, but its `fsr3::Context` (SDK-side pipelines, descriptor pools, its own `VkDeviceMemory` outside gpu-allocator's view) does not. `destroy` calls `self.context.take()` first, but the whole call sits inside the `if let Some(ref alloc) = self.allocator` guard — so on any future allocator-`None` Drop path the SDK context would be dropped after `vkDestroyDevice` (or not at all), the exact failure mode #1483 was filed against.
- **Evidence**: `mod.rs:3169-3208` documents the #1483 rule and its exception list (only `skin_compute` is currently exempted, for descriptor-pool ordering reasons); `self.allocator` is only ever `take()`n inside Drop itself today, so this is latent, not live.
- **Impact**: None today. Becomes a driver-level use-after-free the moment an allocator-`None` Drop path is reintroduced — which #1426/#1483 show has happened before.
- **Related**: #1483, #1426, #665.
- **Suggested Fix**: Split `FrameUpscaler::destroy` into `destroy_device_objects(&device)` (SDK context) and `destroy_allocations(&device, &alloc)` (output images/views), hoisting the first into the allocator-independent block next to `presentation.destroy()`; or add the exception to the ordering comment at `mod.rs:3184-3188` so a future reader knows it was considered.

---

## Findings — Dimension 7 (clean)

No findings. Streaming Drop ordering (#1167), worker↔main data flow (#2111 re-verified), debug-server command queue isolation, allocator sharing discipline, `Send + Sync` bounds, the new `SettingsRegistry` resource, and the new runtime-upscaler-switch path (console command + settings panel, both staging through `PendingUpscalerSwitch` and applied once per frame on the main thread) all verify clean on this sweep.

---

## Prioritized Fix Order

1. **CONC-D5-02** (HIGH) — `pull_dynamic` / `character_controller_system` `Transform`↔`RapierHandles` inversion. Highest-risk of the two HIGH findings (write-lock involved on both sides); fix is a two-line `drop()` reorder in `sync.rs`.
2. **CONC-D5-01** (HIGH) — `PhysicsWorld`↔`GlobalTransform` inversion across `follow.rs`/`escort.rs`/`travel.rs`/`guard.rs`. Four call sites, same shape; hoist the `GlobalTransform` reads above the `PhysicsWorld` acquisition in each.
3. **CONC-D4-NEW-01** (MEDIUM) — Add `BYRO_LOCK_ORDER_CHECK=1` to the `vulkan-validation` CI job. This is the structural fix that would have caught #1 and #2 automatically, and is a one-line env addition — do this alongside or immediately after the two HIGH fixes, then re-run CI to confirm the detector now fires (before the code fix) and stays quiet (after).
4. **CONC-D5-03** (MEDIUM) — `dump_awake_fallers` `PhysicsWorld`↔`RapierHandles` inversion. Same class as #1/#2, lower likelihood; fix alongside them while the file is open.
5. **CONC-D4-NEW-02** (MEDIUM) — Fix the `vulkan-validation` job's `|| true` + substring-match masking so a tripped boot-time `debug_assert` actually fails CI. Do this together with #3 since both touch the same CI job.
6. **RL-D6-01 / RL-D6-02** (MEDIUM) — SSAO / water-caustic resize-failure descriptor dangling. Same shape, same fix pattern (placeholder image + rebind on `Err`); worth doing as one pass over both.
7. **CHAIN-D2-02 / CHAIN-D2-03** (MEDIUM, HYPOTHESIS) — Run `BYRO_VALIDATION=1` for ~200-300 FSR frames to either close both as non-issues or get a concrete VUID/hazard to act on. No code change until then.
8. **LOW findings** — No urgency; batch opportunistically. Reasonable groupings: the three CHAIN-D2 renderer LOWs (-01, -04, -05) with the fix from #7's validation run; the two ECS "undocumented exclusive-scheduling" LOWs (CHARAL-D3-01, SAVE-D3-02) as a single doc-comment pass following the #2126 convention; the three RL-D6 renderer LOWs (-03, -04, -05) opportunistically alongside other renderer work; CONC-D1's three LOWs need validation-layer runs (same session as #7) before any action; CONC-D4-NEW-03 is a standing note, not an actionable fix, and resolves itself once #3 lands.

---

## Suggest

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-25.md
```
