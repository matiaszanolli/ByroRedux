# SKY-D3-NEW-03: Per-NPC FaceTint DDS resolved but never loaded or applied

**Severity**: LOW
**Labels**: low, import-pipeline, legacy-compat, bug
**Location**: `byroredux/src/npc_spawn.rs:2011` (`_tint_path` dropped; `prebaked_facegen_tint_path` defined at line 1784)
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-07-16.md` (SKY-D3-NEW-03)

## Description
`prebaked_facegen_tint_path` computes the correct per-NPC face-tint texture path but the result (`_tint_path`) is dropped at the call site — never fetched or bound to the head material's diffuse slot. Comment frames this as an explicit Phase 4 deferral.

## Impact
Every Skyrim+/FO4+ NPC head renders with the FaceGeom NIF's base diffuse, not Bethesda's per-NPC baked tint blend. Visual-only; does not block spawn or equip.

## Suggested Fix
Wire through the existing `RefrTextureOverlay` machinery the code comment already points at.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
