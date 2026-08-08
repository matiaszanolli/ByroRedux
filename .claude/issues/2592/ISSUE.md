# SKY-D7-04: Material's #2284 doc cites a grayscale_to_palette_scale precedent field that does not exist on Material

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2592
**Finding ID**: SKY-D7-04

**Severity**: LOW
**Dimension**: NIFAL Canonical Material Translation (Skyrim slice)
**Location**: `crates/core/src/ecs/components/material.rs:256-260`; dropped at `material_translate.rs:120-215`; carried on `import/types.rs:489`
**Status**: NEW

## Description
The #2284 doc justifies landing six BSLSP scalars by appealing to a `grayscale_to_palette_scale` "precedent" on `Material` — but `Material` has no such field. The value is captured at import, reaches `ImportedMaterial`, and is then dropped entirely by `translate_material`; the only surviving trace is a `triangle.frag` comment describing a GPU-side gap without disclosing the value never leaves the raw tier.

## Impact
Low and FO4-facing (Skyrim never authors the field — see SKY-D7-01, this session), so no Skyrim content is mis-shaded. Makes the NIFAL boundary look more complete than it is.

## Related
#2284; SKY-D7-01 (this session); `docs/engine/nifal.md`'s "Materials — converged" verdict, slightly overstated by this omission.

## Suggested Fix
Correct the cross-reference to name `ImportedMaterial::grayscale_to_palette_scale` (raw tier) and add the field to `docs/engine/nifal.md`'s known-gap list.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
