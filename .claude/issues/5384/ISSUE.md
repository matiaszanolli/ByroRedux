# #5384: ECS-2026-10-08-D7-01: cinematic re-adoption picks the cell from each entity's *local* `Transform`, so a released convoy's parented render subtree goes to the wrong cell or never gets adopted, and outlives its root as frozen ghost geometry

**Labels**: medium,ecs,scripting,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5384

**Source**: `docs/audits/AUDIT_ECS_2026-10-08.md` — `ECS-2026-10-08-D7-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. Any released tether whose home cell had already unloaded triggers it, which includes the Skyrim intro cart that #3254/#3817 were about. It leaks entities and GPU handles once per release, not per frame, and leaves stale geometry visible.
- **Dimension**: 7 — Cell load/unload symmetry
- **Location**:
  - `byroredux/src/systems/cinematic.rs:589-609` (`retry_cinematic_readoption`)
  - `byroredux/src/systems/cinematic.rs:500-531` (`release_finished_tethers`)
- **Status**: NEW (incomplete fix of the closed #3817)
- **Description**: The pending list holds more than the convoy roots:
  - When the home cell unloads mid-tether, `strip_retained_cell_root` removes `CellRoot` from every retained victim. That set includes each cart's, horse's and rider's full `Children` subtree (`unload.rs:34-47`), and cell load stamps `CellRoot` on every spawned entity.
  - At release, `release_set` walks `Children` again. Every subtree node without a `CellRoot` is queued on `CinematicReAdoption`.

  The retry then reads `world.query::<Transform>()` and computes `world_pos_to_grid(gt.translation.x, gt.translation.z)`. The binding is named `gt`, but it is the *local* `Transform`. That is world space only for parentless roots.

  For a subtree node, the local offset is a few units from its parent, so it maps to grid (0,0), (-1,0), (0,-1) or (-1,-1). The node then either:
  - (a) is adopted onto whatever origin cell happens to be loaded, so the hierarchy is split across two cells and each unload despawns half of it; or
  - (b) stays pending indefinitely.

  In case (b), once the root's real cell unloads, the root is despawned and the subtree survives:
  - Each node still has a `Parent` pointing at a dead id, so it is not a propagation root and its `GlobalTransform` freezes.
  - Its `MeshHandle` rows keep drawing the cart, horse and rider meshes where they last stood, and keep their texture and mesh references.
  - When the home cell reloads, a second copy spawns.
- **Evidence**:
  ```rust
  // cinematic.rs:592-604
  let transforms = world.query::<Transform>();
  ...
  let Some(gt) = transforms.as_ref().and_then(|t| t.get(entity)) else { continue; };
  ...
  let (gx, gy) = crate::streaming::world_pos_to_grid(gt.translation.x, gt.translation.z);
  ```
  `tether_releases_at_the_authored_route_terminal_and_detaches_riders` uses three parentless entities, so it passes.
- **Impact**: After a cinematic convoy ride whose start cell streamed out, the convoy's render hierarchy either splits across cells or is orphaned when the destination cell unloads. Ghost meshes stay frozen in the world and keep their GPU handles. Before #3817 the orphaning applied to everything; now the roots are fixed and the subtree is not.
- **Related**: #3817, #3254 (both closed); #5310 (the detach step only edits `Children` when the parent survives a sweep, so it does not help here).
- **Suggested Fix**: Adopt only the parentless members by position. When a root is adopted, stamp the same `CellRoot` on its whole `Children` subtree and remove those nodes from `pending`. Alternatively, have `release_finished_tethers` queue only the roots. Extend the test with a parented child, and assert that it ends up under the root's cell and that unloading that cell despawns it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other position→grid lookups that read `Transform` where `GlobalTransform` is meant)
- [ ] **TESTS**: A regression test pins this specific fix
