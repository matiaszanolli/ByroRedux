# SF-D9-2026-08-07-03: BGSM distance_field_alpha_texture parsed with no MaterialTextureSet role and no deferral comment

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2642
**Finding ID**: SF-D9-2026-08-07-03

**Severity**: LOW
**Dimension**: 9 (BGSM/BGEM External Material Flow)
**Location**: `crates/bgsm/src/bgsm.rs:38,194-196`
**Status**: NEW

## Description
BGSM `distance_field_alpha_texture` (v≥17) is parsed but has no
`MaterialTextureSet` role and no sink, undocumented unlike its BGEM sibling
deferral. No role exists in `MaterialTextureSet` for this field (genuinely
a deferred-consumer gap, not a wiring bug) — but unlike the #2109 BGEM
glass-overlay deferral, there is no explanatory comment at the BGSM fill
block. v≥17 is exactly the FO76/Starfield-era range this dimension
targets; distance-field alpha drives crisp signage/decal cutouts that
currently fall back to plain alpha test.

## Evidence
`crates/bgsm/src/bgsm.rs:38,194-196` parses `distance_field_alpha_texture`
with no corresponding `MaterialTextureSet` role and no deferral comment,
unlike #2109's precedent.

## Impact
Signage/decal cutouts authored with distance-field alpha fall back to
plain alpha test — a fidelity gap, not a correctness bug.

## Suggested Fix
Add the role + fill, or at minimum a one-line deferral comment mirroring
#2109's precedent.

## Related
#2109 (the BGEM glass-overlay deferral this should mirror in documentation
style).

## Completeness Checks
- [ ] **TESTS**: N/A if only adding a deferral comment; if wiring the role, add a fixture asserting the forwarded texture
