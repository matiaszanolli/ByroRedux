## SAFE-U1: Stale doc comment claims transmute on BuiltinType::from_u32 — impl is a safe checked match

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/sfmaterial/src/types.rs:10`

## Recommended Fix

Replace doc comment text "transmute into this enum" with accurate description: BuiltinType::from_u32 is a fully checked match returning Err(UnsupportedBuiltin) for unknown tags. No unsafe code involved.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
