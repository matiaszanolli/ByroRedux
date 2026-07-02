# Concurrency & Synchronization Audit — 2026-07-01

**Scope**: Vulkan queue/AS sync, compute→AS→fragment chains, ECS lock ordering,
scheduler access declarations, physics RwLock patterns, GPU resource lifecycle,
worker threads. All 7 dimensions run at `--depth deep`.

**Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-06-23.md` (all 7 dimensions
clean; the one open LOW item, CONC-D1-01/#1713, was fixed by commit `806ba7af`
before this audit started and is re-verified fixed below).

**Verdict**: One CRITICAL finding survived adversarial review — a real
use-after-free class bug in the shared BLAS build-scratch buffer, newly
exposed by this audit window's scrutiny rather than by a code regression (the
bug's precondition, per-frame skinned-BLAS refit sharing the scratch buffer
with fence-waited one-time builds, has existed since M29; the closed fixes
#1449/#495/#644 hardened *adjacent* parts of this same buffer's lifecycle but
never covered the immediate-destroy-on-grow/shrink paths). One MEDIUM
(decoupled skin-pipeline init-failure gate). Seven LOW findings, all
declaration/documentation drift or latent hazards gated on code that doesn't
exist yet. Two dimensions (5, 7) are fully clean with zero findings.

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 7 |
| **Total**| **9** |

Dedup baseline: `/tmp/audit/concurrency/issues.json` (21 OPEN). No open issue
covers any finding below. Two additional Dimension-3 observations
(poison-handling nits) are dedup'd against today's companion ECS audit and are
not double-counted here — see the note under Dimension 3.

---

## Findings

### CONC-D1-01: Shared BLAS build-scratch buffer destroyed host-side while an in-flight frame's skinned-BLAS refits still reference its device address

- **Severity**: CRITICAL
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:686-698` (`build_blas_batched` grow path: `old.destroy(device, allocator)` at 689-691)
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:285-296` (`build_blas` single-shot grow path, same shape)
  - `crates/renderer/src/vulkan/acceleration/memory.rs:42-105` (`shrink_blas_scratch_to_fit`: immediate `old.destroy` at 65-66 and 82-84; **stale SAFETY doc** at 33-41)
  - Exposed call sites: `byroredux/src/cell_loader/unload.rs:122-136` (unload-time shrink, stale SAFETY comment "builds run synchronously through fenced one-time command buffers"), `byroredux/src/cell_loader/exterior.rs:349` and `byroredux/src/cell_loader/spawn.rs:1179` (streaming-time `ctx.build_blas_batched`), reached from `App::step_streaming` (`byroredux/src/main.rs:1183`), which is invoked from `about_to_wait` at `byroredux/src/main.rs:2343` — **before** `render_one_frame`/`draw_frame` at `main.rs:2397` in the same tick, with no `device_wait_idle` between them.
- **Status**: NEW (sibling of closed #1449 / commit `a476b256` — that fix rerouted `BlasEntry` destruction through `pending_destroy_blas` in this exact timing window but did NOT cover the shared scratch buffer; the "safe by construction" premise dates to #495/#644 and was invalidated by M29 refits + #911 first-sight builds living on the per-frame cmd)
- **Description**: `AccelerationManager::blas_scratch_buffer` is shared by three writers: cell-load `build_blas`/`build_blas_batched` (one-time submissions, host fence-waited), per-frame `build_skinned_blas_batched_on_cmd` (first-sight builds recorded into the frame command buffer, #911), and per-frame `refit_skinned_blas` (recorded into the frame command buffer every pose-dirty frame; the scratch device address is captured at record time, `blas_skinned.rs:484-530`). `draw_frame` waits both in-flight fences at the TOP of the frame, so during recording no frame is in flight — but after `queue_submit` returns, the just-submitted frame N executes asynchronously on the GPU. Cell streaming (`step_streaming`, and the worldspace-transition drain in `streaming_helpers.rs`) runs in `about_to_wait` **between** `draw_frame` calls — i.e. before the next frame's fence wait, while frame N may still be executing on the GPU. Verified directly against current code: `main.rs:2343` (`step_streaming`) runs strictly before `main.rs:2397` (`render_one_frame`) in the same `about_to_wait` tick, with no `device_wait_idle` call anywhere in that span. In this window:
  1. `build_blas_batched` Phase 2 growth (`scratch_needs_growth` true — a new cell's mesh exceeds the session's scratch high-water mark) takes and **immediately destroys** the old scratch buffer (`vkDestroyBuffer` + `gpu-allocator` free, no deferral).
  2. `unload_cell` → `shrink_blas_scratch_to_fit` does the same on the shrink/drop paths.
  If frame N recorded any skinned-BLAS refit or first-sight build (any animating NPC on screen — the steady-state case in populated cells), its `cmd_build_acceleration_structures` calls reference the destroyed buffer's device address as build scratch (read+write working memory). The host-side free is ordered by nothing: for a dedicated allocation (likely for the 80–200 MB scratch sizes this code explicitly anticipates, per closed issue #495) the underlying `VkDeviceMemory` is freed to the driver → GPU page fault → `VK_ERROR_DEVICE_LOST`; for a sub-allocation the range returns to the pool and a later allocation in the same streaming tick (compacted BLAS buffers, mesh/texture upload targets written at TRANSFER stage, which is NOT in the second sync scope of frame N's trailing `AS_BUILD→AS_BUILD` barrier) can be recycled onto it and race frame N's in-flight scratch access → silent memory corruption.
- **Evidence** (grow path; the shrink path is identical in shape):
  ```rust
  // blas_static.rs:688-698 — build_blas_batched Phase 2, runs from step_streaming
  // in about_to_wait while the last draw_frame submission may still be executing
  if need_new_scratch {
      if let Some(mut old) = self.blas_scratch_buffer.take() {
          old.destroy(device, allocator);          // <-- immediate vkDestroyBuffer + free
      }
      self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit( ... )?);
  }
  ```
  ```rust
  // memory.rs:33-39 — the SAFETY premise this relies on, now stale:
  /// - Caller must guarantee no BLAS build command buffer is
  ///   currently referencing `blas_scratch_buffer`. The two build
  ///   paths use one-time command buffers with synchronous fence
  ///   waits, so any call site that is NOT inside a BLAS build is
  ///   safe by construction.
  ```
  The premise is false since M29/#911: `refit_skinned_blas` (`blas_skinned.rs:484-530`) and `build_skinned_blas_batched_on_cmd` (`blas_skinned.rs:246-255`) capture the scratch device address into the **per-frame** command buffer, which is in flight whenever streaming runs. The engine has already codified the sibling rule (`predicates.rs:452-476`, #1140): the host fence-wait between submissions does not excuse device-side lifetime/ordering obligations — and #1449 was a real, observed device-loss from exactly this window on the `BlasEntry` objects (the scratch buffer was simply not covered by that fix's scope).
- **Impact**: GPU use-after-free — device loss (`VK_ERROR_DEVICE_LOST`, matching the #1449 "crash near water"-era signature) or silent BLAS/scene corruption. Impact class per the severity scale: use-after-free = CRITICAL.
- **Trigger Conditions** (all must coincide — modest frequency, catastrophic effect):
  1. At least one skinned entity refit/built in the most recently submitted frame (any animating NPC visible — near-always true in populated cells);
  2. `step_streaming` (or worldspace-transition unload) runs before the next `draw_frame` fence wait — the normal M40 loop shape, confirmed above;
  3. (grow arm) the incoming cell contains a mesh whose `build_scratch_size` exceeds the session high-water mark, OR (shrink arm) `unload_cell` drops the peak-scratch mesh and `scratch_should_shrink` fires / all BLAS die (`peak == 0` drop-entirely arm).
- **Verification Path**: Standard sync-validation will NOT catch this directly — the frame cmd references the scratch only via `VkDeviceAddress`, which object-lifetime validation cannot track, and sync-validation reasons per-submission, not across the host-side free boundary. Concrete confirming signals, in order of cost:
  1. Code-level: the invariant chain is fully documented in-repo (#1449's timing-window comment, `blas_skinned.rs`'s per-frame scratch capture, `memory.rs`'s now-stale premise) and independently confirmed against current code in this audit (`main.rs:2343` vs `main.rs:2397`, no intervening wait) — this is a lifetime bug provable from the code, not a speculative barrier judgment.
  2. Runtime repro: stream exterior cells with NPCs in view while artificially lowering the initial scratch size (force growth on the first streamed cell) — expect `VK_ERROR_DEVICE_LOST` within a few transitions on a dedicated-allocation driver, or GPU-AV (`VK_LAYER_KHRONOS_validation` with GPU-assisted + buffer-device-address checking, `BYRO_VALIDATION=gpuav`) reporting an OOB/invalid-address access at `vkCmdBuildAccelerationStructuresKHR`.
- **Suggested Fix**: Route retired scratch buffers through the existing deferred-destroy machinery instead of `old.destroy(...)`: add a `DeferredDestroyQueue<GpuBuffer>` (`deferred_destroy.rs` already generic, `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`) on `AccelerationManager`, push the old buffer at all three sites (`build_blas`, `build_blas_batched`, `shrink_blas_scratch_to_fit`), drain it in `tick_deferred_destroy` (already called post-fence-wait in `draw_frame`) and in `drain_pending_destroys` for shutdown (#732 parity). This is CPU-side lifetime management (unit-testable, not a barrier/stage change), so the speculative-Vulkan-fix guardrail does not block it; update the stale SAFETY docs in `memory.rs` and `byroredux/src/cell_loader/unload.rs` in the same change. **Note**: the identical grow-destroy in `build_skinned_blas_batched_on_cmd` (`blas_skinned.rs:212-214`) is SAFE as-is — it runs during `draw_frame` recording, after the both-slot fence wait, when provably nothing is in flight — worth a comment distinguishing it so the fix isn't blindly copied there. `resize.rs`'s own call to `shrink_blas_scratch_to_fit` (`context/resize.rs:48-54`, inside `recreate_swapchain_core`) is likewise already safe (runs immediately after `device_wait_idle` at the top of that function) — the fix must not disturb that path's behavior, only the three unguarded call sites above.
- **Related**: Independently re-derived by two agents from opposite angles (Dimension 1's queue/AS-sync sweep and Dimension 6's destroy-ordering sweep landed on the same three call sites and agree). Sibling of closed #1449 (`BlasEntry` deferred-destroy), #495 (scratch never shrinks — this finding's shrink arm is the shrink path #495 introduced), and #644 (a *different*, already-fixed hazard: missing intra-frame barrier between a sync BUILD and a per-frame refit — not the same bug as this host-side-free UAF).

---

### CONC-D2-01: `skin_palette` init failure is not coupled to `skin_compute` — skin chain can run against a never-populated (uninitialised) palette SSBO

- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1788-1829` (independent init gates), `crates/renderer/src/vulkan/context/draw.rs:1440` (refit gated on `skin_compute` + `accel_manager` only — confirmed: the tuple does not include `skin_palette`), `crates/renderer/src/vulkan/context/draw.rs:2694` (palette dispatch gated on `skin_palette` only), `crates/renderer/src/vulkan/scene_buffer/buffers.rs:457` (palette buffers are `create_device_local_uninit`)
- **Status**: NEW (latent since M29.5; not a regression of the audit-window commits — surfaced while verifying checklist item 1 chain integrity)
- **Description**: `SkinComputePipeline` and `SkinPaletteComputePipeline` are created by two independent `match … Ok/Err → Some/None` blocks, each of which degrades to `None` on failure with only a `log::warn!`. If `skin_palette` creation fails while `skin_compute` succeeds (partial init failure, e.g. mid-init OOM or pipeline-cache corruption), the per-frame chain still runs its downstream links: `record_skinned_blas_refit` (`draw.rs:1440` checks only `skin_compute`/`accel_manager`) dispatches `skin_vertices.comp`, which reads the bone-palette SSBO (`bone_buffers()[frame]`) that no GPU pass ever wrote — the buffer is allocated `create_device_local_uninit` (`buffers.rs:457`) and the sole producer (the palette dispatch at `draw.rs:2694`) is gated out. `triangle.vert`'s inline-skinning path (set 1 binding 3, `triangle.vert:152-155`) reads the same unwritten palette. The `mod.rs:1808-1815` comment acknowledges "downstream `skin_palette.is_some()` checks skip the dispatch (no CPU-multiply fallback exists)" but the paired consumer gates were never coupled.
- **Evidence**:
  ```rust
  // mod.rs:1788 — independent gate 1
  let skin_compute = if device_caps.ray_query_supported {
      match super::skin_compute::SkinComputePipeline::new(... ) { Ok(sc) => Some(sc), Err(e) => { log::warn!(...); None } }
  } else { None };
  // mod.rs:1816 — independent gate 2 (failure here does NOT clear skin_compute)
  let skin_palette = if device_caps.ray_query_supported {
      match super::skin_compute::SkinPaletteComputePipeline::new(&device, pipeline_cache) { Ok(sp) => Some(sp), Err(e) => { log::warn!(...); None } }
  } else { None };
  // draw.rs:1440 — refit chain checks only skin_compute + accel:
  if let (Some(skin_pipeline), Some(ref mut accel)) =
      (self.skin_compute.as_ref(), self.accel_manager.as_mut())
  // buffers.rs:457 — palette contents undefined until first palette dispatch:
  bone_device_buffers.push(GpuBuffer::create_device_local_uninit(
  ```
