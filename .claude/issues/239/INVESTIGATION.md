# Investigation — Issues #239 + #511

## Domain
Renderer — Vulkan staging buffer pool wiring for textures.

## Premise verification (audit hygiene)

✓ **Premise stronger than audit claimed**. `StagingPool` is **currently vestigial**:

- `StagingPool::new` is never called anywhere in the workspace (grep confirmed — zero construction sites).
- Every `create_device_local_buffer` / `create_vertex_buffer` / `create_index_buffer` call site passes `None` for `staging_pool`. The mesh scene-load at `scene.rs:496` has a `TODO: thread StagingPool through scene load (#242)` acknowledging this.
- **Even if a pool were passed**: `create_device_local_buffer` at buffer.rs:783 calls `staging.destroy()` unconditionally, which runs `destroy_buffer` + `allocator.free` — never releases back. So the pool's `release` arm is dead code at this moment.

So the fix must both (a) wire textures and (b) fix the release-back mechanism so the pool actually reuses.

## Fix plan

### 1. buffer.rs

- Add `StagingGuard::release_to(self, pool: &mut StagingPool, capacity: vk::DeviceSize)` — moves the buffer+allocation into the pool's free list. Disarms the guard (`allocation.take()`) so Drop is a no-op.
- Modify `create_device_local_buffer`'s success path: when `staging_pool` is `Some`, capture `allocation.size()` BEFORE moving, then call `staging.release_to(pool, capacity)` instead of `staging.destroy()`.
- Bump `DEFAULT_STAGING_BUDGET_BYTES` from 64 MB → 128 MB (closes #511). Rationale: BC7 4K with full mips ≈ 22 MB; 20-texture burst ≈ 100 MB. 64 MB forces mid-burst eviction, defeating the pool's purpose. 128 MB absorbs a typical interior-to-exterior transition without eviction, still caps retained capacity so long sessions don't balloon. Update doc comment.

### 2. texture.rs

- Add `staging_pool: Option<&mut StagingPool>` parameter to `Texture::from_rgba`, `Texture::from_bc`, `Texture::from_dds` (threaded through to both inner calls).
- Replace the inline `allocator.allocate + bind_buffer_memory` prelude in `from_rgba` and `from_bc` with pool.acquire if pool is present, fallback to the existing inline path.
- Success path: if pool present, `release_to`; else `destroy()`.

### 3. texture_registry.rs

- New field `staging_pool: StagingPool` owned by `TextureRegistry`.
- `TextureRegistry::new` gains a `SharedAllocator` parameter to construct the pool (requires `device` clone + `allocator` clone at pool init — both trivially cheap).
- `load_dds` / `register_rgba` / `replace_rgba_at` all pass `Some(&mut self.staging_pool)` to the underlying Texture creates.
- `TextureRegistry::destroy` calls `self.staging_pool.destroy()` first.

### 4. context/mod.rs

- Update the single `TextureRegistry::new` caller to pass `gpu_allocator.clone()`.
- The fallback Texture::from_rgba at line 480 keeps `None` (one-shot 256×256 checkerboard, no pool benefit).

## Tests

- `buffer.rs`: new unit test `pool_acquire_release_reuses_same_buffer` — stub the Vulkan calls? Actually the pool touches real Vulkan (`create_buffer`, `allocate`). Pure-function testing is limited to `select_evictions`. For the release/reuse cycle, a compile-tested assertion that `StagingGuard::release_to` doesn't double-free via Drop is the realistic coverage — the method explicitly `.take()`s the allocation so Drop skips.
- Runtime coverage: FNV Prospector bench re-run should show cell-load time unchanged or improved (no regression).

## Scope
4 files. Within #239's stated scope + absorbs #511's 1-line budget bump.

## Deferred
Mesh upload pool wiring (#242 — the existing scene.rs TODO). Same pattern would apply, but scoped separately.
