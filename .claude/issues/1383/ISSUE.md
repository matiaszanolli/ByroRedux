## FFI-03: No panic=abort profile — cxx exception bridging relies on unwinding being intact

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ffi
**File:** `Cargo.toml:194`

## Recommended Fix

Document in profile.release that panic=unwind is required for cxx exception safety. Ensure all C++ functions are noexcept if abort is ever desired.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*