## MEM-06: Collision shape recursion depth unbounded for deeply-nested BhkListShape — stack overflow risk from corrupt NIF

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** memory
**File:** `crates/nif/src/import/collision.rs:261`

## Recommended Fix

Add a depth counter to resolve_shape_inner; return None with warn! when depth exceeds 64. Cycle detection already exists but does not prevent deep acyclic chains.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
