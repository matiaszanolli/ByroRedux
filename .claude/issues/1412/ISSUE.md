## TS-08: Scheduler parallel panic leaves partial ECS state; process terminates on any system panic (no catch_unwind)

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** thread
**File:** `crates/core/src/ecs/scheduler.rs:386`

## Recommended Fix

Document the policy explicitly. Medium-term: wrap each rayon task body in catch_unwind and serialize the error to a shared error slot.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId sort ordering

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
