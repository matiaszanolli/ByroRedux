# FO4-D5-03: BSVER 131 empty-flags gap band has no diagnostic

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2603
**Finding ID**: FO4-D5-03

**Severity**: LOW
**Dimension**: 5 (Materials/Shading)
**Location**: shader-flag gate on `bsver >= FALLOUT4`
**Status**: NEW

## Description
BSVER 131 (`FO4_SHADER_GAP`) carries zero shader flags by design, but the
`bsver >= FALLOUT4` gate reads as active regardless of whether this specific
version band actually carries any flags — there's no diagnostic
distinguishing "flags present and zero" from "this version never carries
flags". For vanilla FO4 content this is masked because BGSM-merge supplies
the flags anyway.

## Evidence
The `bsver >= FALLOUT4` shader-flag gate treats BSVER 131 the same as any
other BSVER ≥ FALLOUT4, even though 131 is a documented gap band with no
native flag payload.

## Impact
Masked by BGSM-merge on vanilla FO4 — flags end up correct via the BGSM
path regardless. Would only surface as a real gap for NIF-native-only
content in the 131 band with no BGSM override, which does not occur in
vanilla FO4.

## Suggested Fix
Add a diagnostic or explicit branch for the BSVER 131 gap band so future
debugging doesn't have to rediscover that it's a known-empty flag range.

## Completeness Checks
- [ ] **TESTS**: N/A unless a diagnostic is added — then pin its firing on a BSVER-131 fixture
