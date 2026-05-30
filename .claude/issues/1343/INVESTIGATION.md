# Investigation — #1343 (D3-02): terrain splat-layer refcount leak on early returns

## Root cause (confirmed)
`spawn_terrain_mesh` (`cell_loader/terrain.rs`) calls `build_cell_splat_layers` (terrain.rs:273)
which `resolve_texture`s up to 8 LTEX→TXST splat textures (one refcount bump per resolved layer,
0 for unresolved LTEX). Those handles only reach an unload-droppable owner at
`allocate_terrain_tile` (terrain.rs:~430, which makes a `TerrainTileSlot` whose
`free_terrain_tile` releases them on unload). Between the acquire and that bind there are two
early returns — the no-allocator guard and the mesh-upload `Err` — that bail without allocating a
slot, so the splat refcounts have no owner the unload walk can reach → leaked for the process
lifetime. Trigger paths are degraded states (headless/test allocator-absent; OOM / mesh-budget
mesh-upload failure), so it's not steady-state, but a real leak when it fires.

## Audit's primary suggestion was infeasible
The audit (D3-02) first suggested "move `build_cell_splat_layers` below the mesh upload — it has
no dependency on the mesh." It DOES: the vertex loop (terrain.rs:316) reads
`splat_layers.layers` to pack per-vertex splat WEIGHTS, and that loop runs *before* both early
returns. So the layers must be resolved up front. Took the audit's second option instead:
release the acquired refcounts on each early-return path.

## Fix
- Snapshot the layer texture indices into a `Vec<u32>` right after `build_cell_splat_layers`
  (so the release doesn't re-borrow `splat_layers`, still needed by the vertex loop).
- On both early returns (no-allocator, mesh-upload `Err`), call `release_splat_layer_textures`,
  which `drop_texture`s each non-`0` / non-`fallback` index — the SAME skip rule the unload-side
  `free_terrain_tile` → `push_tex_drop` sweep uses, so a missing-texture fallback isn't
  over-released. The success path is untouched (it hands the indices to `allocate_terrain_tile`),
  so there is no double-release.
- Restructured the no-allocator guard from `let allocator = ctx.allocator.as_ref()?;` to an
  `if ctx.allocator.is_none()` bail + inline `ctx.allocator.as_ref().unwrap()` in the upload call,
  so no immutable `&ctx.allocator` borrow is held across the match (the `Err` arm needs
  `&mut ctx` for the release).

## Sibling completeness
The base `tex_handle` (terrain.rs:~378) is acquired AFTER both early returns, so it isn't leaked
by them. The two guards are the only early returns in `spawn_terrain_mesh`; everything after
`allocate_terrain_tile` is infallible. No other pre-slot acquisition exists.

## Test
The drop logic needs a `VulkanContext`, but the skip rule is pure: `splat_indices_to_release`
unit-tested for (a) only real non-0/non-fallback indices released, (b) empty / all-zero → empty.
Mirrors the #1339 `sky_textures_to_release` approach.

## Files (1)
- `cell_loader/terrain.rs` — snapshot + `splat_indices_to_release` / `release_splat_layer_textures`
  helpers + release on both early returns + 2 unit tests.
