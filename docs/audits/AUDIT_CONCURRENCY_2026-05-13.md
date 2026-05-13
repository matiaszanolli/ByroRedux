# Concurrency Audit — 2026-05-13

Deep concurrency sweep across the 6 audit dimensions (ECS locking, Vulkan
sync, resource lifecycle, thread safety, compute→AS→fragment chains,
worker threads). Architecture is largely sound — the audit's headline
result is **0 CRITICAL / 0 HIGH / 1 MEDIUM / 10 LOW** new findings plus
verification of 5 known-open issues. The MEDIUM is a partial-mitigation
gap on the skin-slot eviction (#643 cousin); the LOWs cluster on the
debug server's request lifecycle and a handful of resource-shrink
opportunities.

**Dedup baseline**: `gh issue list … "deadlock OR race OR sync OR
barrier OR lock OR concurrency OR fence OR mutex OR thread"` (200
results). Known-open issues confirmed still present + correct in
diagnosis: #952, #661, #949, #963, #877. Recently-closed issues
re-verified intact via code inspection: #204 (lock_tracker release
build), #282, #653, #911, #959 (Compute→AS→Fragment guarantees),
#626 (texture refcount leak), #855, #830, #854, #856 (worker threads),
#908, #906, #962 (Vulkan sync).

## Dim-by-dim verdict

| Dim | Verdict | NEW | Existing | Notes |
|---|---|---:|---:|---|
| 1 — ECS Locking | ✓ Clean | 0 | 0 | Per-stage scheduler + TypeId-sorted multi-queries hold up |
| 2 — Vulkan Sync | ⚠ Pre-tracked | 0 | 4 | `#952` / `#661` / `#949` / `#963` cover the known gaps |
| 3 — Resource Lifecycle | ⚠ One MEDIUM | 3 | 0 | Skin slot release indirect via eviction policy |
| 4 — Thread Safety | ✓ Clean | 0 | 0 | Rust types + Resource RwLock discipline carry it |
| 5 — Compute → AS → Fragment | ✓ Clean | 0 | 1 | `#661` flag-naming, no missing barrier |
| 6 — Worker Threads | ⚠ 6 LOWs cluster | 6 | 1 | Debug-server request lifecycle gaps; `#877` BSA mutex serializes I/O |

---

## Dimension 1 — ECS Locking

**Verdict**: Clean. No new findings.

Investigated `byroredux/src/systems/{animation,audio,billboard,bounds,
camera,debug,particle,water,weather}.rs` plus the
`crates/core/src/ecs/{world,query}.rs` lock helpers. The scheduler's
per-stage parallelism (Stage::Early / Update / PostUpdate / Physics /
Late) keeps potentially-conflicting systems serialized via the
exclusive-vs-parallel marker; specifically:

- **Stage::Late** runs `reverb_zone_system`, `audio_system`, and
  `log_stats_system` in parallel — but `reverb_zone_system` and
  `audio_system` both serialize on `AudioWorld` write lock (single
  writer at a time), no ordering issue.
- `audio_system`'s nested pattern AudioWorld(w) → AudioListener(r) →
  GlobalTransform(r) is safe: different storages, no overlap.
- `weather_system`'s acquire/drop sequence on `WeatherTransitionRes`
  (write, then read inside `if transition_t > 0.0`) and
  `WeatherDataRes` (read) is strictly sequential across drops; no
  overlap.
- `animation_system`, `billboard_system`, `bounds_system`: queries
  scoped within `for` iteration blocks; `World::insert` is not used
  mid-iteration in any audited system.
