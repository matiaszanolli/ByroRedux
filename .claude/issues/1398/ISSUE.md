## MEM-01: NifImportRegistry unlimited by default — unbounded process-lifetime RAM growth in long streaming sessions

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** memory
**File:** `byroredux/src/cell_loader/nif_import_registry.rs:107`

## Recommended Fix

Set max_entries default to 2048 rather than 0. Keep BYRO_NIF_CACHE_MAX env-var override. Add startup warning when cache is unlimited and exterior streaming is active.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*