## IOR-03: Ray budget atomicAdd fires unconditionally — rejected threads permanently inflate rayBudgetCount

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ior
**File:** `crates/renderer/shaders/triangle.frag:2124`

## Recommended Fix

Document the overshoot in the comment block. If a telemetry overlay is added reading the budget counter, clamp the displayed value to GLASS_RAY_BUDGET.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*