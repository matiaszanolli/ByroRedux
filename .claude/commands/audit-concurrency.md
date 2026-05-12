---
description: "Audit ECS lock ordering, Vulkan queue sync, RwLock patterns, deadlock potential"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Concurrency and Synchronization Audit

Audit ByroRedux for deadlocks, race conditions, incorrect lock ordering, Vulkan synchronization gaps, and thread safety violations.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.
- `--depth shallow|deep`: `shallow` = check lock presence; `deep` = trace concurrent paths. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: ECS Locking | Vulkan Sync | Resource Lifecycle | Thread Safety | Compute → AS → Fragment Chains | Worker Threads (Streaming, Debug)
- **Trigger Conditions**: Exact timing/concurrency scenario needed to reproduce

## Phase 1: Setup

1. Parse `$ARGUMENTS`
2. `mkdir -p /tmp/audit/concurrency`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/concurrency/issues.json`

## Phase 2: Launch Dimension Agents

### Dimension 1: ECS Lock Ordering
**Entry points**: `crates/core/src/ecs/world.rs`, `crates/core/src/ecs/query.rs`, all system functions under `byroredux/src/systems/` (post-Session-34: `animation.rs`, `audio.rs`, `billboard.rs`, `bounds.rs`, `camera.rs`, `debug.rs`, `particle.rs`, `water.rs`, `weather.rs` — `systems.rs` itself is now a 27-line module index)
**Checklist**: TypeId-sorted lock acquisition in multi-component queries, RwLock held across system function calls, query_mut dropping before next query, resource_mut scope, nested query patterns (animation_system queries Player then Transform), World::insert during system execution.
**Output**: `/tmp/audit/concurrency/dim_1.md`

### Dimension 2: Vulkan Synchronization
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (draw_frame), `crates/renderer/src/vulkan/sync.rs`, `crates/renderer/src/vulkan/acceleration.rs`, `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/composite.rs`
**Checklist**: Frame-in-flight fence wait before command buffer reuse, semaphore signaling order (acquire → render → present), TLAS build barrier before fragment shader read (AS_WRITE → AS_READ, build stage → fragment stage), SVGF compute dispatch barrier (G-buffer write → compute read, compute write → composite read), TAA dispatch barrier (HDR + motion + mesh_id sampled-read → compute read; compute write → composite sampled-read), caustic CLEAR → COMPUTE → FRAGMENT chain (host→compute UBO upload barrier + clear→compute image barrier + compute→fragment composite read barrier), skin_compute COMPUTE → AS-BUILD → FRAGMENT chain (skin write → BLAS refit → ray-query read in fragment shader), descriptor set update timing (only safe after fence wait), swapchain recreate synchronization (device_wait_idle coverage), graphics_queue Mutex lock duration, BLAS build one-time command fence wait (blocks main thread).
**Output**: `/tmp/audit/concurrency/dim_2.md`

### Dimension 3: Resource Lifecycle
**Entry points**: `crates/renderer/src/vulkan/context/mod.rs` (Drop impl), all `destroy()` methods, `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/acceleration.rs`, `crates/renderer/src/vulkan/context/resize.rs`
**Checklist**: Reverse-order destruction (Vulkan requirement), Drop called for all GPU resources, no use-after-destroy during swapchain recreate, allocator freed last, BLAS/TLAS cleanup on shutdown (all BlasEntry buffers + TlasState buffers + scratch), G-buffer/SVGF/TAA/caustic/composite cleanup on swapchain recreate (per-frame-in-flight history images for TAA, accumulator images for caustic), per-skinned-entity SkinSlot output buffers retained until owning entity destroyed, scene_buffer cleanup, MaterialBuffer SSBO cleanup (R1), texture registry cleanup.
**Output**: `/tmp/audit/concurrency/dim_3.md`

### Dimension 4: Thread Safety
**Entry points**: All types with `Send + Sync` bounds (Component, Resource), `Arc<Mutex<Allocator>>`, `Mutex<vk::Queue>`, Ruffle player (wgpu context)
**Checklist**: Component storage accessed only through World query guards, Resource access only through World resource guards, no raw pointer sharing across threads, cxx bridge pointer lifetimes, UI manager thread safety (wgpu device is Send but not Sync).
**Output**: `/tmp/audit/concurrency/dim_4.md`

### Dimension 5: Compute → AS → Fragment Chains
**Entry points**: `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/acceleration.rs` (refit path), `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/context/draw.rs` (master ordering)
**Checklist**: Per-frame-in-flight ping-pong correctness across SVGF history, TAA history, and caustic accum (no slot N reads from slot N's in-flight write). M29.5 skin compute writes the per-skinned-mesh output buffer THEN BLAS refit reads it THEN fragment shader's ray queries hit the refit BLAS — three-stage barrier chain must be intact (drift = stale geometry in shadows / reflections / GI). M29.3 (when shipped): same buffer also flows COMPUTE → VERTEX, requires a `VK_PIPELINE_STAGE_VERTEX_INPUT_BIT` barrier — verify the additional usage flag is set. SVGF reads the previous frame's history (never the current's in-flight write); TAA same invariant for history slots. Caustic accumulator must be cleared BEFORE compute writes (CLEAR → COMPUTE), and compute writes must complete BEFORE composite reads (COMPUTE → FRAGMENT). MaterialBuffer SSBO upload (R1) is HOST_WRITE → VERTEX/FRAGMENT_READ; per-frame upload happens before draw record begins, fence wait already covers it but verify if upload moves into compute path.
**Output**: `/tmp/audit/concurrency/dim_5.md`

### Dimension 6: Worker Threads (Streaming, Debug Server)
**Entry points**: `byroredux/src/streaming.rs` (M40 async pre-parse worker), `crates/debug-server/src/listener.rs` (per-client TCP threads), `crates/debug-server/src/system.rs` (DebugDrainSystem)
**Checklist**: Streaming worker thread: shutdown drain joins cleanly (no detached thread leak on app exit), no `panic` propagation across the channel, parsed cell payload moves to main thread via channel without races on the World. NIF import cache (`Resource`) accessed from worker — verify locking discipline (read-only fast path, write-back deferred to main). Debug server: per-client threads do NOT touch the World directly — all mutations go through DebugDrainSystem on the main thread (Late-stage exclusive). Command queue between TCP listener and main thread is bounded (no unbounded buffering on slow main loop). Screenshot flow: GPU readback completes on a fence wait — verify no race between drain system and present.
**Output**: `/tmp/audit/concurrency/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/concurrency/dim_*.md` files
2. Combine into `docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
3. Remove cross-dimension duplicates

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
