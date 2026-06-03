## MEM-04: BGSM material cache evicts by clearing entire map on overflow

**Severity**: LOW
**Domain**: memory
**Location**: `byroredux/src/asset_provider.rs:900`
**Source audit**: AUDIT_SAFETY_2026-06-01.md

`bgem_cache` (HashMap) and `failed_paths` (HashSet) both call `clear()` on
overflow. Fix: add `VecDeque<String>` insertion-order trackers; on overflow
evict oldest N/2 entries instead of flushing everything.
