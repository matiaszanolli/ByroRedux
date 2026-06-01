## TS-06: SSAO OOM self-deadlock guard on allocator Mutex relies on implicit RAII drop timing with no comment

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** thread
**File:** `crates/renderer/src/vulkan/ssao.rs:149`

## Recommended Fix

Add a comment at each allocator.lock().allocate() + error-cleanup site referencing the #1163 fix pattern. Similar patterns in gbuffer.rs, svgf.rs, caustic.rs.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId sort ordering

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*