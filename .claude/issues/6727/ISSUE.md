# #6727 Volumetric Lighting Phase 3: interior shaft coverage across titles  [OPEN]

Part of the Volumetric Lighting PRD (https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/volumetric-lighting.md, §9 Phase 3). Serves the owner's goal: realistic, procedural god rays in interiors.

## Acceptance Criteria
- [ ] Opening inventory for the A9 cell list (VLM-INT-001)
- [ ] Day and night pair per listed cell (VLM-INT-002); sealed interiors show no shaft (VLM-INT-003); direct boot and door transition (VLM-INT-004) — VLM-INT-002 met on FNV NellisHangar1 only
- [x] `MAX_SKY_APERTURES` overflow logs once per cell and keeps the nearest apertures (VLM-INT-005)
- [ ] Converter coverage published per game (VLM-INT-006) — denominators published for every official master and DLC; live ratio only FO3 / FNV, rest is #6774
- [ ] A9 interior pass count published; A10 still passes


---
## Status check 2026-11-08 (not part of the issue body)
- The body says "rest is #6774": #6774 is CLOSED (2eee69c12, VLM-INT-006 live coverage for every title: FO4 ~100%, Oblivion 92.9%, Skyrim 0/1, Starfield 0/1). That box can be ticked.
- Still open here: VLM-INT-001 (opening inventory for the A9 list), -002 day/night pairs (FNV NellisHangar1 only), -003 sealed interiors, -004 direct boot + door transition, A9 pass count (`docs/smoke-tests/interior-shafts-content.sh` does not exist yet).
