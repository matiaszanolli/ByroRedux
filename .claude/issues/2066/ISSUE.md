# TD2-107: bhkCompressedMeshShapeData reimplements read_vec4 inline 7x instead of calling the collision module's shared reader

**GitHub Issue**: #2066
**Labels**: low,nif-parser,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `crates/nif/src/blocks/collision/compressed_mesh.rs` (7 sites) vs. `collision/mod.rs:83-90`

## Description
6 of 9 collision siblings correctly reuse `read_vec4`; `compressed_mesh.rs` never imports it.

## Evidence
Confirmed live: `collision/mod.rs:83` defines `fn read_vec4(stream: &mut NifStream) -> io::Result<[f32; 4]>` reading 4× `read_f32_le()`. `compressed_mesh.rs` has zero `use super::read_vec4` and instead hand-rolls the identical 4× `stream.read_f32_le()?` pattern at 7 sites (lines ~27-30, 34-37, 111-114, 117-120, 148-151, 154-157, 192-195).

## Suggested Fix
`use super::read_vec4;`, replace all 7 sites.

**Effort**: trivial

## Completeness Checks
- [ ] **SIBLING**: Confirm the other 6 collision-parser siblings that already use `read_vec4` aren't hiding a similar unswapped site
- [ ] **TESTS**: Existing `bhkCompressedMeshShapeData` parse tests cover this path — purely mechanical swap, no new test needed
