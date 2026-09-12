# OB-D5-02: three exterior spawners attach MaterialTextureHandles via the texture-only material boundary without the module doc's stated Phase-2 resolver invariant covering them

**Issue**: #4264 — https://github.com/matiaszanolli/ByroRedux/issues/4264
**Labels**: low,nifal,terrain-exterior,doc-rot,game:oblivion,legacy-compat,documentation

**Severity**: LOW
**Dimension**: Dimension 5 — NIFAL Canonical Material Translation for Oblivion
**Location**: `byroredux/src/scene/terrain.rs; byroredux/src/scene/terrain_lod.rs; byroredux/src/scene/terrain_lod_btr.rs`
**Status**: NEW

## Description
Three exterior spawners (`terrain.rs`, `terrain_lod.rs`, `terrain_lod_btr.rs`) attach `MaterialTextureHandles` via the sibling `translate_texture_only_material` boundary with hardcoded parallax literals, without the module doc's stated Phase-2 resolver invariant explicitly covering them. This surfaced during the Oblivion audit but is not Oblivion-specific — it is EXAL (exterior abstraction layer) territory, affecting the terrain material path used by every game.

## Evidence
Verified structurally inert by construction during this audit: none of the three resolvers' gates can currently fire on this path, so no live behavioral defect exists today. The gap is that the module doc text does not say so, leaving the invariant's actual coverage undocumented for these three call sites.

## Impact
No live behavioral impact today (verified inert by construction). Documentation-completeness gap: a future change to the Phase-2 resolver could silently start applying to (or silently continue not applying to) these three exterior spawners without anyone noticing, since the doc doesn't currently name them either way.

## Related
None filed. Not Oblivion-specific — cross-references the EXAL design doc (`docs/engine/exal.md`).

## Suggested Fix
Update the module doc for `translate_texture_only_material` (or the Phase-2 resolver's own doc) to explicitly state whether `terrain.rs`/`terrain_lod.rs`/`terrain_lod_btr.rs` are in scope for the resolver invariant, and why they are currently inert.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
