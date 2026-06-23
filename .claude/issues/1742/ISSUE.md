# SCR-D7-02: trigger-box rotation frame may not match the permuted half-extents

Filed as: matiaszanolli/ByroRedux#1742
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW (verify-not-confirmed)
- **Dimension**: Engine Attach & Trigger Wiring
- **Location**: `byroredux/src/cell_loader/references.rs:1412-1436`
- **Labels**: low, import-pipeline, legacy-compat, bug

## Description
Half-extents are permuted z-up→y-up (`[x,z,y]`) but `rotation` passes through verbatim; `TriggerVolume::contains` (Box) tests `rotation.inverse() * (p-center)` against the permuted extents. If `rotation` isn't in the same permuted frame, a rotated OBB trigger is wrong. Dedicated tests only use `Quat::IDENTITY` (permute invisible). Exposure limited by Bethesda's mostly axis-aligned boxes.

## Suggested Fix
Add a rotated-box trigger test end-to-end (placement → volume → `contains`) with a non-identity REFR rotation; permute `rotation` into the extents frame if it fails.
