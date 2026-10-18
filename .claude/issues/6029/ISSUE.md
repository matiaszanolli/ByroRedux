# EXT-D6-2026-10-18-01: Doc rot after the tree-LOD ring: it is described as "no BLAS / out of the TLAS" and `IsLodTerrain` points at a function that does not exist

Labels: low,terrain-exterior,documentation,doc-rot
Source: docs/audits/AUDIT_EXTERIOR_2026-10-18.md

- **Severity**: LOW (doc-rot)
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/tree_lod.rs:519` ("No BLAS, lean static draw, out of the TLAS — shared with every LOD") and `:30` area; `docs/engine/exal.md:419` ("`IsLodTerrain`, no BLAS"); `byroredux/src/components/render.rs:177-185` (`[crate::render::collect_lod_terrain_draws]` "dedicated lean draw path", "skipped by the heavy `collect_static_mesh_draws` loop").
- **Status**: NEW (`a9384f9c0`), the `render.rs` pointer predates it.
- **Tier Violated**: n/a
- **Game Affected**: all (docs)
- **Description**:
  - `IsLodTerrain` entities go through the ordinary `collect_static_mesh_draws` loop (`render/static_meshes.rs:525`). Their TLAS membership is `tlas_exclusion(is_lod, ...)`: admitted when within `LOD_SHADOW_CASTER_DISTANCE` (24,000 BU) plus the entity's bound radius (`static_meshes.rs:46-49,137-140`). A tree-LOD quad's bound spans up to ~16k BU, so quads within roughly 35k BU of the camera build a BLAS and enter the TLAS as alpha-tested (`hitMat.alphaThreshold > 0`, handled by the coverage test in `shadow_transport.glsl:74-92`) architecture-layer casters. `terrain_lod.rs:11-14` states this correctly ("no BLAS **at spawn**"); the tree-LOD comment and `exal.md` do not.
  - `collect_lod_terrain_draws` does not exist in the repo or its history.
- **Impact**: A future reader prices the tree ring at zero RT cost, or looks for a lean path that is not there.
- **Suggested Fix**: Reword the three sites to the `terrain_lod.rs` wording; delete or repoint the `render.rs` link.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
