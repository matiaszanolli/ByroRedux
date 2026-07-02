# Concurrency & Synchronization Audit — 2026-07-02

**Scope**: Vulkan queue/AS sync, compute→AS→fragment chains, ECS lock ordering,
scheduler access declarations, physics RwLock patterns, GPU resource lifecycle,
worker threads. All 7 dimensions run at `--depth deep`.

**Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-07-01.md` (prior day). Since
that report the only commits on `main` (HEAD `1b4e8e84`) are CHARAL
character-system and session-53 documentation work — **none touch the
concurrency surface** (renderer AS sync, ECS locks, physics step, streaming
worker, scheduler). This audit re-ran all 7 dimension agents against the live
tree and re-verified every finding against current source; the set is unchanged
from the 2026-07-01 baseline, and the CRITICAL finding was independently
re-confirmed by two dimensions (1 and 6).

**Verdict**: One CRITICAL finding survives adversarial review — a real
use-after-free class bug in the shared BLAS build-scratch buffer (immediate
host-side `vkDestroyBuffer` + free on the streaming-time grow/shrink paths while
an in-flight frame's skinned-BLAS refit still references the buffer's device
address). One MEDIUM (decoupled skin-pipeline init-failure gate). Seven LOW
findings, all declaration / documentation drift or latent hazards gated on code
that does not exist yet. Two dimensions (5, 7) are fully clean with zero
findings. The CRITICAL finding is a CPU-side lifetime bug provable from the code
(not a speculative barrier judgment) — the standing Vulkan-speculative-fix
guardrail does **not** block its fix.

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 7 |
| **Total**| **9** |

Dedup baseline: `/tmp/audit/concurrency/issues.json` (21 OPEN). No open issue
covers any finding below. Two additional Dimension-3 poison-handling
observations (CONC-D3-05/06) are dedup'd against the companion ECS audit
(`AUDIT_ECS_2026-07-01`) and are cross-referenced, not double-counted.
CONC-D3-03 is consolidated into CONC-D4-01 (same underlying declaration gap).

---

## Findings

### CONC-D1-01: Shared BLAS build-scratch buffer destroyed host-side while an in-flight frame's skinned-BLAS refits still reference its device address

- **Severity**: CRITICAL
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:686-698` (`build_blas_batched` grow path: `old.destroy(device, allocator)` at 689-691)
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:285-296` (`build_blas` single-shot grow path, same shape)
  - `crates/renderer/src/vulkan/acceleration/memory.rs:42-104` (`shrink_blas_scratch_to_fit`: immediate `old.destroy` at 64-66 and 80-82; **stale SAFETY doc** at 33-39)
  - Exposed call sites: `byroredux/src/cell_loader/unload.rs` (unload-time shrink, stale SAFETY comment "builds run synchronously through fenced one-time command buffers"), `byroredux/src/cell_loader/exterior.rs:349` and `byroredux/src/cell_loader/spawn.rs:1179` (streaming-time `ctx.build_blas_batched`), reached from `App::step_streaming` in `about_to_wait` — **before** `render_one_frame`/`draw_frame` in the same tick, with no `device_wait_idle` between them.
- **Status**: NEW (sibling of closed #1449 / commit `a476b256` — that fix rerouted `BlasEntry` destruction through `pending_destroy_blas` in this exact timing window but did NOT cover the shared scratch buffer; the "safe by construction" premise dates to #495 and was invalidated by M29 refits + #911 first-sight builds living on the per-frame cmd)
- **Description**: `AccelerationManager::blas_scratch_buffer` is shared by three writers: cell-load `build_blas`/`build_blas_batched` (one-time submissions, host fence-waited), per-frame `build_skinned_blas_batched_on_cmd` (first-sight builds recorded into the frame command buffer, #911), and per-frame `refit_skinned_blas` (recorded into the frame command buffer every pose-dirty frame; the scratch device address is captured at record time, `blas_skinned.rs:520-530`). `draw_frame` waits both in-flight fences at the TOP of the frame, so during recording no frame is in flight — but after `queue_submit` returns, the just-submitted frame N executes asynchronously on the GPU. Cell streaming (`step_streaming`, and the worldspace-transition drain in `streaming_helpers.rs`) runs in `about_to_wait` **between** `draw_frame` calls — i.e. before the next frame's fence wait, while frame N may still be executing on the GPU (the timing model the #1449 fix comment documents, `blas_static.rs:1087-1097`). In that window:
  1. `build_blas_batched` Phase 2 growth (`scratch_needs_growth` true — a new cell's mesh exceeds the session's scratch high-water mark) takes and **immediately destroys** the old scratch buffer (`vkDestroyBuffer` + `gpu-allocator` free, no deferral).
  2. `unload_cell` → `shrink_blas_scratch_to_fit` does the same on the shrink/drop paths.
  If frame N recorded any skinned-BLAS refit or first-sight build (any animating NPC on screen — the steady-state case in populated cells), its `cmd_build_acceleration_structures` calls reference the destroyed buffer's device address as build scratch (read+write working memory). The host-side free is ordered by nothing: for a dedicated allocation (likely for the 80–200 MB scratch sizes this code anticipates, per #495) the underlying `VkDeviceMemory` is freed to the driver → GPU page fault → `VK_ERROR_DEVICE_LOST`; for a sub-allocation the range returns to the pool and a later allocation in the same streaming tick (compacted BLAS buffers, mesh/texture upload targets written at TRANSFER stage, NOT in the second sync scope of frame N's trailing `AS_BUILD→AS_BUILD` barrier) can be recycled onto it and race frame N's in-flight scratch access → silent memory corruption.
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
  // memory.rs:33-39 — the SAFETY premise this relies on, now stale (verified in live source):
  /// - Caller must guarantee no BLAS build command buffer is
  ///   currently referencing `blas_scratch_buffer`. The two build
  ///   paths use one-time command buffers with synchronous fence
  ///   waits, so any call site that is NOT inside a BLAS build is
  ///   safe by construction.
  ```
  The premise is false since M29/#911: `refit_skinned_blas` (`blas_skinned.rs:484-530`) and `build_skinned_blas_batched_on_cmd` capture the scratch device address into the **per-frame** command buffer, which is in flight whenever streaming runs. The engine has already codified the sibling rule (#1140): the host fence-wait between submissions does not excuse device-side lifetime/ordering obligations — and #1449 was a real, observed device-loss from exactly this window on the BLAS objects.
