# #1207 — NIF-DIM4-07: BsTriShapeKind::LOD cutoffs dropped at import

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, LOW)
**Severity**: low / Labels: bug, low, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)
**Paired**: #1206 (same drop site)

## Cause

Same as #1206 — `bs_tri_shape.rs:172+` drops `shape.kind`. No `ImportedMesh` field for the BSLOD triple.

## Fix

Add `bs_lod_cutoffs: Option<[u32; 3]>` to `ImportedMesh`. Populate from `BsTriShapeKind::LOD { lod0, lod1, lod2 }`.

## Game / Risk

FO4 (BSLODTriShape distinct from NiLodTriShape). ZERO risk (additive).

## Estimated impact

Blocks future M35 LOD selector from honouring authored thresholds.
