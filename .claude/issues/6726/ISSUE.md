# #6726 Volumetric Lighting Phase 2: exterior light shafts, including the owner-decided haze floor  [OPEN]

Part of the Volumetric Lighting PRD (https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/volumetric-lighting.md, §9 Phase 2). Serves the owner's goal: realistic, procedural god rays in exteriors.

## Acceptance Criteria
- [x] Game-data-free exterior shaft lab `--shaft-lab`, open and solid variants (VLM-SH-001)
- [x] `docs/smoke-tests/exterior-shafts.sh`: open contrast beats solid by more than twice the repeat noise (VLM-SH-002)
- [x] `shaft_lab_golden_frame` golden frame (VLM-SH-003)
- [x] Shaft range per Q3 and the cloud-shadow rule (VLM-SH-004, VLM-SH-005) — range measured (usable to 90-110 m of the 128 m grid); cloud-shadow sampling stays on the camera-column value per Q3
- [ ] Noon and low-sun captures with shaft contrast on each exterior fixture (VLM-SH-006); frame sequences for owner review (VLM-SH-007); inject and integrate ms recorded separately (VLM-SH-008) — VLM-SH-008 recorded (PRD revision 14); VLM-SH-006 contrast is #6773; VLM-SH-007 sequences prepared, owner review pending
- [x] Alpha-tested cutouts in the froxel sun trace (VLM-SH-009)
- [x] Exterior haze floor: authored haze never thinned, smooth in sun elevation, zero indoors (VLM-SH-010)
- [x] Haze floor default chosen from floor-off and floor-on captures, adjustable at run time, live value on `env.health` (VLM-SH-011)
- [ ] A7 passes; A8 contrast and ms published against the Phase 0 baseline — A7 passes (VLM-SH-002, -003, -009); A8 contrast waits on #6773


---
## Status check 2026-11-08 (not part of the issue body)
- Remaining boxes: VLM-SH-006 contrast is #6773 (OPEN; progress in 6864b8c19), VLM-SH-007 is owner review, A8 waits on #6773.
