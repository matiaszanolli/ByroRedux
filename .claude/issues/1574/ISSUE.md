# documentation, renderer, low

## REN-D16-2026-06-14-01: volumetrics.rs froxel-size doc-comment understates per-slot allocation by 2x

**Severity**: LOW
**Dimension**: Volumetrics
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
The `FROXEL_WIDTH/HEIGHT/DEPTH` doc-comment in `volumetrics.rs` states "14.06 MiB per slot, ×2 frames-in-flight = 28.12 MiB total," but `new_inner` allocates **two** 3D volumes per FIF slot (`lighting_volumes` + `integrated_volumes`), so the real total is ~56 MiB. The pipeline's startup `log::info!` is already correct ("2× MiB/slot inject + integrated"), making the constant comment internally inconsistent.

## Evidence
- `crates/renderer/src/vulkan/volumetrics.rs:161` — doc-comment "14.06 MiB per slot, ×2 frames-in-flight = 28.12 MiB total".
- `new_inner` allocates both volume sets per slot; the info-log at line ~582 already accounts for 2× ("2× {} MiB / slot (inject + integrated)").

## Impact
Doc-only; allocation and logging are correct.

## Suggested Fix
Update the constant doc-comment to ~28 MiB/slot (2 volumes) → ~56 MiB total.

## Completeness Checks
- [ ] **SIBLING**: the doc-comment math agrees with the startup `log::info!` 2× accounting
