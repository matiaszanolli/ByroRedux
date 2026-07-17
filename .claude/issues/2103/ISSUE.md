# DIM4-STARFIELD-01: Phase 0/1 baseline doc's interior-REFR capture count is stale by 16 records

**Severity**: LOW
**Labels**: low, documentation
**Location**: `docs/engine/starfield-esm-phase0-baseline.md:174`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (DIM4-STARFIELD-01)

## Description
The doc's 2026-05-28-captured interior-REFR count (1,971,151) is 16 higher than a live re-run today (1,971,135). Traced to commit `2dc43106` (2026-06-26, post-dating the doc), which correctly skips deletion-tombstone REFRs — 16 vanilla Starfield.esm interior REFRs carry the Deleted flag and are now correctly excluded. This is the intended, more correct behavior of that fix, not a bug; the doc is simply dated.

## Suggested Fix
Add a "superseded by #1660" note next to the stale table, or refresh the captured count. Doc-only, low priority.

## Completeness Checks
No rows apply — this is a documentation-only fix.
