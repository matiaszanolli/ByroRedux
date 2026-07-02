# PERF-D4-NEW-03: upload_indirect_draws lacks the dirty gate both its siblings have

**Issue**: #1809
**Labels**: low,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-03)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-03)

## Location
`crates/renderer/src/vulkan/scene_buffer/upload.rs:594-626`; caller `crates/renderer/src/vulkan/context/draw.rs:3225-3238`

## Description
Instances (#1134) and materials (#878) both got content-hash dirty gates justified by "static interiors produce byte-identical slices each frame." The indirect command list, derived from the same batches, is byte-identical under the exact same conditions but is `copy_nonoverlapping` + `flush_range`'d unconditionally every frame (`upload.rs:617-625`). No `last_uploaded_indirect_hash` field exists.

## Evidence
`upload.rs:501,559` show the existing `last_uploaded_instance_hash` / `last_uploaded_material_hash` gates; `upload_indirect_draws` (`:594-626`) has no equivalent field or early-return.

## Impact
Small (worst realistic ≈160 KB/frame ≈ 10 MB/s at 60 fps; indirect entries are ~20 B vs 112 B for instances) — a consistency/completeness gap in the established pattern.

## Related
#878, #1134.

## Suggested Fix
Reuse the existing FxHash-over-slice + per-FIF `last_uploaded_hash` pattern (~15 lines mirroring `upload_instances`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

