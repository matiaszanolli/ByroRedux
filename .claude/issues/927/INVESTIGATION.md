# Investigation — #927 (REN-LIFE-NEW-01)

## Findings

`SharedAllocator = Arc<Mutex<vulkan::Allocator>>`. Only three struct types
hold an `allocator: SharedAllocator` clone as a field:

1. `GpuBuffer` (`crates/renderer/src/vulkan/buffer.rs:401`)
2. `Texture` (`crates/renderer/src/vulkan/texture.rs:28`)
3. `StagingPool` (`crates/renderer/src/vulkan/buffer.rs:73`)

`StagingPool` is properly `.take()`d by `TextureRegistry::destroy()`, so its
clone drops in time.

The leak comes from `GpuBuffer` and `Texture`: their `destroy(&mut self, ...)`
methods free the underlying GPU `Allocation` but never drop the struct's own
`allocator: SharedAllocator` field. The Arc clone stays alive until the
GpuBuffer / Texture struct itself drops — which happens via natural Drop on
the *containing* struct, *after* `VulkanContext::Drop` has already run
`Arc::try_unwrap`.

## Suspect leakers (8 total in the issue body)

Static analysis identified at least 6 deterministic clone retainers:

- `SceneBuffers.ray_budget_buffer: GpuBuffer` — direct field, never dropped before
  `VulkanContext::Drop` returns. **1 leak.**
- `SceneBuffers.terrain_tile_buffer: GpuBuffer` — same shape. **1 leak.**
- `ClusterCullPipeline.cluster_grid_buffers: Vec<GpuBuffer>` — `destroy()`
  iterated `&mut` but never `.clear()`'d the Vec. With `MAX_FRAMES_IN_FLIGHT = 2`
  that's 2 retained GpuBuffer structs. **2 leaks.**
- `ClusterCullPipeline.light_index_buffers: Vec<GpuBuffer>` — same. **2 leaks.**

Total accounted for: 6. The remaining 2 are likely in deferred-destroy queue
residuals or texture-registry edge cases; the structural fix below catches them
universally.

## Fix shape

Approach A (structural): make every Arc retainer release its clone *during* its
own `destroy()` call, regardless of when the holder struct itself drops.

- `GpuBuffer.allocator: SharedAllocator` → `Option<SharedAllocator>`.
- `Texture.allocator: SharedAllocator` → `Option<SharedAllocator>`.
- Both `destroy()` impls now set `self.allocator = None` after freeing the GPU
  allocation. The wrapped Arc drops immediately.
- Drop's safety-net branch handles the new `None` case explicitly (logs an error
  if `allocation` is `Some` while `allocator` is `None` — an invariant
  violation; the canonical flow keeps them in lockstep).

Approach A is preferable to per-container `Option<...>` / `.clear()` plumbing
because it catches every current and future Arc retainer in two files instead
of N.

Belt-and-braces: `ClusterCullPipeline::destroy` now matches every other
pipeline's pattern and calls `.clear()` on its two `Vec<GpuBuffer>` fields.
With the per-buffer Arc release in place this is no longer strictly necessary,
but the consistency keeps the Vec from retaining empty-shell GpuBuffers between
destroy() and the natural Drop.

## Files touched

1. `crates/renderer/src/vulkan/buffer.rs` — GpuBuffer field → Option, destroy() releases, Drop handles None
2. `crates/renderer/src/vulkan/texture.rs` — Texture: same pattern
3. `crates/renderer/src/vulkan/compute.rs` — ClusterCullPipeline destroy clears its two Vecs

Plus regression test in buffer.rs that pins the `Option<Arc<_>> = None drops
the inner Arc` language semantics the fix relies on.

## Verification path

Integration: re-run the Markarth load and grep the shutdown log for
`outstanding references`. Pre-fix → "GPU allocator has 8 outstanding references".
Post-fix → expect the log line to vanish (clean `Arc::try_unwrap`).

Unit: `cargo test -p byroredux-renderer` — 1912 tests pass.

If the integration test still shows residual references (count < 8 but > 0),
the remaining clones are in code paths static analysis missed; the same
Option-release pattern can be applied there in a follow-up.
