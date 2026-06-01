## SAFE-U6: ~208 of 539 unsafe blocks in renderer lack SAFETY comments — pervasive in gpu_timers.rs and blas_skinned.rs

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/renderer/src/vulkan/:various`

## Recommended Fix

Adopt tiered SAFETY comment policy: trivial destroy/create calls get one-line note; synchronization-dependent calls need full comments. Start with gpu_timers.rs (30+ uncommented) and acceleration/blas_skinned.rs (11 of 13 uncommented).

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
