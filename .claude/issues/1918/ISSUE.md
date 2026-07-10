# REN-D3-02: Authoritative layout docs drifted from code in five places (doc rot — code verified correct)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1918

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `docs/engine/shader-pipeline.md:149-160,126,167,181,186` and `docs/engine/memory-budget.md:30-32`
**Status**: NEW

## Description
Five concrete divergences between the authoritative docs and the (test-pinned, correct) code, all doc rot: (1) instance-flags table missing bit 8 `INSTANCE_FLAG_DIFFUSE_ALPHA` (#1653). (2) GpuCamera row says `dof_params` "zw reserved" but code uploads `z = light_atten_knee`, `w = camera_static`; `gpu_types.rs:254-257` carries the same stale comment. (3) GpuMaterial section says "full layout in `gpu_types.rs`" — it's actually in `material.rs:72`. (4) GpuMaterial table rows mislabel offsets (96-119 "UV transform" is actually parallax fields; 232-255 "BSEffect falloff" starts at `sparkle_intensity`); `material_flags` table lists only 4 of 11 live bits. (5) memory-budget.md says exceeding MAX_INSTANCES "currently a `debug_assert` fires" — #956/#992 replaced that with a warn-once + clamp.

## Evidence
Each point grep/read-confirmed against `constants.rs`, `gpu_types.rs`, `material.rs`, `draw.rs`, and `upload.rs`; all corresponding layout tests pass, so code is the correct side in every case.

## Impact
Future audits and shader authors using the authoritative docs inherit a stale flag catalog, a wrong file pointer, and a wrong overflow-behavior claim.

## Related
Sibling doc-rot findings across this audit (REN-D2-03, REN-D3-03)

## Suggested Fix
Add the bit-8 row, correct the dof_params zw description in both doc and `gpu_types.rs`, point GpuMaterial at `vulkan/material.rs`, relabel the offset rows, complete the material_flags table, update the overflow sentence.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
