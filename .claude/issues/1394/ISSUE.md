## TS-03: Systems with undeclared access generate AccessConflict::Unknown blocking parallel conflict analysis

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** thread
**File:** `crates/core/src/ecs/scheduler.rs:594`

## Recommended Fix

Migrate system registration to include Access declarations. Drive undeclared_parallel_count() to zero.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId sort ordering

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*