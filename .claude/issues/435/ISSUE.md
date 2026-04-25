# NIF-D4-N06: NiTriShape UV transform dropped when NiMaterialProperty precedes NiTexturingProperty

**Severity:** MEDIUM | nif-parser, renderer, legacy-compat
**Source:** `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 4 N06

## Finding
`crates/nif/src/import/material.rs:562-569` gates the base-slot UV transform copy on `!info.has_material_data`. NiMaterialProperty typically precedes NiTexturingProperty in Oblivion/FO3/FNV property arrays — so once material data is set, the texture's own UV transform is silently dropped, even though `NiMaterialProperty` carries no UV transform (the two are orthogonal).

## Impact
Oblivion/FO3/FNV: tapestries, FNV signs, banner meshes, UV-animated water lose authored UV transforms.

## Fix
Add `has_uv_transform: bool` to MaterialInfo, gate UV-transform copy on its negation instead.

## SIBLING
Grep for other `!info.has_material_data` sites in material.rs.

## TESTS
Synthetic NiTriShape with `[NiMaterialProperty, NiTexturingProperty]` property order; base-slot transform = `(offset=[0.5, 0.0], scale=[2.0, 1.0])` → assert MaterialInfo recovers it.
