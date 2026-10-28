# EXT-D6-2026-10-27-02: #6409's rim key covers only the finest band, so a coarse quad can border a hysteresis-resident cell with no skirt or a stale one

Labels: low, terrain-exterior, bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D6-2026-10-27-02` (Dimension 6, Distant LOD and trees) · Severity LOW · Status NEW (coarse-band residual of closed #6409) · Static trace only, not confirmed on screen.

**Location (current)**: `byroredux/src/cell_loader/terrain_lod.rs:673-680` (`(mask, rim) = if level == k { .. } else { (0, 0) }`); `byroredux/src/cell_loader/lod_bands.rs` (refine skip, `quad_min_chebyshev(..) <= refine_within`); `byroredux/src/streaming_helpers.rs:197` (`full_detail_radius: state.radius_unload`); test `coarse_btr_bands_stay_out_of_the_default_streaming_radius` (`terrain_lod.rs`, ~:1808).

## Description
Hysteresis keeps cells resident out to Chebyshev `radius_unload`. #6094 admits a coarse LOD quad once its nearest cell is at `radius_unload + 1`. So when the player steps back from a lattice-aligned coarse edge, the cell at `radius_unload` stays resident directly against the coarse quad.

- On the Combined games (Skyrim, FO4) that quad is a prebaked `.btr` with no skirts.
- On the legacy games (FO3, FNV, Oblivion) it is a synth block whose skirts were decided at spawn but whose rebuild key is `(0, 0)`, so the skirts never follow residency.

The #6094 test asserts only `> radius_unload`, which allows this adjacency.

## Evidence
```rust
let (mask, rim) = if level == k {
    (block_hole_mask(cells_map, qx, qy, resident_full_cells),
     block_rim_mask(qx, qy, resident_full_cells))
} else {
    (0, 0)
};
```
`full_detail_radius: state.radius_unload` feeds `refine_within`; coarse quads are refined only when `d <= refine_within`, so a coarse quad with nearest cell at `radius_unload + 1` is kept coarse yet touches the resident cell at `radius_unload`.

## Impact
The sky-coloured ribbon that #6409 fixed can reappear along a coarse-quad edge in the hysteresis band (visual, intermittent, specific lattice alignments).

## Related
#6409 (closed, finest band only), #6094, `EXT-D6-2026-10-27-01` (same area, different defect).

## Suggested Fix
Refine coarse quads within `radius_unload + 1`. That one-cell margin guarantees a finest-band block (which has skirts and rim keys) between any coarse quad and resident ground. Extend the #6094 test to assert the margin.

## Completeness Checks
- [ ] **SIBLING**: Both the `.btr` (Skyrim/FO4) path and the synth legacy path are covered by the new margin
- [ ] **CANONICAL-BOUNDARY**: The fix lives in LOD band selection, not in shaders or per-game render-time logic
- [ ] **TESTS**: The #6094 pin asserts the `radius_unload + 1` margin for both ladders

