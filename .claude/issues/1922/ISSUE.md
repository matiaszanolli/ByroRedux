# REN-D5-02: flush_pending_uploads doc claims the queue survives a recording error for retry; the implementation mem::takes and drops it

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1922

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/texture_registry.rs:724-731,746-750` vs `750,838-854`
**Status**: NEW

## Description
The fn doc says "On any recording error the queue is left intact so a retry is possible" and a pre-loop comment says entries "are pushed back at the end so a retry sees a non-empty queue". Neither is true: `let pending = std::mem::take(&mut self.pending_dds_uploads)` and the error arm drops `pending`. Worse than the comment implies: handles reserved by `queue_or_hit` keep `texture: None` with live `path_map` entries, so any later enqueue for the same path cache-HITS the dead handle and bumps its refcount — the texture can never be re-uploaded until every ref drops.

## Evidence
Contradicting comments at `texture_registry.rs:725-726` ("queue is left intact") and `:748-749` ("pushed back at the end") vs the actual `take` at `:750`. No push-back code exists in the function.

## Impact
No resource leak (staged textures + staging are destroyed on the error path). Risk is developer-facing: someone implementing the retry the doc promises gets a guaranteed-empty queue, and the permanent-fallback consequence of a failed batch is undocumented. Rare path (whole-batch failure = command-buffer alloc/begin failure).

## Related
#881 (batched flush origin); #1861 (separate: infra fence/cmd leak inside the same helper)

## Suggested Fix
Fix the two stale comments to describe the real contract, or actually restore un-staged entries into `pending_dds_uploads` in the error arm.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
