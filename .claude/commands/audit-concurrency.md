---
description: "Audit ECS lock ordering, Vulkan queue sync, RwLock patterns, deadlock potential"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Concurrency and Synchronization Audit

Audit ByroRedux for deadlocks, race conditions, incorrect lock ordering, Vulkan synchronization gaps, and thread safety violations.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 7.
- `--depth shallow|deep`: `shallow` = check lock presence; `deep` = trace concurrent paths. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: ECS Locking | Vulkan Sync | Resource Lifecycle | Thread Safety | Compute → AS → Fragment Chains | Worker Threads (Streaming, Debug) | Physics Step Lock Ordering
- **Trigger Conditions**: Exact timing/concurrency scenario needed to reproduce

## Phase 1: Setup

1. Parse `$ARGUMENTS`
2. `mkdir -p /tmp/audit/concurrency`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/concurrency/issues.json`

## Phase 2: Launch Dimension Agents

### Dimension 1: ECS Lock Ordering
**Entry points**: `crates/core/src/ecs/world.rs`, `crates/core/src/ecs/query.rs`, all system functions under `byroredux/src/systems/` (post-Session-34: `animation.rs`, `audio.rs`, `billboard.rs`, `bounds.rs`, `camera.rs`, `character.rs`, `debug.rs`, `light_anim.rs`, `metrics.rs`, `particle.rs`, `water.rs`, `weather.rs` — `systems.rs` itself is now a thin (33-line) module index)
**Checklist**: TypeId-sorted lock acquisition in multi-component queries, RwLock held across system function calls, query_mut dropping before next query, resource_mut scope, nested query patterns (animation_system queries Player then Transform), World::insert during system execution.
**Output**: `/tmp/audit/concurrency/dim_1.md`

### Dimension 2: Vulkan Synchronization
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (draw_frame), `crates/renderer/src/vulkan/sync.rs`, `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/volumetrics.rs`, `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/composite.rs`
**Checklist**: Frame-in-flight fence wait before command buffer reuse, semaphore signaling order (acquire → render → present), TLAS build barrier before fragment shader read (AS_WRITE → AS_READ, build stage → fragment stage), SVGF compute dispatch barrier (G-buffer write → compute read, compute write → composite read), TAA dispatch barrier (HDR + motion + mesh_id sampled-read → compute read; compute write → composite sampled-read), caustic CLEAR → COMPUTE → FRAGMENT chain (host→compute UBO upload barrier + clear→compute image barrier + compute→fragment composite read barrier), volumetrics INJECT → INTEGRATE → composite-read chain (`volumetrics.rs`, M55, #1105): HOST→COMPUTE UBO upload barrier, injection (`lighting_volumes` write) → integration (read lighting, write `integrated_volumes`) → COMPUTE_WRITE→FRAGMENT_READ before `composite.frag` samples the integrated volume, plus the `tlas_written` per-frame-in-flight latch — `dispatch()` debug_asserts `write_tlas` ran for this slot first; verify the `dispatch()`-behind-`integrated`-consumed-const gate, bloom pyramid intra-frame mip barriers (`bloom.rs`, M58): per-level COMPUTE_WRITE(`SHADER_WRITE`)→COMPUTE_READ(`SHADER_READ`) `image_memory_barrier` between every downsample mip and every upsample mip (read-after-write within one command buffer; #931 emits only the post-barrier on the just-written mip), then `up_mips[0]` → composite sampled-read, skin_compute palette-build → skin → AS-BUILD → FRAGMENT chain (M29.5 `SkinPaletteComputePipeline` writes the bone palette → COMPUTE_WRITE→SHADER_READ → M29.3 `SkinComputePipeline` skin write → BLAS refit → ray-query read in fragment shader), descriptor set update timing (only safe after fence wait), swapchain recreate synchronization (device_wait_idle coverage), graphics_queue Mutex lock duration, BLAS build one-time command fence wait (blocks main thread).
**Output**: `/tmp/audit/concurrency/dim_2.md`

### Dimension 3: Resource Lifecycle
**Entry points**: `crates/renderer/src/vulkan/context/mod.rs` (Drop impl), all `destroy()` methods, `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/src/vulkan/context/resize.rs`, `crates/renderer/src/vulkan/egui_pass.rs` (`EguiPass`)
**Checklist**: Reverse-order destruction (Vulkan requirement), Drop called for all GPU resources, no use-after-destroy during swapchain recreate, allocator freed last, BLAS/TLAS cleanup on shutdown (all BlasEntry buffers + TlasState buffers + scratch), G-buffer/SVGF/TAA/caustic/volumetrics/bloom/composite cleanup on swapchain recreate (per-frame-in-flight history images for TAA, accumulator images for caustic, `lighting_volumes` + `integrated_volumes` froxel slots for volumetrics, `down_mips` + `up_mips` for bloom), per-skinned-entity SkinSlot output buffers retained until owning entity destroyed, scene_buffer cleanup, MaterialBuffer SSBO cleanup (R1), texture registry cleanup, debug-ui `EguiPass` teardown (`destroy()` releases the egui-ash-renderer Vulkan resources + framebuffers; `egui_pass: Option<EguiPass>` taken/dropped in reverse order in `context/mod.rs` Drop; recreated framebuffers on resize via `context/resize.rs`).
**Output**: `/tmp/audit/concurrency/dim_3.md`

### Dimension 4: Thread Safety
**Entry points**: All types with `Send + Sync` bounds (Component, Resource), `Arc<Mutex<Allocator>>`, `Mutex<vk::Queue>`, Ruffle player (wgpu context), `crates/debug-ui/src/lib.rs` (`EguiPassConfig`)
**Checklist**: Component storage accessed only through World query guards, Resource access only through World resource guards, no raw pointer sharing across threads, cxx bridge pointer lifetimes, UI manager thread safety (wgpu device is Send but not Sync), debug-ui allocator sharing — `EguiPassConfig.allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>` is a NEW consumer of the same allocator Mutex `VulkanContext` holds; the `EguiPass` is built/owned by the renderer (`egui_pass.rs`) and its overlay dispatch runs inside `draw_frame` (single-threaded main loop, after composite), so the only sharing concern is that the egui-ash-renderer holds the `Arc<Mutex<Allocator>>` clone for its whole lifetime — confirm no lock is held across a queue submit. Parse-time `Material` translation (`byroredux/src/material_translate.rs::translate_material`, `Material::resolve_pbr`) is single-threaded with no Mutex/RwLock/resource_mut — OUT of concurrency scope; see also `/audit-nifal` for the canonical translation tier.
**Output**: `/tmp/audit/concurrency/dim_4.md`

### Dimension 5: Compute → AS → Fragment Chains
**Entry points**: `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/acceleration/` (refit path), `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/volumetrics.rs`, `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/src/vulkan/context/draw.rs` (master ordering)
**Checklist**: Per-frame-in-flight ping-pong correctness across SVGF history, TAA history, caustic accum, and volumetrics (`lighting_volumes` → `integrated_volumes` per FIF) — no slot N reads from slot N's in-flight write. M29.5 palette-build compute writes the per-frame bone palette → COMPUTE_WRITE→SHADER_READ → M29.3 skin compute writes the per-skinned-mesh output buffer THEN BLAS refit reads it THEN fragment shader's ray queries hit the refit BLAS — the full chain (palette → skin → refit → ray-query) must be intact (drift = stale geometry in shadows / reflections / GI). M29.3 raster path: same skinned output buffer also flows COMPUTE → VERTEX (deferred — `triangle.vert` inline-skinning is the live path); when raster-from-SSBO ships, requires a `VK_PIPELINE_STAGE_VERTEX_INPUT_BIT` barrier — verify the additional usage flag is set. SVGF reads the previous frame's history (never the current's in-flight write); TAA same invariant for history slots. Caustic accumulator must be cleared BEFORE compute writes (CLEAR → COMPUTE), and compute writes must complete BEFORE composite reads (COMPUTE → FRAGMENT). Volumetrics injection writes `lighting_volumes` BEFORE integration reads it (COMPUTE → COMPUTE), integration writes `integrated_volumes` BEFORE composite samples it (COMPUTE → FRAGMENT), and `write_tlas` must run BEFORE `dispatch` each frame the integrated-volume gate is on (`tlas_written` per-FIF latch, debug_asserted; #1105). Bloom is a within-frame RAW chain (no cross-frame history): the down-pyramid and up-pyramid each need a per-mip COMPUTE_WRITE→COMPUTE_READ barrier so mip[i+1] sees mip[i]'s write, and `up_mips[0]` must complete before composite samples it — verify the #931 "post-barrier on the just-written mip only" accounting holds (no missing publish on the final up-mip). MaterialBuffer SSBO upload (R1) is HOST_WRITE → VERTEX/FRAGMENT_READ; per-frame upload happens before draw record begins, fence wait already covers it but verify if upload moves into compute path.
**Output**: `/tmp/audit/concurrency/dim_5.md`

### Dimension 6: Worker Threads (Streaming, Debug Server)
**Entry points**: `byroredux/src/streaming.rs` (M40 async pre-parse worker), `crates/debug-server/src/listener.rs` (per-client TCP threads), `crates/debug-server/src/system.rs` (DebugDrainSystem)
**Checklist**: Streaming worker thread: shutdown drain joins cleanly (no detached thread leak on app exit) — **#1167 Drop-ordering pin**: `WorldStreamingState::request_tx` is `Option<mpsc::Sender<LoadCellRequest>>` and the `Drop` impl `take()`s + drops the Sender BEFORE the `worker: Option<JoinHandle<()>>` is dropped (declaration-order field-drop would otherwise drop/detach the worker before the channel closes, defeating the join); `shutdown(&mut self)` takes the worker handle so the later `Drop` safety-net observes `worker: None` and short-circuits. No `panic` propagation across the channel, parsed cell payload moves to main thread via channel without races on the World. Worker off-thread NIF/texture extract uses `Arc<TextureProvider>` whose inner `BsaArchive`/`Ba2Archive` serialise `File` access via `Mutex` (concurrent extracts are safe); BGSM resolution (`MaterialProvider::merge_bgsm_into_mesh`, `&mut`) stays main-thread-only — worker does NOT touch BGSM. NIF import cache (`Resource`) accessed from worker — verify locking discipline (read-only fast path, write-back deferred to main). Debug server: per-client threads do NOT touch the World directly — all mutations go through DebugDrainSystem on the main thread (Late-stage exclusive). Command queue between TCP listener and main thread is bounded (no unbounded buffering on slow main loop). Screenshot flow: GPU readback completes on a fence wait — verify no race between drain system and present.
**Output**: `/tmp/audit/concurrency/dim_6.md`

### Dimension 7: Physics Step Lock Ordering
**Entry points**: `crates/physics/src/sync.rs` (`physics_sync_system`, `set_linear_velocity`, `set_kinematic_translation`), `crates/physics/src/world.rs` (`PhysicsWorld`), `crates/physics/src/components.rs` (`RapierHandles`), `crates/physics/src/config.rs` (`ContactConfig`), `byroredux/src/systems/character.rs` (KCC consumer, M28.5)
**Checklist**: `physics_sync_system` is a 4-phase ECS system (register newcomers → push kinematic → step → pull dynamic) that takes sequential write locks (`resource_mut::<PhysicsWorld>()`) interleaved with `query`/`query_mut::<RapierHandles>()` and `try_resource::<ContactConfig>()`. Verify the documented "release read locks before acquiring write locks" discipline holds: Phase 1 `collect_newcomers` collects to a `Vec` under read guards, drops them, THEN `register_newcomers` takes the `PhysicsWorld` write guard + the `RapierHandles` write guard — no read guard on the same storage may still be live when the write guard is taken (would deadlock under the RwLock). No `resource_mut::<PhysicsWorld>()` guard held across a `query`/`query_mut` iteration over a component storage and vice-versa (TypeId-sorted acquisition does not cover the Resource↔Storage lock pair). `ContactConfig` is read via `try_resource` (optional) and snapshotted once per batch — confirm it is not re-locked inside the loop. Single-threaded main-loop placement: the system runs after `transform_propagation_system` and is not dispatched concurrently with any other system that touches `PhysicsWorld` / `RapierHandles` / `Transform`. The `set_linear_velocity` / `set_kinematic_translation` helpers each take a `RapierHandles` read guard then a `PhysicsWorld` write guard — verify the read guard is dropped (out of the `match`) before the write guard, and that callers (e.g. `character_controller_system`) don't already hold a `PhysicsWorld` guard.
**Output**: `/tmp/audit/concurrency/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/concurrency/dim_*.md` files
2. Combine into `docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
3. Remove cross-dimension duplicates

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
