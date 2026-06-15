# Concurrency & Synchronization Audit — 2026-06-14

- **Scope**: Full `--focus 1,2,3,4,5,6,7` (all 7 dimensions). Depth: deep. Run as part of a `comprehensive` audit-suite sweep.
- **Baseline**: `main` @ `435e265d` (post PHYSAL ragdoll PR #1529 + Rapier cell-unload #1520).
- **Prior clean baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-06-11.md` @ `1e8a25ab` (dims 2/3/5-renderer, 0 findings).
- **Dedup pool**: `gh issue list --state all` snapshot — 400 issues (23 OPEN), `/tmp/audit/concurrency/issues_all.json` + prior-audit adjudications.
- **Result**: **4 findings** — 0 CRITICAL, 0 HIGH, **1 MEDIUM**, **3 LOW**. All NEW. The MEDIUM and one LOW originate in the fresh PHYSAL ragdoll work; the other two LOWs are a scheduler-guard coverage gap and a residual screenshot-cancel race.

The renderer/Vulkan surface (Dimensions 1, 2, 6 and the GPU half of 7) is at audit saturation and re-verified **clean** — every queue submit, fence, semaphore, AS build→read barrier, ping-pong slot, and GPU teardown path carries a prior-finding annotation that re-derives correctly on current `main`, and the recent ragdoll/cell-unload work touches **zero** renderer/vulkan/acceleration files (`git show --stat` confirmed). All NEW findings are on the **CPU/ECS** side, in the newly-landed ragdoll path and one debug-server residual.

---

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 3 |
| **Total**| **4** |

| ID | Severity | Dimension | Title |
|----|----------|-----------|-------|
| CONC-2026-06-14-01 | MEDIUM | Scheduler Access Declarations / RwLock Patterns | `ragdoll_writeback_system` reintroduces a declared `GlobalTransform` WriteWrite conflict in the Stage::Late parallel batch |
| CONC-2026-06-14-02 | LOW | Scheduler Access Declarations | Build-time scheduler guard checks only `undeclared_parallel_count()`, not `known_conflict_count()` / `unknown_pair_count()` |
| CONC-2026-06-14-03 | LOW | RwLock Patterns (Physics) | Ragdoll multibody bodies leak on cell unload — `unload_cell` does not sweep `Ragdoll`; `PhysicsWorld::remove_ragdoll` is dead code |
| CONC-2026-06-14-04 | LOW | Worker Threads (Debug Server) | Screenshot straggler survives `ScreenshotBridge::cancel()` — renderer `screenshot_pending_readback` latch not cleared |

> **Convergence note**: CONC-2026-06-14-01 was independently surfaced by both the
> Dimension 4 (scheduler access) and Dimension 5 (physics RwLock) agents as the
> same root cause. The two agents disagreed on severity (D5 applied the
> "ECS deadlock potential = HIGH" floor; D4 rated MEDIUM after a runtime test).
> **MEDIUM is the adjudicated severity**: it is verifiably *not* a deadlock or
> data race (single `RwLock` serialises, entity sets are disjoint), so the HIGH
> floor does not apply. See the finding for the proof.

---

## Findings

### CONC-2026-06-14-01: `ragdoll_writeback_system` reintroduces a declared `GlobalTransform` WriteWrite conflict in the Stage::Late parallel batch
- **Severity**: MEDIUM
- **Dimension**: Scheduler Access Declarations (primary) / RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/main.rs:864-871` (ragdoll registration), `byroredux/src/main.rs:846-858` (camera_follow registration), `byroredux/src/main.rs:933-938` (build-time guard), `byroredux/src/ragdoll.rs` (writeback body — `query_mut::<GlobalTransform>()`)
- **Status**: NEW (introduced by commit `2a14b2b7` "ragdoll activation + writeback wiring", PR #1529). Not a regression of any single tracked issue — the #1394 guard only ever covered `undeclared_parallel_count`. It regresses the *M27/R7 closed invariant* of "0 known conflicts" on the engine binary.
- **Trigger Conditions**: Engine built with `parallel-scheduler` (default ON), running with at least one `RagdollActive` actor. Both `ragdoll_writeback_system` and `camera_follow_system` are in the `Stage::Late` parallel batch and both declare `.writes::<GlobalTransform>()`; the scheduler runs them concurrently on rayon workers every frame.
- **Verification Path**: `cargo test` — add an assertion that `Scheduler::access_report().known_conflict_count() == 0` after the engine schedule is built (parallels the existing `undeclared_parallel_count() == 0` debug_assert at `main.rs:934`); it fails today. Single-lock contention, so `BYRO_LOCK_ORDER_CHECK=1` is **not** required — this is not an ABBA cycle.
- **Description**: PR #1529 added a second `Stage::Late` parallel system that writes `GlobalTransform`. `camera_follow_system` (Late, parallel) already declares `writes::<GlobalTransform>()` + `writes::<Transform>()`; `ragdoll_writeback_system` (Late, parallel, NEW) declares `writes::<GlobalTransform>()`. `analyze_pair` (`crates/core/src/ecs/access.rs`) classifies write∩write on the same component TypeId as `ConflictKind::WriteWrite` → `AccessConflict::Conflict`. `access_report` (`crates/core/src/ecs/scheduler.rs`) walks every parallel-parallel pair within a stage, so this pair pushes `known_conflict_count()` from 0 to 1. The build-time guard at `main.rs:933-938` asserts only `undeclared_parallel_count() == 0`; both systems *are* declared, so the guard passes and the conflict slips through.
- **Evidence**:
  - `main.rs:864-871` — `add_to_with_access(Stage::Late, ragdoll::ragdoll_writeback_system, Access::new()…writes::<GlobalTransform>())`.
  - `main.rs:846-858` — `add_to_with_access(Stage::Late, camera_follow_system, Access::new()…writes::<GlobalTransform>()…writes::<Transform>())`.
  - `main.rs:934-937` — sole guard: `debug_assert_eq!(report_snapshot.undeclared_parallel_count(), 0, …)`.
  - `GlobalTransform` is one `PackedStorage` → one `RwLock`.
  - **Runtime proof** (Dimension-4 agent, throwaway test reverted): a synthetic mirror of the exact pairing — two declared parallel Late systems both writing the same `PackedStorage` component — produced `known_conflict_count=1 unknown_pair_count=0 undeclared_parallel_count=0`.
- **Impact**:
  1. **Diagnostic regression (definite).** `sys.accesses` now prints `1 known conflicts`, breaking the post-M27 "0 unknown / 0 conflicts" invariant this dimension guards. Operators relying on a clean report for "is the parallel schedule sound?" get a false alarm, or learn to ignore conflict rows — eroding the diagnostic.
  2. **Runtime: NOT a data race today (verified).** The two systems write entity-**disjoint** sets — `camera_follow_system` writes only the active camera entity; `ragdoll_writeback_system` writes only ragdoll bone entities. The per-storage `RwLock` serialises the two `write()` acquisitions regardless, so there is no torn read / UB — but the systems lose their intended parallelism on the `GlobalTransform` storage whenever a ragdoll is active (one blocks the other on the lock every Late frame).
  3. **Latent hazard.** Declaring two parallel writers of one storage is exactly the pattern M27 exclusive-staging was designed to eliminate (cf. the camera_follow ↔ audio resolution that re-staged `audio_system` to exclusive). Were the entity sets ever to overlap, ordering would become nondeterministic.
  4. **Secondary (cross-ref #1375, CLOSED).** The `main.rs:805-813` invariant warns that no Late system may write `GlobalTransform` on a `LocalBound`-bearing entity or its WorldBound lags one frame. Camera/audio carry no LocalBound; ragdoll bones plausibly *do*. This is a distinct correctness concern from the access-conflict above and worth checking when fixing.
- **Related**: #1394 (TS-03, CLOSED — the guard that only covers undeclared count); #1375 (PERF-D4-NEW-01, CLOSED — Late GT writers / stale bounds); CONC-2026-06-14-02 (the guard gap that let this land).
- **Suggested Fix**: Demote `ragdoll_writeback_system` to `add_exclusive(Stage::Late, …)` (dropping the access arg). This mirrors the M27 Phase 3 treatment of `audio_system` and `spin_system`: exclusive systems are never paired by the analyzer, so `known_conflict_count()` returns to 0. The system already wants to run "last word before render" and its ordering vs. `camera_follow_system` is irrelevant given disjoint entities, so exclusive sequencing is harmless and there is no parallelism worth preserving (one short writeback loop). Pair with CONC-2026-06-14-02.

---

### CONC-2026-06-14-02: Build-time scheduler guard checks only `undeclared_parallel_count()`, not declared conflicts or unknown pairs
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/main.rs:933-938`
- **Status**: NEW (the structural coverage gap that allowed CONC-2026-06-14-01 to ship undetected). Not previously filed — #1394 deliberately scoped the guard to the migration KPI.
- **Trigger Conditions**: Any future registration that adds a *declared* parallel system whose access conflicts (write/read or write/write) with an existing parallel system in the same stage. `undeclared_parallel_count() == 0` stays satisfied, so the new conflict is not caught at build time.
- **Verification Path**: Read `main.rs:929-938`. The comment and the assertion both speak only to "undeclared parallel systems … use add_to_with_access instead of add_to." `known_conflict_count()` and `unknown_pair_count()` exist on `AccessReport` and are consumed by `sys.accesses` (`byroredux/src/commands.rs`) but never asserted at construction.
- **Description**: The #1394 guard was scoped to the migration KPI (drive undeclared parallel count to 0). It is not a guard on the post-migration invariant (0 known conflicts, 0 unknown pairs). This is the proximate reason CONC-2026-06-14-01 was not caught. Flagged separately so the fix can be evaluated independently of the ragdoll re-stage.
- **Evidence**: `main.rs:934-937` — single `debug_assert_eq!(report_snapshot.undeclared_parallel_count(), 0, …)`; no companion assertion on `known_conflict_count()` / `unknown_pair_count()`.
- **Impact**: A declared write-write / write-read conflict between two parallel same-stage systems compiles, runs, and only surfaces if a human runs `sys.accesses` and reads the conflict rows. No automated gate.
- **Related**: CONC-2026-06-14-01 (the conflict this gap let through); #1394.
- **Suggested Fix**: Add `debug_assert_eq!(report_snapshot.known_conflict_count(), 0, …)` and `debug_assert_eq!(report_snapshot.unknown_pair_count(), 0, …)` alongside the existing assertion (cheap, no false positives once NEW-01 is fixed), turning the whole class of regressions into a debug-build failure. Optionally promote to a CI-visible integration test that builds the real engine `Scheduler` and asserts all three counts are 0 — there is currently none.

---

### CONC-2026-06-14-03: Ragdoll multibody bodies leak on cell unload — `unload_cell` does not sweep `Ragdoll`; `PhysicsWorld::remove_ragdoll` is dead code
- **Severity**: LOW
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics) — teardown completeness
- **Location**: `byroredux/src/cell_loader/unload.rs` (`release_victim_rapier_bodies` — handles only `RapierHandles`); `crates/physics/src/ragdoll.rs:319` (`remove_ragdoll` — never called); `crates/physics/src/components.rs` (`Ragdoll` holds its own `bodies`/`joints`, not `RapierHandles`)
- **Status**: NEW (ragdoll path landed in PR #1529; the #1520 teardown predates it and was written only for `RapierHandles`). **Not** a regression of #1520 — the original fix never covered ragdoll bodies (they didn't exist yet).
- **Trigger Conditions**: An actor is `RagdollActive` (ragdoll multibody built via `build_ragdoll`) when its owning cell unloads (exterior radius streaming or interior door transition).
- **Verification Path**: `cargo test` — extend the `rapier_release_tests` pattern: build a ragdoll on an actor stamped with the unloading `CellRoot`, run `unload_cell`, assert `PhysicsWorld` body count returned to baseline. Fails today. (Single-threaded unload — no concurrency hazard, this is a pure resource-leak finding.)
- **Description**: #1520's `release_victim_rapier_bodies` collects each victim's `RapierHandles` and calls `pw.remove_body`. Ragdoll member bodies are not stored in `RapierHandles`; they live in the `Ragdoll` component's `bodies: Vec<(EntityId, RigidBodyHandle)>` + `joints: Vec<MultibodyJointHandle>`. `unload_cell` never queries `Ragdoll`, so when the actor entity is despawned, only the `Ragdoll` ECS row is dropped — the rigid bodies, colliders, and multibody joints remain in `RigidBodySet` / `ColliderSet` / `MultibodyJointSet` and the broad-phase/query BVH forever. `PhysicsWorld::remove_ragdoll` exists for precisely this teardown but has **zero callers** (grep-confirmed: only the definition + a doc-comment reference) — dead code.
- **Evidence**:
  - `release_victim_rapier_bodies` reads only `query::<RapierHandles>()`.
  - No `query::<Ragdoll>()` / `remove_ragdoll` anywhere under `byroredux/src/cell_loader/`.
  - `grep -rn remove_ragdoll` → definition (`crates/physics/src/ragdoll.rs:319`) + one doc-comment reference; no call site.
- **Impact**: Per-cell-crossing leak of ragdoll bodies/colliders/joints for any actor that ragdolled in a cell the player then leaves; unbounded under exterior streaming. No deadlock or correctness hazard beyond growing `RigidBodySet` and BVH cost.
- **Related**: #1520 (cell-unload Rapier body leak, CLOSED — the `RapierHandles` analogue).
- **Suggested Fix**: In `unload_cell`, before the despawn loop, add a `release_victim_ragdolls(world, &victims)` two-phase helper mirroring `release_victim_rapier_bodies`: collect victims' `Ragdoll` components (clone the body/joint handle lists) under the read guard, drop it, then take `resource_mut::<PhysicsWorld>()` and call `pw.remove_ragdoll(&ragdoll)` for each. This wires up the already-written-but-dead `remove_ragdoll` and follows the established release-reads-before-write discipline.

---

### CONC-2026-06-14-04: Screenshot straggler survives `cancel()` — renderer `screenshot_pending_readback` latch not cleared, can leak a cancelled capture's pixels into a later request
- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:16-71` (`screenshot_finish_readback`) + `crates/core/src/ecs/resources.rs:140-155` (`ScreenshotBridge::cancel`) + `crates/debug-server/src/system.rs:72-78` (drain cancel path)
- **Status**: NEW (residual of #1011 / #1007 — those fixes cover the `requested`/`result`/`owner` triple but not the renderer-private readback latch). Distinct from #1448 (stale extent after resize, CLOSED) and #1174 (poison panic, CLOSED).
- **Trigger Conditions**: A debug-server `Screenshot` request is claimed and its copy is recorded into the staging buffer in one `draw_frame` (`screenshot_pending_readback = Some`), THEN the engine stalls > 5 s before the next `draw_frame` runs `screenshot_finish_readback`. The client's 5 s `recv_timeout` fires during the stall → the drain calls `bridge.cancel()`. When the engine resumes, the still-latched readback writes the cancelled capture's PNG into `screenshot_result` with `owner == NONE`. A subsequent screenshot request claims the bridge and its first drain-frame `take_result_for` returns the **stale** bytes before the new readback overwrites them.
- **Verification Path**: Code trace only (no `cargo test` reproduction — requires a multi-second engine stall window). The four touch-sites of `screenshot_pending_readback` confirm it is set at `screenshot.rs:171` and cleared only by being consumed at `screenshot.rs:17`; `cancel()` (`resources.rs:140-155`) mutates only `requested`/`result`/`owner` and has no handle to the renderer latch. `screenshot_finish_readback` writes `*screenshot_result.lock() = Some(png_bytes)` **unconditionally** once `pending_readback` is `Some` (`screenshot.rs:67-70`), with no owner/generation/requested gate.
- **Description**: The #1011 fix clears `requested` + `result` on `cancel()` to stop a straggler PNG being served to the wrong consumer. It is correct when the renderer hasn't yet *recorded* the copy. It does not cover the case where the copy was already recorded (`screenshot_pending_readback = Some`) but the readback hasn't completed: `cancel()` clears `result` *before* the readback writes it, so the straggler reappears after cancel, persisting with `owner == NONE` until the next claimant's `take_result_for` consumes it.
- **Evidence**:
  - `resources.rs:140-155` — `cancel()` body touches only `requested`, `result`, `owner`.
  - `screenshot.rs:16-71` — readback consumes `pending_readback`, then writes `screenshot_result` with no owner/requested gate.
  - `screenshot_pending_readback` touch-sites = {set @171, take @17} only; never cleared by the bridge.
  - `system.rs:72-78` — drain cancel path calls `bridge.cancel()` but has no way to invalidate the in-flight renderer readback.
- **Impact**: A debug-server screenshot client that times out during a multi-second engine stall, followed by a subsequent screenshot request, can receive the *previous (cancelled)* frame's pixels instead of a current capture. Debug/diagnostic only; loopback-only operator-controlled surface (#857); no crash, no memory unsafety, no World corruption.
- **Related**: #1011 / #1007 (the `cancel()` straggler fix this is a residual of); #1448 (stale extent, CLOSED); #1174 (poison panic, CLOSED); #1006 (owner-tagging discipline).
- **Suggested Fix**: Tag each recorded capture with a `u64` generation/capture-id, store `(capture_id, bytes)` in `result`, and have `take_result_for` reject ids older than the current claim. Alternatively, give `ScreenshotBridge::cancel()` a renderer-observed "discard next readback" atomic that `screenshot_finish_readback` checks before writing. Lowest-touch: skip the `screenshot_result` write in `screenshot_finish_readback` when no live claim exists. Mirror the existing `owner`-tagging discipline from #1006.

---

## Verified-clean dimensions

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface) — CLEAN
Re-verified at HEAD `435e265d`; the 2026-06-11 clean verdict holds. PHYSAL ragdoll (#1529) + Rapier cell-unload (#1520) touch zero renderer/vulkan/acceleration files (`git show --stat`).

| Chain | Verdict | Evidence |
|---|---|---|
| Queue topology: `present_queue = Arc::clone(&graphics_queue)` (single Mutex) on common-family path; separate Mutex only on distinct-family fallback | CLEAN | `context/mod.rs` family-match branch (#284) |
| Main `queue_submit` / `queue_present` / egui submit / one-time-cmd submit: guard bound to local, deref'd inline, dropped — not held across submit (vk::Queue-is-Copy race) | CLEAN | `context/draw.rs` all 4 `.lock()` sites (CONC-D2-NEW-01 pattern) |
| FIF fence wait before cmd/resource reuse; per-frame `image_available` with leak recovery; `reset_fences` adjacent to submit | CLEAN | `draw.rs` (#282/#910/#952) |
| Acquire → render → present semaphore chain; per-image `render_finished` | CLEAN | `sync.rs` + `draw.rs` (post-#906) |
| AS build→read barriers (static BLAS WRITE→READ; skinned refit WRITE→WRITE scratch-serialise; refit→TLAS; TLAS→FRAGMENT\|COMPUTE) | CLEAN | `acceleration/blas_static.rs`, `blas_skinned.rs`, `tlas.rs`, `draw.rs` (#415/#642/#644/#983) |
| Swapchain recreate `device_wait_idle` before destroy/rebuild | CLEAN | `context/resize.rs` |
| No one-time blocking submit in the per-frame hot path; first-sight skinned BLAS uses on-cmd batched builder | CLEAN | `draw.rs` (#911/#1141) |

One checklist-prompted hazard disproved: queue guard held across a blocking fence wait in the one-time-commands helper is benign because the Vulkan queue is touched only from the single winit main thread (debug-server threads never reach a queue → no second queue user to serialise against).

### Dimension 2 — Compute → AS → Fragment Chains — CLEAN
Re-verified + full renderer-tree diff `1e8a25ab..HEAD`. All six chains intact.

| Chain | Verdict | Evidence |
|---|---|---|
| Skin chain (palette → COMPUTE→AS_BUILD → BLAS refit → TLAS → ray query); inline raster skinning needs no VERTEX_INPUT barrier | CLEAN | `draw.rs` COMPUTE→AS_BUILD barrier; builds/refits ⊆ dispatches so barrier can't be skipped |
| Cross-frame ping-pong (SVGF/TAA/caustic/water-caustic/volumetrics read prev slot) | CLEAN | per-FIF history + MFIF≥2 compile-time asserts (#918) |
| Volumetrics `tlas_written` latch set/reset symmetry (#1105) | CLEAN | `volumetrics.rs` `write_tlas` sets, `dispatch` asserts+resets; dormant behind `VOLUMETRIC_OUTPUT_CONSUMED=false` (#928) |
| Bloom per-mip RAW chain; final up-mip publishes to FRAGMENT (#931) | CLEAN | `bloom.rs` post-barrier accounting |
| Caustic CLEAR→COMPUTE→FRAGMENT, TLAS-gated | CLEAN | `caustic.rs` |
| MaterialBuffer SSBO (R1) HOST_WRITE pre-render-pass, not moved into compute | CLEAN | `material.rs` / `draw.rs` |

New ReSTIR-DI reservoir G-buffer attachment (7th color attachment, `R32G32B32A32_UINT`) confirmed **write-only this phase** (no prev-frame reservoir sampler, no new barrier/subpass lines in the diff) → not a ping-pong member, adds no sync dependency. `cargo test -p byroredux-renderer --lib`: 312 passed.

### Dimension 3 — ECS Lock Ordering & Deadlock — CLEAN

| Check | Verdict | Evidence |
|---|---|---|
| TypeId-sorted acquisition; tracker scopes armed in acquire order (#313) | CLEAN | all four pair accessors (`query_2_mut`, `query_2_mut_mut`, `resource_2_mut`, `try_resource_2_mut`) branch on `id_a < id_b`; all four `assert_ne!` on same-type; no new unsorted accessor (grep) |
| `lock_tracker` coverage; cross-thread ABBA detector wired into CI | CLEAN | same-thread reentrancy panic not cfg-gated; `.github/workflows/ci.yml` `lock-order-check` job runs with `BYRO_LOCK_ORDER_CHECK=1` (#1410) |
| Guard lifetime in system bodies | CLEAN | animation `NameIndex`-before-`Name` (#827); `weather.rs` explicit `drop()` before re-acquire; no `&mut World` structural mutation mid-system |
| Poisoning resolved through `*_lock_poisoned` helpers | CLEAN | no silent poison unwrap (grep; only a `#[cfg(test)]` reset) |

`Scheduler::run` relies on per-storage RwLocks to serialise conflicts; the only multi-system parallel batches are **Early** (disjoint lock sets) and **Late** (shared lock = GlobalTransform, single lock → contention not ABBA). No live schedule pair acquires a two-lock pair in opposite orders. ECS core tests: 300 passed. (The Late GlobalTransform contention is the access-report conflict reported as CONC-2026-06-14-01 — a diagnostic/parallelism issue, not an ABBA.)

### Dimension 5 — RwLock Patterns (Physics) — CLEAN except findings 01 / 03
The new PHYSAL/ragdoll path was audited carefully (fresh in PR #1529). All Resource↔Storage discipline is correct:

| Pattern | Verdict | Evidence |
|---|---|---|
| `physics_sync_system` 4-phase; collect drops read guards before register's writes | CLEAN | `collect_newcomers` returns owned `Vec`; `register_newcomers` explicit `drop(pw)` before `query_mut::<RapierHandles>()` |
| `set_linear_velocity` / `set_kinematic_translation`: RapierHandles read guard dropped (Copy) before `resource_mut::<PhysicsWorld>()`; callers hold no PW guard | CLEAN | `physics/src/sync.rs`; callers in `character.rs` / `camera.rs` close their write blocks first |
| `ContactConfig` snapshotted once per batch, not re-locked in loop | CLEAN | `register_newcomers` / `activate_ragdoll` / `character_controller_system` copy `Copy` value out |
| #1520 cell-unload teardown: collect RapierHandles under read, drop, then remove from PW; before despawn | CLEAN | `release_victim_rapier_bodies` (ragdoll-body completeness gap → finding 03) |
| `physics_sync_system` single-threaded placement (sole Physics-stage system) | CLEAN | `main.rs` Physics stage; other PW writers in Early/Late stages run sequentially |
| NEW `activate_ragdoll` / `build_ragdoll` / `ragdoll_writeback_system` internal lock order | CLEAN | strict collect-read → write-resource → write-storage; `build_ragdoll` takes `&mut PhysicsWorld` by ref, touches no ECS storage (cross-system conflict → finding 01) |

### Dimension 6 — Resource Lifecycle (GPU teardown) — CLEAN
Full Drop impl + resize path + 29 `destroy()` sites read; all 18 GPU-owning struct fields map 1:1 to a destroy/take.

| Area | Verdict | Evidence |
|---|---|---|
| Reverse-order Drop, allocator freed last; #1483 hoist sound (hoisted destroys take `&device` only) | CLEAN | `context/mod.rs` Drop; allocator taken at the last GPU step; `water.rs` / `skin_compute.rs` destroys allocator-independent |
| No use-after-destroy across swapchain recreate; per-FIF history/accumulator freed every slot; egui FBs rebuilt | CLEAN | `resize.rs`; #654 view-ordering pinned by in-file regression test |
| AS cleanup on shutdown (pending-destroy drain, BLAS, all TLAS slots, skinned BLAS, both scratch sets) | CLEAN | `acceleration/mod.rs` |
| scene_buffer / MaterialBuffer SSBO / texture registry / EguiPass teardown | CLEAN | `scene_buffer/descriptors.rs`, `egui_pass.rs` |
| No per-frame leaks (draw.rs allocates zero per-frame buffers/descriptors/cmd-buffers; cmd reset+reused) | CLEAN | `draw.rs` |

Existing OPEN LOWs noted, not re-reported: #1426 (allocator-leak early-return skips `device_wait_idle`), #1427 (EguiPass `pending_free` flush before Renderer drop). #1483 confirmed FIXED (`5c2b0137`).

### Dimension 7 — Worker Threads & Thread-Safety Bounds — CLEAN except finding 04

| Check | Verdict | Evidence |
|---|---|---|
| Streaming Drop ordering (#1167): `request_tx.take()` before worker drop; `shutdown` takes worker; join-with-timeout | CLEAN | `streaming.rs` `shutdown` + `Drop` delegate; `assert_send::<PartialNifImport>` (#1171) |
| Worker↔main data flow: payload via channel, no shared `&mut World`; `Arc<TextureProvider>` Mutex<File>; BGSM/MaterialProvider main-thread-only; import-registry read-only snapshot | CLEAN | worker chain takes no `&World`; BGSM/StringPool only in main-thread `finish_partial_import` |
| Debug server: per-client threads don't touch World; mutations via `DebugDrainSystem` (Late exclusive); bounded queue (cap 64); screenshot fence-gated | CLEAN | `listener.rs` (#1010/#1172), `debug-server/lib.rs` — **but see finding 04 for the cancel-straggler residual** |
| Allocator `Arc<Mutex>` not held across queue submit; egui dispatch after composite | CLEAN | `allocator.rs`; egui records into frame cmd, set/free textures use own one-shot buffer |
| Send+Sync bounds; Ruffle/wgpu single-thread | CLEAN | `Resource`/`Component: 'static + Send + Sync` compiler-enforced; `UiManager` deliberately not a Resource (Ruffle Player not Send+Sync) |

Non-findings verified: unbounded per-frame payload drain is boundary-gated (bounded by load-radius ring delta — not reachable today); Rapier `parallel` feature OFF (single-threaded solver, no off-thread physics).

---

## Dedup summary

All 4 findings are **NEW** — verified against the 400-issue pool (`gh issue list --state all`) and prior audits. Adjacent-but-distinct closed issues cross-referenced: #1394 (guard scope), #1375 (Late GT writers / stale bounds), #1520 (RapierHandles cell-unload leak), #1011/#1007/#1006 (screenshot straggler/owner), #1448 (screenshot stale extent), #1174 (screenshot poison panic), #1521 (no `AccessConflict::Parallel` variant). Prior-audit-adjudicated renderer sync/lifecycle issues (#282, #415, #418, #639, #642/#644, #654, #661/#1436, #906–#911, #918, #928, #931, #952, #983, #1003, #1031, #1105, #1138, #1141, #1211, #1255, #1297/#1298, #1483) re-confirmed intact, not regressed.

## Method notes

- Per the speculative-Vulkan-fix guardrail, **no** barrier/stage/layout change is proposed anywhere in this report. The renderer surface is reported clean by trace + diff, not by reasoning about hypotheticals.
- All NEW findings are CPU/ECS-side (scheduler access, physics teardown, debug-server bridge) and are observable by `cargo test` (findings 01/03) or static code trace (findings 02/04) — none depend on RenderDoc.
- Dimension scratch files: `/tmp/audit/concurrency/dim_{1..7}.md`.

Next step: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-06-14.md`
