# SAFE-2026-07-02-01: Skinned-BLAS first-sight BUILD → same-cmd UPDATE refit lacks AS_READ in the barrier

_Filed as #1790 from `docs/audits/AUDIT_SAFETY_2026-07-02.md`._

**Severity**: HIGH · **Dimension**: 5 — Vulkan Spec Compliance (Missing AS barrier) · Source: `AUDIT_SAFETY_2026-07-02` (SAFE-2026-07-02-01, unchanged carry-forward of SAFE-2026-07-01-01 — never previously filed as a GitHub issue; fresh dedup pull confirms no open/closed issue covers the missing src-AS read)

## Location
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:606-620` (`record_scratch_serialize_barrier`)
- Hazard sequence recorded in `crates/renderer/src/vulkan/context/draw.rs:1780-1909` (`record_skinned_blas_refit`: first-sight `build_skinned_blas_batched_on_cmd` at `:1781` → refit loop at `:1835-1899` → closing barrier at `:1902-1909`)

## Description
`refit_skinned_blas` records an UPDATE-mode build with `src == dst == entry.accel`. Per the Vulkan spec, an UPDATE-mode build **reads** `srcAccelerationStructure` with `VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR` at the `ACCELERATION_STRUCTURE_BUILD` stage. On an entity's first-sight frame the same command buffer records the fresh BLAS BUILD (writes the AS) at `draw.rs:1781`, then falls through unconditionally to the refit loop (first-sight entities are in `pose_dirty`, so the #1196 skip gate at `draw.rs:1860-1870` does not fire for them). The only barrier between the two builds is the self-emitted scratch-serialize barrier (`blas_skinned.rs:610-618`): src `AS_WRITE` → dst `AS_WRITE` only — **no `AS_READ` bit on the destination mask**. The BUILD's write to the AS backing buffer is therefore never made visible to the refit's `AS_READ` access → RAW hazard.

The cross-frame steady-state case is fine: the closing `AS_WRITE → AS_READ` barrier after the refit loop (`draw.rs:1902-1909`) covers frame N+1's refit reading frame N's write; the gap is specifically the same-command-buffer first-sight case.

The `refit_skinned_blas` docstring (`blas_skinned.rs:380-383`) still claims "The barrier is idempotent (`MEMORY_READ | MEMORY_WRITE → MEMORY_READ | MEMORY_WRITE`)" — the code it describes is `AS_WRITE → AS_WRITE` only; the docstring documents READ coverage the implementation does not have.

## Evidence
Validation-layer output captured on a 180-frame FNV `GSProspectorSaloonInterior` run (`VK_LAYER_KHRONOS_validation` + sync-validation, RTX 4070 Ti) — 10 occurrences (one per first-sight skinned NPC), then the layer's `duplicate_message_limit` suppressed further instances:

```
vkCmdBuildAccelerationStructuresKHR(): READ_AFTER_WRITE hazard detected.
vkCmdBuildAccelerationStructuresKHR reads VkBuffer 0x2a450000002a45, which was
previously written by another vkCmdBuildAccelerationStructuresKHR command. The
buffer backs pInfos[0].srcAccelerationStructure (VkAccelerationStructureKHR
0x2a460000002a46).
    The current synchronization allows VK_ACCESS_2_ACCELERATION_STRUCTURE_WRITE_BIT_KHR
accesses at VK_PIPELINE_STAGE_2_ACCELERATION_STRUCTURE_BUILD_BIT_KHR, but to
prevent this hazard, it must allow VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR
accesses at VK_PIPELINE_STAGE_2_ACCELERATION_STRUCTURE_BUILD_BIT_KHR.
    Buffer access region: { offset = 0  size = 88192 }
```

The barrier the layer is describing, re-read verbatim from current source (`blas_skinned.rs:606-620`):

```rust
pub fn record_scratch_serialize_barrier(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
    unsafe {
        memory_barrier(
            device, cmd,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,   // <- no AS_READ
        );
    }
}
```
Confirmed byte-identical against current `HEAD` (`1b4e8e84`) during this publish pass.

## Impact
On first-sight frames the refit may consume a partially-written source BVH — driver-dependent BVH corruption on real hardware (garbage or missing skinned geometry in every RT effect: shadows, GI, reflections), formally UB per the spec's synchronization requirements. Fires for **every** newly-spawned skinned NPC (cell transitions, streaming) — realistic, common-path conditions, matching the severity table's "Vulkan validation layer errors in normal operation" / "Missing AS barrier" HIGH rows. Not CRITICAL: geometry, addresses, and counts are all correct (#907/#1145 guards re-verified intact); the defect is visibility ordering only.

## Related
#983 / #1140 (scratch-serialize invariant — the pinned predicate `requires_scratch_serialize_barrier_before` codifies scratch WAW only, not the src-AS read), #911 (moved first-sight builds onto the per-frame cmd, creating the same-cmd adjacency that exposes this gap), #1436 (build-input access flags — a different access class), #1139 (older docstring drift in the same function), #1300 / #1095 (prior AS_WRITE→AS_WRITE serialize-barrier fixes — neither covers the src-AS read gap), #1782 (CONC-D1-01 — different bug: host-side scratch-buffer *destruction* timing, not this barrier's access-mask content).

## Suggested Fix
Widen the dst access mask in `record_scratch_serialize_barrier` to `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR` (one line; src mask stays `AS_WRITE`). Correct the `refit_skinned_blas` docstring to match the actual mask. Re-run the validation-layer scenario (FNV saloon, 180 frames) and confirm zero hazards; extend the #1140 predicate test to pin the READ bit so a future refactor can't narrow it again.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other AS-build barrier call sites)
- [ ] **LOCK_ORDER**: N/A — no RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (extend the #1140 predicate test to pin the READ bit)