- **Impact**: Garbage (undefined-memory) bone matrices → garbage skinned vertices written by `skin_vertices.comp` → per-entity BLAS built/refit over garbage geometry (potentially NaN/huge AABBs degrading TLAS traversal) → garbage skinned silhouettes in RT shadows/reflections/GI, plus garbage rasterized skinned meshes via the inline path. No memory corruption / UAF (all accesses stay in-bounds of allocated buffers); impact class is broken-geometry visual artifact for every skinned entity.
- **Trigger Conditions**: `SkinPaletteComputePipeline::new` fails while `SkinComputePipeline::new` succeeds on an RT-capable device — requires a partial init failure (both do near-identical work, so this is rare), then any skinned draw (`bone_offset != 0`).
- **Verification Path**: Inject an `Err` return into `SkinPaletteComputePipeline::new` (or force its `create_compute_pipelines` to fail), run the M41 equip smoke test (`docs/smoke-tests/m41-equip.sh`); observe garbage/collapsed NPC geometry in raster + RT and (with sync-validation off) no barrier complaints — confirming the chain ran against unwritten memory. No barrier/stage change involved, so the speculative-fix guardrail does not apply — the fix is host-side init coupling, testable via `cargo test` with a fault-injection seam.
- **Suggested Fix**: Couple the gates: after the `skin_palette` match, if `skin_palette.is_none()` force `skin_compute = None` (the reverse coupling is unnecessary — palette-only is harmless). Alternatively gate `record_skinned_blas_refit` and the palette dispatch on the SAME `skin_compute.is_some() && skin_palette.is_some()` predicate. One-line coupling + a note in the `mod.rs` comment.

