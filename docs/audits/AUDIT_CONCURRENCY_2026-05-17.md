# Concurrency & Synchronization Audit — 2026-05-17

**Scope**: ECS locking, Vulkan synchronization, GPU resource lifecycle, Send/Sync correctness, compute → AS → fragment chains, worker threads (streaming + debug server).

**Methodology**: Six dimension agents ran in parallel. Each agent appended findings to `/tmp/audit/concurrency/dim_N.md` incrementally; this report merges and de-duplicates them.

**Coverage caveat**: Three agents (Dim 1, Dim 2, Dim 5) terminated mid-investigation after their highest-priority items were exhausted. The unreached items are listed at the end of each dimension's section and recommended as a focused follow-up sweep.

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 200` (32 open issues, snapshotted to `/tmp/audit/concurrency/issues.json`). No finding below collides with an open issue.

---

## Headline

| Severity | Count | Findings |
|---|---:|---|
| CRITICAL | 0 | — |
| HIGH | 2 | TS-D4-NEW-01, TS-D4-NEW-02 (both SSAO init / allocator-lock pattern) |
| MEDIUM | 4 | REN-D2-2026-05-17-01 (bloom pre-TAA), TS-D4-NEW-03 (depth-image init), REN-D3-NEW-02 (skin slot lifecycle), CONC-D6-NEW-01 (streaming worker detach) |
| LOW | 10 | REN-D3-NEW-01, REN-D3-NEW-03, CONC-D6-NEW-02..09 |
| **Total** | **16** | |

**The two HIGH-severity items share a single root cause**: a method-chained `.lock()` inside a `match` scrutinee extends the `MutexGuard` past the Err arm, holding the allocator lock during cleanup that re-enters the same `std::sync::Mutex`. TS-D4-NEW-01 is an active deadlock on the SSAO OOM path; TS-D4-NEW-02 is a latent variant. **Uniform fix**: hoist `let result = allocator.lock().expect(...).allocate(&desc);` to a `let` binding so the `MutexGuard` drops at end-of-statement.

---

## Dimension 1 — ECS Lock Ordering

**Result**: No findings. ECS locking primitives are sound.

- **clean**: `query_2_mut` / `query_2_mut_mut` / `resource_2_mut` in `crates/core/src/ecs/world.rs:390-649` all branch on `id_a < id_b` and acquire in TypeId-ascending order. No 3-way query API exists.
- **clean**: `lock_tracker` (`crates/core/src/ecs/lock_tracker.rs`) guards same-thread re-entrance always-on and cross-thread ABBA via opt-in `BYRO_LOCK_ORDER_CHECK=1`.
- **clean**: `animation_system` (`byroredux/src/systems/animation.rs:450`) holds a `SubtreeCache` read guard across phase-2 application loop that acquires write guards on Transform, RootMotionDelta, AnimatedAlpha, etc. No ABBA today — `SubtreeCache` has exactly one access site.
- **architectural note**: `Scheduler::run` (`crates/core/src/ecs/scheduler.rs:315-335`) does not consult `access_report` for enforcement — parallel systems within a stage rely on per-storage `RwLock` to serialize. Two parallel systems writing the same storage (e.g. `animation_system` + `spin_system` both writing `Transform`) will serialize behind one write lock. Correct, but defeats parallelism silently.

**Not reached**: nested-guard sweep across `systems/{camera,water,weather,billboard}.rs`, `resource_mut` cross-query holds, `World::insert` sneak paths.

---

## Dimension 2 — Vulkan Synchronization

### REN-D2-2026-05-17-01: Bloom reads pre-TAA raw HDR; comment claims post-TAA

- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync (image-source mis-binding, not a barrier hazard)
- **Location**: [`crates/renderer/src/vulkan/context/draw.rs:2312-2329`](crates/renderer/src/vulkan/context/draw.rs#L2312-L2329); cross-ref [`context/mod.rs:1675-1716`](crates/renderer/src/vulkan/context/mod.rs#L1675-L1716), [`taa.rs:556-575`](crates/renderer/src/vulkan/taa.rs#L556-L575)
- **Status**: NEW
- **Trigger Conditions**: Every frame whenever TAA is enabled (default path). When `self.taa_failed=true`, the comment becomes coincidentally correct.
- **Description**: `draw.rs:2324` dispatches bloom with `composite.hdr_image_views[frame]` — the raw G-buffer HDR attachment from the main render pass. At context construction (`context/mod.rs:1713-1716`) composite's binding 0 is rebound to `taa.output_view(i)` so that composite samples TAA, but the **bloom binding is never rewired** and continues to consume the pre-TAA HDR. The inline comment at `draw.rs:2312-2318` claims "Reads the post-TAA resolved HDR" — factually inverted. TAA writes to `self.history[frame]` (see `taa.rs:562` binding 5 = `out_taa` = `self.history[f].view`), never to `composite.hdr_image_views`.
- **Impact**: Bloom is computed from jittered, AA-free HDR; post-bloom output is then mixed into composite (which samples the de-jittered TAA result), creating a low-amplitude shimmer in bloom haloes on high-contrast geometry. Below "obvious by inspection" — needs RenderDoc validation. The comment lying about wiring is its own hazard for future maintainers.
- **Suggested Fix**: Either (a) rebind bloom's binding 0 to `taa.output_view(frame)` (and switch its expected layout to `GENERAL` per `taa.rs:606-636`) — bringing the comment into truth and giving bloom the AA'd input; or (b) keep the wiring as-is and rewrite the comment to "bloom intentionally reads the pre-TAA raw HDR attachment so the pyramid sees only spatial signal, not temporal jitter — TAA's resolved output is consumed by composite separately." Needs RenderDoc validation.

### Clean sub-areas

- **clean**: TLAS build → fragment ray-query barrier. `context/draw.rs:1021-1028` emits a global `memory_barrier(AS_BUILD_KHR/AS_WRITE_KHR → FRAGMENT_SHADER|COMPUTE_SHADER/AS_READ_KHR)` immediately after `accel.build_tlas`, covering main render pass + caustic + volumetrics + all later compute ray-query consumers.

**Not reached**: device_wait_idle resize coverage; `graphics_queue` lock duration (partially answered by Dim 4 — clean).

---

## Dimension 3 — Resource Lifecycle

(Findings from a 2026-05-13 sweep; preserved because the architecture is unchanged in the intervening 4 days. Today's agent terminated before producing additional content.)

### REN-D3-NEW-01: `failed_skin_slots` HashSet retains despawned EntityIds across cell transitions

- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: [`crates/renderer/src/vulkan/context/mod.rs:808`](crates/renderer/src/vulkan/context/mod.rs#L808) (field decl) + [`crates/renderer/src/vulkan/context/draw.rs:682`](crates/renderer/src/vulkan/context/draw.rs#L682), [`draw.rs:922`](crates/renderer/src/vulkan/context/draw.rs#L922)
- **Status**: NEW (cousin of MEM-2-1 / #643)
- **Description**: `failed_skin_slots: HashSet<EntityId>` is inserted into on slot-allocation failure and cleared only when the per-frame idle-eviction pass actually evicts at least one slot. On cell unload, the ECS World despawns entities but the renderer-side set is not notified — those EntityIds remain indefinitely.
- **Impact**: ~16 B per entry. More importantly: when an EntityId is recycled (ECS slot reuse) and the fresh entity tries to allocate a skin slot, the cached "failed" bit suppresses retry, silently dropping skin from a freshly-spawned NPC. Reproducible on a long Vegas/Skyrim playthrough cycling NPCs faster than EntityId pool wraps.
- **Trigger Conditions**: Cell unload while entities had outstanding `failed_skin_slots` entries (pool exhaustion + cell change). Compounded by ECS EntityId recycle.
- **Suggested Fix**: In `unload_cell` (after the `for eid in victims { world.despawn(eid); }` loop) call `ctx.failed_skin_slots.retain(|eid| !victim_set.contains(eid));`.

### REN-D3-NEW-02: skin_compute output buffers freed by eviction policy, not by entity despawn

- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: [`crates/renderer/src/vulkan/context/draw.rs:889-913`](crates/renderer/src/vulkan/context/draw.rs#L889-L913) (per-frame idle eviction) + `cell_loader/unload.rs` (no slot release)
- **Status**: Partial mitigation of #643 / MEM-2-1 — re-flag for closure verification
- **Description**: `SkinSlot` output buffers (VERTEX_STRIDE_BYTES × vertex_count of DEVICE_LOCAL, plus 2 descriptor sets) are released only by per-frame eviction when `last_used_frame` ages past `MAX_FRAMES_IN_FLIGHT + 1`. Cell unload despawns the owning entity but does not call `skin_pipeline.destroy_slot`. The slot is reachable only via `ctx.skin_slots`; after despawn the entity never renders, so `last_used_frame` freezes — but the next-frame eviction DOES catch it. Leak bound to ≤ MAX_FRAMES_IN_FLIGHT + 1 frames (~3 frames). **Risk surface**: if the per-frame eviction were ever skipped (paused renderer, headless mode, abnormal `draw_frame` early-return), slots accumulate.
- **Impact**: ~few KB to ~MB per skinned entity. Normal operation handles it within 3 frames of cell unload. Cell-unload-without-render-tick silently retains.
- **Suggested Fix**: Hook into `unload_cell`: walk `ctx.skin_slots` and `ctx.accel_manager.skinned_blas_entities()` for membership in the victim set, calling `skin.destroy_slot` + `accel.drop_skinned_blas` directly. Makes the leak window deterministic.

### REN-D3-NEW-03: AccelerationManager scratch buffer not shrunk on resize

- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:3511-3513` (Drop) + `cell_loader/unload.rs:207-210` (shrink site)
- **Status**: NEW (VRAM-budget)
- **Description**: `accel.blas_scratch_buffer` is grow-only across process lifetime by design (#495). Shrunk on cell unload via `shrink_blas_scratch_to_fit` and destroyed in `accel.destroy`. On swapchain resize, no shrink trigger — the buffer holds whatever the worst-case mesh demanded.
- **Impact**: VRAM pressure on long sessions without cell crossings (rendering a heavy mesh then resizing 100× keeps the ~80–200 MB scratch resident). Not a hard leak.
- **Suggested Fix**: Call `shrink_blas_scratch_to_fit` from `recreate_swapchain` after `device_wait_idle` — resize already paid the device-wait cost.

> **Note**: file path `crates/renderer/src/vulkan/acceleration.rs` is from the May 13 sweep. Session 35 split this into `crates/renderer/src/vulkan/acceleration/` (mod.rs + 7 siblings). The shrink helper now lives in `acceleration/memory.rs`. The May 13 finding is structurally still valid.

---

## Dimension 4 — Thread Safety

### TS-D4-NEW-01: Allocator MutexGuard held across `partial.destroy()` re-entry in SSAO init — single-thread deadlock on OOM path

- **Severity**: HIGH
- **Dimension**: Thread Safety
- **Location**: [`crates/renderer/src/vulkan/ssao.rs:149-166`](crates/renderer/src/vulkan/ssao.rs#L149-L166)
- **Status**: NEW
- **Trigger Conditions**: VRAM allocation failure during `SsaoPipeline::new` — the per-frame `R8_UNORM` AO image allocation returns `Err`. Reachable on near-OOM startup or with an undersized allocation block pool.
- **Description**: The match scrutinee is `allocator.lock().expect("allocator lock").allocate(&desc)`. Per Rust temporary-lifetime rules for match scrutinees, the intermediate `MutexGuard` lives until end of `match`, so the lock is still held inside the `Err(e)` arm. That arm calls `unsafe { partial.destroy(device, allocator) }`, and `SsaoPartial::destroy` re-acquires `allocator.lock()` at `ssao.rs:596`. `std::sync::Mutex` is non-reentrant — the same thread blocks forever.
- **Evidence**:
  ```rust
  let ao_allocation = match allocator.lock().expect("allocator lock").allocate(...) {
      Ok(a) => a,
      Err(e) => {
          unsafe { partial.destroy(device, allocator) };  // re-locks the still-held Mutex
          return Err(anyhow::anyhow!("Failed to allocate AO memory {fi}: {e}"));
      }
  };
  ```
- **Impact**: Process hang (not a clean error return) on the first VRAM-pressure failure during SSAO init.
- **Suggested Fix**: Bind to a local before matching: `let result = allocator.lock().expect("allocator lock").allocate(&desc); match result { … }`. Audit `composite.rs:935` (uses `?` — fine but worth normalising) and `context/helpers.rs:313` (TS-D4-NEW-03).

### TS-D4-NEW-02: Bind-failure path on SSAO image re-locks allocator twice with destroy() in between

- **Severity**: HIGH
- **Dimension**: Thread Safety
- **Location**: [`crates/renderer/src/vulkan/ssao.rs:170-182`](crates/renderer/src/vulkan/ssao.rs#L170-L182)
- **Status**: NEW (related to TS-D4-NEW-01)
- **Trigger Conditions**: `device.bind_image_memory` fails on the SSAO AO image. Reachable on driver / extension issues, not just OOM.
- **Description**: After bind failure: (1) `allocator.lock().expect(...).free(ao_allocation).ok();`, then (2) `unsafe { partial.destroy(device, allocator) };`. The first lock drops at end-of-statement, the second re-locks per-allocation. **Locks don't overlap → no deadlock here**, but `partial` still has `ao_image` pushed; `partial.destroy` iterates `self.ao_images` calling `device.destroy_image`. The `ao_allocation` is owned by the caller (not yet pushed into `partial.ao_allocations` — that happens AFTER the bind-error early return), so free-then-destroy_image order is correct *today*. A future maintainer reordering `partial.ao_allocations.push(...)` before the bind would re-introduce a double-free.
- **Impact**: Latent. Reordering refactor → double-free on bind failure.
- **Suggested Fix**: Wrap the (image, allocation, view) trio in a single RAII guard that frees on drop, or hoist `partial.ao_allocations.push(Some(ao_allocation));` to immediately after bind success so the partial's invariant is structural.

### TS-D4-NEW-03: Allocator MutexGuard held across `device.destroy_image` in depth-buffer init error path

- **Severity**: MEDIUM
- **Dimension**: Thread Safety
- **Location**: [`crates/renderer/src/vulkan/context/helpers.rs:313-332`](crates/renderer/src/vulkan/context/helpers.rs#L313-L332)
- **Status**: NEW
- **Trigger Conditions**: `allocate(...)` fails for the depth image at startup. Lock is held while `destroy_image` is called in the Err arm.
- **Description**: Same temporary-extension pattern as TS-D4-NEW-01. Err arm only calls `device.destroy_image` (not a re-lock) — no deadlock today. The global allocator Mutex is held across a Vulkan API call for no reason. Latent: someone adding a second cleanup path that touches the allocator silently introduces a deadlock identical to TS-D4-NEW-01.
- **Impact**: Contention window; latent deadlock if pattern extended.
- **Suggested Fix**: Hoist the lock-and-allocate to a `let` binding so the guard drops at end of statement.

### Clean sub-areas (Dim 4)

- **clean**: Component/Resource trait bounds enforce `'static + Send + Sync`; 89 `impl Component for` sites, none use `Rc` / `RefCell` / raw pointers. The two `RefCell` / `Rc` uses are inside `thread_local!` blocks.
- **clean**: Zero `unsafe impl Send` / `unsafe impl Sync` blocks in the workspace.
- **clean**: Queue locks scoped tightly. `graphics_queue: Arc<Mutex<vk::Queue>>` and `present_queue: Mutex<vk::Queue>` are locked only across `queue_submit`/`queue_present`. Lock order is consistent: **fence → queue** at every `with_one_time_commands_inner` site.
- **clean**: CXX bridge has no live pointer surface (stub: `engine_info()`, `native_hello()`).
- **clean**: UiManager is single-thread by design; not an ECS Resource (Ruffle's Player isn't Send+Sync).
- **clean**: Streaming worker uses `std::sync::mpsc`; `TextureProvider` is Sync-by-Mutex.
- **clean**: Debug-server uses canonical `Arc<Mutex<…>>` + `Arc<AtomicBool>`.
- **clean**: Storage `&mut` access guard-scoped via outer `RwLock`.
- **clean**: Rayon parallel scheduler shares only `&World` (Sync via per-storage `RwLock`).
- **clean**: Animation engine has no locks (pure-data structs).
- **clean**: Plugin parser thread-locals use RAII guards (`LocalizedPluginGuard`, `StringsTableGuard`).

---

## Dimension 5 — Compute → AS → Fragment Chains

**Result**: No findings on the three items covered.

- **clean: SVGF history ping-pong** — `svgf.rs:650` reads slot `(f + 1) % MAX_FRAMES_IN_FLIGHT`, writes slot `f`. `MAX_FRAMES_IN_FLIGHT = 2` aliasing prevented by compile-time `static_assert` at `svgf.rs:65-77`. Both-slots fence wait at `context/draw.rs:170-183`. Out-slot self-barrier at `svgf.rs:861-869` is intentional belt-and-braces per #962. Compute-write → fragment/compute consumer barrier at `svgf.rs:887-911` covers same-frame composite read and next-frame SVGF prev_indirect_hist tap. Per-FIF force-history-reset gate per #964.
- **clean: TAA history ping-pong** — `taa.rs:524` reads `(f + 1) % MAX_FRAMES_IN_FLIGHT`, writes `f`. Compile-time guard at `taa.rs:43-54`. Pre-write barrier at `taa.rs:706-714` (COMPUTE→COMPUTE, GENERAL→GENERAL). Post-write barrier at `taa.rs:738-751` exposes result to composite FRAGMENT + next-frame TAA COMPUTE. First-frame skip at `taa.rs:656-660` avoids reading UNDEFINED-layout history on bootstrap.
- **clean: Caustic CLEAR → COMPUTE → FRAGMENT** — pre-clear barrier at `caustic.rs:720-735` (COMPUTE|FRAGMENT → TRANSFER). `cmd_clear_color_image` with `GENERAL` layout. Post-clear barrier at `caustic.rs:750-765` (TRANSFER → COMPUTE_SHADER) guarantees atomicAdd reads zeros. Post-dispatch barrier at `caustic.rs:782-797` (COMPUTE → FRAGMENT for composite sampler binding).

**Not reached** (priority follow-up):
- Item 4: skin_compute write → BLAS refit read → fragment ray-query chain. The TLAS-build → fragment barrier is verified clean (Dim 2), but the per-BLAS skin-refit input-side barrier was not separately checked.
- Item 5: M29.3 — whether skin output also flows into a vertex shader binding (needs `VERTEX_INPUT|VERTEX_ATTRIBUTE_READ` barrier).
- Item 6: MaterialBuffer (R1) host-write to vertex/fragment read.

---

## Dimension 6 — Worker Threads (Streaming + Debug Server)

### CONC-D6-NEW-01: Streaming worker silently detached on non-CloseRequested exits

- **Severity**: MEDIUM
- **Dimension**: Worker Threads
- **Location**: [`byroredux/src/streaming.rs:121-173`](byroredux/src/streaming.rs#L121-L173) (struct, no `Drop` impl) + [`byroredux/src/main.rs:1023-1078`](byroredux/src/main.rs#L1023-L1078) (only call site of `shutdown`)
- **Status**: NEW
- **Trigger Conditions**: Process exits via any path other than `WindowEvent::CloseRequested`. Six other `event_loop.exit()` sites in `main.rs` (lines 938, 948, 1011, 1085, 1267, 1275, 1594). Most notably the `--bench-frames` natural-exit branch (line 1594) — the CI/automation hot path.
- **Description**: `WorldStreamingState` has no `Drop` impl. Field drop order on `App` Drop closes `request_tx` and then drops `worker: Option<JoinHandle<()>>` — the latter is a plain `JoinHandle` drop which **detaches** the thread. Graceful join (`shutdown` + `join_with_timeout`) is only wired into the `CloseRequested` arm.
- **Impact**: Worker detached, may be mid-`BsaArchive::extract()`. On `--bench-frames` (most common automation path) this is a guaranteed thread leak. Worker holds an `Arc<TextureProvider>` keeping BSA file handles open. Re-introduces the exact behaviour #856 closed.
- **Suggested Fix**: (a) implement `Drop for WorldStreamingState` mirroring `shutdown(Duration::from_secs(1))` so detach is impossible regardless of exit path, or (b) factor `App` teardown into `fn shutdown(&mut self)` called from every `event_loop.exit()` site.

### CONC-D6-NEW-02: `--bench-frames` (no `--bench-hold`) leaks the streaming worker every CI run

- **Severity**: LOW (sub-case of CONC-D6-NEW-01)
- **Location**: [`byroredux/src/main.rs:1592-1594`](byroredux/src/main.rs#L1592-L1594)
- **Status**: NEW
- **Trigger Conditions**: `cargo run --release -- … --bench-frames 300` without `--bench-hold`. Every nightly bench / regression run on FNV WastelandNV is exactly this.
- **Impact**: Every CI / bench run leaks a worker thread + holds BSA files open. Process exit may delay 100–300 ms on slow disk.
- **Suggested Fix**: Insert `if let Some(state) = self.streaming.take() { state.shutdown(Duration::from_secs(1)); }` before `event_loop.exit()` at line 1594, or fold into the unified `App::shutdown()` from CONC-D6-NEW-01.

### CONC-D6-NEW-03: `join_with_timeout` leaks watcher thread + joined handle on timeout

- **Severity**: LOW
- **Location**: [`byroredux/src/streaming.rs:272-308`](byroredux/src/streaming.rs#L272-L308)
- **Status**: NEW
- **Impact**: On process teardown the leak is reaped by the OS — zero impact in practice. If `join_with_timeout` ever ships beyond shutdown, every timeout leaks one thread + Arc-held resources.
- **Suggested Fix**: Rename to `join_with_timeout_terminal` (explicit policy), or convert the watcher to a `try_join` loop using `Thread::is_finished` (Rust 1.61+).

### CONC-D6-NEW-04: BSA mutex poison cascades across per-NIF rayon panic guard

- **Severity**: LOW
- **Location**: [`byroredux/src/streaming.rs:474-515`](byroredux/src/streaming.rs#L474-L515)
- **Status**: NEW (flagged as a BSA-crate follow-up)
- **Trigger Conditions**: A panic inside `parse_nif` / `import_nif_lights` / `extract_bsx_flags` while a `BsaArchive`'s inner `Mutex<File>` is held.
- **Description**: `pre_parse_cell` wraps each per-NIF parse in `catch_unwind(AssertUnwindSafe(...))`. If a panic fires while the BSA mutex is held, the mutex is poisoned; every subsequent `.lock().unwrap()` panics the worker thread again. The per-NIF guard transforms one panic into N panics.
- **Impact**: One parser panic → archive mutex poisoned → every subsequent `extract_mesh` panics. Worker keeps recovering with `None`, but user-visible cells start failing to load.
- **Suggested Fix**: In `BsaArchive::extract` / `Ba2Archive::extract`, on `lock_err.is_poisoned()` clear the poison via `into_inner()` and proceed — file-position state is reset on each `seek` anyway, so poison is recoverable. Mark as a follow-up audit on the BSA crate.

### CONC-D6-NEW-05: `MaterialProvider` thread-affinity invariant not statically asserted

- **Severity**: LOW (informational — no bug today)
- **Location**: [`byroredux/src/streaming.rs:131-136`](byroredux/src/streaming.rs#L131-L136) (doc) + lines 96-115 (`PartialNifImport`)
- **Status**: NEW
- **Suggested Fix**: Add `static_assertions::assert_impl_all!(PartialNifImport: Send);` near the struct definition.

### CONC-D6-NEW-06: Listener thread can accept clients between shutdown-check and WouldBlock sleep

- **Severity**: LOW
- **Location**: [`crates/debug-server/src/listener.rs:194-238`](crates/debug-server/src/listener.rs#L194-L238)
- **Status**: NEW
- **Description**: Between accept and spawn there's a small window where a per-client thread is spawned for a stream that would otherwise have been dropped. Per-client thread self-terminates on first read iteration.
- **Suggested Fix**: Move the `shutdown.load` check at line 204 to AFTER `active_streams.lock()` and before the spawn.

### CONC-D6-NEW-07: 50 ms accept poll cadence inflates shutdown latency

- **Severity**: LOW (CI-perf)
- **Location**: [`crates/debug-server/src/listener.rs:229-231`](crates/debug-server/src/listener.rs#L229-L231)
- **Status**: NEW
- **Impact**: Up to 50 ms added to process shutdown. Not a correctness bug.
- **Suggested Fix**: Wrap `shutdown` in a `(Mutex<()>, Condvar)` pair so `notify_all` wakes the listener immediately. Or reduce sleep to 5 ms.

### CONC-D6-NEW-08: Per-client thread shutdown observation tied to TCP read activity, not flag

- **Severity**: LOW
- **Location**: [`crates/debug-server/src/listener.rs:267-289`](crates/debug-server/src/listener.rs#L267-L289)
- **Status**: Existing-mitigation: #1009
- **Impact**: Hypothetical today — would require API misuse from inside the crate.
- **Suggested Fix**: Document the invariant that "signal shutdown" must always be paired with "socket teardown" in `DebugServerHandle` method-level docs.

### CONC-D6-NEW-09: Screenshot bridge ownership across threads relies on Mutex poisoning being recoverable

- **Severity**: LOW
- **Location**: [`crates/core/src/ecs/resources.rs:97-115`](crates/core/src/ecs/resources.rs#L97-L115), `:130-139`
- **Status**: NEW
- **Impact**: Hypothetical today (renderer encode unlikely to panic). If `--screenshot` ever wires through a worker thread, a panic poisons the bridge for the rest of the session.
- **Suggested Fix**: Replace `.lock().unwrap()` with `.lock().unwrap_or_else(|e| e.into_inner())` in `ScreenshotBridge::result` accesses.

### Clean sub-areas (Dim 6)

- **clean**: `cell_pre_parse_worker` panic propagation (#854).
- **clean**: `NifImportRegistry` worker access via `Arc<HashSet<String>>` snapshot — no shared mutex.
- **clean**: `DebugDrainSystem` is a standard ECS exclusive system on `&World`; per-client threads only push into `CommandQueue`.
- **clean**: `CommandQueue` bounded by `MAX_QUEUED_COMMANDS = 64` (#1010); `try_enqueue_command` atomically check+pushes.
- **clean**: `pending_screenshot` cancel flow with `Arc<AtomicBool>` (#1007).
- **clean**: `DebugServerHandle::Drop` calls `shutdown_and_join` (`listener.rs:138-142`); `#[must_use]` on `start()`.
- **clean**: `active_streams` registry uses `Weak<TcpStream>` (#1009).
- **clean**: No `thread_local!` / `ThreadId` assumptions in debug server.

---

## Cross-Dimension Notes

- **REN-D2-2026-05-17-01 (bloom pre-TAA)** and **Dim 5 §2 (TAA history)** are complementary: Dim 5 verifies the TAA history slot ping-pong is correct, Dim 2 finds that bloom is reading the *wrong source image entirely*. No duplication.
- **CONC-D6-NEW-04 (BSA mutex poison)** lives at the boundary between Dim 6 (worker discovers it) and the BSA crate (where the fix belongs). Recommended as a focused follow-up audit on `crates/bsa/`.
- The session 35 acceleration split (`crates/renderer/src/vulkan/acceleration/` mod.rs + 7 siblings) doesn't invalidate REN-D3-NEW-03's substance, but the file path should be updated when fixing — shrink helper now lives in `acceleration/memory.rs`.

## Recommended Follow-Up Sweep

The current sweep left these items uninvestigated; each is worth a focused mini-audit:

1. **skin_compute → BLAS refit barrier chain** (Dim 5 item 4) — highest priority. Stale geometry in shadows/reflections/GI if missing.
2. **MaterialBuffer (R1) host→GPU barrier** (Dim 5 item 6).
3. **device_wait_idle in `recreate_swapchain`** — transfer-queue coverage if separate from graphics (Dim 2 item 6).
4. **Nested guards across `systems/camera.rs`, `systems/water.rs`, `systems/weather.rs`, `systems/billboard.rs`** (Dim 1 item 3).
5. **`resource_mut` held across queries** (Dim 1 item 4).
6. **BSA crate poison recovery** (cross-dim follow-up from CONC-D6-NEW-04).

---

## Next Steps

Suggest running:
```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-05-17.md
```

to convert findings into GitHub issues with the deduplication baseline above.
