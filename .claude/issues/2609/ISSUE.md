# FO4-D7-04: BGSM_AUTHORED set identically on BGEM arm despite overrides left None

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2609
**Finding ID**: FO4-D7-04

**Severity**: LOW
**Dimension**: 7 (Canonical Material)
**Location**: `merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
**Status**: NEW

## Description
`BGSM_AUTHORED` is set identically on both the BGSM and BGEM merge arms, but
only the BGSM arm actually writes `metalness_override`/`roughness_override`
— the BGEM arm sets `from_bgsm`/`BGSM_AUTHORED` with those overrides left
`None`. A future consumer reading `BGSM_AUTHORED` as "PBR scalars authored"
(the assumption FO4-D7-01's suggested fix would introduce) would be wrong on
BGEM content.

## Evidence
`merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
sets `BGSM_AUTHORED` on both arms, but `metalness_override`/
`roughness_override` are only populated on the `.bgsm` arm — the `.bgem` arm
leaves them `None` while still setting the same flag.

## Impact
Not a live bug today, but a correctness trap for future work — specifically
including FO4-D7-01's suggested fix, which proposes gating an overwrite on
`BGSM_AUTHORED` assuming it means "PBR scalars present."

## Suggested Fix
Either split the flag (`BGSM_AUTHORED` vs. a separate
"scalar-overrides-present" flag), or only set `BGSM_AUTHORED` on the arm
that actually populates the override fields it's meant to signal.

## Related
FO4-D7-01 (a consumer that would be broken by this ambiguity if implemented
naively).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Flag semantics fix belongs at the merge boundary
- [ ] **TESTS**: A regression test asserts `BGSM_AUTHORED`'s meaning is consistent (or split) across both BGSM/BGEM arms
