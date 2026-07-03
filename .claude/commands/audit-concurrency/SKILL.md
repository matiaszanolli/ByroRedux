---
description: "Audit Vulkan queue/AS sync, ECS lock ordering, scheduler access declarations, RwLock patterns, deadlock potential"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Concurrency and Synchronization Audit

Audit ByroRedux for data races, deadlocks, incorrect lock ordering, missing
Vulkan synchronization, and thread-safety violations.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, finding format, and the path-reference convention.
See `.claude/commands/_audit-severity.md` for the severity scale — the rows that
matter here:

| Condition | Minimum Severity |
|-----------|-----------------|
| Data race on Vulkan queue / use-after-free / AS built at wrong address | CRITICAL |
| Vulkan spec violation (missing barrier, fence misuse) | HIGH |
| Missing AS barrier (build → shader read) | HIGH |
| Resource / descriptor / command-buffer leak per frame | HIGH |
| Missing cleanup on swapchain recreate | HIGH |
| ECS deadlock potential (RwLock ordering violation) | HIGH |
| FFI lifetime violation across the cxx bridge | CRITICAL |

Dimensions below are ordered by **concurrency blast radius**: GPU queue / AS
data races (CRITICAL) first, then ECS deadlock surfaces, then the
already-closed scheduler/access machinery (now regression guards), then the
slower-moving lifecycle / worker / chain dimensions.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 7.
- `--depth shallow|deep`: `shallow` = check barrier/lock presence; `deep` = trace concurrent paths and timing windows. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Vulkan Queue & AS Sync | Compute → AS → Fragment Chains | ECS Lock Ordering | Scheduler Access Declarations | RwLock Patterns (Resource↔Storage, Physics) | Resource Lifecycle | Worker Threads (Streaming, Debug)
- **Trigger Conditions**: Exact timing/concurrency window needed to reproduce
- **Verification Path**: For Vulkan-sync findings — whether the failure is observable in `cargo test`, the validation layer, or only RenderDoc (see "Speculative-fix guardrail" below)

## Speculative-fix guardrail (read before reporting any Vulkan-sync finding)

Vulkan render-pass / pipeline-barrier / semaphore / fence bugs are largely
**invisible to `cargo test`** — there is no headless device assertion that
catches a missing image barrier or a wrong stage mask. Per the project's
standing rule, do **not** propose shipping a barrier/stage/layout change on
reasoning alone. Frame each such finding as **"needs validation-layer or
RenderDoc confirmation"** and state the concrete signal that would confirm it
(a specific `VUID-*` validation message, a RenderDoc resource-state mismatch,
or a visible artifact class). A finding whose only evidence is "this barrier
looks wrong" is a HYPOTHESIS row, not a fix.