---

### CONC-D3-01: `World` accessor docs claim the same-thread lock tracker is "debug only / release no-op" — it is compiled and active in release builds

- **Severity**: LOW
- **Dimension**: ECS Lock Ordering
- **Location**: `crates/core/src/ecs/world.rs:374-380` (and the "debug only" panic headers on `query_mut`:395-398, `get`:271-275, `has`:311-314, `count`:321-322, `try_resource`:687-691, `try_resource_mut`:705-707) vs. `crates/core/src/ecs/lock_tracker.rs:7-12`
- **Status**: NEW (doc rot)
- **Description**: `world.rs:378-380` states: *"Release builds do not enforce the check (production hot paths get a zero-cost no-op)."* This is false. `track_read` / `track_write` (`lock_tracker.rs:58-137`) carry no `cfg(debug_assertions)` gate; only the `held_others` Vec + `global_order::record_and_check` block (`lock_tracker.rs:83-93, 123-135`) and the graph module (`lock_tracker.rs:194`) are debug-only. `TrackedRead::new` / `TrackedWrite::new` are called unconditionally from every `&self` acquisition site in `world.rs` (e.g. 384, 402, 445-446, 507-508, 580, 605). The module doc has it right: *"Thread-local check (always on — debug and release builds)"* (`lock_tracker.rs:9`).
- **Evidence**:
  ```rust
  // world.rs:377-380 (query::<T> doc)
  /// Drop the offending guard before calling. Release builds do
  /// not enforce the check (production hot paths get a zero-cost
  /// no-op).
  ```
  vs. `lock_tracker.rs:99-137` — `track_write` panics on conflict with no cfg gate, and the #823 fix comment (`lock_tracker.rs:80-82`) explicitly discusses the *release-build* per-frame cost of this function, confirming it runs in release.
