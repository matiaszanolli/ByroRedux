# SF-2026-09-11-D2-03: #3930 made SkinAttach the primary skin-name-recovery channel but did not short-circuit the now-redundant #3549 geometric solve, which still runs (and is discarded) on ~89.5% of skinned shapes

**Issue**: #4270 — https://github.com/matiaszanolli/ByroRedux/issues/4270
**Labels**: low,nif-parser,nif,tech-debt,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 2 — BSGeometry Mesh Extraction
**Location**: `crates/nif/src/import/mesh/skin.rs (or sibling skin-name-recovery module — #3549 geometric solve, #3930 SkinAttach channel)`
**Status**: NEW

## Description
#3930 made the `SkinAttach` channel the primary path for Starfield skin-name recovery (now covering 18,990/18,990 all-NULL-ref skins), superseding #3549's geometric-solve fallback for the dominant case. The geometric solve was not made conditional on `SkinAttach` having already succeeded, so it still runs — and its result is discarded — on ~89.5% of Starfield skinned shapes where `SkinAttach` already resolved the name.

## Evidence
Measured during this audit: `SkinAttach` now resolves 18,990/18,990 of the previously-hard all-NULL-ref cases; the #3549 geometric solve continues executing unconditionally and its output is thrown away in ~89.5% of skinned-shape imports.

## Impact
Pure CPU waste, no correctness impact — the geometric solve's more expensive per-vertex computation runs and is discarded on the vast majority of Starfield skinned meshes during import, a measurable but not currently profiled cost on import time.

## Related
Adjacent to #3549 (introduced the geometric solve) and #3930 (added the now-primary SkinAttach channel).

## Suggested Fix
Short-circuit the #3549 geometric solve when `SkinAttach` has already produced a name for the shape, running it only as an actual fallback.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
