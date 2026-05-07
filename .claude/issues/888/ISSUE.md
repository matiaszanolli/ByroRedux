# #888 — SK-D1-NN-02: SSE packed-buffer decoder hard-codes full-precision positions

**Audit**: `docs/audits/AUDIT_SKYRIM_2026-05-06_DIM1_4.md` (Dim 1)
**Severity**: LOW (latent)
**Labels**: low, nif-parser, bug
**Created**: 2026-05-07

## Site
- `crates/nif/src/import/mesh.rs:1370-1380` — `decode_sse_packed_buffer`

## Summary
Decoder hard-codes 12-byte f32 positions with comment "SSE always uses full-precision". Correct today (`bsver in [100, 130)` gate at `try_reconstruct_sse_geometry`, and `BSVertexDataSSE` is unconditionally f32 by schema). Latent: extending the decoder to FO4 (bsver=130) without re-introducing the `bsver() < 130 || vertex_attrs & VF_FULL_PRECISION != 0` rule will mis-decode every FO4 mesh that ships without `VF_FULL_PRECISION`.

## Fix
Either mirror the inline parser's full-precision rule and produce a half-precision branch, OR add `debug_assert!(bsver < 130, "decode_sse_packed_buffer is SSE-only")` so the foot-gun fires in tests.

## Completeness
- SIBLING: inline parser at `tri_shape.rs:646-666` already correct — verify no other SSE-style decoder shares the assumption.
- TESTS: `debug_assert` itself is the regression guard if option (2) is taken.
