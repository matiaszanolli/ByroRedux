# EXT-D2-2026-10-19-05: Doc rot in the terrain / LOD stack: displaced doc blocks, stale radius premises, a wrong return doc

Labels: low,terrain-exterior,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6095

**Severity**: LOW (doc-rot) · **Dimension**: Terrain / Distant LOD · **Games**: all (docs)

## Location
- `byroredux/src/cell_loader/terrain.rs` - the "Resolve the authored `LTEX.GNAM` grass association" doc block sits on `cover_affinity_key`; `authored_grass_for_splat_layers` has none.
- `byroredux/src/cell_loader/terrain.rs` - the "Generate a terrain mesh from LAND heightmap data" doc and a stray `#[allow(clippy::too_many_arguments)]` sit on `splat_indices_to_release`; `spawn_terrain_mesh` has neither.
- `byroredux/src/cell_loader/terrain_lod.rs` ("the default full-detail radius-5 view", "~6-cell streaming radius") and `byroredux/src/cell_loader/terrain_lod_btr.rs` ("far outside the streaming radius").
- `byroredux/src/cell_loader/spawn/authored_cover.rs` ("or `None` when no record's model loaded"; the function returns `Some(cover_root)` unconditionally).
- `docs/engine/exterior-grid-streaming.md` section 4 (describes LOD "settle" but not the #6054 `Starved` stop, nor that no ring retries a refused quad).

## Description
Same family as #6046 / #6064 (a file split spliced doc comments onto the wrong item). The radius comments are the premises EXT-D6-2026-10-19-04 depends on; the default moved from 3 / 5 to 12 without them.

## Evidence
Verified at publish time: the doc block / `allow` placement in `terrain.rs`, and the `None` doc in `authored_cover.rs`.

## Impact
A reader trusts the 6-cell premise (the cause of EXT-D6-2026-10-19-04) and the misplaced `GNAM` / `spawn_terrain_mesh` docs describe the wrong function.

## Related
#6046, #6064, EXT-D6-2026-10-19-04.

## Suggested Fix
Move the two doc blocks (and the `allow`) back onto their functions; restate the radius premises against `DEFAULT_EXTERIOR_RADIUS` once EXT-D6-2026-10-19-04 is decided; correct the `authored_cover` return doc; add the `Starved` paragraph to the streaming doc.

Source: docs/audits/AUDIT_EXTERIOR_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
