## FFI-02: No test coverage for the cxx bridge

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ffi
**File:** `crates/cxx-bridge/src/lib.rs:1`

## Recommended Fix

Add a #[cfg(test)] module with at least one test calling ffi::native_hello().

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*