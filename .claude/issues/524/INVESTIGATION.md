# #524 Investigation

## Current behavior

`TextureRegistry.path_map` caches `path → handle` but carries no refcount. `load_dds` cache-hit returns the existing handle without incrementing anything. `drop_texture` unconditionally:
- moves the texture into `pending_destroy`,
- redirects the bindless slot to the fallback,
- purges `path_map` entries pointing at the handle.

`unload_cell` collects texture handles via a HashSet (dedup) from `TextureHandle` components of victim entities and calls `drop_texture` once per unique handle.

Consequence: if cell A loads chair.dds (→ handle 42) and cell B later also resolves chair.dds (→ same handle 42, no bump), then cell A unloads, `drop_texture(42)` frees it — but cell B still references handle 42 and now sees the fallback checkerboard.

## Scope of the refcount contract

Each `resolve_texture` call that returns a non-fallback handle acquires one ref. Each `drop_texture` call releases one ref. Symmetric: the cell loader's load-side calls `resolve_texture` once per entity's base-texture slot → the unload-side calls `drop_texture` once per entity's `TextureHandle` component (drop the HashSet dedup).

Ancillary texture slots (`NormalMapHandle`, `DarkMapHandle`, `ExtraTextureMaps` with 6 fields) are acquired via `resolve_texture` per entity during `spawn_placed_instances` (cell_loader.rs:1669,1676,1688). Each bump needs a matching drop on unload. Extend `unload_cell` to walk these three component storages too.

`register_rgba` (UI/dynamic paths) bypasses the path cache and is not tracked by entity components. Start at `ref_count = 1`; if never dropped, lives for the process lifetime (no regression from current behavior).

Fallback handle is process-wide. Keep its refcount acquisition a no-op — cell_loader already filters `th.0 != fallback` before dropping.

## Fix plan

**texture_registry.rs**
- Add `ref_count: u32` to `TextureEntry`.
- Initialize `ref_count = 1` at every registration site (fallback, `load_dds`, `register_rgba`).
- `load_dds` cache hit: bump `ref_count += 1` before returning.
- New `acquire_by_path(&mut self, path) -> Option<TextureHandle>`: like `get_by_path`, but bumps on hit. Used by `resolve_texture`.
- `drop_texture`: decrement; return early if still > 0; else run existing drop logic (including `path_map.retain`).
- Update the test harness constructor (`make_registry_for_overflow_test`) to include `ref_count`.
- Add unit tests covering the sharing scenario.

**asset_provider.rs**
- `resolve_texture`: swap `get_by_path` → `acquire_by_path` so the short-circuit path bumps too.

**cell_loader.rs**
- `unload_cell`: replace `texture_handles: HashSet<u32>` with `Vec<u32>` (no dedup). Walk `NormalMapHandle`, `DarkMapHandle`, `ExtraTextureMaps` storages and push their non-zero slots onto the drop list. Call `drop_texture` once per entry.

Files touched: 3. Within scope.

## LOCK_ORDER / DROP / SIBLING

- **LOCK_ORDER**: `TextureRegistry` is accessed via `&mut VulkanContext.texture_registry`, not through the ECS Resource table — no RwLock. Refcount updates are serialized by the `&mut` borrow.
- **DROP**: `drop_texture` only calls `pending_destroy.push_back` when refcount hits zero. Ensures `TextureEntry` GPU resources are freed exactly once.
- **SIBLING**: Mesh registry (`MeshRegistry::drop_mesh`) does not dedup across cells in practice — parsed meshes are per-NIF-per-call uploads. Out-of-scope for this fix. BLAS (`AccelerationManager::drop_blas`) is keyed by mesh handle, inherits mesh lifetime. Out-of-scope.
