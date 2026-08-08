# FO4-D5-01: BGSM resolve failure silently indistinguishable from authored-non-PBR fallback

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2601
**Finding ID**: FO4-D5-01

**Severity**: MEDIUM
**Dimension**: 5 (Materials/Shading)
**Location**: `byroredux/src/asset_provider/material.rs:787-807`
**Status**: NEW

## Description
When BGSM resolve fails, the code silently falls back to the legacy
Lambert+keyword classifier with no diagnostic distinguishing "material is
authored as non-PBR" from "BGSM lookup failed" (missing file, bad path,
parse error). This is the documented root cause of the recurring
"chrome/posterized FO4 surface" bug class — from the caller's perspective
both cases look identical, so a broken BGSM path is indistinguishable from
intentional legacy material authoring.

## Evidence
`byroredux/src/asset_provider/material.rs:787-807` — the BGSM-resolve
failure path falls through to the keyword classifier with no log/diagnostic
marking that resolution failed (vs. succeeded-but-non-PBR).

## Impact
Directly responsible for a recurring, hard-to-diagnose bug class: content
with a broken or missing BGSM reference silently renders with the fallback
Lambert path and looks like "chrome"/posterized specular, with no signal
telling a developer *why* — per [[feedback_chrome_means_missing_textures]],
this pattern has already cost debugging time on unrelated investigations.

## Suggested Fix
Add a diagnostic (log line, and/or a `tex.missing`-style debug-CLI counter)
that fires specifically on BGSM *resolve failure*, separate from the
"authored non-PBR, correctly falling back" case.

## Completeness Checks
- [ ] **TESTS**: A regression test with a deliberately-broken BGSM path pins the new diagnostic firing