- `lock_tracker` (#204 closure) verified present in release builds.

NIF import registry access from `streaming.rs` worker thread is
read-only on the fast path; main thread holds the only write — covered
by Dim 6 below.

---

## Dimension 2 — Vulkan Synchronization

**Verdict**: All known gaps are already tracked. The audit agent for
this dimension ran out of turn budget mid-investigation, so this entry
covers only the dedup verification — Dim 5 below also independently
audited and confirmed the per-frame barrier chain.

### Existing: #952 — `reset_fences` error-path deadlock window
- Status: OPEN, MEDIUM
- Mirrors #908 fix surface

### Existing: #661 — Skin → BLAS barrier flag is `ACCELERATION_STRUCTURE_READ_KHR` (legacy)
- Status: OPEN, LOW (per Dim 5's re-verification, this is a
  flag-naming / modernisation question rather than a missing barrier;
  the spec-canonical bit IS `VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR`)

### Existing: #949 — `gbuffer::initialize_layouts` uses deprecated `TOP_OF_PIPE` source stage with non-empty `dst_access_mask`
- Status: OPEN, LOW

### Existing: #963 — Composite render-pass external dep lacks `UNIFORM_READ`
- Status: OPEN, LOW

No new findings surfaced in the agent's interim investigation before
budget exhaustion. The high-priority chains (TLAS build → fragment ray
query; SVGF/TAA history ping-pong; caustic CLEAR→COMPUTE→FRAGMENT) are
re-verified clean by Dim 5 below.

---

## Dimension 3 — Resource Lifecycle

**Verdict**: 1 MEDIUM, 2 LOW.

### REN-D3-NEW-01: `failed_skin_slots` HashSet retains despawned EntityIds across cell transitions
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: [crates/renderer/src/vulkan/context/mod.rs:808](crates/renderer/src/vulkan/context/mod.rs#L808) + [crates/renderer/src/vulkan/context/draw.rs:682, 922](crates/renderer/src/vulkan/context/draw.rs#L682)
- **Status**: NEW (cousin of MEM-2-1 / #643)
- **Description**: `failed_skin_slots: HashSet<EntityId>` is inserted on slot-allocation failure and cleared only when the per-frame idle-eviction pass actually evicts at least one slot. On cell unload, the World despawns every owning entity, but the renderer-side set is not notified — those EntityIds remain in the set indefinitely.
- **Impact**: Tiny memory leak (~16 B/entry). More importantly: when an EntityId is recycled (ECS slot reuse) and that fresh entity later tries to allocate a skin slot, the cached "failed" bit suppresses the retry, silently dropping skin from a freshly-spawned NPC.
- **Trigger Conditions**: Cell unload while entities had outstanding `failed_skin_slots` entries (pool exhaustion + cell change). Compounded by ECS EntityId recycle.
- **Suggested Fix**: In `unload_cell` (after `for eid in victims { world.despawn(eid); }`) call `ctx.failed_skin_slots.retain(|eid| !victim_set.contains(eid));`.

### REN-D3-NEW-02: skin_compute output buffers freed by eviction policy, not by entity despawn
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: [crates/renderer/src/vulkan/context/draw.rs:889-913](crates/renderer/src/vulkan/context/draw.rs#L889-L913) + [byroredux/src/cell_loader/unload.rs](byroredux/src/cell_loader/unload.rs)
- **Status**: Partial mitigation of #643 / MEM-2-1 — re-flag for closure verification
- **Description**: SkinSlot output buffers (VERTEX_STRIDE_BYTES × vertex_count of DEVICE_LOCAL, plus 2 descriptor sets) are released only by the per-frame eviction pass when `last_used_frame` ages past `MAX_FRAMES_IN_FLIGHT + 1`. Cell unload despawns the owning entity but does not call `skin_pipeline.destroy_slot`. The next frame's eviction pass DOES catch it, so the leak is bounded to ~3 frames in normal operation.
- **Impact**: ~few KB to ~MB per skinned entity. On normal operation the eviction handles it within 3 frames of cell unload. Risk surface: the renderer must keep ticking; cell-unload-without-render-tick (headless smoke tests, paused world) silently retains.
- **Trigger Conditions**: Cell unload + renderer skipping `draw_frame` for ≥ MAX_FRAMES_IN_FLIGHT frames.
- **Suggested Fix**: Hook into `unload_cell`: after collecting victim EntityIds and before `world.despawn`, walk `ctx.skin_slots` and `ctx.accel_manager.skinned_blas_entities()` for membership in the victim set, calling `skin.destroy_slot` + `accel.drop_skinned_blas` directly. Symmetric with the mesh/texture refcount drop loop in `unload.rs:217-222`.

### REN-D3-NEW-03: AccelerationManager scratch buffer not shrunk on resize, only on cell unload
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: [crates/renderer/src/vulkan/acceleration.rs:3511-3513](crates/renderer/src/vulkan/acceleration.rs#L3511) + [byroredux/src/cell_loader/unload.rs:207-210](byroredux/src/cell_loader/unload.rs#L207-L210)
- **Status**: NEW (cosmetic / VRAM-budget)
- **Description**: `accel.blas_scratch_buffer` is grow-only across process lifetime by design (#495). Shrunk on cell unload via `shrink_blas_scratch_to_fit`. No shrink trigger on swapchain resize — the buffer holds whatever the worst-case mesh demanded.
- **Impact**: VRAM pressure on long sessions without cell crossings. Not a hard leak; missed shrink opportunity.
- **Trigger Conditions**: Long session with rare cell crossings but frequent resizes.
- **Suggested Fix**: Call `shrink_blas_scratch_to_fit` from `recreate_swapchain` after `device_wait_idle`. Resize already paid the device-wait cost.

---

## Dimension 4 — Thread Safety

**Verdict**: Clean. 3 INFO entries documenting *why* patterns are safe.

### F1 — Queue MutexGuard held across `queue_submit` in `draw_frame` — INFO
- [crates/renderer/src/vulkan/context/draw.rs:2272-2293](crates/renderer/src/vulkan/context/draw.rs#L2272-L2293), `:2320-2334`
- The submit-site dereferences `*self.graphics_queue.lock().expect(...)` — `*` returns `vk::Queue` (Copy u64 wrapper), so the MutexGuard is the temporary that drops at end-of-statement. The named `queue` is a plain value, not a guard. The present-site uses the same lock but holds the guard across `queue_present` — that's the intended Vulkan-mandated submit/present serialization.
- **Verdict**: No issue. Recording is outside both locks.

### F3 — `SwfPlayer` wgpu device thread-safety — INFO
- [crates/ui/src/player.rs:23-29](crates/ui/src/player.rs#L23-L29)
- wgpu `Device` is `Send` but not historically `Sync`. `SwfPlayer` wraps `player: Arc<Mutex<Player>>`; every operation goes through `.lock().unwrap()`. The `UiManager` resource also holds the `SwfPlayer` through World's exclusive `ResourceMut` — two layers of serialization.
- **Verdict**: Pattern correct.

### F7 — `AudioWorld` resource accessed from `audio_system` — INFO
- [crates/audio/src/lib.rs:215-248, :638-650](crates/audio/src/lib.rs#L215-L248)
- Kira's `AudioManager`, `ListenerHandle`, etc. are `Send` but not `Sync`. `audio_system` opens with `let Some(mut audio_world) = world.try_resource_mut::<AudioWorld>() else { return; };` — exclusive `ResourceMut` guard from World's RwLock. No other system can hold `AudioWorld` while this guard is alive; kira's `!Sync` requirement satisfied.
- **Verdict**: Correct. World-level RwLock IS the discipline.

### Clean (no findings)
- F2 — Allocator `Arc<Mutex<vulkan::Allocator>>`: 40+ call sites surveyed. Every lock is a single-shot allocate/free; no lock-in-loop pattern.
- F4 — cxx-bridge FFI: 37-line stub; no `*mut T` / `*const T` shared across the boundary.
- F5 — `Cell<T>` / `RefCell<T>`: only inside `thread_local!` blocks (`plugin/esm/records/common.rs`, `core/ecs/lock_tracker.rs`). Never crosses thread boundaries.
- F6 — Raw pointer fields in structs: only Vulkan validation callback (`extern "system" fn`) and ephemeral Vulkan init extension/layer name lists. Driver-owned, single-shot.

---

## Dimension 5 — Compute → AS → Fragment Chains

**Verdict**: Clean. 5 INFO + 1 LOW (dedup #661).

### F1 — SVGF ping-pong: read-prev / write-curr correctly indexed
- [crates/renderer/src/vulkan/svgf.rs:69-73, 660-707, 854-1013](crates/renderer/src/vulkan/svgf.rs)
- Slot indexing `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT` at line 661. Three independent guards:
  1. Compile-time `const _: () = assert!(MAX_FRAMES_IN_FLIGHT >= 2, ...)` at line 69-73 (#918): a future sync-tier change lowering MFIF to 1 aliases prev/curr at the same slot — caught at build.
  2. `should_force_history_reset(frames_since_creation)` returns true for the first MFIF frames after `new()` / `recreate_on_resize`; shader takes the no-history branch via `params.z >= 0.5`.
  3. `mark_frame_completed()` advances `frames_since_creation` ONLY after `queue_submit` returns Ok (#917) — a record-time or submit-time failure leaves the counter back so the reset window is honoured.
- Closes verification ask for #282 / #653.

### F2 — TAA ping-pong: mirror of SVGF; first-frame guard via param.y
- [crates/renderer/src/vulkan/taa.rs:46-50, 513, 683-696, 800-814](crates/renderer/src/vulkan/taa.rs)
- Same `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT`, same compile-time MFIF≥2 gate (#918), same two-step counter advance (#917). Session frame 0: the OTHER slot's history is UNDEFINED but shader's `params.params.y > 0.5` first-frame guard at `taa.comp:93` skips the prev texelFetch.
- Closes #653 verification.

### F3 — Caustic CLEAR → COMPUTE → FRAGMENT chain: 3 barriers present and correct
- [crates/renderer/src/vulkan/caustic.rs:771-893](crates/renderer/src/vulkan/caustic.rs#L771-L893)
- `pre_clear_barrier` (SHADER_R|W → TRANSFER_W), `post_clear_barrier` (TRANSFER_W → SHADER_R|W), output barrier (SHADER_W → SHADER_R at COMPUTE → FRAGMENT). UBO HOST→COMPUTE barrier per-dispatch, not folded into bulk barrier — flagged in `draw.rs:1469` comment as a sibling sweep, not a correctness bug.

### F4 — Skin chain: COMPUTE → AS BUILD/refit → TLAS → fragment ray query — LOW (dedup #661)
- [crates/renderer/src/vulkan/context/draw.rs:533-866](crates/renderer/src/vulkan/context/draw.rs#L533-L866), [crates/renderer/src/vulkan/acceleration.rs:1488](crates/renderer/src/vulkan/acceleration.rs#L1488)
- **Status**: Existing #661
- Per-frame ordering: bone-palette HOST→{VS|FS|COMPUTE} bulk barrier → skin compute → `COMPUTE_SHADER + SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_KHR + ACCELERATION_STRUCTURE_READ_KHR` (#661 cited the flag as "legacy" but the spec-canonical bit IS `VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR` for AS-build vertex/index input reads in Vulkan 1.3). → per-iteration `record_scratch_serialize_barrier` (#983) → `AS_WRITE → AS_READ` at AS_BUILD↔AS_BUILD between BLAS refits and TLAS build → TLAS build → `AS_WRITE → AS_READ` at `AS_BUILD → FRAGMENT_SHADER | COMPUTE_SHADER` (covers main pass ray queries AND caustic compute).
- The #661 dedup target is a naming/modernisation question, not a missing barrier.

### F5 — draw.rs per-frame ordering: skin→TLAS relocation correct
- [crates/renderer/src/vulkan/context/draw.rs:372-972, 1473-1492](crates/renderer/src/vulkan/context/draw.rs)
- Structural ordering: bone upload (host) → bulk HOST barrier (1473) → skin compute (726) → COMPUTE→AS_BUILD barrier (744) → first-sight BUILDs (777) → scratch-serialise barriers (833) → refits (834) → AS_WRITE→AS_READ (854) → TLAS build (936) → AS_WRITE→AS_READ to FS|CS (950) → render pass / caustic. M29 Phase 2 comment at line 372-377 documents the relocation of TLAS build to AFTER the skin chain.
- Bulk HOST barrier covers MaterialBuffer SSBO upload (F6), instance SSBO, composite UBO (#909 fold), and SVGF UBO (#961 fold) into a single execution dependency.

### F6 — MaterialBuffer host upload covered by bulk barrier
- [crates/renderer/src/vulkan/context/draw.rs:1257-1261, 1473-1492](crates/renderer/src/vulkan/context/draw.rs#L1257-L1492)
- `scene_buffers.upload_materials(...)` at line 1259 (host-visible SSBO write). Bulk barrier at 1473-1492 fires after the upload with `src=HOST/HOST_WRITE`, `dst=VERTEX_SHADER|FRAGMENT_SHADER|COMPUTE_SHADER|DRAW_INDIRECT`. `triangle.frag` is the sole reader at FRAGMENT_SHADER. Covered.

---

## Dimension 6 — Worker Threads (Streaming, Debug Server)

**Verdict**: 6 NEW LOW + 1 Existing (#877). All cluster on the debug
server's request-lifecycle contract with the screenshot bridge.
Streaming worker is structurally sound — channel handoff, panic
recovery (#854), bounded-timeout shutdown (#856), stale-generation
gating all verified.

### Existing: #877 — `pre_parse_cell` rayon par-iter serializes on BSA/BA2 file Mutex
- Status: OPEN, LOW
- Confirmed: `BsaArchive::file: Mutex<File>` held through `read_exact` ([archive.rs:524](crates/bsa/src/archive.rs#L524)) on uncompressed path; `Ba2Archive::file: Mutex<File>` ([ba2.rs:347](crates/bsa/src/ba2.rs#L347)) lock spans seek + read + decompress. BA2 path is worse than BSA-compressed (which drops the lock before zlib/LZ4 at line 600).

### C6-NEW-04: Screenshot bridge result slot shared between CLI and debug-server — last-drainer-wins race
- **Severity**: LOW
- **Location**: [crates/core/src/ecs/resources.rs:21-37](crates/core/src/ecs/resources.rs#L21-L37), [crates/debug-server/src/system.rs:51-96](crates/debug-server/src/system.rs#L51-L96), [byroredux/src/main.rs:1510-1543](byroredux/src/main.rs#L1510-L1543), [crates/renderer/src/vulkan/context/screenshot.rs:65](crates/renderer/src/vulkan/context/screenshot.rs#L65)
- **Status**: NEW
- **Description**: `ScreenshotBridge { requested: AtomicBool, result: Mutex<Option<Vec<u8>>> }` is a single-producer / single-consumer slot but has two consumers: CLI `--screenshot` poll and debug-server `DebugDrainSystem`. If both fire in the same session, the first drainer wins the PNG.
- **Trigger Conditions**: `--screenshot path.png --bench-hold` plus a debug-server `screenshot` command before the 60-frame CLI deadline.
- **Suggested Fix**: Route the CLI path through `DebugRequest::Screenshot` (consolidates the polling logic) — smaller than tagging requests with IDs.

### C6-NEW-05: `pending_screenshot` orphans renderer slot when debug client's `recv_timeout` (5s) outraces engine 10-frame ceiling
- **Severity**: LOW
- **Location**: [crates/debug-server/src/system.rs:48-98](crates/debug-server/src/system.rs#L48-L98), [crates/debug-server/src/listener.rs:185-199](crates/debug-server/src/listener.rs#L185-L199)
- **Status**: NEW
- **Description**: Per-client thread blocks on `rx.recv_timeout(5s)`. If the engine takes longer (paused, swapchain recreate), client sends synthetic timeout error and drops its receiver. The matching `pending.response_tx.send(response)` returns `Err` swallowed by `let _ =`. File still gets written to disk; client thinks it failed and re-issues — multiple PNGs accumulate.
- **Suggested Fix**: Use `crossbeam::channel`'s `is_disconnected()` to detect abandoned clients in `DebugDrainSystem::run`; cancel the pending capture and release the renderer slot.

### C6-NEW-06: `handle_client` `set_nonblocking(false).expect(...)` panics silently kill per-client threads
- **Severity**: LOW
- **Location**: [crates/debug-server/src/listener.rs:139-144](crates/debug-server/src/listener.rs#L139-L144)
- **Status**: NEW
- **Description**: `.expect()` calls panic on socket setup failure. No `log::error!` indicates "per-client thread died here." Brittle on FD exhaustion / socket-level kernel errors.
- **Suggested Fix**: Replace with `match ... { Err(e) => { log::warn!(...); return; } }`. 3 lines. Mirrors `cell_pre_parse_worker`'s recovery pattern.

### C6-NEW-07: Per-client 300s read timeout blinds shutdown signal
- **Severity**: LOW
- **Location**: [crates/debug-server/src/listener.rs:142-158](crates/debug-server/src/listener.rs#L142-L158)
- **Status**: NEW
- **Description**: Idle per-client threads observe shutdown only on read timeout (5min) or client disconnect. Threads are detached so `DebugServerHandle::Drop` doesn't block, but a future switch to runtime-managed `join_all` would expose the long tail.
- **Suggested Fix**: On listener shutdown, `shutdown(Shutdown::Both)` every active stream. Requires sharing per-client stream refs with the listener (~20 lines).

### C6-NEW-08: CommandQueue is unbounded across clients
- **Severity**: LOW (theoretical for real-world; degenerate-client risk only)
- **Location**: [crates/debug-server/src/listener.rs:23-28, 175-183](crates/debug-server/src/listener.rs#L23-L28)
- **Status**: NEW
- **Description**: `CommandQueue = Arc<Mutex<Vec<PendingCommand>>>`. Per-client backpressure is naturally 1-in-flight, but the Vec capacity is unbounded across clients. Debug server is loopback-only (#857) so attack surface is operator-controlled; a CLI bug firing commands in a tight loop with `--bench-hold` could balloon memory.
- **Suggested Fix**: `crossbeam_channel::bounded(64)` or fixed-cap circular buffer. On overflow return `DebugResponse::error("server overloaded — drop and retry")`.

### C6-NEW-09: Screenshot timeout in drain system leaves `bridge.requested` set; next request reads stale bytes
- **Severity**: LOW
- **Location**: [crates/debug-server/src/system.rs:81, 84, 92](crates/debug-server/src/system.rs#L81-L96)
- **Status**: NEW
- **Description**: If drain system gives up after 10 frames (renderer paused), `pending_screenshot = None` clears engine-side bookkeeping but `ScreenshotBridge.requested` may still be `true` if renderer hasn't observed it. On next frame the renderer drains the request and writes a result that nobody is waiting for — the next debug screenshot command takes those stale bytes.
- **Suggested Fix**: When `DebugDrainSystem` cancels due to timeout, also call `bridge.requested.store(false, Release)` and `bridge.result.lock().take()`. Two lines.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 10 |
| **Total NEW** | **11** |
| Existing (open) verified | 5 |
| Closed (re-verified) | 12 |

### NEW findings (publish candidates)

| ID | Severity | Dimension | Title |
|----|----------|-----------|-------|
| REN-D3-NEW-01 | LOW | Resource Lifecycle | `failed_skin_slots` retains despawned EntityIds |
| REN-D3-NEW-02 | MEDIUM | Resource Lifecycle | Skin output buffers released by eviction policy, not despawn |
| REN-D3-NEW-03 | LOW | Resource Lifecycle | BLAS scratch buffer not shrunk on swapchain resize |
| C6-NEW-04 | LOW | Worker Threads | Screenshot bridge CLI vs debug-server race |
| C6-NEW-05 | LOW | Worker Threads | `pending_screenshot` orphan on `recv_timeout` mismatch |
| C6-NEW-06 | LOW | Worker Threads | `.expect()` silent panic in `handle_client` socket setup |
| C6-NEW-07 | LOW | Worker Threads | 300s per-client read timeout blinds shutdown signal |
| C6-NEW-08 | LOW | Worker Threads | Unbounded `CommandQueue<Vec<PendingCommand>>` |
| C6-NEW-09 | LOW | Worker Threads | Screenshot timeout leaves `bridge.requested` set |

### Existing OPEN issues confirmed accurate

- #952 — `reset_fences` error-path deadlock (Vulkan sync)
- #661 — Skin → BLAS barrier uses `ACCELERATION_STRUCTURE_READ_KHR` (flag-naming, no missing barrier)
- #949 — `gbuffer::initialize_layouts` deprecated TOP_OF_PIPE source stage
- #963 — Composite render-pass external dep lacks UNIFORM_READ
- #877 — `pre_parse_cell` rayon serializes on BSA/BA2 file Mutex

### Notes on Dim 2 coverage

The Vulkan sync dimension agent ran out of turn budget mid-investigation, having confirmed the four existing open issues against current code but without surfacing additional NEW findings. Dim 5's audit independently covered the highest-priority Vulkan sync surface (skin compute → AS → fragment chain, MaterialBuffer host upload, master per-frame ordering, SVGF/TAA history ping-pong) and found that surface clean — so Dim 2's coverage gap is limited to lower-priority paths (descriptor set update timing, swapchain recreate boundary). Worth a focused re-audit on the next pass.

### Suggested next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-05-13.md
```
