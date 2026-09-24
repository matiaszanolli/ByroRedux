# #4811: PERF-D8-2026-09-23b-02: `GpuTimerSnapshot::composite_ms` documents a pre-#4202 / pre-#2796 composite

**Severity**: LOW
**Labels**: low, renderer, doc-rot, documentation
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D8-2026-09-23b-02)

- **Severity**: LOW (doc)
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:203-205`
- **Status**: NEW
- **Description**:
  - The doc says composite writes "into the swapchain image with ACES tone-mapping" with bloom as input. In fact composite writes linear HDR; bloom runs after it (#2796); tone mapping (ACES|AgX) is in presentation.
  - The new aperture mask (PERF-D5-2026-09-23b-03) is now also part of `composite_ms`.
- **Suggested Fix**: Rewrite the doc, and fold in the stale "exposure + ACES" presentation label tracked under #4618.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