**The cheapest evidence channel is now sync-validation in release (#ec81f233).**
`BYRO_VALIDATION=<v>` (`instance.rs::validation_enabled`) turns on the Khronos
validation layer + Synchronization Validation in a **release** build — debug
builds are too slow to stream into the dense cells that fault. `BYRO_VALIDATION=gpuav`
additionally enables GPU-Assisted Validation (shader OOB / descriptor checks).
The debug messenger routes layer messages into the Rust log whenever validation
is enabled. A sync-validation RAW/WAR hazard count (e.g. the ~40/frame the
`--cornell` harness reported pre-#507945d8) is the confirmed-bug signal — prefer
it over "looks wrong." Confirm any AS-build-input / barrier finding against a
captured validation run before escalating past HYPOTHESIS.

## Phase 1: Setup

1. Parse `$ARGUMENTS`
2. `mkdir -p /tmp/audit/concurrency`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/concurrency/issues.json`

## Phase 2: Launch Dimension Agents

### Dimension 1: Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`), `crates/renderer/src/vulkan/sync.rs`, `crates/renderer/src/vulkan/acceleration/` (`blas_static.rs`, `blas_skinned.rs`, `tlas.rs`), `crates/renderer/src/vulkan/context/resize.rs`
**Checklist**:
- **Queue submission is single-Mutex.** `graphics_queue` and `present_queue` are both `Arc<Mutex<vk::Queue>>` (`context/mod.rs`); `present_queue` is an `Arc::clone` of `graphics_queue`, so one Mutex serialises all submits + presents. `vk::Queue` is `Copy`, so the canonical pattern is "lock → copy the handle out → drop the guard → `queue_submit`": confirm the guard is **not** held across `queue_submit`/`queue_present` (a held guard during a blocking submit would serialise every other queue user; a copied-out handle used after a *second* thread re-submits is a data race). Cross-check the `.lock()` sites in `draw.rs` (main submit ~`queue_submit`, and the secondary one-time-command submit) against this rule.
- **Frame-in-flight discipline.** The `in_flight[frame]` fence must be waited on before its command buffer / per-frame resources are reused; `image_available[frame]` semaphore must not be reused while a prior acquire is still pending (the draw.rs comment block around the acquire→submit window documents the ordering — verify it still holds).
- **Acquire → render → present semaphore chain** is correct and uses per-image (not per-frame) signal semaphores where required by the swapchain image index.
- **AS build → read barrier.** Every BLAS/TLAS build or refit that a later ray query reads must be followed by an `ACCELERATION_STRUCTURE_WRITE_KHR → ACCELERATION_STRUCTURE_READ_KHR` barrier before the fragment-stage ray-query consumer. Static BLAS: `blas_static.rs` (`memory_barrier`, WRITE→READ). Skinned BLAS refit: `blas_skinned.rs` (regression guard, #1790: `record_scratch_serialize_barrier`'s dst mask is WRITE|READ, not WRITE-only — it must cover a first-sight frame's same-command-buffer BUILD-then-UPDATE-refit adjacency, where the UPDATE reads `srcAccelerationStructure`). TLAS: `tlas.rs` (`cmd_pipeline_barrier`). A missing or wrong-stage barrier here is HIGH (CRITICAL if the AS is built at a wrong/stale device address — wrong geometry in shadows/reflections/GI).
- **Deferred BLAS-scratch destruction (regression guard, #1782).** `blas_scratch_buffer` retirement on grow/shrink routes through `pending_destroy_scratch` (deferred) instead of an immediate free — same use-after-free class as #a476b256 below, but for the scratch allocation rather than a `BlasEntry`. `build_skinned_blas_batched_on_cmd`'s own grow-destroy is deliberately immediate (safe: runs after that frame's own fence wait) — don't flag it as a missed instance.
- **AS build INPUT barrier access flag (regression guard, #507945d8).** Distinct from the build→read barrier above: the *inputs* to a build (instance-buffer copy → TLAS build in `tlas.rs`; skinned-vertex compute write → BLAS build in `draw.rs`) must be made visible with `SHADER_READ` at the `ACCELERATION_STRUCTURE_BUILD` stage, NOT `ACCELERATION_STRUCTURE_READ_KHR` (that flag reads an AS structure, not build inputs). The wrong flag is a copy/compute → build RAW hazard sync-validation catches — confirm via a `BYRO_VALIDATION` run, don't escalate on reasoning alone.
- **Deferred AS destruction vs in-flight reads (#a476b256).** BLAS eviction/drop routes the AS handle + buffers through `pending_destroy_blas` (deferred countdown), so an unload/eviction can't free an AS the in-flight frame's ray queries still read. Verify no path re-introduces an immediate `destroy_acceleration_structure` at the eviction site (use-after-free = CRITICAL) and that shutdown drains the queue.
- **Swapchain recreate sync.** `recreate_swapchain` (`context/resize.rs`) must cover all in-flight work with `device_wait_idle` (or equivalent fence drain) before destroying/rebuilding swapchain-dependent resources — no use-after-destroy across the recreate.
- **One-time command buffers** (BLAS initial build, staging copies) block the main thread on a fence — flag if any such blocking submit runs inside the per-frame hot path rather than at load time.
**Output**: `/tmp/audit/concurrency/dim_1.md`

### Dimension 2: Compute → AS → Fragment Chains
**Entry points**: `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` (refit path), `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/water_caustic.rs`, `crates/renderer/src/vulkan/volumetrics.rs`, `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/src/vulkan/material.rs`, `crates/renderer/src/vulkan/context/draw.rs` (master ordering)
**Checklist**:
- **Skin chain (M29).** Palette build (`skin_compute.rs`) → `COMPUTE_WRITE→SHADER_READ` → per-mesh skin output → BLAS refit (`blas_skinned.rs`) reads it → fragment ray query hits the refit BLAS. The full palette→skin→refit→ray-query chain must be intact; a drift = stale geometry in shadows/reflections/GI. The live raster path uses inline skinning in `triangle.vert` (not the SSBO output), so a `VERTEX_INPUT` barrier is **not** currently required — flag only if a raster-from-skinned-SSBO path is added without one.
- **Cross-frame ping-pong (no slot N reads slot N's in-flight write).** SVGF history, TAA history, caustic accumulator, water-caustic per-FIF `R32_UINT` accumulator, and volumetrics (`lighting_volumes` → `integrated_volumes`) all read the *previous* frame's slot. Verify the per-frame-in-flight indexing.
- **Volumetrics gate (#1105).** Injection writes `lighting_volumes` (COMPUTE) → integration reads it, writes `integrated_volumes` (COMPUTE→COMPUTE) → `composite.frag` samples it (COMPUTE_WRITE→FRAGMENT_READ). `write_tlas` must run before `dispatch` each gated frame — `volumetrics.rs` keeps a `tlas_written: [bool; MAX_FRAMES_IN_FLIGHT]` latch that `dispatch` `debug_assert!`s and then resets. Verify the latch set/reset symmetry.
- **Bloom within-frame RAW chain (#931).** Down-pyramid and up-pyramid each need a per-mip `COMPUTE_WRITE(SHADER_WRITE)→COMPUTE_READ(SHADER_READ)` image barrier so mip[i+1] sees mip[i]'s write; `up_mips[0]` must complete before composite samples it. Confirm the "post-barrier on the just-written mip only" accounting leaves no missing publish on the final up-mip.
- **Caustic CLEAR → COMPUTE → FRAGMENT.** Accumulator cleared before compute writes; compute completes before composite reads.
- **MaterialBuffer SSBO (R1).** `material.rs` upload is `HOST_WRITE → VERTEX/FRAGMENT_READ`; today it lands before draw recording so the frame fence already covers it — flag only if the upload moves into a compute path mid-frame.
**Output**: `/tmp/audit/concurrency/dim_2.md`

### Dimension 3: ECS Lock Ordering & Deadlock
**Entry points**: `crates/core/src/ecs/world.rs` (`query_2_mut`, `query_2_mut_mut`, the resource-pair queries), `crates/core/src/ecs/query.rs`, `crates/core/src/ecs/lock_tracker.rs`, all system functions under `byroredux/src/systems/` (`animation.rs`, `audio.rs`, `billboard.rs`, `bounds.rs`, `camera.rs`, `character.rs`, `debug.rs`, `light_anim.rs`, `metrics.rs`, `particle.rs`, `water.rs`, `weather.rs`)
**Checklist**:
- **TypeId-sorted acquisition is the deadlock-prevention invariant.** `world.rs` multi-component queries acquire storage locks in `TypeId`-ascending order regardless of the generic-parameter order the caller spells (the `if id_a < id_b { … } else { … }` branches in `query_2_mut`). The `lock_tracker` scope guards are set up in the *same* order so the lock-order graph never sees a spurious ABBA edge when the caller writes `<B, A>` with `TypeId(A) < TypeId(B)` (#313). Verify any new multi-lock accessor follows this and that same-type access still hits the `assert_ne!` panic.
- **`lock_tracker` coverage.** Same-thread re-entrant conflict detection is **always-on** in debug builds (`track_read`/`track_write` panic on a conflicting held lock). The cross-thread global lock-order graph is **opt-in via `BYRO_LOCK_ORDER_CHECK=1`** (`global_order` module) — so an ABBA risk between two *parallel* systems is only caught when that env var is set. Flag any CI/test path that should run with it but doesn't (see `docs/contributing.md` `lock-order-check` job).
- **Guard lifetime in system bodies.** No `query_mut`/`resource_mut` guard held across a call that re-enters the same storage/resource; nested query patterns (e.g. animation querying Player then Transform) must drop the first guard or use the paired `query_N_mut` accessor; no `World::insert` (structural mutation, `&mut self`) during system execution (systems hold `&World`).
- **Poisoning.** Storage `RwLock`s poison on panic; every acquisition resolves `PoisonError` through `storage_lock_poisoned::<T>()` (re-panics with a diagnostic). Confirm no acquisition path silently `unwrap()`s a poisoned guard into torn state.
**Output**: `/tmp/audit/concurrency/dim_3.md`

### Dimension 4: Scheduler Access Declarations (regression guard — M27 closed)
**Entry points**: `crates/core/src/ecs/scheduler.rs`, `crates/core/src/ecs/access.rs`, `byroredux/src/commands/world_info.rs` (`sys.accesses`)
**Status**: M27 (parallel dispatch) and R7 (access declarations) are **closed**
(ROADMAP.md). The `parallel-scheduler` feature is **on by default**; the
post-migration `sys.accesses` report is **0 unknown / 0 conflicts**. This
dimension is therefore a **regression guard**, not migration tracking.
**Checklist**:
- **The conflict model is sound and matches the enum.** `AccessConflict` has exactly three variants — `None`, `Unknown { left_undeclared, right_undeclared }`, `Conflict { pairs }` (`access.rs`); there is **no `Parallel` variant**. `analyze_pair` returns `Unknown` whenever either side is undeclared (the *pessimistic* fallback, not "no conflict"), `Conflict` on a write/read or write/write overlap on the same component or resource, `None` otherwise. Verify any new variant or analyzer change preserves the "undeclared ⇒ Unknown ⇒ assume serialise" semantics.
- **Migration KPIs.** `AccessReport::undeclared_parallel_count()` is the migration KPI (the population the analyzer can reason about); `undeclared_count()` = parallel + exclusive split via `undeclared_parallel_count()` + `undeclared_exclusive_count()` (#1237). `known_conflict_count()` and `unknown_pair_count()` must stay **0** on the engine binary — a regression here means a parallel-stage system was added without an `add_to_with_access` declaration (closures/bare-fns can't override `System::access`, so they need the registration-site channel; #1236).
- **Exclusive phase.** Exclusive systems run serially after the parallel batch (`StageData.exclusive`) — they're listed in the report but never paired; the four runtime-mutually-exclusive systems re-staged in M27 Phase 3 (audio, spin, character-mode dispatcher, `player_controller_system`) must stay exclusive. Flag any move back to parallel.
- **Re-entry & panic policy.** `Scheduler` is owned by `App`, never a `Resource` — re-entry from a system body is structurally impossible (#868). Panic-in-system is fail-fast by design (#1412); do not report "missing catch_unwind" as a bug.
**Output**: `/tmp/audit/concurrency/dim_4.md`

### Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step
**Entry points**: `crates/physics/src/sync.rs` (`physics_sync_system`, `set_linear_velocity`, `set_kinematic_translation`), `crates/physics/src/world.rs` (`PhysicsWorld`), `crates/physics/src/components.rs` (`RapierHandles`), `crates/physics/src/config.rs` (`ContactConfig`), `byroredux/src/cell_loader/unload.rs` (`release_victim_rapier_bodies`), `byroredux/src/systems/character.rs`
**Checklist**:
- **TypeId-sorting does NOT cover the Resource↔Storage pair.** The deadlock-prevention sort in `query_N_mut` orders *storage* locks; a Resource lock (`resource_mut`) and a storage lock (`query`/`query_mut`) are an unordered pair. So no `resource_mut::<PhysicsWorld>()` guard may be held across a `query`/`query_mut` iteration and vice-versa. `physics_sync_system` is a 4-phase system (collect newcomers → register → step → pull dynamic). Verify Phase 1 `collect_newcomers` collects to a `Vec` under read guards and **drops them** before `register_newcomers` takes the `PhysicsWorld` + `RapierHandles` write guards.
- **Helper lock order.** `set_linear_velocity` / `set_kinematic_translation` read `RapierHandles` via `world.query::<RapierHandles>()...copied()` (the read guard drops at the end of the `match`/`let` expression because the handle is `Copy`), *then* take `resource_mut::<PhysicsWorld>()`. Confirm the read guard is genuinely dropped before the write guard, and that callers (e.g. `character_controller_system`) don't already hold a `PhysicsWorld` guard when they call these.
- **`ContactConfig`** is read via `try_resource` (optional) and snapshotted once per batch in `register_newcomers` — confirm it is not re-locked inside the per-newcomer loop.
- **Cell-unload teardown (#1520).** `release_victim_rapier_bodies` (`unload.rs`) collects each victim's `RapierHandles` into a scratch `Vec` under the storage read guard, drops it, then removes bodies/colliders from `PhysicsWorld` — same release-reads-before-write discipline; verify it runs before the despawn loop drops the handles.
- **Single-threaded placement.** `physics_sync_system` runs in `Stage::Physics` after transform propagation and must not be co-scheduled (parallel) with any other system that touches `PhysicsWorld` / `RapierHandles` / `Transform`.
**Output**: `/tmp/audit/concurrency/dim_5.md`

### Dimension 6: Resource Lifecycle (GPU teardown ordering)
**Entry points**: `crates/renderer/src/vulkan/context/mod.rs` (Drop impl), all `destroy()` methods, `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/src/vulkan/context/resize.rs`, `crates/renderer/src/vulkan/egui_pass.rs`, `crates/renderer/src/vulkan/scene_buffer/`, `crates/renderer/src/vulkan/material.rs`
**Checklist**:
- **Reverse-order destruction** (Vulkan requirement); Drop reached for all GPU resources; allocator freed last. Note #1483 hoisted allocator-independent destroys out of the allocator-guard scope in Drop — verify no resource that *needs* the allocator is destroyed after the guard is dropped.
- **No use-after-destroy across swapchain recreate.** G-buffer / SVGF / TAA / caustic / water-caustic / volumetrics (`lighting_volumes` + `integrated_volumes`) / bloom (`down_mips` + `up_mips`) / composite resources rebuilt on recreate; per-FIF history/accumulator images freed for every in-flight slot; egui framebuffers rebuilt (`resize.rs`).
- **AS cleanup on shutdown.** All `BlasEntry` buffers + `TlasState` buffers + scratch released; per-skinned-entity skin output buffers retained until the owning entity is destroyed.
- **Other GPU SSBO/descriptor cleanup.** `scene_buffer` cleanup, `MaterialBuffer` SSBO (R1), texture registry, `EguiPass::destroy()` (releases egui-ash-renderer resources + framebuffers; `egui_pass: Option<EguiPass>` taken/dropped in reverse order in `context/mod.rs` Drop).
- **Per-frame leaks.** Any descriptor / command-buffer / staging allocation created per frame but not freed/reset is HIGH (compounds).
**Output**: `/tmp/audit/concurrency/dim_6.md`

### Dimension 7: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds
**Entry points**: `byroredux/src/streaming.rs` (M40 async pre-parse worker), `crates/debug-server/src/listener.rs` (per-client TCP threads), `crates/debug-server/src/system.rs` (`DebugDrainSystem`), `crates/debug-ui/src/lib.rs`, all types with `Send + Sync` bounds (`Component`, `Resource`), `crates/renderer/src/vulkan/allocator.rs` (`SharedAllocator`)
**Checklist**:
- **Streaming Drop ordering (#1167).** `WorldStreamingState::request_tx` is `Option<mpsc::Sender<LoadCellRequest>>` and `worker` is `Option<JoinHandle<()>>`. The `Drop` impl must `take()` + drop the Sender BEFORE the worker handle is dropped (declaration-order field drop would otherwise detach the worker before the channel closes, defeating the join). `shutdown(&mut self, timeout)` takes the worker handle so the later `Drop` safety-net observes `worker: None` and short-circuits. Verify the field-drop / `take()` ordering and the join-with-timeout path.
- **Worker ↔ main data flow.** Parsed cell payload moves to the main thread via channel — no shared `&mut World` from the worker. Off-thread NIF/texture extract goes through `Arc<TextureProvider>` whose inner `BsaArchive`/`Ba2Archive` serialise `File` access via Mutex (concurrent extracts safe). BGSM resolution (`&mut`, `MaterialProvider::merge_bgsm_into_mesh`) stays main-thread-only — confirm the worker doesn't touch it. NIF import cache (`Resource`) accessed from the worker must use a read-only fast path with write-back deferred to main.
- **Debug server.** Per-client TCP threads do **not** touch the World directly — all mutations route through `DebugDrainSystem` on the main thread (Late-stage exclusive). The command queue between listener and main thread must be bounded (no unbounded buffering on a slow main loop). Screenshot readback completes on a fence wait — verify no race between the drain system and present.
- **Allocator sharing.** `SharedAllocator = Arc<Mutex<vulkan::Allocator>>` (`allocator.rs`) is held by `VulkanContext` and cloned into `EguiPass` (`egui_pass.rs`), volumetrics, ssao, scene_buffer, etc. All dispatch runs single-threaded inside `draw_frame`; the only correctness concern is that no holder keeps the Mutex locked across a queue submit. The egui overlay dispatch runs after composite on the main loop.
- **`Send + Sync` bounds.** Component/Resource storage reached only through World query/resource guards; no raw pointer shared across threads; cxx-bridge pointer lifetimes bounded; Ruffle/wgpu (UI) device is `Send` but not `Sync` — confirm it stays on one thread.
- **Out of scope:** parse-time `Material` translation (`byroredux/src/material_translate.rs::translate_material`, `Material::resolve_pbr`) is single-threaded with no Mutex/RwLock/resource_mut — see `/audit-nifal` for that boundary.
**Output**: `/tmp/audit/concurrency/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/concurrency/dim_*.md` files
2. Combine into `docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
3. Remove cross-dimension duplicates

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
