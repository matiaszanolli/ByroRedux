# INVESTIGATION — Issue #732

## What the destroy paths actually do

Audited the three deferred-destroy queues against the user's
"countdown never drains on shutdown" hypothesis:

* `AccelerationManager::destroy` — already drains `pending_destroy_blas`
  via `.drain(..)` (acceleration.rs:2377), per #639/LIFE-H1.
* `MeshRegistry::destroy_all` — already drains `deferred_destroy`
  (mesh.rs:412).
* `TextureRegistry::destroy` — already drains each entry's
  `pending_destroy` (texture_registry.rs:764).

So when `ctx.Drop` runs, the deferred queues are emptied as part of
the destroy chain. **The user's literal hypothesis ("queue never
drains") is incorrect** — but the symptom (22 outstanding allocator
Arc refs, SIGSEGV after "Vulkan context destroyed cleanly") is real.

## Where the 22 refs likely come from

`Arc<Mutex<Allocator>>` clones live in every `GpuBuffer`,
`Texture`, and `StagingPool`. Long-lived framework subsystems hold
many such structs in `Vec<GpuBuffer>` or `Option<...>` fields:

* `SceneBuffers` — 6 `Vec<GpuBuffer>` + 1 `GpuBuffer` (terrain).
* `SsaoPipeline.param_buffers` — `Vec<GpuBuffer>`, never cleared.
* `TextureRegistry.staging_pool` — `Option<StagingPool>`, never taken.

Each of those subsystems' `destroy()` methods correctly free the
GPU allocation (sets each buffer's `allocation: None`) but leave the
struct in place. The `SharedAllocator` field inside the struct still
holds an Arc clone. Those clones only release when the parent
struct naturally drops — which happens AFTER `VulkanContext::Drop`
returns, AFTER `allocator.take()` already failed `Arc::try_unwrap`.

## Why streaming surfaces it

Pre-streaming, the framework Arc count was the same (~22). The
shutdown still leaked the allocator. But the *natural* field drop
that runs after the manual `Drop` impl just released those Arcs
without touching the device — `GpuBuffer::Drop` checks
`allocation.is_none()` first and returns early, so a destroyed
device is never re-touched. No SIGSEGV.

With M40 streaming, the per-cell unload sweep added in 7dc354a moves
cell resources into the registries' deferred-destroy queues, which
the registries' `destroy()` methods drain. That part is fine.

The user reports the SIGSEGV happens after "Vulkan context destroyed
cleanly". The 22 Arc refs persist regardless — they're framework
overhead, not per-cell. The SIGSEGV must come from something
exposed by the streaming workload that has `allocation: Some` at
natural-drop time. Without a runtime repro it's hard to pinpoint
which buffer leaks.

## Plan

Implement the user's option 2 verbatim — add explicit
`drain_pending_destroys` methods on the three registries and call
them from the App shutdown sweep before `self.renderer.take()`. This
ensures all per-cell deferred-destroy queue items are processed
synchronously *before* `ctx.Drop` runs, instead of relying on the
in-Drop drain. The drain happens at the same point semantically but
moves the work outside the `if let Some(ref alloc)` block in `Drop`,
which keeps the `ctx.Drop` path tighter and matches the user's
prescription. If the SIGSEGV persists after this lands, the next
follow-up should target the framework-Arc-count side (clear the Vec
fields in `SceneBuffers::destroy` / `SsaoPipeline::destroy`, take
the `staging_pool` Option in `TextureRegistry::destroy`).

Pure-Rust unit tests for each drain method — they're queue
manipulation only, no Vulkan device needed.
