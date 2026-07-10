# REN-D2-03: shader-pipeline.md descriptor + instance-flag tables lag the live Set-1 layout (bindings 15/16/17, flag bit 8, binding-11 consumer list, GpuLight header size)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1915

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `docs/engine/shader-pipeline.md` (Descriptor Sets table + GpuInstance flags table + GpuLight section) vs `crates/renderer/shaders/include/bindings.glsl`
**Status**: NEW

## Description
Four current-state divergences, all doc rot (code verified correct and internally consistent): (1) Set-1 table ends at binding 14; live layout also has binding 15 `depthHistoryTex`, binding 16 `ReservoirCurrBuffer`, binding 17 `ReservoirPrevBuffer` (Session-49 ReSTIR). (2) The GpuInstance flags table lists bits 0-7 and 16-31 but omits bit 8 `INSTANCE_FLAG_DIFFUSE_ALPHA` (the BC1 guard). (3) The binding-11 row lists consumers "triangle, volumetrics" but no volumetrics shader binds the ray-budget buffer (volumetrics uses its own set-0 layout). (4) The GpuLight section says "Prefixed by a `u32 lightCount`"; the actual prefix is a 16-byte header (`u32 count` + 3×`u32` pad).

## Evidence
Symbol-anchored against `bindings.glsl` and `buffers.rs:20-28`; doc read at HEAD post the prior doc-sync commits which fixed other rows but not these.

## Impact
Documentation only, but this file is the designated authoritative reference for GPU layouts — a contributor wiring a new pass from the doc would under-declare the set-1 layout and mis-place the lights array base offset.

## Related
#1872 (memory ledger sibling doc gaps)

## Suggested Fix
Add rows 15/16/17 and the bit-8 flag row; correct the binding-11 consumer list; state the 16-B light header.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
