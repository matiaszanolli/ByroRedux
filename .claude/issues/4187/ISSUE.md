# CONC-D6-01: `SceneBuffers::terrain_tile_staging_pool` holds a non-optional `SharedAllocator` clone that nothing can release, so `Arc::try_unwrap` fails on every shutdown

Labels: high,sync,memory,renderer,bug

**Description**: `StagingPool` stores its allocator as a **bare** `allocator: SharedAllocator` (confirmed at `buffer.rs:145`), not the `Option<SharedAllocator>` every sibling GPU-resource type uses (`GpuBuffer`, `Texture`, `GpuImage`, `WaterPipeline`, `ExposureResource`). Its `destroy()` is just `self.trim_to(0)` — it frees pooled buffers but structurally cannot drop the `Arc` clone. `SceneBuffers` owns one such pool non-optionally (confirmed at `buffers.rs:189,995`), and `VulkanContext::scene_buffers` is a plain (non-`Option`) field, so that clone stays alive until the struct itself drops — after the `Arc::try_unwrap` in `teardown.rs`. The two sibling staging pools (`TextureRegistry::staging_pool` #732, `MeshRegistry::geometry_staging_pool` #1055) were both already given the `Option`+`take()` fix; this third pool, added by #3664 (2026-09-03, "make terrain tile uploads incremental"), was not.

**Evidence**:
`buffer.rs:145` — `allocator: SharedAllocator,` (bare field, confirmed); `buffer.rs:329-331` — `pub fn destroy(&mut self) { self.trim_to(0); }` — no allocator release; `buffers.rs:995` — `terrain_tile_staging_pool: StagingPool::new(device.clone(), allocator.clone())`; `context/mod.rs:1583` — `pub scene_buffers: scene_buffer::SceneBuffers,` — never `take()`n. `buffer.rs:1888-1906` (#927's own test doc) states the regression check is "the absence of the 'outstanding references' error log on engine shutdown" — a check this pool makes permanently unsatisfiable. `ROADMAP.md:1297-1307` records that log at 100% incidence (75/75 bench runs at HEAD).

**Impact**: `Arc::try_unwrap` fails -> `teardown.rs` logs "GPU allocator has N outstanding references", fires `debug_assert!(false, ...)` (a panic inside `Drop` on every debug-build exit), and returns early — deliberately leaking the `VkDevice`, `VkSurfaceKHR`, `VkInstance`, and debug messenger rather than doing a clean teardown. Every future real outstanding-reference regression is now masked by this permanent one. **Honest caveat**: #3664 (2026-09-03) predates the clean control commit `e6282349` (2026-09-07) that segfaulted 0/30 runs, so this defect alone does not explain the *segfault* regression — it does fully explain the outstanding-references log, and fixing it is a prerequisite for bisecting the actual segfault. Verify with `cargo run` (debug build), observe the `debug_assert!(false, ...)` panic on exit.

**Related**: #927 (the `Option<SharedAllocator>` mechanism + its regression test), #1055 (`MeshRegistry` fix), #732/LIFE-N1 (`TextureRegistry` fix), #665/LIFE-L1 (the leak-guard branch this trips), #1477/#1640 (the app-side `AllocatorResource` removal this defeats), #3664 (introduced the pool).

**Suggested Fix**: Change `StagingPool::allocator` to `Option<SharedAllocator>` and have `destroy()` `take()` it after `trim_to(0)` (matching `GpuBuffer::destroy`'s contract), fixing all three pools at the type level. Narrower alternative: make `SceneBuffers::terrain_tile_staging_pool` itself an `Option<StagingPool>` and `take()` it in `SceneBuffers::destroy`. Either way, add a source-shape pin (next to #927's `option_arc_dropped_when_set_to_none`) asserting no owned `StagingPool` field is reachable from `VulkanContext` without a `take()` in its owner's `destroy`.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
