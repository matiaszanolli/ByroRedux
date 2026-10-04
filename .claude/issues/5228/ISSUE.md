# #5228 — FO4-D1-01: exterior precombines are placed at ≈2× world position — world-absolute `_oc.nif` instance transforms composed with a cell-grid origin

https://github.com/matiaszanolli/ByroRedux/issues/5228

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D1-01)

- **Severity**: HIGH. It breaks rendering correctness on every FO4 exterior precombine, and it removes collision where the architecture belongs, because the absorbed REFRs are skipped wholesale.
- **Dimension**: M49 precombines (CSG + decode + spawn)
- **Location**: the origin flows through five places:
  1. `byroredux/src/cell_loader/exterior.rs:2034-2043` passes it: `job.advance(cell, cell_grid_to_world_yup(self.gx, self.gy), …)`.
  2. It becomes the placement root in `PrecombinedPlacement::new` / `advance` (`byroredux/src/cell_loader/spawn/precombined.rs:23-35,73-77`).
  3. The non-resumable arm uses it as well: `spawn_placed_instances(…, cell_origin, …)` (`byroredux/src/cell_loader/precombined.rs:450-455`).
  4. `byroredux/src/cell_loader/spawn/mesh_instance.rs:610-618` composes it: `compose_trs(pc.ref_pos, …, nif_position = mesh.translation, …)`.
  5. The instance translation comes from `crates/nif/src/import/precombine.rs:79` (`mesh.translation = zup_point_to_yup(&instance.translation)`).
- **Status**: NEW.
  - Introduced by 1ed8dc0bd (#1221/#1222, 2026-05-21). That commit assumed "the bake's cell-local coords" and warned "Without this offset every Commonwealth tile's bake would stack at the world origin".
  - The premise was written when no CSG reader existed: the gate always took the 0-spawn bypass. Nobody measured it once M49 decode shipped.
  - Closed-issue searches (precombine origin / offset / exterior / world position) find only #1221/#1222.
- **Description**:
  - On an exterior `_oc.nif`, `BSPackedGeomDataCombined::transform` is in world space. That is the same frame as the exterior REFRs the bake absorbs. The per-REFR path spawns those REFRs at `zup_to_yup_pos(placed_ref.position)` with no cell offset (`byroredux/src/cell_loader/references/mod.rs:536`).
  - The precombine path composes `ref_pos = cell_grid_to_world_yup(gx, gy)` with that already-absolute translation. Every exterior bake therefore lands at world + (gx·4096, gy·4096) in Z-up terms.
  - Unaffected: interiors (`load.rs:525/921` pass `Vec3::ZERO`) and grid (0,0).
- **Evidence** (real data, read-only):
  - **Probe** (`/tmp/audit/fo4/d1/probe`, production `collect_precombine_geom_refs`): 3,000 exterior `_oc.nif`s from `Fallout4 - MeshesExtra.ba2`, covering 1,959 cells. All 3,000 have their mean instance translation inside the file's own cell in world coordinates. Examples: grid (−28,−32) has cell min (−114688, −131072) and mean instance (−110973, −127612). Grid (−20,22) has cell min (−81920, 90112) and mean (−80506, 91161). 0 of 3,000 root `NiNode`s carry a non-identity transform.
  - **Absorbed REFRs agree with no offset**: cell `000a801e` has 42 XPRI-absorbed REFRs with mean world position (−80384, 91044), against the precombine instance mean (−80506, 91161). Cells `0000d944` and `0000e117` agree the same way.
  - **Authored bounds**: 400 exterior bakes decoded against `Fallout4 - Geometry.csg` through the production `read_psg` → `decode_shared_geom_object` → `into_imported_mesh` chain. 4,977 of 4,977 instances sit inside their authored bounding sphere as decoded; 0 of 4,977 do once the production offset is added.
  - **Orchestrator probe**, independent scratch crate, restricted to max(|gx|,|gy|) ≥ 4 so a cell-local frame cannot masquerade as absolute:
    ```
    far cells probed=1500; mean closer to world-abs cell centre=1500 (worst 2847 u);
    closer to cell-local=0; production(instance+origin) lands within 1 cell of own cell=0
    ```
- **Impact**:
  - On every FO4 exterior cell except (0,0) whose precombine decodes (`pc_spawned > 0`), the absorbed STAT REFRs are skipped, geometry and collision (`references/mod.rs:514-525`).
  - Their replacement spawns one grid-offset away: the architecture plus its `ArchitectureTriMesh` fallback collider, usually in cells that are not even loaded.
  - Result: Commonwealth buildings, rubble and roads are missing in place, so the player walks through them; phantom architecture appears in far cells; the BLAS/TLAS carry geometry at the wrong place.
  - **Blast radius**: all 4,287 `Fallout4.esm` exterior cells with XCRI precombines, plus DLC exteriors (assumed to share the frame; not separately probed).
  - Docs that read as covering it: `docs/feature-matrix.md` marks "Cell-loader spawn from XCRI hash list ✓ Shipped" with no interior/exterior qualifier; `ROADMAP.md` lists "CSG precombines (M49). Commonwealth exterior streaming." side by side as verified content.
- **Related**: #1221, #1222 (origin of the premise); #1188 (the absorb gate gates correctly, but on misplaced geometry); #2376 (the resumable job carries the same origin). Both the worker and job routes inherit this, because it is applied at spawn, not decode. `switchboard_precombine_transforms_match_authored_bounds` is interior-only, which is why it stays green.
- **Suggested Fix**:
  - Pass `Vec3::ZERO` as the precombine origin on the exterior path (`exterior.rs:2037`), as the interior loader does. Re-document `cell_origin` in `spawn_precombined_meshes` / `PrecombinedSpawnJob::advance` (the bake is in the cell's own frame, which is world space on exteriors).
  - Add an `#[ignore]`d real-data guard: decode one exterior `_oc.nif` and assert that the placed instance translations and decoded bounds fall inside the cell's world XY rectangle. Mirror the switchboard test.
  - Correct `docs/engine/fo4-csg-format.md` §Placement and the `/audit-fo4` Dimension 1 checklist line (it states the wrong premise as the invariant).
  - Accept on an FO4 exterior capture: Diamond City outskirts or Sanctuary, both far from (0,0).

## Completeness Checks
- [x] **SIBLING**: both precombine routes (resumable `PrecombinedSpawnJob` and non-resumable `spawn_placed_instances`) place exterior bakes at zero origin; interiors and grid (0,0) unchanged
- [x] **TESTS**: an `#[ignore]`d real-data guard decodes one exterior `_oc.nif` and asserts placed instance translations fall inside the cell's world XY rectangle (mirrors `switchboard_precombine_transforms_match_authored_bounds`)
- [x] **DOCS**: `docs/engine/fo4-csg-format.md` §Placement, the `/audit-fo4` Dimension 1 checklist line, and the `precombined.rs` module doc (#5233) no longer state the cell-local premise
