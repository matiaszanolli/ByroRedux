# #4801: PERF-D2-2026-09-23b-03: The #4580 comment says two-sided intent rides on `PipelineKey`; it is dynamic cull state, deliberately not a key axis

**Severity**: LOW
**Labels**: low, renderer, doc-rot, documentation
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D2-2026-09-23b-03)

- **Severity**: LOW (doc rot on a batching invariant)
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:598-600` (`f1fadbf9f`)
- **Status**: NEW
- **Description**: Two-sided is not a `PipelineKey` axis (#930; `pipeline.rs:114-123`). It is a `group_state` batch-merge axis and sort slot 5. The comment invites an unnecessary pipeline variant or sort slot.
- **Suggested Fix**: Reword to "…applied through dynamic `cmd_set_cull_mode` (#930; a `group_state` axis, not a `PipelineKey` axis)."

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
