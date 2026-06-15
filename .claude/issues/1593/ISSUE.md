# #1593 — FO4-D9-MEDIUM-01: ROADMAP FO4 parse-rate row stale (96.46% -> measured 100.00%)

**Severity**: MEDIUM · **Dimension**: Real-Data Validation + Forward Scope
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D9-MEDIUM-01)
**Location**: `ROADMAP.md:201` (compat matrix), `ROADMAP.md:69` (95-97% band prose), `ROADMAP.md:733` (project-stats line)

## Description
The ROADMAP FO4 compat-matrix row reads `96.46% (33,757 / 34,995) · recover 100%` with the note "FaceGen NIFs dominate the truncation tail (1,235 of 1,238 truncated files)". The live re-run measures 100.00% clean / 0 truncated on BOTH base mesh archives. The 34,995 total matches `Fallout4 - Meshes.ba2` exactly — ROADMAP measured only the single base archive AND that figure is now stale (test floors recalibrated to 0.995 on 2026-06-14 per #1457). L69 and L733 carry the same stale number.

## Evidence
`parse_rate_fo4_all_meshes` → both archives 100.00% clean, 0 truncated (`Fallout4 - Meshes.ba2` 34,995/34,995 + `Fallout4 - MeshesExtra.ba2` 124,871/124,871 = 159,866 NIFs). `nif_stats --tsv` → 0 NiUnknown. The FaceGen truncation tail (1,238) cited in the matrix is gone.

## Impact
Doc-rot misrepresenting capability. A reader/future audit would under-state FO4 coverage by ~3.5% and chase a non-existent FaceGen truncation tail. No open issue covers the FO4 figure.

## Related
#1457 (test floor calibration, CLOSED), #1218 (matrix drift, CLOSED), #1225 (FaceGen zero-mesh, CLOSED).

## Suggested Fix
Update the three ROADMAP sites to 100.00% clean / 100% recoverable (Meshes 34,995 + MeshesExtra 124,871, 159,866 total, 0 truncated), drop the FaceGen-truncation note, refresh the L69 band prose to exclude FO4, refresh the sweep date.

## Completeness Checks
- [ ] **SIBLING**: All three ROADMAP sites (L201 matrix, L69 prose band, L733 project-stats) updated in lockstep; no other doc repeats the 96.46% figure
- [ ] **TESTS**: N/A (doc-only) — verify the cited `parse_rate_fo4_all_meshes` figure is reproducible before editing