- **Impact**: Documentation-only, but it points the wrong way on two operational facts: (a) a same-thread write-conflict acquisition **panics in release too** (which is good — it converts a silent `std::sync::RwLock` deadlock into a diagnosable crash), and (b) the release hot path pays a thread-local HashMap probe per acquisition, not zero. Someone tuning release hot paths or triaging a release-build panic from this message would be misled.
- **Trigger Conditions**: None at runtime; misleads maintainers.
- **Verification Path**: `cargo test --release -p byroredux-core` — `lock_tracker::tests::write_then_write_same_type_panics` passes in release, proving the check is live there.
- **Suggested Fix**: Rewrite the `# Panics (debug only)` headers: the thread-local re-entrancy check is always-on; only the cross-thread ABBA graph is debug-only + `BYRO_LOCK_ORDER_CHECK`-gated. Delete the "zero-cost no-op" sentence.

### CONC-D3-02: `animation_system` access declaration omits three color-sink component writes (`AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedShaderColor`)

- **Severity**: LOW (latent — animation is the *only* parallel system in `Stage::Update`, so no conflicting pair can exist today)
- **Dimension**: ECS Lock Ordering (declaration drift weakening the analyzer that upholds the no-ABBA/no-conflict invariant; overlaps Dimension 4)
- **Location**: `byroredux/src/main.rs:783-813` (declaration) vs. `byroredux/src/systems/animation.rs:150-172` (writes)
- **Status**: NEW
- **Description**: `apply_color_channels` lazily takes `world.query_mut::<AnimatedAmbientColor>()` (`animation.rs:154`), `AnimatedSpecularColor` (:156-162), and `AnimatedShaderColor` (:170-172) for `ColorTarget::Ambient/Specular/ShaderColor` channels (reachable from NiMaterialColorController-style clips; the post-#517 per-target routing). The `add_to_with_access` declaration at `main.rs:791-812` declares `AnimatedDiffuseColor` and `AnimatedEmissiveColor` writes but none of the other three, despite the comment "The declaration is the UNION across all paths" (`main.rs:787-790`).
- **Evidence**: `grep AnimatedAmbientColor\|AnimatedSpecularColor\|AnimatedShaderColor byroredux/src/main.rs` → zero hits; `animation.rs:153-155` `write_lazy!(ambient_q, AnimatedAmbientColor, …)` expands to `world.query_mut::<AnimatedAmbientColor>()`.
- **Impact**: The scheduler's conflict analyzer (and the `#1394`/`#1602` `debug_assert_eq!` guards at `main.rs:1013-1029`) trust declarations. A future system added to the Update parallel batch that touches any of the three storages would be co-scheduled with animation as "no conflict", opening a genuine cross-thread write-write / ABBA window that none of the startup asserts can see.
- **Trigger Conditions**: Requires a future parallel Update-stage system touching these types — code that doesn't exist yet → latent.
- **Verification Path**: `BYRO_LOCK_ORDER_CHECK=1` runs won't catch it (declaration-level, not acquisition-level). Catchable by a (not-yet-existing) declaration-vs-acquisition audit; until then, code review only.
- **Suggested Fix**: Add `.writes::<AnimatedAmbientColor>() .writes::<AnimatedSpecularColor>() .writes::<AnimatedShaderColor>()` to the animation declaration in `main.rs`.

### CONC-D3-03: Undeclared `ContactConfig` resource reads in `player_controller_system` and `physics_sync_system`

- **Severity**: LOW (latent — `ContactConfig` has no runtime writer; inserted once at startup, `main.rs:548`)
- **Dimension**: ECS Lock Ordering (declaration drift; overlaps Dimension 4/5)
- **Location**: `byroredux/src/systems/character.rs:230-233` and `crates/physics/src/sync.rs:371` vs. declarations at `byroredux/src/main.rs:655-670` and `main.rs:887-908`
- **Status**: NEW — same underlying gap independently confirmed by Dimension 4 as **CONC-D4-01** (part a), which additionally identifies three more undeclared reads (`RenderLayer`, `FormIdComponent`, `FormIdPool`) in the env-gated `#1698` faller-diagnostic added by commit `0a0bf640`. Reported once, consolidated under CONC-D4-01 below to avoid double-counting; this entry is retained for cross-reference from the Dimension 3 sweep.
- **Description**: The character-controller path snapshots `world.try_resource::<byroredux_physics::ContactConfig>()` per tick (`character.rs:230-233`); `physics_sync_system` does the same (`sync.rs:371`). Neither system's `Access` declaration includes `reads_resource::<ContactConfig>()` (player_controller declares PlayerMode/PlayerEntity/ActiveCamera/InputState/PhysicsWorld/CharacterController/RapierHandles/Transform only; physics_sync declares PhysicsWorld/PhysicsWaterConstants/components).
- **Impact**: Same class as CONC-D3-02 — a future parallel same-stage system that *writes* `ContactConfig` (e.g. a live-tuning system for the KCC offset) would be co-scheduled as conflict-free, creating a read/write race and a potential lock-order edge invisible to the analyzer.
- **Trigger Conditions**: Requires a future runtime `ContactConfig` writer in `Stage::Early` or `Stage::Physics` — latent.
- **Suggested Fix**: See CONC-D4-01 (consolidated fix: append `.reads_resource::<byroredux_physics::ContactConfig>()` — plus the three additional types D4 identified — to both/all affected declarations).

### CONC-D3-04: `CommandRegistry` read guard is held across arbitrary command execution; `help` re-enters the same lock

- **Severity**: LOW (latent — safe today: same-thread read-read, no runtime writer of `CommandRegistry` exists)
- **Dimension**: ECS Lock Ordering
- **Location**: dispatchers `crates/debug-server/src/evaluator.rs:413-417`, `byroredux/src/main.rs:268-269`, `byroredux/src/main.rs:2688-2689`; re-entry `byroredux/src/commands/world_info.rs:17`
- **Status**: NEW
- **Description**: All three command dispatch sites hold a `ResourceRead<CommandRegistry>` while calling `reg.execute(world, expr)` (structurally unavoidable — the registry owns the boxed `ConsoleCommand` objects, `crates/core/src/console.rs:87`). Every command body therefore runs with a live read guard on the `CommandRegistry` RwLock. `HelpCommand::execute` re-acquires it read-only (`world.resource::<CommandRegistry>()`, `world_info.rs:17`). The always-on thread-local tracker permits read-read (`track_read` only counts, `lock_tracker.rs:74`), and no runtime writer exists, so this is currently benign.
- **Evidence**:
  ```rust
  // evaluator.rs:413-415
  if let Some(reg) = world.try_resource::<CommandRegistry>() {
      if reg.list().iter().any(|(name, _)| *name == first_word) {
          let output = reg.execute(world, expr);   // guard `reg` held across execution
  ```
  All commands run on the main thread (`DebugDrainSystem` is `add_exclusive(Stage::Late, …)`, `crates/debug-server/src/lib.rs:33`; drain releases its queue guard before evaluating, `system.rs:128-142`).
- **Impact**: Two latent failure modes, both gated on code that doesn't exist yet: (a) any future command that takes `resource_mut::<CommandRegistry>()` (e.g. runtime alias registration) panics via the always-on tracker (release included); (b) if a *cross-thread* writer ever queues on the lock between the dispatcher's read and `help`'s re-entrant read, `std::sync::RwLock` may deadlock (re-entrant read under a queued writer is platform-dependent) — a window the read-count-based tracker cannot flag.
- **Trigger Conditions**: Future runtime `CommandRegistry` writer, or a command acquiring the registry mutably.
- **Verification Path**: `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace` records `CommandRegistry → X` edges from command bodies and would flag a reverse edge if a writer path appears; the write-under-read case panics via the thread-local tracker at the offending line.
- **Suggested Fix**: Document the contract on `ConsoleCommand::execute` ("runs under a read guard on `CommandRegistry` — commands must never acquire it mutably"); optionally have `HelpCommand` receive the listing via the dispatcher instead of re-locking.

> **Dedup note**: Dimension 3 additionally re-confirmed two poison-handling
> nits (`clear_entities`'s nameless panic message, `world.rs:227-233`, and
> `insert_resource`'s silent `.ok()` swallow of a poisoned *prior* value,
> `world.rs:542-552`) that are already filed by today's companion ECS audit
> as **ECS-2026-07-01-02** and **ECS-2026-07-01-03**. Not re-counted in this
> report's totals.

---

### CONC-D4-01: `physics_sync_system` under-declares its read surface (`ContactConfig` + the #1698 faller-dump reads)

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `crates/physics/src/sync.rs:226-244` and `crates/physics/src/sync.rs:371` (body) vs `byroredux/src/main.rs:887-908` (declaration)
- **Status**: NEW (part (b) landed 2026-06-25 inside the regression window; part (a) pre-existing since 2026-05-22, missed by the prior clean pass — see also CONC-D3-03, consolidated here)
- **Description**: `physics_sync_system` is registered in the Stage::Physics parallel batch via `add_to_with_access` with a declared surface (`main.rs:890-907`) that omits four accesses actually performed by its body:
  - (a) **`ContactConfig` resource read** — `world.try_resource::<ContactConfig>()` in `register_newcomers` (`sync.rs:371`; present since commit `525c690c`, 2026-05-22).
  - (b) **`RenderLayer` (component read), `FormIdComponent` (component read), `FormIdPool` (resource read)** — the #1698 awake-faller diagnostic `dump_awake_fallers` (`sync.rs:242-244`), reachable from the system body at `sync.rs:169-171`, gated behind the `BYRO_PROFILE_FALLERS` env var and a one-shot `AtomicBool` (`sync.rs:183, 231`).
- **Evidence** (declaration vs actual body access, side by side):

  | Declared (main.rs:890-907) | Actual body (sync.rs + water.rs helpers) |
  |---|---|
  | PhysicsWorld r+w | PhysicsWorld r+w (sync.rs:137, 151, 226, 375, 508, 565; water.rs:215, 284) ✓ |
  | PhysicsWaterConstants r | read in buoyancy phase ✓ |
  | CollisionShape r, RigidBodyData r, GlobalTransform r | sync.rs:320-331, 501-504, 557 ✓ |
  | RapierHandles r+w | r sync.rs:237, 498, 554; w (query_mut) sync.rs:479-490 ✓ |
  | Transform w | sync.rs:587-595 ✓ |
  | WaterPlane/WaterVolume/WaterFlow r, WaterContact w | water.rs:142-145, 242, 408 ✓ |
  | — (not declared) | **ContactConfig r** — sync.rs:371 ✗ |
  | — (not declared) | **RenderLayer r** — sync.rs:242 ✗ (env-gated) |
  | — (not declared) | **FormIdComponent r** — sync.rs:243 ✗ (env-gated) |
  | — (not declared) | **FormIdPool r** — sync.rs:244 ✗ (env-gated) |

- **Impact**: No live hazard today — `physics_sync_system` is the **only** system registered in Stage::Physics (`main.rs:887` is the sole `Stage::Physics` registration), so it pairs against nothing in `access_report()` and the missing entries are all read-side. The latent hazard is the "silently defeats the analyzer" class: if a future system is added to the Stage::Physics parallel batch that *writes* `RenderLayer` / `FormIdComponent` / `FormIdPool` / `ContactConfig`, `analyze_pair` will return `None` (both sides declared, no visible overlap) instead of flagging a read/write conflict, and neither startup `debug_assert` can catch it — the `#1394`/`#1602` guards only detect *undeclared* systems and *declared* conflicts, not declared-but-incomplete surfaces. Worst realized outcome would be a torn read inside a one-shot diagnostic log dump — hence LOW even scoring impact-not-likelihood.
- **Trigger Conditions**: Requires BOTH a second parallel Stage::Physics system that writes one of the four types AND (for the part-(b) reads) `BYRO_PROFILE_FALLERS` set with ≥16 awake dynamic bodies. Not triggerable in the current schedule.
- **Verification Path**: `sys.accesses` shows `physics_sync_system`'s declared row without the four types; `grep -n "ContactConfig\|RenderLayer\|FormIdComponent\|FormIdPool" crates/physics/src/sync.rs` vs `main.rs:890-907`. Startup asserts stay green (they cannot see this class).
- **Suggested Fix**: Append `.reads_resource::<byroredux_physics::ContactConfig>()`, `.reads::<RenderLayer>()`, `.reads::<FormIdComponent>()`, `.reads_resource::<FormIdPool>()` to the registration at `main.rs:890-907`, matching the existing "declaration completeness; read-only" convention already used there for `PhysicsWaterConstants` (`main.rs:893-895`).
- **Note**: The `dump_awake_fallers` diagnostic exists *for* open perf issue #1698; this finding is about its undeclared access surface only, not a scheduling-serialization contribution to #1698.

### CONC-D4-02: `DebugDrainSystem` is registered after the access-report / `SystemList` snapshot — omitted from `sys.accesses` and `systems` output

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/main.rs:1071` (snapshot) vs `byroredux/src/main.rs:1083` (registration); `crates/debug-server/src/lib.rs:33`
- **Status**: NEW (pre-existing behaviour — identical ordering at the 2026-06-23 base — but unreported; NOT a regression of the #1670 `App::new` split, which preserved the order verbatim)
- **Description**: `App::new` (`main.rs:1067`) builds the scheduler (1070), then calls `install_runtime_registries` (1071) which snapshots `scheduler.access_report()` (`main.rs:1012`) and `scheduler.system_names()` (`main.rs:1038-1043`) into the `SchedulerAccessReport` and `SystemList` resources. Only afterwards does `byroredux_debug_server::start(&mut scheduler, …)` (`main.rs:1083`) add `DebugDrainSystem` via `add_exclusive(Stage::Late, drain_system)` (`debug-server/src/lib.rs:33`). The drain system therefore never appears in the `sys.accesses` report rows or the `systems` console listing.
- **Impact**: Introspection completeness only. `DebugDrainSystem` is exclusive, so it is never paired by the analyzer (`scheduler.rs:556-568` pairs only `data.parallel`) and the three startup `debug_assert`s are unaffected (it did not exist when they ran, and exclusive+undeclared is permitted by design, #1237).
- **Trigger Conditions**: Always — every debug-mode launch; operator running `sys.accesses` or `systems` sees a schedule missing one Late-stage exclusive entry.
- **Verification Path**: `cargo run -- --bench-hold` + `byro-dbg` → `systems` / `sys.accesses`; count Late-stage exclusive rows vs `Scheduler::system_names()` after `start()`.
- **Suggested Fix**: Either move the `SchedulerAccessReport`/`SystemList` snapshot after `debug_server::start()` (registration order permitting), or have `sys.accesses` note "+ debug-server drain (registered post-snapshot)". Cosmetic; fine to fold into the next introspection touch-up.

---

### CONC-D6-01: Stale `context/mod.rs` line-number citations in `acceleration/mod.rs::destroy()` comments

- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs:251-252,292-293`
- **Status**: NEW
- **Description**: `AccelerationManager::destroy()`'s doc comments cite `context/mod.rs:1300`, `context/mod.rs:1859`, and `context/mod.rs:2093` as the locations of the `device_wait_idle()` calls that make the immediate (non-deferred) destroys in this function safe. Those line numbers predate the #1670/#1671 (`0409b6d6`) and #1749 (`26439046`) refactors that grew `mod.rs` and moved code around; the actual `device_wait_idle()` calls in the current tree are at `context/mod.rs:2521` (`flush_pending_destroys`) and `context/mod.rs:2836` (`Drop::drop`). The referenced invariant itself (drain `pending_destroy_blas` + `skinned_blas` unconditionally, because a `device_wait_idle` upstream already covers any in-flight reference) is still correct and still held by both call sites — only the citation is stale.
- **Evidence**: Comment: `// (#372 …) the parent Drop's device_wait_idle (context/mod.rs:1300) …` at `acceleration/mod.rs:251-252`, and `// … device_wait_idle (context/mod.rs:1859 / context/mod.rs:2093) …` at `acceleration/mod.rs:292-293`. Live `device_wait_idle()` sites: `context/mod.rs:2521` (`flush_pending_destroys`) and `context/mod.rs:2836` (`Drop::drop`), neither matching the cited line numbers.
- **Impact**: None functionally — documentation/traceability defect, not a code-correctness one. A future reader chasing the comment to verify the safety argument lands on unrelated code, costing review time or risking a redundant "fix".
- **Trigger Conditions**: N/A (static documentation drift, not a runtime bug).
- **Verification Path**: `grep -n "device_wait_idle" crates/renderer/src/vulkan/context/mod.rs` — confirms only two call sites, at 2521 and 2836.
- **Suggested Fix**: Update the two comment blocks in `acceleration/mod.rs` to cite `context/mod.rs::flush_pending_destroys` / `context/mod.rs::Drop::drop` by name/anchor rather than by line number. Low priority — bundle with the next touch of this file.

---

## Dimension-by-dimension summary

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync
One CRITICAL finding (CONC-D1-01). Every other checklist item verified PASS
against current code, including the post-audit-window refactor surface
(`draw_frame` extraction #1748, `recreate_swapchain` 3-phase split #1671,
`build_core_device` extraction #1749, skin-workgroup-constant plumbing #1758,
pipeline/descriptor helper reroutes #1751/#1752) and the #1713 queue-Mutex
fix (`806ba7af`), which is confirmed correctly landed at both the
`with_one_time_commands_inner` and egui `dispatch` sites. Four candidate
findings were raised and disproven during the sweep (documented in
`/tmp/audit/concurrency/dim_1.md` for the next audit's baseline): a
suspected missing scratch-serialize barrier in `build_blas_batched`, a
suspected skin-workgroup-size drift (disproven via SPIR-V binary decode),
a benign `frame_counter` reset interaction with skin-slot LRU, and
draw_frame-extraction call-ordering drift.

### Dimension 2 — Compute → AS → Fragment Chains
One MEDIUM finding (CONC-D2-01). Every barrier chain, dispatch-sizing
constant, ping-pong index, gate latch, and pass-ordering item verified clean:
skin chain end-to-end across the extracted draw.rs helpers, `#1758`
workgroup-size lockstep (Rust constant / GLSL define / both compiled SPIR-V
binaries all agree at 64), SVGF/TAA/caustic/water-caustic/volumetrics
cross-frame ping-pong indexing, the volumetrics `tlas_written` latch
set/reset symmetry, the bloom per-mip RAW chain, and the `#1751`/`#1752`
pipeline/descriptor-helper reroutes (verified parameter-preserving at every
rerouted call site).

### Dimension 3 — ECS Lock Ordering & Deadlock
Four NEW LOW findings (CONC-D3-01 through 04), all doc-rot or
declaration-drift latent hazards gated on code that does not yet exist; two
additional observations dedup to today's companion ECS audit (not
double-counted). The core invariant — TypeId-sorted acquisition in every
multi-lock `world.rs` accessor (`query_2_mut`, `query_2_mut_mut`,
`resource_2_mut`, `try_resource_2_mut`), matching `lock_tracker` scope
ordering, the #313 regression guard — holds. All 12 files under
`byroredux/src/systems/` plus every externally-registered system (including
the two newly-scheduled scripting systems from #1768 and the new
`setav`/`modav`/`cond` console command paths) were swept for guard-lifetime
discipline; no re-entrant same-type acquisition or structural-mutation-in-
system-body pattern was found.

### Dimension 4 — Scheduler Access Declarations (regression guard — M27 closed)
Two NEW LOW findings (CONC-D4-01, CONC-D4-02). The conflict model
(`AccessConflict::{None, Unknown, Conflict}`, no `Parallel` variant,
undeclared⇒`Unknown`⇒pessimistic-serialise) is unchanged and sound. The
`#1394`/`#1602` startup KPI guard survived the `#1670` `App::new` split
(now lives in `install_runtime_registries`, still runs unconditionally
before the first frame). Both systems added since the last audit
(`quest_fragment_dispatch`, `recurring_update_tick_system`, from #1768) are
correctly registered on the exclusive lane, requiring no declaration.

### Dimension 5 — RwLock Patterns (Resource↔Storage, Physics)
**Zero findings.** All code added since the last audit — water buoyancy
(`1645112c`), the substep wall-clock budget (`a608fbb7`), the awake-faller
diagnostic (`0a0bf640`), and the ragdoll keyframed-follower teardown
(`da4a849d`, #1772) — was traced end-to-end and confirmed to preserve the
release-reads-before-write discipline this dimension's invariant requires.
The ragdoll teardown path in particular (new code, audited from scratch) was
confirmed to use three sequential non-overlapping guard scopes
(storage-read collect → resource-write free → storage-write ECS-row
removal) with `PhysicsWorld` never mutated while a live query guard is held.

### Dimension 6 — Resource Lifecycle (GPU teardown ordering)
One NEW LOW finding (CONC-D6-01, documentation drift). The `#1671`
`recreate_swapchain` 3-phase split and the `#1749` `build_core_device`
extraction are both confirmed behavior-preserving: every resolution-
dependent resource created in `VulkanContext::new` has a matching recreate
site across the three resize phases (full coverage table in
`/tmp/audit/concurrency/dim_6.md`), and every field `build_core_device`
introduces is destroyed by the existing Drop impl. The reverse-order Drop
discipline (`#1483` allocator-guard hoist, `#665` dangling-Arc leak-not-UAF
path) is unchanged and correct. This dimension's independent sweep of
`destroy()`/create() pairing landed on the same BLAS-scratch immediate-
destroy code Dimension 1 flagged as CRITICAL — cross-referenced as
CONC-D1-01 rather than duplicated (see that finding; not separately
counted).

### Dimension 7 — Worker Threads & Thread-Safety Bounds
**Zero findings.** Streaming Drop ordering (#1167) confirmed intact —
`shutdown()` takes the worker handle, then drops `request_tx` (closing the
channel) strictly before `join_with_timeout`; `Drop::drop` delegates to
`shutdown` as a safety net. Worker↔main data flow confirmed clean (no
shared `&mut World`, `MaterialProvider`/BGSM resolution stays main-thread-
only, NIF-import-cache reads a point-in-time snapshot with write-back
deferred to main). Debug-server per-client threads never touch the World
directly; the command queue is bounded (`MAX_QUEUED_COMMANDS = 64`,
regression-tested). The new `setav`/`modav`/`cond` console commands and all
new CHARAL/scripting modules (session 52-53) introduce no threads, channels,
or cross-thread shared state — grepped clean for
`thread::spawn|mpsc::|Mutex|RwLock|Arc<|std::sync` across all touched files.

---

## Notes for the next auditor

- **CONC-D1-01 is the headline finding of this audit.** It is a genuine,
  code-provable use-after-free (not a barrier/stage judgment call), so the
  "needs validation-layer or RenderDoc confirmation" guardrail does not gate
  it — the hazard is proven by tracing the CPU-side call graph
  (`step_streaming` → `about_to_wait` timing vs. `draw_frame`'s fence-wait
  boundary, confirmed against current `main.rs` line numbers in this audit)
  plus the buffer-sharing fact already documented in-repo
  (`blas_skinned.rs`'s per-frame scratch capture). Runtime/validation-layer
  repro is offered only as an additional confirming signal, not as a
  precondition for treating this as real.
- The fix for CONC-D1-01 is purely CPU-side lifetime management (route
  through the existing `DeferredDestroyQueue` pattern already used for
  `BlasEntry` objects) — no Vulkan barrier/stage/layout change is proposed,
  so ordinary `cargo test` coverage plus a fault-injection/telemetry check
  can validate the fix without a GPU capture.
- Two dimensions (5, 7) are fully clean — the physics RwLock and worker-
  thread invariants remain as robust as the 2026-06-23 baseline found them,
  including against substantial new code (ragdoll teardown, buoyancy,
  CHARAL/scripting systems).
- All LOW findings in Dimensions 3, 4, and 6 are declaration/documentation
  drift with no live trigger path — worth batching into a single low-priority
  cleanup pass rather than individual fixes.

Suggested next step:

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-01.md
```
