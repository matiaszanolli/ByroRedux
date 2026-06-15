# #1599 — FO4-D7-LOW-01: Stale 'PBR_BSDF is a dead gate' docs contradict the #1352 fix

**Severity**: LOW · **Dimension**: NIFAL Canonical Material Translation (doc-rot)
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D7-LOW-01)
**Location**: `crates/renderer/src/vulkan/material.rs:452-455` (PBR_BSDF doc), `crates/renderer/src/vulkan/material.rs:516-520` (BGSM_AUTHORED doc)

## Description
Two doc comments still describe pre-#1352 routing: `PBR_BSDF` (452-455) "Currently set by the BGSM translator on `BgsmFile.pbr == true`"; `BGSM_AUTHORED` (516-520) "so `PBR_BSDF` alone is a dead gate." As of #1352, `merge_bgsm_into_mesh` sets `mesh.is_pbr = true` for all `from_bgsm` content unconditionally, and `pack_bgsm_material_flags` ORs `PBR_BSDF` whenever `mesh.is_pbr` is set — so PBR_BSDF is now set on every FO4 BGSM material, the opposite of a "dead gate."

## Evidence
`asset_provider.rs:1095` `mesh.is_pbr = true;` (unconditional); `cell_loader.rs:215-217` `if mesh.is_pbr { flags |= PBR_BSDF; }`; `triangle.frag:1013,3050` gate the Disney/Burley diffuse on `MAT_FLAG_PBR_BSDF`. #1504 revised the BGSM_AUTHORED doc but left the "dead gate" sentence intact.

## Impact
Documentation only — runtime is correct. Maintenance risk: a future engineer could wrongly conclude FO4 BGSM uses Lambert and regress #1352, or prune PBR_BSDF packing as dead.

## Related
#1352 (CLOSED, the fix this doc lags), #1504 (CLOSED, touched the same comment but missed this).

## Suggested Fix
Update `material.rs:452-455` to state PBR_BSDF is set for all BGSM/BGEM-sourced content (via `is_pbr` → `pack_bgsm_material_flags`); delete the "so `PBR_BSDF` alone is a dead gate" sentence at 520.

## Completeness Checks
- [ ] **SIBLING**: Both doc blocks (PBR_BSDF :452-455 and BGSM_AUTHORED :516-520) corrected; no other comment repeats the "dead gate" claim
- [ ] **TESTS**: N/A (doc-only)
