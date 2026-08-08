# FO4-D5-04: fire-refraction mask reads skyrim_slsf1:: constants against an FO4 F4SF1 property

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2604
**Finding ID**: FO4-D5-04

**Severity**: LOW
**Dimension**: 5 (Materials/Shading)
**Location**: `crates/nif/src/import/material/dedicated_shader.rs:328-358`
**Status**: NEW

## Description
The fire-refraction override unconditionally overwrites the
shader_type-derived `material_kind` whenever F4SF1 bits 15+16 are both set.
Separately, the bitmask used to test those bits is built from
`skyrim_slsf1::`-namespaced constants applied to an FO4 (`F4SF1`) property.
The values are numerically identical between the two vocabularies, so this
is not a live behavioral bug, but it violates the vocabulary-isolation rule
the codebase otherwise follows — the same pattern previously flagged in
#414.

## Evidence
`crates/nif/src/import/material/dedicated_shader.rs:328-358` — mask
construction reads `skyrim_slsf1::` constants to test bits on an FO4 F4SF1
property.

## Impact
No functional impact today (values are numerically identical across the two
per-game flag vocabularies), but it's a latent trap: if a future FO4-specific
flag bit ever gets renumbered relative to Skyrim's, this site would silently
read the wrong bit with no compile-time signal.

## Suggested Fix
Use FO4-namespaced (`f4sf1::`-style) constants for the F4SF1 bitmask instead
of borrowing `skyrim_slsf1::`, matching the vocabulary-isolation fix already
applied at #414's site.

## Related
Same pattern as #414.

## Completeness Checks
- [ ] **SIBLING**: Grep for other `skyrim_slsf1::` uses against FO4-property (`F4SF1`) reads
- [ ] **TESTS**: N/A if purely a constant-namespace rename (behavior-preserving)
