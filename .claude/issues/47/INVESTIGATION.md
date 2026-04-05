# Investigation: #47 — Staging buffer allocate/free per upload

## Current State
Each mesh/texture upload creates a staging VkBuffer + gpu_allocator Allocation,
copies data, then destroys both. A cell with 500 meshes + 200 textures = ~1200
allocate/free cycles through gpu_allocator.

StagingGuard (added in #87) wraps the lifecycle but doesn't reuse buffers.

## Fix Plan
Add StagingPool to buffer.rs:
- Pool of reusable (VkBuffer, Allocation) pairs sorted by size
- `acquire(size)` → returns existing buffer >= size, or creates new
- `release(buffer, allocation)` → returns to pool for reuse
- `destroy_all()` for cleanup

Update StagingGuard to use the pool instead of direct create/destroy.
Update create_device_local_buffer and texture upload paths.

## Scope
2 files: buffer.rs (StagingPool + StagingGuard update), texture.rs (pass pool)
Callers in context.rs/main.rs need pool plumbing — this touches >5 files.

Actually, simpler approach: make StagingPool a standalone resource, and have
StagingGuard::new accept a pool reference. The pool is created once on VulkanContext.
