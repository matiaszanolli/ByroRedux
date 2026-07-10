# REN-D1-02: build_tlas resize comment sizes TLAS instances at 88 B; VkAccelerationStructureInstanceKHR is 64 B

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1912

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:364-378`
**Status**: NEW

## Description
The pre-size rationale comment claims "8192 slots × 88 B per instance × 2 FIF = ~1.4 MB across both slots" and derives "~660 KB BAR" per slot. `size_of::<vk::AccelerationStructureInstanceKHR>()` is 64 B (48 B transform + 4 B idx/mask + 4 B SBT/flags + 8 B AS reference), so the real numbers are 512 KB per slot / ~1.0 MB across both. The two sibling docs are correct: `constants.rs:33` ("64 B/entry") and `memory.rs:316-318` (telemetry doc, "64 bytes").

## Evidence
`INSTANCE_STRIDE` in `memory.rs:154-155` is computed from `size_of::<vk::AccelerationStructureInstanceKHR>()`; the padded-size math at `tlas.rs:388` uses the same `size_of`, so only the prose is wrong — the code allocates correctly.

## Impact
Comment-only; overstates BAR waste by ~38% in the exact paragraph an auditor reads when re-evaluating the 8192 floor trade-off.

## Related
REN-D2-NEW-02 / REN-D8-NEW-10 (audit 2026-05-09) — the notes this comment encodes

## Suggested Fix
Replace 88 B → 64 B and rederive (~512 KB/slot, ~1.0 MB across 2 FIF).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
