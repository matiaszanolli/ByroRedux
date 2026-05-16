# REN-D16-001: Starfield BSGeometry tangents always Vec::new()

**GitHub**: #1086
**Domain**: nif (import mesh)
**Location**: crates/nif/src/import/mesh/bs_geometry.rs:155

## Root Cause
tangents: Vec::new() hardcoded. BSGeometryMeshData.tangents_raw has UDEC3-packed
tangent data. Y-up already (Starfield-native) — NO bs_tangents_zup_to_yup swap.
unpack_udec3_xyzw(raw) → [x, y, z, w] where w = bitangent sign.

## Fix (2 files)
1. bs_geometry.rs: decode tangents_raw → Vec<[f32;4]>
2. bs_geometry_tangent_tests.rs: regression test
