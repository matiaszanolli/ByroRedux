# #887 — SK-D1-NN-01: BSTriShape Bitangent X / Unused W slot read into tangent variable when VF_TANGENTS clear

**Audit**: `docs/audits/AUDIT_SKYRIM_2026-05-06_DIM1_4.md` (Dim 1)
**Severity**: LOW
**Labels**: low, nif-parser, bug
**Created**: 2026-05-07

## Sites
- `crates/nif/src/blocks/tri_shape.rs:647-666`
- `crates/nif/src/import/mesh.rs:1374-1380` (SSE-buffer twin)

## Summary
Trailing 4-byte slot after the position triplet is read into a local named `bitangent_x` regardless of whether `VF_TANGENTS` is set. Per `nif.xml:2107-2126` (`BSVertexData`) the slot is `Bitangent X` only when `(ARG & 0x411) == 0x411`; otherwise it is `Unused W`. Output is correct because tangent assembly gates on `bitangent_z`, but the dual semantic isn't visible to a reader.

## Fix
Either gate `bitangent_x = Some(_)` on `VF_TANGENTS` and `skip(4)` otherwise, or rename the local to `bitangent_x_or_unused`.

## Completeness
- SIBLING: lockstep with the SSE-buffer twin in `import/mesh.rs:1374-1380`.
- TESTS: regression test for "no-tangents BSTriShape, full-precision".
