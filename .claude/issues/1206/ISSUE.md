# #1206 — NIF-DIM4-06: BsTriShapeKind::SubIndex segmentation dropped at import

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, LOW)
**Severity**: low / Labels: bug, low, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)
**Paired**: #1207 (same drop site, different discriminator variant)

## Cause

`bs_tri_shape.rs:172+` `ImportedMesh` literal drops `shape.kind`. `ImportedMesh` has no field modeling the wire-type discriminator for BsTriShape (only ImportedNode does, via `range_kind` etc.).

## Fix

Add `bs_sub_index: Option<BsSubIndexData>` to `ImportedMesh`. Populate from `BsTriShapeKind::SubIndex(data)`.

## Game / Risk

Skyrim SE DLC / FO4 / FO76. ZERO risk (additive).

## Estimated impact

Blocks dismemberment system implementation.
