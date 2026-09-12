# SF-2026-09-11-D3-06: no open tracker covers the loose .mat JSON resolver — #762 closed with its named first deliverable (Stage A) unbuilt, and #3398 is CDB-only

**Issue**: #4277 — https://github.com/matiaszanolli/ByroRedux/issues/4277
**Labels**: low,import-pipeline,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 3 — CDB Material Database Correctness
**Location**: `byroredux/src/asset_provider/material/cdb.rs (loose .mat JSON path, currently a stub/fallback only)`
**Status**: NEW

## Description
There is no open GitHub issue covering the loose `.mat` JSON resolver's actual implementation. #762 (which named this as its first deliverable, "Stage A") is closed without that deliverable being built, and #3398 (CDB Phase 2) is scoped to CDB-embedded materials only — it does not cover the separate loose-`.mat`-file-on-disk case. Starfield ships only 20 loose `.mat` files across the full 129-archive + Creation corpus, all third-party (per the census cited in `cdb.rs`'s own doc comment), so this is low-volume but currently untracked.

## Evidence
Verified during this audit: `gh issue list` search for "loose .mat JSON resolver" and related terms returns no open issue; #762's closing state does not reflect its own named Stage A deliverable; #3398's scope statement is CDB-only.

## Impact
No live defect — Starfield's loose-`.mat` population is small and third-party-only. The gap is purely one of project tracking: this remaining work has no open issue naming it, so it could be silently dropped rather than deliberately deferred.

## Related
#762 (closed, Stage A deliverable unbuilt), #3398 (CDB Phase 2, explicitly CDB-only).

## Suggested Fix
File this issue as the tracker for the loose-`.mat` JSON resolver (or fold it explicitly into #3398's scope if that is the intended home), so the remaining work is discoverable.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
