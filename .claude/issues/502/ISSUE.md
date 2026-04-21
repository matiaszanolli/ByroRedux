# Issue #502: Bulk read methods for NIF geometry arrays

Severity: MEDIUM
Labels: nif-parser, medium, performance
Location: `crates/nif/src/stream.rs`

## Problem
Per-element `read_exact` calls for geometry arrays. ~50K calls per 1000-block NIF.

## Required methods
- `read_ni_point3_array(count)` ✅ exists (line 251)
- `read_u16_array(count)` ✅ exists (line 303)
- `read_u32_array(count)` ✅ exists (line 315)
- `read_f32_array(count)` ✅ exists (line 327)
- `read_vec2_array(count)` ⚠️ equivalent to `read_uv_array` (line 286)

## Remaining work
1. Add `read_vec2_array` alias/generalization
2. Sweep blocks/*.rs for ~15-20 per-element hot sites
3. Wire through NiTriShapeData, BsTriShape, NiSkinPartition, NiMorphData
4. Byte-exact equivalence tests
