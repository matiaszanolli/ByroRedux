# EXT-D2-2026-10-26-03: Stale GpuTerrainTile layout comments survive #6352's prose sweep

Labels: low,terrain-exterior,renderer,doc-rot,documentation
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW / **Dimension**: 2 / **Tier Violated**: n/a
- **Game Affected**: n/a (docs) / **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:468-470`; `gpu_types.rs:78-81`; `shaders/include/bindings.glsl:535` / **Status**: NEW
- **Description**: The numeric asserts are right (224 B, offsets to 220), but comments describe older strides: the offsets test says it
  "Matches `ArrayStride 160` + members at 0 / 32 / 64 / 96 / 112 / 128 / 136 / 140 / 144" (the stride is 224, members go to 220);
  `gpu_types.rs` says the band slot kept "the stride stays 208"; `bindings.glsl` says "Appended at the tail; stride 176 -> 208".
  The last two are history phrased as present tense after the record grew to 224.
- **Evidence**: `grep -n "208\|ArrayStride 160" ...` hits above; `gpu_terrain_tile_is_224_bytes` and the shipped-SPIR-V stride test say 224.
- **Impact**: Misleads the next person changing the record (the exact audience of the #6352 sweep).
- **Related**: #6352 (closed, fixed 208->224 in other places), #6206.
- **Suggested Fix**: Reword the three comments to "was 208 at #6196, 224 since #6197" / "ArrayStride 224 + members to 220".

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
