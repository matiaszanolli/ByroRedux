## SAFE-U7: Test code calls String::as_bytes_mut without SAFETY comment

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/core/src/string/mod.rs:233`

## Recommended Fix

Add SAFETY comment: "only ASCII single-byte codepoints are written (b'x' = 0x78); replacing an ASCII X keeps the string valid UTF-8." before the unsafe block.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*