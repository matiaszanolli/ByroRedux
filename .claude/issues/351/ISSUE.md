# S1-03: BSTriShape tangent/bitangent parsed but discarded; renderer Vertex has no tangent attribute

**Severity:** MEDIUM
**Labels:** bug, nif-parser, renderer

## Locations
- `crates/nif/src/blocks/tri_shape.rs:~339,346,362,371-373` (parse-time discards)
- `crates/nif/src/import/mesh.rs:~226-250` (import discards)
- `crates/renderer/src/vertex.rs:17-30` (no tangent field)

## Problem
BSTriShape packed vertex format encodes per-vertex tangent space (bitangent_x, bitangent_y, 3-byte tangent, bitangent_z). All four are consumed but discarded, and the renderer `Vertex` has no tangent attribute. Result: derivative-reconstructed tangents in shader → wrong specular on curved / mirrored surfaces.

## Fix scope (5+ files — pause for confirmation)
1. `BsTriShape` — store `tangents: Vec<NiPoint3>` + `bitangent_signs: Vec<f32>`
2. `Vertex` — add `tangent: [f32; 4]`  (xyz + sign)
3. `ImportedMesh` — wire through
4. NiTriShape path — read `Tangent space (binormal & tangent vectors)` NiBinaryExtraData
5. Shaders — update Vertex struct in `triangle.vert`, `ui.vert`, `caustic_splat.comp`

## Audit source
`docs/audits/AUDIT_SKYRIM_2026-04-16.md` — finding S1-03
