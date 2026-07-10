# REN-D5-03: shrink_blas_scratch_to_fit doc still claims TLAS-scratch shrink "needs a follow-up" tracked by #495 — the follow-up shipped in the same file

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1923

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:24-28`
**Status**: NEW

## Description
The doc comment reads "Shrinking TLAS scratch needs a follow-up that mirrors [`pending_destroy_blas`]. Issue #495 tracks this gap." `shrink_tlas_scratch_to_fit` has since landed 200 lines below in the same file — it fills the gap via the fenced end-of-frame call site rather than a pending-destroy queue, and #495 is not an open issue.

## Evidence
`grep -n "shrink_tlas_scratch_to_fit" memory.rs` → definition at `:231` in the same file as the stale claim at `:27-28`; live call site `draw.rs:3994`.

## Impact
Doc rot only, but inside a safety-adjacent comment on an unsafe fn in the memory-reclaim path — the kind of comment audits and future fixes anchor on.

## Related
#682 / MEM-2-7; #1782

## Suggested Fix
Rewrite the sentence to point at `Self::shrink_tlas_scratch_to_fit` and its fence-positioned call-site contract; drop the #495 reference.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
