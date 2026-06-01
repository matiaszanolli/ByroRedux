## FFI-01: engine_info exported Rust function declared in extern Rust but never called from C++

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ffi
**File:** `crates/cxx-bridge/src/lib.rs:17`

## Recommended Fix

Remove the extern Rust engine_info declaration if aspirational, or add a corresponding C++ test caller in native_utils.cpp.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*