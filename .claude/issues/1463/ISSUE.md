# REN-DIM18-02

**Issue:** #1463
**Filed:** 2026-06-04
**Source report:** docs/audits/AUDIT_RENDERER_2026-06-04_DIM18.md

---

**Severity:** LOW (latent — safe today because `dt` is immutable; a WAR hazard only if Phase 5 makes `dt` dynamic without converting to per-FIF)
**Dimension:** Volumetrics (M55)
**Source report:** `docs/audits/AUDIT_RENDERER_2026-06-04_DIM18.md`
**Location:** `crates/renderer/src/vulkan/volumetrics.rs:404-418` (`integration_param_buffer` construction), bound at `:528-543`

## Description
The **injection** param UBO (`param_buffers`) is correctly per-frame-in-flight (written each frame in `dispatch`). The **integration** param UBO (`integration_param_buffer`) is a single `Option<GpuBuffer>` (`volumetrics.rs:192`) written **once at construction** with the constant `dt = DEFAULT_VOLUME_FAR / FROXEL_DEPTH` (`:411-417`), with no per-frame update path. Both integration descriptor sets across all FIF slots bind this one buffer.

This is correct **today** because `dt` is immutable, so all FIF slots can safely alias one read-only UBO. But the shader doc and Phase-5 plan (`volumetrics_integrate.comp:14-15`, `volumetrics.rs:126-132`) call for a per-slice / per-frame exponential `dt`. If a future contributor starts writing this buffer per-frame without first making it per-FIF, frame N+1's host write will race frame N's in-flight integrate read — the exact WAR hazard the per-FIF `param_buffers` already avoids.

## Impact
None currently. Latent WAR hazard if Phase 5 makes `dt` dynamic without converting to per-FIF.

## Suggested Fix
Add a comment at `volumetrics.rs:411` ("single-buffered because dt is immutable; convert to a per-FIF `Vec<GpuBuffer>` before making dt dynamic — Phase 5") so the constraint is visible at the edit site. When Phase 5 lands, mirror the per-FIF pattern already used by `param_buffers`.

## Completeness Checks
- [ ] **SIBLING**: when converted to per-FIF, match the existing `param_buffers` per-FIF write/bind pattern exactly
- [ ] **DROP**: per-FIF `Vec<GpuBuffer>` teardown covers every slot (parity with the current single-buffer `.take()` at `volumetrics.rs:965`)
- [ ] Constraint comment added at the construction site now (cheap, prevents the future regression)

_No action required now — the comment is the only immediate step._
