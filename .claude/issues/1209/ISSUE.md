# #1209 — NIF-DIM4-09: BSGeometry external-LOD slot 0 short-circuit (carryover)

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, LOW — carryover NIF-D4-NEW-07)
**Severity**: low / Labels: bug, low, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)

## Cause

`bs_geometry.rs:28-62` Stage A calls `shape.meshes.first()` and bails when LOD 0 is `External`. No fallback to scan other slots. Stage B iterates correctly.

## Fix

Replace `.first().and_then(...)` with `.iter().find_map(...)` matching `Internal { data, .. }` arm.

## Game / Risk

Starfield (theoretical — vanilla doesn't mix LOD slot kinds). ZERO risk on vanilla.

## Estimated impact

Insurance against future content / mods. #982 explicitly deferred.
