## MEM-05: read_pod_vec generic bound (Copy + Default) does not enforce all-bit-patterns-valid at the type level

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** memory
**File:** `crates/nif/src/stream.rs:311`

## Recommended Fix

Introduce unsafe trait AnyBitPattern or use bytemuck::AnyBitPattern and change the bound to T: Copy + Default + AnyBitPattern.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
