## VKC-001: PipelineStageFlags::NONE used in sync1 barriers without synchronization2 feature gate (VUID-vkCmdPipelineBarrier-srcStageMask-4957)

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/bloom.rs:399`

## Recommended Fix

Option A: require synchronization2 as mandatory device feature (preferred given RT/VRAM baseline). Option B: fall back to TOP_OF_PIPE per barrier site when sync2 is false. Affects: bloom.rs, ssao.rs, caustic.rs, volumetrics.rs, texture.rs.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
