# #5251: REN-D1-2026-10-05-02: The first-sight batch comment in `skinned_blas_refit.rs` still says the helper sizes the shared scratch from `build_scratch_size`

**Labels**: low,renderer,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5251

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-05.md` — `REN-D1-2026-10-05-02` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: AS Correctness (doc)
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`, the comment above `accel.build_skinned_blas_batched_on_cmd(` ("The helper queries every entity's `build_scratch_size`, grows `blas_scratch_buffer` ONCE to the max demand of the batch").
- **Status**: NEW (residue of #5195)
- **Description / Fix**: Since #5195 the helper grows to `max(build, update)` across the batch. Reword the comment. It is the deletion-inviting kind of text #5195 removed elsewhere.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