- **Impact**: GPU use-after-free — device loss (`VK_ERROR_DEVICE_LOST`, matching the #1449 "crash near water"-era signature) or silent BLAS/scene corruption. Impact class per the severity scale: use-after-free = CRITICAL.
- **Trigger Conditions** (all must coincide — modest frequency, catastrophic effect):
  1. At least one skinned entity refit/built in the most recent submitted frame (any animating NPC visible — near-always in populated cells);
  2. `step_streaming` (or worldspace-transition unload) runs before the next `draw_frame` fence wait — the normal M40 loop shape;
  3. (grow arm) the incoming cell contains a mesh whose `build_scratch_size` exceeds the session high-water mark, OR (shrink arm) `unload_cell` drops the peak-scratch mesh and `scratch_should_shrink` fires / all BLAS die (`peak == 0` drop-entirely arm).
- **Verification Path**: Standard validation will NOT catch this — the frame cmd references the scratch only via `VkDeviceAddress`, which object-lifetime validation cannot track, and sync-validation reasons per-submission. Confirming signals, in order of cost: (1) Code-level — the invariant chain is fully documented in-repo (#1449 comment for the timing window, `blas_skinned.rs` for the per-frame scratch capture, `memory.rs` for the stale premise); this is a lifetime bug provable from the code, not a speculative barrier judgment. (2) Runtime repro — stream exterior cells with NPCs in view while artificially lowering the initial scratch size (force growth on the first streamed cell) — expect `VK_ERROR_DEVICE_LOST` within a few transitions on a dedicated-allocation driver, or GPU-AV (`BYRO_VALIDATION=gpuav`, buffer-device-address checking) reporting an OOB/invalid-address access at `vkCmdBuildAccelerationStructuresKHR`.
- **Related**: Sibling of closed #1449 (`a476b256`); cross-confirmed by Dimension 6 (see CONC-D6-02 cross-reference). The identical grow-destroy in `build_skinned_blas_batched_on_cmd` (`blas_skinned.rs:212-214`) is **SAFE** as-is — it runs during `draw_frame` recording, after the both-slot fence wait, when provably nothing is in flight.
- **Suggested Fix**: Route retired scratch buffers through the existing deferred-destroy machinery instead of `old.destroy(...)`: add a `DeferredDestroyQueue<GpuBuffer>` (`deferred_destroy.rs` already generic, `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`) on `AccelerationManager`, push the old buffer in all three sites (`build_blas`, `build_blas_batched`, `shrink_blas_scratch_to_fit`), drain it in `tick_deferred_destroy` (already called post-fence-wait in `draw_frame`) and in `drain_pending_destroys` for shutdown (#732 parity). This is CPU-side lifetime management (unit-testable, not a barrier/stage change), so the speculative-Vulkan-fix guardrail does not block it; update the stale SAFETY docs in `memory.rs` and `byroredux/src/cell_loader/unload.rs` in the same change, and add a distinguishing comment on the safe `build_skinned_blas_batched_on_cmd` grow-destroy so the fix isn't blindly copied there.

---

### CONC-D2-01: `skin_palette` init failure is not coupled to `skin_compute` — skin chain can run against a never-populated (uninitialised) palette SSBO

- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1788-1829` (independent init gates), `crates/renderer/src/vulkan/context/draw.rs:1440` (refit gated on `skin_compute` + `accel_manager` only), `crates/renderer/src/vulkan/context/draw.rs:2675` (palette dispatch gated on `skin_palette` only), `crates/renderer/src/vulkan/scene_buffer/buffers.rs:457` (palette buffers are `create_device_local_uninit`)
- **Status**: NEW (latent since M29.5; not a regression — surfaced while verifying skin-chain integrity)
- **Description**: `SkinComputePipeline` and `SkinPaletteComputePipeline` are created by two independent `match … Ok/Err → Some/None` blocks, each degrading to `None` on failure with only a `log::warn!`. If `skin_palette` creation fails while `skin_compute` succeeds (partial init failure — mid-init OOM or pipeline-cache corruption), the per-frame chain still runs its downstream links: `record_skinned_blas_refit` (draw.rs:1440 checks only `skin_compute`/`accel_manager`) dispatches `skin_vertices.comp`, which reads the bone-palette SSBO (`bone_buffers()[frame]`) that no GPU pass ever wrote — the buffer is `create_device_local_uninit` (buffers.rs:457) and the sole producer (the palette dispatch at draw.rs:2694) is gated out. `triangle.vert`'s inline-skinning path reads the same unwritten palette. The mod.rs comment acknowledges "downstream `skin_palette.is_some()` checks skip the dispatch (no CPU-multiply fallback exists)" but the paired consumer gates were never coupled.
- **Evidence**:
  ```rust
  // mod.rs:1788 — independent gate 1
  let skin_compute = if device_caps.ray_query_supported {
      match SkinComputePipeline::new(...) { Ok(sc) => Some(sc), Err(e) => { log::warn!(...); None } }
  } else { None };
  // mod.rs:1816 — independent gate 2 (failure here does NOT clear skin_compute)
  let skin_palette = if device_caps.ray_query_supported {
      match SkinPaletteComputePipeline::new(&device, pipeline_cache) { Ok(sp) => Some(sp), Err(e) => { log::warn!(...); None } }
  } else { None };
  // draw.rs:1440 — refit chain checks only skin_compute + accel:
  if let (Some(skin_pipeline), Some(ref mut accel)) =
      (self.skin_compute.as_ref(), self.accel_manager.as_mut())
  // buffers.rs:457 — palette contents undefined until first palette dispatch:
  bone_device_buffers.push(GpuBuffer::create_device_local_uninit(
  ```
- **Impact**: Garbage (undefined-memory) bone matrices → garbage skinned vertices → per-entity BLAS built/refit over garbage geometry (potentially NaN/huge AABBs degrading TLAS traversal) → garbage skinned silhouettes in RT shadows/reflections/GI, plus garbage rasterized skinned meshes via the inline path. No memory corruption / UAF (all accesses stay in-bounds of allocated buffers); impact class is broken-geometry visual artifact for every skinned entity.
- **Trigger Conditions**: `SkinPaletteComputePipeline::new` fails while `SkinComputePipeline::new` succeeds on an RT-capable device — a partial init failure (both do near-identical work, so this is rare), then any skinned draw (`bone_offset != 0`).
- **Verification Path**: Inject an `Err` return into `SkinPaletteComputePipeline::new`, run the M41 equip smoke test (`docs/smoke-tests/m41-equip.sh`); observe garbage/collapsed NPC geometry in raster + RT and (with sync-validation off) no barrier complaints — confirming the chain ran against unwritten memory. No barrier/stage change involved — the fix is host-side init coupling, testable via a fault-injection seam.
- **Related**: None.
- **Suggested Fix**: Couple the gates: after the `skin_palette` match, if `skin_palette.is_none()` force `skin_compute = None`. Alternatively gate `record_skinned_blas_refit` and the palette dispatch on the SAME `skin_compute.is_some() && skin_palette.is_some()` predicate. One-line coupling + a note in the mod.rs comment.

---

### CONC-D3-01: `World` accessor docs claim the same-thread lock tracker is "debug only / release no-op" — it is compiled and active in release builds

- **Severity**: LOW
- **Dimension**: ECS Lock Ordering
- **Location**: `crates/core/src/ecs/world.rs:374-380` (and the "debug only" panic headers on `query_mut`:395-398, `get`:271-275, `has`:311-314, `count`:321-322, `try_resource`:687-691, `try_resource_mut`:705-707) vs. `crates/core/src/ecs/lock_tracker.rs:7-12`
- **Status**: NEW (doc rot)
- **Description**: `world.rs:378-380` states: *"Release builds do not enforce the check (production hot paths get a zero-cost no-op)."* This is false. `track_read` / `track_write` (`lock_tracker.rs:58-137`) carry no `cfg(debug_assertions)` gate; only the `held_others` Vec + `global_order::record_and_check` block and the graph module are debug-only. `TrackedRead::new` / `TrackedWrite::new` are called unconditionally from every `&self` acquisition site in `world.rs`. The module doc has it right: *"Thread-local check (always on — debug and release builds)"* (`lock_tracker.rs:9`).
- **Evidence**:
  ```rust
  // world.rs:377-380 (query::<T> doc)
  /// Drop the offending guard before calling. Release builds do
  /// not enforce the check (production hot paths get a zero-cost
  /// no-op).
  ```
  vs. `lock_tracker.rs:99-137` — `track_write` panics on conflict with no cfg gate; the #823 fix comment explicitly discusses the *release-build* per-frame cost of this function.
- **Impact**: Documentation-only, but it misleads on two operational facts: (a) a same-thread write-conflict acquisition **panics in release too** (good — it converts a silent `RwLock` deadlock into a diagnosable crash), and (b) the release hot path pays a thread-local HashMap probe per acquisition, not zero.
- **Trigger Conditions**: None at runtime; misleads maintainers.
- **Verification Path**: `cargo test --release -p byroredux-core` — `lock_tracker::tests::write_then_write_same_type_panics` passes in release, proving the check is live.
- **Related**: CONC-D3-02/03/04 (same declaration-trust surface).
- **Suggested Fix**: Rewrite the `# Panics (debug only)` headers: the thread-local re-entrancy check is always-on; only the cross-thread ABBA graph is debug-only + `BYRO_LOCK_ORDER_CHECK`-gated. Delete the "zero-cost no-op" sentence.

---

### CONC-D3-02: `animation_system` access declaration omits three color-sink component writes

- **Severity**: LOW (latent — animation is the *only* parallel system in `Stage::Update`, so no conflicting pair can exist today)
- **Dimension**: ECS Lock Ordering (declaration drift weakening the analyzer; overlaps Dimension 4)
- **Location**: `byroredux/src/main.rs:783-813` (declaration) vs. `byroredux/src/systems/animation.rs:150-172` (writes)
- **Status**: NEW
- **Description**: `apply_color_channels` lazily takes `world.query_mut::<AnimatedAmbientColor>()` (animation.rs:154), `AnimatedSpecularColor` (:156-162), and `AnimatedShaderColor` (:170-172) for `ColorTarget::Ambient/Specular/ShaderColor` channels. The `add_to_with_access` declaration at main.rs:791-812 declares `AnimatedDiffuseColor` and `AnimatedEmissiveColor` writes but none of the other three, despite the comment "The declaration is the UNION across all paths."
- **Evidence**: `grep AnimatedAmbientColor\|AnimatedSpecularColor\|AnimatedShaderColor byroredux/src/main.rs` → zero hits; `animation.rs:153-155` `write_lazy!(ambient_q, AnimatedAmbientColor, …)` expands to `world.query_mut::<AnimatedAmbientColor>()`.
- **Impact**: The scheduler's conflict analyzer (and the #1394/#1602 startup `debug_assert_eq!` guards) trust declarations. A future Update-stage parallel system touching any of the three storages would be co-scheduled with animation as "no conflict," opening a genuine cross-thread write-write / ABBA window none of the startup asserts can see.
- **Trigger Conditions**: Requires a future parallel Update-stage system touching these types — latent.
- **Verification Path**: `BYRO_LOCK_ORDER_CHECK=1` runs won't catch it (declaration-level, not acquisition-level); code review only until a declaration-vs-acquisition audit exists.
- **Related**: CONC-D4-01 (sibling declaration gap).
- **Suggested Fix**: Add `.writes::<AnimatedAmbientColor>() .writes::<AnimatedSpecularColor>() .writes::<AnimatedShaderColor>()` to the animation declaration in main.rs.

---

### CONC-D3-04: `CommandRegistry` read guard is held across arbitrary command execution; `help` re-enters the same lock

- **Severity**: LOW (latent — safe today: same-thread read-read, no runtime writer of `CommandRegistry` exists)
- **Dimension**: ECS Lock Ordering
- **Location**: dispatchers `crates/debug-server/src/evaluator.rs:413-417`, `byroredux/src/main.rs:268-269`, `byroredux/src/main.rs:2688-2689`; re-entry `byroredux/src/commands/world_info.rs:17`
- **Status**: NEW
- **Description**: All three command dispatch sites hold a `ResourceRead<CommandRegistry>` while calling `reg.execute(world, expr)` (structurally unavoidable — the registry owns the boxed `ConsoleCommand` objects). Every command body runs with a live read guard on the `CommandRegistry` RwLock. `HelpCommand::execute` re-acquires it read-only (world_info.rs:17). The always-on thread-local tracker permits read-read, and no runtime writer exists, so this is currently benign.
- **Evidence**:
  ```rust
  // evaluator.rs:413-415
  if let Some(reg) = world.try_resource::<CommandRegistry>() {
      if reg.list().iter().any(|(name, _)| *name == first_word) {
          let output = reg.execute(world, expr);   // guard `reg` held across execution
  ```
  All commands run on the main thread (`DebugDrainSystem` is `add_exclusive(Stage::Late, …)`; drain releases its queue guard before evaluating).
- **Impact**: Two latent failure modes, both gated on code that does not exist yet: (a) any future command taking `resource_mut::<CommandRegistry>()` (e.g. runtime alias registration) panics via the always-on tracker (release included); (b) a cross-thread writer queued on the lock between the dispatcher's read and `help`'s re-entrant read could deadlock `std::sync::RwLock` (re-entrant read under a queued writer is platform-dependent).
- **Trigger Conditions**: Future runtime `CommandRegistry` writer, or a command acquiring the registry mutably.
- **Verification Path**: `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace` records `CommandRegistry → X` edges; the write-under-read case panics via the thread-local tracker at the offending line.
- **Related**: CONC-D3-01 (same tracker behavior).
- **Suggested Fix**: Document the contract on `ConsoleCommand::execute` ("runs under a read guard on `CommandRegistry` — commands must never acquire it mutably"); optionally have `HelpCommand` receive the listing via the dispatcher instead of re-locking.

---

### CONC-D4-01: `physics_sync_system` under-declares its read surface (`ContactConfig` + the #1698 faller-dump reads)

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `crates/physics/src/sync.rs:226-244` and `crates/physics/src/sync.rs:371` (body) vs `byroredux/src/main.rs:887-908` (declaration)
- **Status**: NEW (part (b) landed 2026-06-25 inside the regression window; part (a) pre-existing since 2026-05-22, missed by the prior clean pass). Consolidates the Dimension-3 sibling observation CONC-D3-03 (undeclared `ContactConfig` reads in `player_controller_system` + `physics_sync_system`), reported once here to avoid double-counting.
- **Description**: `physics_sync_system` is registered in the `Stage::Physics` parallel batch via `add_to_with_access` with a declared surface (main.rs:890-907) that omits four accesses actually performed by its body:
  - (a) **`ContactConfig` resource read** — `world.try_resource::<ContactConfig>()` in `register_newcomers` (sync.rs:371; present since 525c690c, 2026-05-22). The same undeclared read exists in `player_controller_system` (character.rs:230-233).
  - (b) **`RenderLayer` (component read), `FormIdComponent` (component read), `FormIdPool` (resource read)** — the #1698 awake-faller diagnostic `dump_awake_fallers` (sync.rs:242-244), reachable from the system body at sync.rs:169-171, gated behind `BYRO_PROFILE_FALLERS` + a one-shot `AtomicBool`.
- **Evidence**: `grep -n "ContactConfig\|RenderLayer\|FormIdComponent\|FormIdPool" crates/physics/src/sync.rs` vs main.rs:890-907 — the four types appear in the body, not the declaration. `grep ContactConfig byroredux/src/main.rs` → only the `insert_resource` at :548, no `Access` mention.
- **Impact**: No live hazard today — `physics_sync_system` is the **only** system registered in `Stage::Physics`, so it pairs against nothing in `access_report()` and the missing entries are all read-side. Latent "silently defeats the analyzer" class: a future `Stage::Physics`/`Stage::Early` parallel system that *writes* any of the four types would have `analyze_pair` return `None` (both sides declared, no visible overlap) instead of `Conflict`, invisible to the #1394/#1602 startup asserts (which detect *undeclared systems* and *declared conflicts*, not declared-but-incomplete surfaces).
- **Trigger Conditions**: Requires a future parallel writer of one of the four types AND (for the part-(b) reads) `BYRO_PROFILE_FALLERS` set with ≥16 awake dynamic bodies. Not triggerable in the current schedule.
- **Verification Path**: `sys.accesses` shows the declared row without the four types; startup asserts stay green (they cannot see this class).
- **Related**: CONC-D3-02/03 (same declaration-completeness class).
- **Suggested Fix**: Append `.reads_resource::<byroredux_physics::ContactConfig>()`, `.reads::<RenderLayer>()`, `.reads::<FormIdComponent>()`, `.reads_resource::<FormIdPool>()` to the registration at main.rs:890-907, and `.reads_resource::<ContactConfig>()` to the `player_controller_system` declaration (main.rs:655-670).

---

### CONC-D4-02: `DebugDrainSystem` is registered after the access-report / `SystemList` snapshot — omitted from `sys.accesses` and `systems` output

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/main.rs:1071` (snapshot) vs `byroredux/src/main.rs:1083` (registration); `crates/debug-server/src/lib.rs:33`
- **Status**: NEW (pre-existing behaviour — identical ordering at the 2026-06-23 base — but unreported; NOT a regression of the #1670 `App::new` split, which preserved the order verbatim)
- **Description**: `App::new` builds the scheduler, then `install_runtime_registries` (main.rs:1071) snapshots `scheduler.access_report()` and `scheduler.system_names()` into the `SchedulerAccessReport`/`SystemList` resources. Only afterwards does `byroredux_debug_server::start(&mut scheduler, …)` (main.rs:1083) add `DebugDrainSystem` via `add_exclusive(Stage::Late, drain_system)`. The drain system therefore never appears in the `sys.accesses` rows or the `systems` listing.
- **Evidence**: install order main.rs:1070→1071→1083; `sys.accesses` reads the frozen resource, not a live report (world_info.rs:229-234).
- **Impact**: Introspection completeness only. `DebugDrainSystem` is exclusive, so it is never paired by the analyzer and the three startup asserts are unaffected (it did not exist when they ran; exclusive+undeclared is permitted by design, #1237). Checklist item "exclusive systems are listed in the report" fails for exactly this one system.
- **Trigger Conditions**: Always — every debug-mode launch; an operator running `sys.accesses`/`systems` sees a schedule missing one Late-stage exclusive entry.
- **Verification Path**: `cargo run -- --bench-hold` + `byro-dbg` → `systems` / `sys.accesses`; count Late-stage exclusive rows vs `Scheduler::system_names()` after `start()`.
- **Related**: None.
- **Suggested Fix**: Either move the `SchedulerAccessReport`/`SystemList` snapshot after `debug_server::start()` (registration order permitting), or have `sys.accesses` note "+ debug-server drain (registered post-snapshot)." Cosmetic.

---

### CONC-D6-01: Stale `context/mod.rs` line-number citations in `acceleration/mod.rs::destroy()` comments

- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs:251-252,292-293`
- **Status**: NEW
- **Description**: `AccelerationManager::destroy()`'s doc comments cite `context/mod.rs:1300`, `context/mod.rs:1859`, and `context/mod.rs:2093` as the locations of the `device_wait_idle()` calls that make the immediate (non-deferred) destroys in this function safe. Those line numbers predate the #1670/#1671 (`0409b6d6`) and #1749 (`26439046`) refactors; the actual `device_wait_idle()` calls in the current tree are at `context/mod.rs:2521` (`flush_pending_destroys`) and `context/mod.rs:2836` (`Drop::drop`). The referenced invariant itself (drain `pending_destroy_blas` + `skinned_blas` unconditionally, because an upstream `device_wait_idle` already covers any in-flight reference) is still correct and still held by both call sites — only the citation is stale.
- **Evidence**: `grep -n "device_wait_idle" crates/renderer/src/vulkan/context/mod.rs` confirms only two call sites, at 2521 and 2836, neither matching the cited line numbers.
- **Impact**: None functionally — documentation/traceability defect. A future reader chasing the comment lands on unrelated code (pipeline creation), which could cost review time or lead someone to "fix" a correctly-documented invariant redundantly.
- **Trigger Conditions**: N/A (static documentation drift).
- **Verification Path**: `grep -n "device_wait_idle" crates/renderer/src/vulkan/context/mod.rs`.
- **Related**: CONC-D1-01 (same file family; the immediate-destroy hazard there is the *code* issue, this is the *comment* issue).
- **Suggested Fix**: Update the two comment blocks in `acceleration/mod.rs` to cite `context/mod.rs::flush_pending_destroys` / `context/mod.rs::Drop::drop` by name/anchor rather than by line number (refactor-resistant). Bundle with the next touch of this file.

---

## Cross-referenced / dedup'd (not separately counted)

- **CONC-D3-03** — undeclared `ContactConfig` resource reads in `player_controller_system` and `physics_sync_system`. Same underlying gap as **CONC-D4-01**; consolidated there.
- **CONC-D3-05** — `clear_entities` poison panic drops the component type name (`world.rs:227-233`). Filed by the companion **AUDIT_ECS_2026-07-01 (ECS-2026-07-01-02)**. Not a torn-state unwrap — still panics loud.
- **CONC-D3-06** — `insert_resource` silently swallows a poisoned prior-value lock (`world.rs:542-552`). Filed by **AUDIT_ECS_2026-07-01 (ECS-2026-07-01-03)**. Requires `&mut World` (post-`catch_unwind` recovery path only), no live-system exposure.
- **CONC-D6-02** — BLAS scratch buffer immediate-destroy on grow/shrink paths, independently re-derived by Dimension 6 from the teardown-correctness angle. This is Dimension 1's **CONC-D1-01** and is owned there. Dimension 6 confirmed the `resize.rs` call to `shrink_blas_scratch_to_fit` IS safe (runs immediately after `device_wait_idle` at the top of `recreate_swapchain_core`); the hazard is specific to the mid-frame streaming grow/shrink call sites.

---

## Dimension summaries

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync
One CRITICAL (CONC-D1-01). All 9 checklist items otherwise PASS: single-Mutex queue submission with copy-out-then-drop discipline verified at every `.lock()` site (draw.rs, texture.rs, egui_pass.rs); both-slot fence discipline + per-image `render_finished` semaphores intact; acquire→render→present chain correct; AS build→read and AS build-INPUT (#507945d8/#1436) barriers correct-flag at every site; deferred AS-object destruction (#1449) + shutdown drain (#732) correct — the shared scratch **buffer** is the sole residual gap. Swapchain-recreate wait-idle-first ordering preserved through the #1671 3-phase split. Four candidate findings raised and disproven (recorded in `/tmp/audit/concurrency/dim_1.md` for the next baseline).

### Dimension 2 — Compute → AS → Fragment Chains
One MEDIUM (CONC-D2-01). Skin chain, dispatch-sizing constant (#1758, SPIR-V LocalSize decode = 64), cross-frame ping-pong indices (SVGF/TAA/caustic/water-caustic/volumetrics), volumetrics gate latch (#1105), bloom within-frame RAW chain (#931), caustic CLEAR→COMPUTE→FRAGMENT, MaterialBuffer SSBO, and SSAO barrier chains all verified clean. The #1751/#1752/#1748 refactors are behavior-preserving.

### Dimension 3 — ECS Lock Ordering & Deadlock
Core invariant holds — no HIGH/MEDIUM. TypeId-sorted acquisition intact across all four multi-lock accessors (`query_2_mut`, `query_2_mut_mut`, `resource_2_mut`, `try_resource_2_mut`); same-type access panics via `assert_ne!`; the #313 ABBA-edge guard holds; `lock-order-check` CI job current. All 12 systems + newly-scheduled scripting systems (#1768) + CHARAL commands swept for guard-lifetime violations — clean. Four NEW LOW (CONC-D3-01/02/04 + the consolidated D3-03) + two Existing (D3-05/06, ECS audit).

### Dimension 4 — Scheduler Access Declarations (regression guard — M27 closed)
Two LOW (CONC-D4-01/02). Conflict model sound (`None`/`Unknown`/`Conflict`, no `Parallel` variant; undeclared ⇒ `Unknown` ⇒ serialise). Migration KPIs (`known_conflict_count`/`unknown_pair_count` == 0) held; the #1394/#1602 startup guard survived the #1670 `App::new` split and runs before the first frame. The two #1768 systems correctly use the exclusive lane. 5/5 checklist PASS.

### Dimension 5 — RwLock Patterns (Resource↔Storage, Physics)
**Zero findings.** `physics_sync_system` 4-phase release-reads-before-write discipline intact; helper lock order (`set_linear_velocity`/`set_kinematic_translation` copy-out `RapierHandles` before `resource_mut::<PhysicsWorld>()`) correct; `ContactConfig` snapshotted once per batch; cell-unload teardown (#1520/#1531) + ragdoll teardown (#1772) collect-under-read-then-write correct; single-threaded placement confirmed (sole `Stage::Physics` registration; ragdoll writeback exclusive-Late). New water-buoyancy / substep-budget / awake-faller code all preserve the invariant.

### Dimension 6 — Resource Lifecycle (GPU teardown ordering)
One LOW (CONC-D6-01, stale comment). Reverse-order destruction + allocator-last + #1483/#665 guards intact through the #1671/#1749/#1748 refactors; every resolution-dependent resource has a matching recreate site (full coverage table in `/tmp/audit/concurrency/dim_6.md`); AS + skin-slot + SSBO/descriptor shutdown cleanup correct; no per-frame descriptor/command-buffer/staging allocation leak. The new `placement_lod.rs` module's mesh/texture handles have matching drops. CONC-D1-01 independently re-confirmed here (cross-reference only).

### Dimension 7 — Worker Threads & Thread-Safety Bounds
**Zero findings.** Streaming Drop ordering (#1167) correct — explicit `Drop`→`shutdown` takes the worker handle, drops the `Sender` before `join_with_timeout`. Worker↔main data flow is channel-only (no shared `&mut World`); `Arc<TextureProvider>` extract serialises on the inner `Mutex<File>`; BGSM/`MaterialProvider` stays main-thread-only; NIF import cache read-only fast path with deferred write-back. Debug server: per-client threads never touch the World, bounded 64-command queue, polled screenshot readback (no fence race). Allocator Mutex never held across a queue submit. `Component: Send+Sync`/`Resource: Send+Sync` bounds; zero `unsafe impl Send/Sync`; `UiManager` (Ruffle/wgpu) confined to the main thread; cxx-bridge still a placeholder. New CHARAL/scripting/console code introduces no threads or cross-thread state.

---

## Report Finalization

Recommended next step:

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-02.md
```
