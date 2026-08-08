# SF2D2-D2-04: UDEC3-decoded normals feed unnormalized into tangent synthesis Gram-Schmidt

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2632
**Finding ID**: SF2D2-D2-04

**Severity**: LOW
**Dimension**: 2 (BSGeometry Mesh Extraction)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:150-162,217`, `crates/nif/src/blocks/bs_geometry.rs:569-580`, `crates/nif/src/import/mesh/tangent.rs:442-505`
**Status**: NEW

## Description
UDEC3-decoded normals feed unnormalized into `synthesize_tangents_yup`'s
Gram-Schmidt, which assumes unit N. `unpack_udec3_xyzw`'s raw remap has no
normalization (unit-length only to 10-bit quantization); the Gram-Schmidt
projection is only correct for `|n| == 1`, and the degenerate fallback
branch (`t_y = [n[1], n[2], n[0]]`) is neither normalized nor
orthogonalized against `n`.

## Evidence
`unpack_udec3_xyzw` (`crates/nif/src/blocks/bs_geometry.rs:569-580`)
performs no normalization; `synthesize_tangents_yup`'s Gram-Schmidt
(`crates/nif/src/import/mesh/tangent.rs:442-505`) assumes unit-length input.

## Impact
Quantization error (~0.1%) is visually negligible on the non-degenerate
path (shader renormalizes); the degenerate branch's non-orthogonality is a
pre-existing shared divergence (AUDIT_INCREMENTAL_2026-05-22 ID-4),
sub-pixel in practice.

## Suggested Fix
`normalize_inplace` the copy fed to `synthesize_tangents_yup`; orthogonalize
the degenerate `t_y` against `n` with Gram-Schmidt + normalize before the
cross product.

## Related
AUDIT_INCREMENTAL_2026-05-22 ID-4 (the pre-existing shared divergence this
overlaps with).

## Completeness Checks
- [ ] **TESTS**: A degenerate-normal fixture asserts the fallback tangent is normalized and orthogonal to N
