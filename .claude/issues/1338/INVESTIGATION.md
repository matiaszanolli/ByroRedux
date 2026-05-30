# Investigation — #1338 (D3-01): Water-plane normal-map texture refcount leak

## Domain
Cell loading (byroredux) + texture-registry refcount lifecycle.

## Root cause (confirmed)
`spawn_water_plane` (`byroredux/src/cell_loader/water.rs:187-201`) calls
`resolve_texture(...)` for the WATR TNAM normal map, which bumps the texture-registry
refcount (`acquire_by_path`/`load_dds`, one ref per call — #524: "each resolve pairs with
one drop_texture on cell unload"). The resolved index is stored **only** in
`WaterMaterial.normal_map_index` (water.rs:200). The spawned water entity carries
`Transform`/`GlobalTransform`/`MeshHandle`/`WaterPlane`/`WaterVolume`/`RenderLayer::Decal`
but **no texture-handle component**.

The unload victim walk (`byroredux/src/cell_loader/unload.rs:76-118`) drops textures only
via the `MeshHandle`/`TextureHandle`/`NormalMapHandle`/`DarkMapHandle`/`ExtraTextureMaps`
component queries (+ terrain tile slots). The water normal map is reachable through none of
them, so `drop_texture` is never called for it → one leaked refcount + bindless slot per
water-bearing cell unloaded.

## Why reusing `NormalMapHandle` is render-safe
The water entity has a `MeshHandle`, so the static-mesh emit loop
(`render/static_meshes.rs:129`) DOES produce a `DrawCommand` for it and reads
`NormalMapHandle` at line 179 to set that command's `normal_map_index`. But
`reemit_water_planes` (`render/water.rs:38`) then flips the command's `is_water` flag so the
regular triangle path **skips** it, and the water pipeline draws the plane from
`WaterPlane.material.normal_map_index` via `WaterDrawCommand`/`WaterPush` instead. So the
`NormalMapHandle` value never reaches a visible draw — it serves purely as the unload walk's
drop anchor. Verified: no second consumer of `NormalMapHandle` would mis-handle the water
entity.

## Pairing correctness
- Acquire is gated on `resolved_normal_idx != 0` (water.rs:199); attach the handle under the
  same gate so the acquire/drop counts match.
- `push_tex_drop` (unload.rs:57-61) skips `handle == 0 || handle == fallback_tex`, so a
  fallback/neutral resolve is not over-dropped — consistent with every other handle sweep.
- The entity is spawned inside the `CellRoot` `[first..last)` ID span, so it is a victim of
  `unload_cell` and the new `NormalMapHandle` is reached.

## Fix
Attach `NormalMapHandle(resolved_normal_idx)` on the water entity when
`resolved_normal_idx != 0`, right after the `WaterPlane` insert. Add
`use crate::components::NormalMapHandle;` to water.rs.

## Sibling check
`DarkMapHandle`/`ExtraTextureMaps` are not resolved by the water path (water uses only the
single normal map), so no sibling leak in this spawner. The greyscale-LUT omission is a
separate component-walk gap (#1341 / D3-05).
