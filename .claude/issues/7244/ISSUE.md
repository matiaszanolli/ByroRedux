# #7244: INC-2026-11-22-02: `terrain_patch.rs` module header and `record_terrain_patches` doc and SAFETY text still describe the 4.1 frame order and "raster only"

**Labels**: low,renderer,terrain-exterior,documentation,doc-rot

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-A-01**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: audit-safety Dim 4 (is the stated invariant true) and audit-renderer Dim 4 (documentation of recording order)
- **Location**: `crates/renderer/src/vulkan/terrain_patch.rs:10-14`; `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:589-591` and `:612-613`
- **Changed in**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` and `crates/renderer/src/vulkan/terrain_patch.rs`, `ff339f658` (the code motion happened here; the comments were left behind)
- **Status**: NEW
- **Description**: #7221 moved the patch pass ahead of `dispatch_skin_and_cluster` (and so ahead of the TLAS build and the instance upload) and put the draws in the TLAS. Three comments still say otherwise.
- **Evidence**:
  - `terrain_patch.rs:10-14`: "re-displaces each resident slot in place with `terrain_patch.comp`, between the instance upload and the main render pass ... (the host clones the base cell's draw command onto the patch mesh with `in_tlas = false`); they do not enter the TLAS until stage 4.2."
  - `dispatch_skin_and_cluster.rs:589-591`: "after the instance upload (which also copies any dirty terrain tiles the pass reads) and before the geometry pass".
  - `dispatch_skin_and_cluster.rs:612`: `// SAFETY: outside any render pass (between the instance upload and the geometry pass); ...`.
  - The actual order, in `draw.rs:192-195`: `upload_terrain_tiles_if_dirty`, `record_terrain_patches` (compute, then patch BLAS), then `dispatch_skin_and_cluster`. The terrain-tile copy no longer lives in the instance upload at all (`build_and_upload_instances.rs:1385-1391`, `:1448-1451`). The host side now sets `patch.in_tlas = base.in_tlas && state.rt` (`byroredux/src/terrain_patches.rs`).
  - The "outside any render pass" half of the SAFETY text is still true; the position parenthetical is not.
- **Impact**: A reader who trusts the SAFETY text will reason about the wrong predecessor commands (the tile copy and the TLAS build are the real neighbours). No runtime effect.
- **Related**: #7221. The `.claude/commands/audit-{renderer,concurrency,performance,safety}` skills carry the same stale 4.1 prose ("between the instance upload and the geometry pass", "raster-only, `in_tlas = false`", `QUERIES_PER_FRAME` 70); that is `/audit-sync` territory, noted here only so it is not lost.
- **Suggested Fix**: Rewrite the three comments to say "after the terrain-tile upload, before the patch BLAS and the TLAS build; patch draws enter the TLAS once their BLAS matches the frame".

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
