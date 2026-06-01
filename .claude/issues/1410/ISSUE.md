## TS-02: Global ABBA lock-order detector (BYRO_LOCK_ORDER_CHECK=1) is absent from CI

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** thread
**File:** `crates/core/src/ecs/lock_tracker.rs:217`

## Recommended Fix

Add BYRO_LOCK_ORDER_CHECK=1 to at least one CI job running cargo test with the parallel-scheduler feature enabled. Zero code change required.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId sort ordering

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*