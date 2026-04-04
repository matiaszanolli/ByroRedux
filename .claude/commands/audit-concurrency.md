---
description: "Audit ECS lock ordering, Vulkan queue sync, RwLock patterns, deadlock potential"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Concurrency and Synchronization Audit

Audit ByroRedux for deadlocks, race conditions, incorrect lock ordering, Vulkan synchronization gaps, and thread safety violations.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 4.
- `--depth shallow|deep`: `shallow` = check lock presence; `deep` = trace concurrent paths. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: ECS Locking | Vulkan Sync | Resource Lifecycle | Thread Safety
- **Trigger Conditions**: Exact timing/concurrency scenario needed to reproduce

## Phase 1: Setup

1. Parse `$ARGUMENTS`
2. `mkdir -p /tmp/audit/concurrency`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/concurrency/issues.json`

## Phase 2: Launch Dimension Agents

### Dimension 1: ECS Lock Ordering
**Entry points**: `crates/core/src/ecs/world.rs`, `crates/core/src/ecs/query.rs`, all system functions in `byroredux/src/main.rs`
**Checklist**: TypeId-sorted lock acquisition in multi-component queries, RwLock held across system function calls, query_mut dropping before next query, resource_mut scope, nested query patterns (animation_system queries Player then Transform), World::insert during system execution.
**Output**: `/tmp/audit/concurrency/dim_1.md`

### Dimension 2: Vulkan Synchronization
**Entry points**: `crates/renderer/src/vulkan/context.rs` (draw_frame), `crates/renderer/src/vulkan/sync.rs`, `crates/renderer/src/vulkan/acceleration.rs`
**Checklist**: Frame-in-flight fence wait before command buffer reuse, semaphore signaling order (acquire → render → present), TLAS build barrier before fragment shader read, descriptor set update timing (only safe after fence wait), swapchain recreate synchronization (device_wait_idle coverage), graphics_queue Mutex lock duration.
**Output**: `/tmp/audit/concurrency/dim_2.md`

### Dimension 3: Resource Lifecycle
**Entry points**: `crates/renderer/src/vulkan/context.rs` (Drop impl), all `destroy()` methods, `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/acceleration.rs`
**Checklist**: Reverse-order destruction (Vulkan requirement), Drop called for all GPU resources, no use-after-destroy during swapchain recreate, allocator freed last, BLAS/TLAS cleanup on shutdown, scene_buffer cleanup, texture registry cleanup.
**Output**: `/tmp/audit/concurrency/dim_3.md`

### Dimension 4: Thread Safety
**Entry points**: All types with `Send + Sync` bounds (Component, Resource), `Arc<Mutex<Allocator>>`, `Mutex<vk::Queue>`, Ruffle player (wgpu context)
**Checklist**: Component storage accessed only through World query guards, Resource access only through World resource guards, no raw pointer sharing across threads, cxx bridge pointer lifetimes, UI manager thread safety (wgpu device is Send but not Sync).
**Output**: `/tmp/audit/concurrency/dim_4.md`

## Phase 3: Merge

1. Read all `/tmp/audit/concurrency/dim_*.md` files
2. Combine into `docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
3. Remove cross-dimension duplicates

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`
