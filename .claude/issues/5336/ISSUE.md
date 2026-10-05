# #5336: FO4-2026-10-05-D5-01: `feature-matrix.md`'s M49 table still says "one LOD tier", "texture wiring from owning REFR" and "`_precomb.nif` collision", and all three are superseded

Labels: low,documentation,doc-rot,game:fo4,legacy-compat
Filed from: docs/audits/AUDIT_FO4_2026-10-05.md

**Source**: `docs/audits/AUDIT_FO4_2026-10-05.md` (FO4-2026-10-05-D5-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW (doc rot in the status floor; no runtime effect)
- **Dimension**: Archives + real data + forward scope (status docs)
- **Location**:
  - `docs/feature-matrix.md:63-65`.
  - Secondary: `crates/nif/src/import/material/mod.rs:1891`. That doc names a hypothetical "`_precomb.nif`
    collision-visual path".
- **Status**: NEW.
  - Searches of `docs/audits/`, the open-issue cache and closed issues found no prior report. The search terms
    were "one tier, not all three", "Texture wiring from owning REFR", "`_precomb.nif`" and "feature-matrix
    precombine".
  - The 2026-10-03 report quoted the `✗ Deferred` collision row as forward scope but did not check the table's
    other rows.
- **Description**: `_audit-common.md` names `feature-matrix.md` as the status floor. Its "FO4 Precombined Geometry
  (M49)" table was written at M49 close (78540d8ef7, 2026-06-02) and has not been touched since. Three rows now
  state things the code or the data contradict:
  1. **`:63` "LOD tier selection (one tier, not all three) ✓ Fixed"** is the reverse of the shipped #4234
     contract.
     - The three bands are disjoint sub-meshes, and every populated band is decoded (`precombine_lod_bands`).
     - Picking one band drops 12.45% of `MeshesExtra.ba2`'s baked triangles. That is the exact behaviour #4234
       (closed) removed.
  2. **`:64` "Texture wiring from owning REFR ✓ Shipped"** is not how the path works.
     - Precombine meshes take their material from the owning *shape*'s shader and alpha properties
       (`precombine_material_from_shape`, `crates/nif/src/import/precombine.rs:304,379`), followed by the BGSM
       merge.
     - There is deliberately no REFR overlay: `pre_merge_materials: Vec::new()` with the comment "Precombines
       spawn with no REFR overlay" (`byroredux/src/cell_loader/precombined.rs:1022-1024`).
  3. **`:65` "`_precomb.nif` collision ✗ Deferred"** names a file that does not exist.
     - `precombined.rs:34` and `docs/engine/exterior-readiness-plan.md:794` already give the correction: the
       collision sibling is `<cell>_physics.nif`.
- **Evidence** (Python census of the GNRL name tables of the 8 FO4 mesh archives, this session):

  | Archive | `_oc.nif` | `_physics.nif` | `_precomb.nif` |
  |---|---|---|---|
  | Fallout4 - MeshesExtra.ba2 | 120,387 | 4,484 | 0 |
  | DLCCoast - Main.ba2 | 28,983 | 1,251 | 0 |
  | DLCNukaWorld - Main.ba2 | 23,043 | 1,041 | 0 |
  | DLCRobot - Main.ba2 | 2,554 | 16 | 0 |
  | DLCworkshop01/02/03 - Main.ba2 | 27 / 0 / 1,575 | 3 / 0 / 8 | 0 |
  | Fallout4 - Meshes.ba2 | 0 | 0 | 0 |
  | **Total** | **176,569** | **6,803** | **0** |

  - `git blame docs/feature-matrix.md:56-67` traces every row to 78540d8ef7.
  - `grep -rn "_precomb\.nif\|one tier"` over `docs/` and the code finds no other live statement of either claim.
    The other hits already say the opposite, or are the hypothetical in `material/mod.rs:1891`.
- **Impact**:
  - Nothing at runtime.
  - The table is the "status floor" a future auditor or fixer reads first. The LOD row tells them to keep exactly
    the single-band selection #4234 removed.
  - The texturing row points them at the REFR-overlay path, which this route intentionally leaves empty (#4290).
  - The collision row sends forward-scope work at a file name no archive contains.
- **Related**: #4234 (closed, all LOD bands), #5228 (closed; the table also says nothing of the exterior
  world-absolute frame), #3809 (closed, the precombine collision spike), #4290, FO4-2026-10-03-D1-02 (#5233,
  the sibling rot in the `precombined.rs` module doc, fixed by f3e1bba62).
- **Suggested Fix**: rewrite rows 63-65 as follows.
  - LOD: "Every populated LOD band decoded (#4234)".
  - Texturing: "Material from the owning shape + BGSM merge; no REFR overlay".
  - Collision: "`<cell>_physics.nif` collision ✗ Deferred (packed Havok)".
  - Optionally add a row "Exterior placement: world-absolute, zero origin (#5228)".
  - Reword the `material/mod.rs:1891` example to "a `_physics.nif` collision-visual path".

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
