# FNV-D3-01: SkyParamsRes cloud + sun textures leak on every cell unload

## Finding: FNV-D3-01

- **Severity**: HIGH
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Game Affected**: All games with weather (FNV/FO3/Skyrim/FO4) — exterior cells in particular
- **Locations**:
  - Acquire: [byroredux/src/scene.rs:235, 278, 321, 344, 377](byroredux/src/scene.rs#L235) — five `texture_registry.load_dds` calls
  - Storage: `SkyParamsRes` (Resource, not an ECS component)
  - Release path: [byroredux/src/cell_loader.rs:189-321](byroredux/src/cell_loader.rs#L189-L321) — `unload_cell` only walks ECS components

## Description

`scene.rs` loads 4 cloud-layer textures + 1 sun texture via `texture_registry.load_dds`, each bumping refcount from 0→1 (per [crates/renderer/src/texture_registry.rs:331-335](crates/renderer/src/texture_registry.rs#L331-L335)). The resulting handles land in `SkyParamsRes`, a Resource (not an ECS component).

`unload_cell` walks ECS components only — `MeshHandle`, `TextureHandle`, `NormalMapHandle`, `DarkMapHandle`, `ExtraTextureMaps`, `TerrainTileSlot`. `SkyParamsRes` is never touched, so its texture refcounts are never released.

Each WastelandNV reload leaks ~5 textures (4-8 MB DDS each). The leak is invisible today because cells don't unload during normal play, but **M40 doorwalking will hemorrhage VRAM** at every cell-cell transition.

The `texture_count` `stats` field reports the global registry size (FNV-D5-02), so this leak is also invisible in current observability.

## Suggested Fix

In `unload_cell`, after the entity-component sweep but before the GPU drops, release sky resources:

```rust
// Add to byroredux/src/cell_loader.rs unload_cell
if let Some(sky) = world.try_resource::<SkyParamsRes>() {
    for idx in [
        sky.cloud_texture_index,
        sky.cloud_texture_index_1,
        sky.cloud_texture_index_2,
        sky.cloud_texture_index_3,
        sky.sun_texture_index,
    ] {
        if idx != 0 {
            ctx.texture_registry.drop_texture(&ctx.device, idx);
        }
    }
}
world.remove_resource::<SkyParamsRes>();
```

Same pattern for `CellLightingRes` / `WeatherDataRes` / `WeatherTransitionRes` (state leak, not VRAM, but worth including for consistency).

## Related

- FNV-D3-02 (companion) — terrain splat layer refcounts also leak.
- FNV-D5-02 — `texture_count` stats are registry-wide; this leak is invisible until that's split into `textures_in_use`.
- M40 doorwalking — once that lands, this becomes a per-transition regression.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other Resources holding texture handles (CellLightingRes / WeatherDataRes / WeatherTransitionRes); apply the same release pattern.
- [ ] **DROP**: N/A — `drop_texture` already handles the Vulkan resource cleanup.
- [ ] **LOCK_ORDER**: N/A — `unload_cell` already takes `&mut World` and `&mut VulkanContext`.
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a leak-counter regression — load WastelandNV, unload, load again, assert `TextureRegistry.live_count` returned to baseline.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
