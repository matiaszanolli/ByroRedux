## MEM-04: BGSM material cache evicts by clearing entire map on overflow instead of LRU

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** memory
**File:** `byroredux/src/asset_provider.rs:900`

## Recommended Fix

Replace flush-on-overflow with LRU eviction retaining most-recently-used half. Applies to bgem_cache and failed_paths.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
