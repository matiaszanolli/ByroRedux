# #1208 — NIF-DIM4-08: NiVertexColorProperty inherited-property override (carryover)

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, LOW — carryover NIF-D4-NEW-08)
**Severity**: low / Labels: bug, low, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)

## Cause

`walker.rs:888-891` NiVertexColorProperty consumer has no `has_material_data` gate. BSLightingShader branch sets `has_material_data=true` at line 296; this consumer ignores it.

## Fix

Gate on `!info.has_material_data`, mirroring the `texture_path.is_none()` pattern used by every other secondary-source consumer in this loop.

## Game / Risk

Skyrim+ modded content only (vanilla doesn't field both). TINY risk.

## Estimated impact

Niche modded corner. Needs measurement to size.
