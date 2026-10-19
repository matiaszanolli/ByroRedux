# EXT-D6-2026-10-19-01: A capacity-refused LOD upload is cached as a permanent known miss, and nothing re-arms the stopped reconcile when admission reopens

Labels: medium,terrain-exterior,renderer,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6092

**Severity**: MEDIUM · **Dimension**: Distant LOD and trees · **Games**: FNV, FO3, Skyrim, FO4 (any game whose radius-12 residency saturates the 4 M-vertex geometry pool)

## Location
- `byroredux/src/cell_loader/terrain_lod.rs` (`stream_lod_blocks`: `missing_blocks.insert((level, qx, qy), mask)` on any `None`; a recorded miss at the same mask is skipped)
- `byroredux/src/cell_loader/object_lod.rs` (`ObjectLodBlock::empty()` sentinel; a refused sub-mesh upload is a `continue`, so a quad can be kept with only the sub-meshes that fit)
- `byroredux/src/cell_loader/tree_lod.rs`, `byroredux/src/cell_loader/placement_lod.rs` (same shape)
- `byroredux/src/app_step.rs` (`Starved` verdict clears `lod_reconcile_pending`)
- `crates/renderer/src/mesh/geometry_ssbo.rs` (admission reopens at the next compaction), `crates/renderer/src/mesh.rs` (`scene_geometry_admission_open`)

## Description
`upload_scene_mesh_global_only` fails with "scene geometry admission is closed" once the pool hits its hard cap. Each LOD ring treats that exactly like "this quad has no asset": the terrain ring records the quad in `missing_blocks` at its current hole mask, the object / tree / placement rings insert an empty sentinel so "we don't re-extract on every boundary crossing".

`scene_geometry_admission_closed` is cleared at the next pool compaction after a cell unload. By then the refused quads are already sentinels. The terrain ring skips a quad whose recorded mask equals its current mask, and coarse bands always have mask 0, so they are never retried. The object / tree / placement candidate filter is `!blocks.contains_key(coord)`, so a sentinel is never a candidate.

#6054 made the verdict `Starved` and cleared `lod_reconcile_pending`. Only `grid_changed`, a finished cell apply or world setup set it again, and even then the sentinels veto the retry. The #6054 warning says "the horizon is incomplete until a cell unload frees pool space"; freeing pool space does not restart anything. A partially uploaded object quad (some sub-meshes refused) is stored as a normal resident block with no record that parts are missing.

## Evidence
- `stream_lod_blocks`: `else { missing_blocks.insert((level, qx, qy), mask); }` follows `block = btr_block.or_else(|| ... spawn_lod_block(..))`, whose `None` covers a refused upload.
- `stream_object_lod_blocks`: `None => { blocks.insert((level, qx, qy), ObjectLodBlock::empty()); }`.
- `MeshRegistry::scene_geometry_admission_open()` exists and is already the gate the full-detail cell path uses (`cell_loader/spawn/mesh_instance.rs`); no LOD ring consults it (verified by grep at publish time).
- `lod_reconcile_pending` writers: `streaming_helpers.rs`, `app_step.rs`, `scene/world_setup.rs` only; the compaction itself does not re-arm it.
- Static analysis only (no engine launch).

## Impact
After the saturating transition (the #6054 door exit, 352 refused uploads on FNV; FO4 saturates first), distant terrain and objects beyond the refused set never fill in, even once the pool has room, until the player has moved far enough to change those quads' band or leave and re-enter the ring.

## Related
#6054 (closed, introduced `Starved`), EXT-D6-2026-10-19-02 (telemetry half of the same fix), REN-SD-2026-10-19-01 (full-detail analogue: different fix site, filed separately and cross-linked).

## Suggested Fix
Treat a capacity refusal as a retryable state, not a miss. Smallest change: before an attempt, `if !ctx.mesh_registry.scene_geometry_admission_open() { starved = true; break; }` (also saves the archive read and decode); never write a sentinel or `missing_blocks` entry from a refused attempt; re-arm `lod_reconcile_pending` on the admission closed -> open edge. Pin it with a test that drives a fake admission state through two reconciles.

Source: docs/audits/AUDIT_EXTERIOR_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
