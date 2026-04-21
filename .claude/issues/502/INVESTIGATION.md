# Investigation

## Pre-existing state
Most bulk-read methods already existed in `stream.rs`:
- `read_ni_point3_array`, `read_ni_color4_array`, `read_uv_array`
- `read_u16_array`, `read_u32_array`, `read_f32_array`

Missing per issue: `read_vec2_array` (made an alias of `read_uv_array`).

## Hot sites rewired
- `NiSkinPartition` (skin.rs): bones, vertex_map, weights, strip_lengths, triangles
- `NiMorphData` (controller.rs:1107): vertex deltas via `read_ni_point3_array`
- `BsSkinBoneData` (skin.rs:450): per-bone 17 f32s via single `read_f32_array`
- `NiTriStripsData` (tri_shape.rs:938): strip_lengths + strip contents

Left untouched: `NiTriShapeData` already used bulk reads; `BsTriShape` vertex decode
is heterogeneous (half-float + byte-packed normals) and not amenable to uniform bulk reads.

## Safety
All bulk readers use `from_le_bytes` on `chunks_exact`, not `Vec::from_raw_parts` or
raw reinterpret. No unsafe blocks added. LE decode is explicit, so big-endian hosts
behave identically.

## Tests
Added `bulk_reads_match_per_element_loops`, `bulk_reads_handle_zero_count`,
`bulk_reads_reject_oversized_count` (3 new tests, 349→352 nif lib tests).
