# OBL-D5-01: Three raw-tier ImportedMaterial fields bypass the NIFAL boundary and are re-read at each spawn site

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2571
**Finding ID**: OBL-D5-01

**Severity**: MEDIUM
**Dimension**: NIFAL Canonical Material Translation for Oblivion
**Location**: `byroredux/src/cell_loader/spawn.rs:1367,1533,1565`, `byroredux/src/scene/nif_loader.rs:786,830,915`
**Status**: NEW

## Description
`texture_clamp_mode`, `src_blend_mode`, and `dst_blend_mode` have no canonical `Material` field — they are read directly off the raw `ImportedMaterial` at four spawn sites instead of through `translate_material`, exactly the hand-synced-duplication failure mode the NIFAL boundary's own module doc says it was created to eliminate. Oblivion relevance is direct: `texture_clamp_mode` (`CLAMP_S_CLAMP_T`) is authored on Oblivion architecture trim/signs/banners (#610); `src_blend_mode`/`dst_blend_mode` come from `NiAlphaProperty`'s Oblivion-era blend-factor authoring. The two sites are byte-identical today (latent, not live), but a third spawn path (FO4's `cell_loader/precombined.rs`) already reads the same raw fields independently and could silently diverge, and these values are invisible to `mat.*`/`material_dump` console diagnostics since those inspect the canonical `Material`.

## Evidence
Confirmed directly: `spawn.rs:1367,1533,1565` and `nif_loader.rs:786,830,915` all read `mesh.material.texture_clamp_mode`/`src_blend_mode`/`dst_blend_mode` (raw `ImportedMaterial` fields), never through `translate_material`'s canonical `Material`.

## Impact
Latent today (all sites byte-identical) but a real drift risk — the third independent reader (FO4 precombine path) could silently diverge, and none of these values are visible to `mat.*`/`material_dump` diagnostics.

## Suggested Fix
Add the three fields to `Material`, copy them in `translate_material`, point both spawn sites at the canonical component, extend the canonical-completeness harness.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New fields copied only inside `translate_material`, all four (now consolidated) spawn sites read the canonical component
- [ ] **TESTS**: Extend the canonical-completeness harness to cover these three new fields
