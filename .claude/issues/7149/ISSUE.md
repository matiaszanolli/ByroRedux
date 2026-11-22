# #7149: TSD: BTXT-only terrain cells get no tile slot, so no height map, parallax or displacement

Labels: enhancement,renderer,terrain-exterior

## Problem
A terrain cell with no splat layers (only base textures, BTXT) gets no terrain tile slot. `cell_loader/terrain.rs` allocates one only when `splat_layers.layers` is non-empty ("Allocate a terrain tile slot only when the cell actually has splat layers"). The cell then renders on the single-texture path, and its generated base height map (#6195) is never bound. So:

- TSD-3 parallax (#6197) does nothing on these cells; #6195 deferred binding their base map "to Phase 3", and Phase 3 did not pick it up.
- TSD-4 displacement patches (#6198 design note, risk 5) could not be placed on them either.

## Fix
Allocate a tile slot for BTXT-only cells too, carrying the base height index (with no splat layers), so the blended height function sees the base layer alone. The slot is freed in `unload_cell` like any other.

Check the cost first: one more `GpuTerrainTile` per BTXT-only cell against the tile-slot capacity, and the per-tile height work in `terrain_height::acquire_spawn_heights`.

## Acceptance
- A BTXT-only cell binds its base height map, and TSD-3 parallax shows on it.
- A test pins the allocation for a cell with zero splat layers.
- Tile-slot capacity is still enough at radius 12 (state the count).

Decided in the #6198 review (2026-11-21): do this before the TSD-4 spike.

