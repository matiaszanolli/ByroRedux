## ANIM-08: Multi-emitter NIFs: color curve and rate extraction are first-match only — secondary emitters share first emitter values

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** anim
**File:** `crates/nif/src/import/walk/mod.rs:616`

## Recommended Fix

Defer until a multi-emitter regression surfaces. Add a comment at the extraction site documenting the first-match limitation and referencing this issue.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*