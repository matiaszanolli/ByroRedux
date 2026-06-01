## R1-MAT-07: Over-cap material intern silently degrades to slot 0 with only a Once-gated warn — overflow count not in telemetry

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** material
**File:** `crates/renderer/src/vulkan/material.rs:1066`

## Recommended Fix

Add debug_assert_eq!(self.overflow_count, 0) at frame end in DebugStats drain, or expose overflow_count in the frame stats HUD and mem console command.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*