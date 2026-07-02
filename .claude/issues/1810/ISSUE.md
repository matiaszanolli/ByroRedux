# PERF-D4-NEW-04: Stale byte-math in scene_buffer comments — GpuLight quoted at 48 B (is 64 B), GpuInstance quoted at 72 B (is 112 B)

**Issue**: #1810
**Labels**: low,renderer,documentation
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-04)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-04)

## Location
`crates/renderer/src/vulkan/scene_buffer/constants.rs:10`; `upload.rs:492-494`; `descriptors.rs:292-295`

## Description
`GpuLight` is 64 B and `GpuInstance` is 112 B (pinned by the layout test), but three in-code comments still quote the pre-R1 sizes, understating live PCIe traffic estimates.

## Evidence
`constants.rs:10` "512 lights × 48 bytes = 24 KB per frame"; `upload.rs:494` "(7359 × 72 B)"; `descriptors.rs:292` "MedTek ships 7359 draws at 72 B" — all stale vs the pinned 64 B / 112 B sizes in `gpu_types.rs` + `gpu_instance_layout_tests.rs`.

## Impact
Doc-rot only — sizes that matter (buffer allocation, flush ranges, tests) all derive from `size_of`, not the comments.

## Related
memory-budget.md (already correct); #1134, #1587.

## Suggested Fix
Update the three comments to 64 B / 112 B and recompute the quoted per-frame figures.

## Completeness Checks
- [ ] **SIBLING**: Other doc cross-references checked for the same rot

