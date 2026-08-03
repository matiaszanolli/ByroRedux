# PERF-D-DOC-01: ROADMAP.md Bench-of-record predates ~90 commits of substantial rendering/streaming work

Filed from: `docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2279
Labels: low, performance, documentation

**Severity**: LOW
**Dimension**: Cross-cutting (documentation/process)

**Location**: `ROADMAP.md:71` ("Bench-of-record (LIVE) — R6a-stale-17 refresh (2026-07-26, HEAD `3a02b02d`)")

## Description
The pinned bench-of-record HEAD (`3a02b02d`) sits roughly 90 commits behind the current tree (`1ae86f62`). The intervening work is not cosmetic: the full procedural volumetric-fog rewrite (froxel V-buffer, temporal reprojection, new `composite.frag`/`volumetrics_inject.comp` GPU cost), the entire resumable/budgeted streaming rearchitecture (see the sibling PERF-D7-01/02/03 issues from this same report), a materials-pipeline refactor (`ImportedMaterial`/`MaterialTextureSet<T>`), and the Scaleform/FSR3 host-bridge work have all landed since. ROADMAP already flags the block as stale in its own text, so this is not new information, but the gap has grown large enough (a full session, several GPU-cost-relevant features) that the next `/session-close` bench refresh is now overdue rather than optional.

## Evidence
`ROADMAP.md:71` pins HEAD `3a02b02d` (2026-07-26); current HEAD is `1ae86f62`, roughly 90 commits later per `git log --oneline`.

## Impact
No functional impact — this is a documentation/process gap. It does mean any FPS claim made against the current tree cannot be sanity-checked against a comparably-recent baseline right now.

## Suggested Fix
Run `scripts/fsr-bench-matrix.sh 3 300` against current HEAD at the next opportunity with GPU access and refresh the ROADMAP block.

## Completeness Checks
N/A — this is a documentation-refresh action item (re-run the bench script and update a ROADMAP.md block), not a code fix. No UNSAFE/SIBLING/DROP/LOCK_ORDER/FFI/CANONICAL-BOUNDARY/TESTS checks apply.
