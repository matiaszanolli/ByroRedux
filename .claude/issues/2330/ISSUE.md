# SKY-D7-03: Canonical PBR roughness is written at a second spawn-time site outside translate_material

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 7)
**GitHub issue**: #2330

**Severity**: LOW
**Location**: `byroredux/src/material_translate.rs:300-338` (`resolve_normal_alpha_spec_roughness`), called from `byroredux/src/scene/nif_loader.rs:924` and `byroredux/src/cell_loader/spawn.rs:1350`

## Description

Both spawn paths call `resolve_normal_alpha_spec_roughness` after texture
handles are attached, re-deriving `roughness` from `glossiness`/
`specular_strength` plus resolved normal/gloss textures. This is the
dominant path for Skyrim specifically (no dedicated gloss map; spec mask
lives in the normal-map alpha). **Not a defect** — the helper is idempotent
and NaN-guarded — this is a documentation-precision finding only.

## Evidence

Confirmed at HEAD (1ae86f62): both spawn paths call
`resolve_normal_alpha_spec_roughness` after texture resolution.

## Impact

None functionally. `material_translate.rs`'s own "single site" doc claim and
`nifal.md` describe a one-shot boundary where the real implementation is a
documented two-phase one.

## Suggested Fix

Amend the module doc and `nifal.md`'s Materials row to describe the
boundary as two-phase.

## Completeness Checks
- [ ] **SIBLING**: Check whether other canonical fields get a similar second write at either spawn site
- [ ] **CANONICAL-BOUNDARY**: Document the two-phase boundary explicitly in `material_translate.rs` and `nifal.md`. See `/audit-nifal`.
